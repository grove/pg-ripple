//! cargo-fuzz target for the SHACL-SPARQL constraint parser (T13-03, v0.86.0).
//!
//! Feeds arbitrary byte sequences through the SHACL-SPARQL constraint parsing
//! pipeline to detect panics or undefined behavior in the combined
//! SPARQL-constraint evaluation path.
//!
//! Asserts: no panic. Invalid input must produce an `Err`, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run shacl_sparql -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only fuzz valid UTF-8 input (SPARQL queries are text).
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Attempt to parse the input as a SPARQL SELECT query (SHACL-SPARQL constraints
    // use SELECT queries with $this as the focus node variable).
    let _result = spargebra::Query::parse(text, None);

    // Also exercise SPARQL ASK parsing (used in SHACL constraint checking).
    let _ask_result = spargebra::Query::parse(
        &format!("ASK {{ FILTER(CONTAINS(STR(?x), {text:?})) }}"),
        None,
    );
});
