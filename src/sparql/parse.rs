//! SPARQL query parsing and algebra complexity helpers.
//!
//! Provides query-complexity enforcement (algebra depth + triple-pattern count limits)
//! and pre-processing of ARQ aggregate extensions before spargebra parsing.

// ─── Algebra complexity helpers ───────────────────────────────────────────────

/// Count the algebra tree depth of a SPARQL `GraphPattern`.
fn algebra_depth(pattern: &spargebra::algebra::GraphPattern) -> u32 {
    use spargebra::algebra::GraphPattern as GP;
    match pattern {
        GP::Bgp { .. } | GP::Values { .. } => 1,
        GP::Join { left, right }
        | GP::LeftJoin { left, right, .. }
        | GP::Union { left, right }
        | GP::Minus { left, right } => 1 + algebra_depth(left).max(algebra_depth(right)),
        GP::Filter { inner, .. }
        | GP::Graph { inner, .. }
        | GP::Extend { inner, .. }
        | GP::Distinct { inner }
        | GP::Reduced { inner }
        | GP::Project { inner, .. }
        | GP::Slice { inner, .. }
        | GP::OrderBy { inner, .. }
        | GP::Group { inner, .. }
        | GP::Service { inner, .. } => 1 + algebra_depth(inner),
        _ => 1,
    }
}

/// Count the total number of triple patterns in a SPARQL `GraphPattern`.
fn count_triple_patterns(pattern: &spargebra::algebra::GraphPattern) -> u32 {
    use spargebra::algebra::GraphPattern as GP;
    match pattern {
        GP::Bgp { patterns } => patterns.len() as u32,
        GP::Values { .. } => 0,
        GP::Join { left, right }
        | GP::LeftJoin { left, right, .. }
        | GP::Union { left, right }
        | GP::Minus { left, right } => count_triple_patterns(left) + count_triple_patterns(right),
        GP::Filter { inner, .. }
        | GP::Graph { inner, .. }
        | GP::Extend { inner, .. }
        | GP::Distinct { inner }
        | GP::Reduced { inner }
        | GP::Project { inner, .. }
        | GP::Slice { inner, .. }
        | GP::OrderBy { inner, .. }
        | GP::Group { inner, .. }
        | GP::Service { inner, .. } => count_triple_patterns(inner),
        _ => 0,
    }
}

/// Check that a query's algebra tree depth and triple-pattern count are within
/// the configured limits.  Raises `PT440` if either limit is exceeded.
///
/// Called before any SQL translation to provide early, cheap DoS protection.
pub(crate) fn check_query_complexity(pattern: &spargebra::algebra::GraphPattern) {
    let max_depth = crate::SPARQL_MAX_ALGEBRA_DEPTH.get();
    if max_depth > 0 {
        let depth = algebra_depth(pattern);
        if depth > max_depth as u32 {
            pgrx::error!(
                "PT440: SPARQL algebra tree depth {} exceeds sparql_max_algebra_depth limit of {}; \
                 simplify the query or raise pg_ripple.sparql_max_algebra_depth",
                depth,
                max_depth
            );
        }
    }

    let max_patterns = crate::SPARQL_MAX_TRIPLE_PATTERNS.get();
    if max_patterns > 0 {
        let count = count_triple_patterns(pattern);
        if count > max_patterns as u32 {
            pgrx::error!(
                "PT440: SPARQL query contains {} triple patterns, exceeding \
                 sparql_max_triple_patterns limit of {}; simplify the query or raise \
                 pg_ripple.sparql_max_triple_patterns",
                count,
                max_patterns
            );
        }
    }
}

// ─── ARQ aggregate preprocessing ─────────────────────────────────────────────

/// Rewrite ARQ extension aggregate keywords to IRI form that spargebra can parse.
///
/// Jena ARQ supports `MEDIAN(?v)` and `MODE(?v)` as aggregate extensions.
/// spargebra 0.4 doesn't recognise these keywords, but DOES accept custom
/// aggregates written as `<IRI>(?v)`.  This function rewrites:
///
/// - `median(` → `<urn:arq:median>(`
/// - `mode(` → `<urn:arq:mode>(`
///
/// at word boundaries (not inside identifiers or prefixed names).
/// The rewrite is idempotent: already-rewritten queries are returned unchanged.
pub(crate) fn preprocess_arq_aggregates(src: &str) -> String {
    let lc = src.to_ascii_lowercase();
    if !lc.contains("median") && !lc.contains("mode") {
        return src.to_owned();
    }
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n + 64);
    let mut i = 0;
    while i < n {
        // Word boundary: previous char must not be an identifier char,
        // and must not be ?, $, or : (which precede variable/prefix names).
        let at_boundary = i == 0 || {
            let pb = bytes[i - 1];
            !pb.is_ascii_alphanumeric() && pb != b'_' && pb != b'?' && pb != b'$' && pb != b':'
        };
        if at_boundary {
            if let Some(j) = try_arq_agg_keyword(bytes, i, b"median", 6) {
                out.push_str("<urn:arq:median>");
                i = j; // j points to '('
                continue;
            }
            if let Some(j) = try_arq_agg_keyword(bytes, i, b"mode", 4) {
                out.push_str("<urn:arq:mode>");
                i = j;
                continue;
            }
        }
        // Advance by full UTF-8 codepoint to avoid splitting multibyte sequences.
        let char_len = utf8_char_len(bytes[i]);
        out.push_str(&src[i..i + char_len]);
        i += char_len;
    }
    out
}

