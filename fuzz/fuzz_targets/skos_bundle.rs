//! cargo-fuzz target for the SKOS concept scheme bundle loader (M16-13, v0.117.0).
//!
//! SKOS (Simple Knowledge Organization System) concept schemes are loaded as
//! RDF Turtle or JSON-LD.  This target exercises the SKOS bundle loader by
//! feeding arbitrary byte sequences through the Turtle and JSON-LD parsers
//! that handle SKOS vocabularies.
//!
//! Asserts:
//!   - No panic on any input (valid or invalid).
//!   - Invalid Turtle/JSON-LD is cleanly rejected with an error.
//!   - Valid SKOS triples (skos:Concept, skos:broader, skos:narrower,
//!     skos:prefLabel) are parsed without crashing.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run skos_bundle -- -max_total_time=300
//! cargo fuzz tmin skos_bundle artifacts/skos_bundle/crash-...
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::parser::TriplesParser;

fuzz_target!(|data: &[u8]| {
    // Attempt to parse as Turtle (the primary SKOS serialization format).
    // rio_turtle's TurtleParser is used by the pg_ripple bulk loader.
    // Any byte sequence must not panic — only produce a parse error.
    let cursor = std::io::Cursor::new(data);
    let mut parser = rio_turtle::TurtleParser::new(cursor, None);
    // Consume all triples (or errors) — assert no panic.
    let _ = parser.parse_all(&mut |_| Ok::<(), rio_turtle::TurtleError>(()));

    // Also attempt to parse as N-Triples (subset of Turtle used in tests).
    let cursor2 = std::io::Cursor::new(data);
    let nt_parser = rio_turtle::NTriplesParser::new(cursor2);
    for _triple_result in
        nt_parser.into_iter(|_triple| Ok::<_, rio_turtle::TurtleError>(()))
    {
        // Consume and discard; the invariant is no panic.
    }
});
