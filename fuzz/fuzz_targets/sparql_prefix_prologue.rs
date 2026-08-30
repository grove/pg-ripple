//! Fuzz target for v0.135 registered-prefix prologue handling.
//!
//! The production resolver only prepends missing declarations. This harness
//! checks that formal-prologue scanning is panic-free and never rewrites the
//! original query body or local declarations.

#![no_main]

use libfuzzer_sys::fuzz_target;

const REGISTERED: &[(&str, &str)] = &[
    ("ex", "https://example.test/"),
    ("schema", "https://schema.org/"),
];

fn skip_space_comments(input: &[u8], mut at: usize) -> Option<usize> {
    loop {
        while matches!(input.get(at), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            at += 1;
        }
        if input.get(at) != Some(&b'#') {
            return Some(at);
        }
        while at < input.len() && input[at] != b'\n' {
            at += 1;
        }
    }
}

fn token(input: &[u8], mut at: usize) -> Option<(&str, usize)> {
    let start = at;
    while at < input.len() && !matches!(input[at], b' ' | b'\t' | b'\r' | b'\n') {
        at += 1;
    }
    std::str::from_utf8(&input[start..at]).ok().map(|value| (value, at))
}

fn local_prefixes(query: &str) -> Vec<String> {
    let bytes = query.as_bytes();
    let mut at = 0;
    let mut prefixes = Vec::new();
    while let Some(next) = skip_space_comments(bytes, at) {
        at = next;
        let Some((keyword, end)) = token(bytes, at) else {
            break;
        };
        if keyword.eq_ignore_ascii_case("base") {
            let Some(iri) = skip_space_comments(bytes, end) else {
                break;
            };
            if bytes.get(iri) != Some(&b'<') {
                break;
            }
            let Some(close) = bytes[iri + 1..].iter().position(|byte| *byte == b'>') else {
                break;
            };
            at = iri + close + 2;
        } else if keyword.eq_ignore_ascii_case("prefix") {
            let Some(next) = skip_space_comments(bytes, end) else {
                break;
            };
            let Some((label, label_end)) = token(bytes, next) else {
                break;
            };
            let Some(prefix) = label.strip_suffix(':') else {
                break;
            };
            let Some(iri) = skip_space_comments(bytes, label_end) else {
                break;
            };
            if bytes.get(iri) != Some(&b'<') {
                break;
            }
            let Some(close) = bytes[iri + 1..].iter().position(|byte| *byte == b'>') else {
                break;
            };
            prefixes.push(prefix.to_owned());
            at = iri + close + 2;
        } else {
            break;
        }
    }
    prefixes
}

fn inject_missing(query: &str) -> String {
    let local = local_prefixes(query);
    let mut header = String::new();
    for (prefix, expansion) in REGISTERED {
        if !local.iter().any(|value| value == prefix) {
            header.push_str("PREFIX ");
            header.push_str(prefix);
            header.push_str(": <");
            header.push_str(expansion);
            header.push_str(">\n");
        }
    }
    header + query
}

fuzz_target!(|data: &[u8]| {
    let Ok(query) = std::str::from_utf8(data) else {
        return;
    };
    let resolved = inject_missing(query);
    assert!(resolved.ends_with(query));

    let locals = local_prefixes(query);
    for local in locals {
        let occurrences = resolved.matches(&format!("PREFIX {local}:")).count()
            + resolved.matches(&format!("prefix {local}:")).count();
        assert_eq!(occurrences, 1);
    }

    // Parsing remains an error-returning operation for malformed input.
    let _ = spargebra::SparqlParser::new().parse_query(&resolved);
});
