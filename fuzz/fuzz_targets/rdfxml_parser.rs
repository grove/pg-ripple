//! cargo-fuzz target for the RDF/XML parser (v0.53.0).
//!
//! Feeds arbitrary byte sequences through `rio_xml::RdfXmlParser`.
//! Asserts: no panic.  Invalid XML or RDF syntax must produce a parse error,
//! never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run rdfxml_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::parser::TriplesParser;
use rio_xml::{RdfXmlError, RdfXmlParser};

fuzz_target!(|data: &[u8]| {
    // Feed arbitrary bytes to the RDF/XML parser; exhaust output (ignore errors).
    let mut parser = RdfXmlParser::new(data, None);
    let _ = parser.parse_all(&mut |_| -> Result<(), RdfXmlError> { Ok(()) });
});
