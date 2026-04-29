#![no_main]
// FUZZ-02 (v0.72.0): Fuzz target for SPARQL Update grammar.
//
// This target exercises `spargebra::SparqlParser::parse_update` with arbitrary
// byte inputs to find panics or incorrect error handling in the parser.
// The seed corpus in fuzz/corpus/sparql_update/ covers all SPARQL Update
// operation types: INSERT DATA, DELETE DATA, DELETE/INSERT WHERE, COPY, MOVE,
// ADD, CLEAR, DROP, LOAD, CREATE.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Parsing must never panic; errors are expected for invalid inputs.
        let _ = spargebra::SparqlParser::new().parse_update(s);
    }
});
