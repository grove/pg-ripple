//! cargo-fuzz target for the SPARQL 1.1 query parser (v0.47.0).
//!
//! Feeds arbitrary byte sequences through `spargebra::Query::parse` and
//! `spargebra::Update::parse`.  Asserts: no panic.  Invalid SPARQL must
//! produce a parse error, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run sparql_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    // Parse as SELECT/CONSTRUCT/ASK/DESCRIBE — ignore all errors.
    let _ = spargebra::Query::parse(s, None);
    // Also try as an Update query.
    let _ = spargebra::Update::parse(s, None);
});
