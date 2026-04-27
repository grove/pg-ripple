//! cargo-fuzz target for the R2RML mapping document parser (v0.60.0 A7-1).
//!
//! R2RML mapping documents are expressed as RDF/Turtle graphs.  This harness
//! feeds arbitrary bytes through the same Turtle parser pipeline used by
//! `src/r2rml.rs` when loading mapping documents.  Asserts: no panic.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run r2rml_mapping -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::parser::TriplesParser;
use rio_turtle::{TurtleError, TurtleParser};

fuzz_target!(|data: &[u8]| {
    // R2RML documents are Turtle/RDF — parse them through the same Turtle
    // parser that `src/r2rml.rs` uses.  Any parse error is acceptable; a
    // panic is not.  Use None for base IRI (mirrors the no-base usage in
    // r2rml.rs when the document does not declare a base).
    let mut parser = TurtleParser::new(data, None);
    let _ = parser.parse_all(&mut |_| -> Result<(), TurtleError> { Ok(()) });
});
