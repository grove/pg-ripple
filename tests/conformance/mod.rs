//! Unified conformance test infrastructure for pg_ripple.
//!
//! This module provides the shared runner, result types, and reporting
//! infrastructure used by all conformance suites (W3C, Jena, WatDiv).
//!
//! # Suite prefix convention
//!
//! All known-failures entries use a `suite:` prefix to disambiguate:
//! - `w3c:<test-IRI>` — W3C SPARQL 1.1 test suite
//! - `jena:<test-IRI>` — Apache Jena test suite
//! - `watdiv:<template-id>` — WatDiv benchmark template

pub mod report;
pub mod runner;

pub use runner::{RunConfig, RunReport, TestOutcome, TestResult};

/// Unified known-failures format.
///
/// Each non-comment, non-blank line must start with a `suite:` prefix,
/// e.g. `w3c:http://...`, `jena:http://...`, or `watdiv:S1`.
///
/// The `suite` argument is the prefix to strip when looking up entries
/// (e.g. `"w3c"` strips `w3c:` before comparing to the test IRI).
pub fn load_known_failures(
    path: &std::path::Path,
    suite: &str,
) -> std::collections::HashSet<String> {
    let prefix = format!("{suite}:");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .filter_map(|l| {
            let token = l.split_whitespace().next().unwrap_or("");
            // Support both new `suite:key` format and legacy bare keys.
            if let Some(stripped) = token.strip_prefix(&prefix) {
                Some(stripped.to_string())
            } else if !token.contains(':') || token.starts_with("http") {
                // Legacy bare IRI (no suite prefix) — include unconditionally.
                Some(token.to_string())
            } else {
                // Different suite prefix — skip.
                None
            }
        })
        .collect()
}
