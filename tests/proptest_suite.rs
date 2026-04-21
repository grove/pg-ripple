//! Property-based test suite for pg_ripple (v0.46.0).
//!
//! Three property suites run 10,000 cases each:
//!
//! 1. **SPARQL algebra round-trip** — encoding the same query twice yields
//!    byte-identical SQL; whitespace variants produce equivalent SQL.
//! 2. **Dictionary encode/decode** — XXH3-128 is stable and collision-free
//!    for 10,000 random distinct terms.
//! 3. **JSON-LD framing round-trip** — framed output contains expected entities;
//!    non-matching frames produce empty graphs.
//!
//! No database connection is required — all tests run in pure Rust.
//!
//! # Running
//!
//! ```sh
//! cargo test --test proptest_suite
//!
//! # Increase case count for deeper coverage:
//! PROPTEST_CASES=50000 cargo test --test proptest_suite
//! ```

#[path = "proptest/sqlgen_bridge.rs"]
mod sqlgen_bridge;

#[path = "proptest/dictionary.rs"]
mod dictionary;
#[path = "proptest/jsonld_framing.rs"]
mod jsonld_framing;
#[path = "proptest/sparql_roundtrip.rs"]
mod sparql_roundtrip;
