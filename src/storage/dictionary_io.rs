//! Dictionary / RDF-term I/O helpers -- extracted from storage/mod.rs (MOD-01, v0.72.0).
//!
//! Term parsing, encoding, and FTS tokenisation helpers.

use crate::dictionary;

// ─── In-update deduplication tracking ────────────────────────────────────────

// Thread-local set tracking (p, s, o, g) quads inserted during the current

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a bare IRI `<…>` or return the term as-is for encode dispatch.
pub(crate) fn strip_angle_brackets(term: &str) -> &str {
    let t = term.trim();
    if t.starts_with('<') && t.ends_with('>') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Public wrapper for strip_angle_brackets — used by lib.rs.
pub fn strip_angle_brackets_pub(term: &str) -> &str {
    strip_angle_brackets(term)
}

/// Parse an N-Triples–style term and return `(clean_value, kind, datatype, lang)`.
/// Supports IRIs, blank nodes, plain/typed/lang literals.
pub fn parse_rdf_term(s: &str) -> (String, i16, Option<String>, Option<String>) {
    let s = s.trim();
    if s.starts_with('<') && s.ends_with('>') {
        return (
            s[1..s.len() - 1].to_owned(),
            dictionary::KIND_IRI,
            None,
            None,
        );
    }
    if let Some(rest) = s.strip_prefix("_:") {
        return (rest.to_owned(), dictionary::KIND_BLANK, None, None);
    }
    if s.starts_with('"') {
        // Find closing quote (handling \" escapes)
        let bytes = s.as_bytes();
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                break;
            } else {
                i += 1;
            }
        }
        let raw_value = &s[1..i];
        let rest = &s[i + 1..];
        // Unescape basic sequences
        let value = raw_value
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t");
        if rest.starts_with("^^<") && rest.ends_with('>') {
            let dt = rest[3..rest.len() - 1].to_owned();
            return (value, dictionary::KIND_TYPED_LITERAL, Some(dt), None);
        }
        if let Some(lang_part) = rest.strip_prefix('@') {
            return (
                value,
                dictionary::KIND_LANG_LITERAL,
                None,
                Some(lang_part.to_owned()),
            );
        }
        return (value, dictionary::KIND_LITERAL, None, None);
    }
    // Fall back: treat as a bare IRI string (v0.1.0 backward-compat)
    (s.to_owned(), dictionary::KIND_IRI, None, None)
}

/// Encode an RDF term string (N-Triples format) to a dictionary id.
pub fn encode_rdf_term(s: &str) -> i64 {
    let s = s.trim();
    // v0.48.0: Handle RDF-star quoted triple syntax `<< s p o >>`.
    if s.starts_with("<<") && s.ends_with(">>") {
        let inner = s[2..s.len() - 2].trim();
        let tokens = tokenize_rdf_terms(inner);
        if tokens.len() >= 3 {
            let s_id = encode_rdf_term(&tokens[0]);
            let p_id = encode_rdf_term(&tokens[1]);
            // Object may span multiple tokens (e.g. typed literal with spaces)
            let o_str = if tokens.len() == 3 {
                tokens[2].clone()
            } else {
                tokens[2..].join(" ")
            };
            let o_id = encode_rdf_term(&o_str);
            return dictionary::encode_quoted_triple(s_id, p_id, o_id);
        }
    }
    let (value, kind, datatype, lang) = parse_rdf_term(s);
    match kind {
        k if k == dictionary::KIND_TYPED_LITERAL => {
            dictionary::encode_typed_literal(&value, datatype.as_deref().unwrap_or(""))
        }
        k if k == dictionary::KIND_LANG_LITERAL => {
            dictionary::encode_lang_literal(&value, lang.as_deref().unwrap_or(""))
        }
        _ => dictionary::encode(&value, kind),
    }
}

/// Tokenize a space-separated sequence of N-Triples terms, respecting IRIs,
/// quoted literals and nested `<< >>` quoted triples.
pub(crate) fn tokenize_rdf_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;
    let mut quoted_depth: usize = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_iri => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    quoted_depth += 1;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && quoted_depth > 0 => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    quoted_depth -= 1;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            ' ' | '\t' | '\n' if !in_literal && !in_iri && quoted_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current.trim().to_owned());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_owned());
    }
    tokens
}
