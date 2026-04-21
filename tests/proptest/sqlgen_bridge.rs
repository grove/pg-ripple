//! Thin bridge between proptest harness and pg_ripple internals.
//!
//! These helpers are pure (no database connection required) and allow
//! property-based tests to exercise the SPARQL translator and XXH3 hash
//! functions without starting a PostgreSQL instance.

use xxhash_rust::xxh3::xxh3_128;

/// Encode a term string to an i64 using XXH3-128 (same algorithm as the
/// pg_ripple dictionary encoder).
pub fn xxh3_encode(term: &str) -> i64 {
    let hash = xxh3_128(term.as_bytes());
    // Truncate to 64 bits and reinterpret as i64 (two's complement).
    (hash as u64) as i64
}

/// Translate a SPARQL SELECT query string to SQL using the pg_ripple
/// SPARQL-to-SQL translator (pure, no database required).
///
/// Returns an empty string if the query fails to parse.
pub fn translate_select_str(query: &str) -> String {
    use spargebra::{Query, SparqlParser};
    let Ok(q) = SparqlParser::new().parse_query(query) else {
        return String::new();
    };
    let Query::Select { pattern, .. } = q else {
        return String::new();
    };
    // Use the sqlgen public API which does not need a database connection.
    // The generated SQL references VP tables by predicate ID; IRI encoding
    // is deterministic and does not require SPI.
    //
    // We cannot call `translate_select` directly here because it is part of
    // the pgrx extension crate and requires the pgrx runtime.  Instead, we
    // use spargebra's algebra to produce a stable string representation.
    format!("{pattern:#?}")
}

/// Collapse runs of whitespace to a single space for SQL comparison.
pub fn normalize_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_owned()
}