/// Returns the index of `(` if `bytes[pos..]` starts with `kw` (case-insensitive),
/// followed by optional whitespace and `(`.  Also verifies word-boundary end
/// (char after keyword is not an identifier char).
fn try_arq_agg_keyword(bytes: &[u8], pos: usize, kw: &[u8], klen: usize) -> Option<usize> {
    if pos + klen > bytes.len() {
        return None;
    }
    if !bytes[pos..pos + klen].eq_ignore_ascii_case(kw) {
        return None;
    }
    // Word boundary end
    if pos + klen < bytes.len() && {
        let b = bytes[pos + klen];
        b.is_ascii_alphanumeric() || b == b'_'
    } {
        return None;
    }
    // Skip optional whitespace
    let mut j = pos + klen;
    while j < bytes.len()
        && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
    {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'(' {
        Some(j)
    } else {
        None
    }
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

// ─── PGSS-NORM-01: SPARQL query normalisation ─────────────────────────────────

/// Normalise a SPARQL query text for use as a `pg_stat_statements` query key.
///
/// Replaces:
/// - String literals (`"…"` and `'…'`) with `$S`
/// - IRI literals (`<…>`) with `$I`
/// - Numeric literals (integer and float) with `$N`
///
/// The result groups structurally equivalent queries so that `pg_stat_statements`
/// (and the plan cache) treat them as the same query class. (v0.82.0 PGSS-NORM-01)
pub(crate) fn normalise_sparql_for_pgss(query: &str) -> String {
    let bytes = query.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        match bytes[i] {
            // Double-quoted string literal
            b'"' => {
                i += 1;
                // Check for triple-quoted
                if i + 1 < len && bytes[i] == b'"' && bytes[i + 1] == b'"' {
                    i += 2;
                    // Scan for closing """
                    while i + 2 < len
                        && !(bytes[i] == b'"' && bytes[i + 1] == b'"' && bytes[i + 2] == b'"')
                    {
                        i += 1;
                    }
                    i += 3;
                } else {
                    while i < len && bytes[i] != b'"' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    i += 1; // closing "
                }
                out.push_str("$S");
            }
            // Single-quoted string literal
            b'\'' => {
                i += 1;
                if i + 1 < len && bytes[i] == b'\'' && bytes[i + 1] == b'\'' {
                    i += 2;
                    while i + 2 < len
                        && !(bytes[i] == b'\'' && bytes[i + 1] == b'\'' && bytes[i + 2] == b'\'')
                    {
                        i += 1;
                    }
                    i += 3;
                } else {
                    while i < len && bytes[i] != b'\'' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    i += 1;
                }
                out.push_str("$S");
            }
            // IRI literal <…>
            b'<' => {
                // Peek: if the next char is a space or = it is a comparison operator
                let next = if i + 1 < len { bytes[i + 1] } else { 0 };
                if next == b' ' || next == b'=' || next == b'\t' || next == b'\n' {
                    out.push('<');
                    i += 1;
                } else {
                    // Skip until closing >
                    i += 1;
                    while i < len && bytes[i] != b'>' {
                        i += 1;
                    }
                    i += 1; // skip >
                    out.push_str("$I");
                }
            }
            // Hash comment: skip to end of line
            b'#' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            // Numeric literal (standalone digit, possibly preceded by space)
            b if b.is_ascii_digit() => {
                // Check word boundary: previous non-space char should not be alnum/?/:
                let prev_is_id = out.ends_with(|c: char| {
                    c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '?'
                });
                if prev_is_id {
                    let char_len = utf8_char_len(bytes[i]);
                    out.push_str(&query[i..i + char_len]);
                    i += char_len;
                } else {
                    // Consume full number including decimal / exponent
                    while i < len
                        && (bytes[i].is_ascii_digit()
                            || bytes[i] == b'.'
                            || bytes[i] == b'e'
                            || bytes[i] == b'E'
                            || ((bytes[i] == b'+' || bytes[i] == b'-')
                                && i > 0
                                && (bytes[i - 1] == b'e' || bytes[i - 1] == b'E')))
                    {
                        i += 1;
                    }
                    out.push_str("$N");
                }
            }
            // Everything else: copy verbatim
            _ => {
                let char_len = utf8_char_len(bytes[i]);
                out.push_str(&query[i..i + char_len]);
                i += char_len;
            }
        }
    }
    out
}

#[cfg(test)]
mod normalise_tests {
    use super::normalise_sparql_for_pgss;

    #[test]
    fn test_normalise_literals() {
        let q = r#"SELECT ?s WHERE { ?s <http://example.org/name> "Alice" . ?s <http://schema.org/age> 42 }"#;
        let n = normalise_sparql_for_pgss(q);
        assert!(!n.contains("Alice"), "string literal should be replaced");
        assert!(!n.contains("42"), "numeric literal should be replaced");
        assert!(!n.contains("http://"), "IRI should be replaced");
        assert!(n.contains("$S"), "should have $S placeholder");
        assert!(n.contains("$I"), "should have $I placeholder");
        assert!(n.contains("$N"), "should have $N placeholder");
    }
}
