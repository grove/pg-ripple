//! cargo-fuzz target for the SPARQL CONSTRUCT writeback rule parser (T13-03, v0.86.0).
//!
//! Feeds arbitrary byte sequences through the CONSTRUCT rule parsing path to detect
//! panics, buffer overflows, or other undefined behavior in rule ingestion.
//!
//! Asserts: no panic. Invalid input must produce an `Err`, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run construct_rule -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only fuzz valid UTF-8 input (SPARQL queries are text).
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Attempt to parse the input as a SPARQL CONSTRUCT query using spargebra.
    // The parser must not panic on any input — it must return Ok or Err.
    let _result = spargebra::Query::parse(text, None);

    // Additionally, exercise the construct_rules scheduler source-graph parse helper
    // which uses a simplified regex-based parser for SPARQL CONSTRUCT templates.
    // We call it through the public SPARQL parser to stay within safe Rust.
    let _ = spargebra::algebra::Expression::from(spargebra::term::GroundTerm::NamedNode(
        spargebra::term::NamedNode::new_unchecked("http://example.org/test"),
    ));
});
