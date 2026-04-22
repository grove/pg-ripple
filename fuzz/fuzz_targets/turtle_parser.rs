//! cargo-fuzz target for the Turtle / N-Triples parser (v0.47.0).
//!
//! Feeds arbitrary byte sequences through `rio_turtle::TurtleParser` and
//! `rio_turtle::NTriplesParser`.  Asserts: no panic.  Invalid syntax must
//! produce a parse error, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run turtle_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::parser::TriplesParser;
use rio_turtle::{NTriplesParser, TurtleError, TurtleParser};

fuzz_target!(|data: &[u8]| {
    // Fuzz Turtle parser — exhaust all triples (ignore all errors).
    let mut turtle = TurtleParser::new(data, None);
    let _ = turtle.parse_all(&mut |_| -> Result<(), TurtleError> { Ok(()) });

    // Fuzz N-Triples parser — exhaust all triples (ignore all errors).
    let mut ntriples = NTriplesParser::new(data);
    let _ = ntriples.parse_all(&mut |_| -> Result<(), TurtleError> { Ok(()) });
});
