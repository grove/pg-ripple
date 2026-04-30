/// Property-based test suite for pg_ripple (v0.46.0 + v0.78.0).
///
/// Suites:
///
/// 1. **SPARQL algebra round-trip** — encoding the same query twice yields
///    byte-identical SQL; whitespace variants produce equivalent SQL.
/// 2. **Dictionary encode/decode** — XXH3-128 is stable and collision-free
///    for 10,000 random distinct terms.
/// 3. **JSON-LD framing round-trip** — framed output contains expected entities;
///    non-matching frames produce empty graphs.
/// 4. **Bidi convergence** (v0.78.0 BIDIOPS-PROPTEST-01) — random insert/update/delete
///    sequences from N sources satisfy determinism, order-independence, no-loss,
///    source-priority, linkback round-trip, and retry-convergence properties.
///
/// No database connection is required — all tests run in pure Rust.
///
/// # Running
///
/// ```sh
/// cargo test --test proptest_suite
///
/// # Increase case count for deeper coverage:
/// PROPTEST_CASES=50000 cargo test --test proptest_suite
/// ```

#[path = "proptest/sqlgen_bridge.rs"]
mod sqlgen_bridge;

#[path = "proptest/bidi_convergence.rs"]
mod bidi_convergence;
#[path = "proptest/construct_template.rs"]
mod construct_template;
#[path = "proptest/dictionary.rs"]
mod dictionary;
#[path = "proptest/jsonld_framing.rs"]
mod jsonld_framing;
#[path = "proptest/sparql_roundtrip.rs"]
mod sparql_roundtrip;
