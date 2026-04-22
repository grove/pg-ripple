//! cargo-fuzz target for the SHACL shapes-graph parser (v0.47.0).
//!
//! Feeds arbitrary byte sequences through the Turtle parser and then
//! through a minimal shape-statement recogniser that mirrors the key
//! `sh:` predicate dispatch in `src/shacl/mod.rs`.
//! Asserts: no panic.  Invalid input must produce an error, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run shacl_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::model::Triple;
use rio_api::parser::TriplesParser;
use rio_turtle::{TurtleError, TurtleParser};

// Well-known SHACL predicates we recognise.
const SHACL_PREDICATES: &[&str] = &[
    "http://www.w3.org/ns/shacl#targetClass",
    "http://www.w3.org/ns/shacl#targetNode",
    "http://www.w3.org/ns/shacl#targetSubjectsOf",
    "http://www.w3.org/ns/shacl#targetObjectsOf",
    "http://www.w3.org/ns/shacl#property",
    "http://www.w3.org/ns/shacl#path",
    "http://www.w3.org/ns/shacl#minCount",
    "http://www.w3.org/ns/shacl#maxCount",
    "http://www.w3.org/ns/shacl#datatype",
    "http://www.w3.org/ns/shacl#nodeKind",
    "http://www.w3.org/ns/shacl#minInclusive",
    "http://www.w3.org/ns/shacl#maxInclusive",
    "http://www.w3.org/ns/shacl#minExclusive",
    "http://www.w3.org/ns/shacl#maxExclusive",
    "http://www.w3.org/ns/shacl#minLength",
    "http://www.w3.org/ns/shacl#maxLength",
    "http://www.w3.org/ns/shacl#pattern",
    "http://www.w3.org/ns/shacl#flags",
    "http://www.w3.org/ns/shacl#in",
    "http://www.w3.org/ns/shacl#hasValue",
    "http://www.w3.org/ns/shacl#equals",
    "http://www.w3.org/ns/shacl#disjoint",
    "http://www.w3.org/ns/shacl#lessThan",
    "http://www.w3.org/ns/shacl#lessThanOrEquals",
    "http://www.w3.org/ns/shacl#uniqueLang",
    "http://www.w3.org/ns/shacl#closed",
    "http://www.w3.org/ns/shacl#not",
    "http://www.w3.org/ns/shacl#and",
    "http://www.w3.org/ns/shacl#or",
    "http://www.w3.org/ns/shacl#xone",
    "http://www.w3.org/ns/shacl#node",
    "http://www.w3.org/ns/shacl#qualifiedValueShape",
    "http://www.w3.org/ns/shacl#qualifiedMinCount",
    "http://www.w3.org/ns/shacl#qualifiedMaxCount",
];

fuzz_target!(|data: &[u8]| {
    // Parse the input as Turtle — ignore all parse errors.
    let mut triples_seen = 0usize;
    let mut shacl_triples = 0usize;

    let mut parser = TurtleParser::new(data, None);
    let _ = parser.parse_all(&mut |t: Triple<'_>| -> Result<(), TurtleError> {
        triples_seen += 1;
        let pred_iri = match t.predicate {
            rio_api::model::NamedNode { iri } => iri,
        };
        if SHACL_PREDICATES.contains(&pred_iri) {
            shacl_triples += 1;
        }
        Ok(())
    });
    // Ensure the counts are used so the optimizer doesn't eliminate the loop.
    let _ = (triples_seen, shacl_triples);
});
