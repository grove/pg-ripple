//! Property-based tests comparing pg_ripple's N-Triples parsing against
//! oxigraph as a reference implementation (PROPTEST-02, v0.83.0).
//!
//! Strategy: generate random well-formed N-Triples documents, parse them
//! with both rio_turtle (pg_ripple's parser) and oxigraph's N-Triples parser,
//! and assert that both produce identical triple counts.
//!
//! No database connection is required — all tests run in pure Rust.

use oxigraph::io::{GraphFormat, GraphParser};
use proptest::prelude::*;
use rio_api::parser::TriplesParser;
use rio_turtle::{NTriplesParser, TurtleError};

/// Generate a valid absolute IRI for use in N-Triples.
fn arb_iri() -> impl Strategy<Value = String> {
    "[a-z]{3,6}"
        .prop_map(|s| format!("http://example.org/{s}"))
}

/// Generate a plain string literal.
fn arb_literal() -> impl Strategy<Value = String> {
    // Restrict to printable ASCII to avoid encoding edge cases.
    "[A-Za-z0-9 ]{1,20}".prop_map(|s| s)
}

/// Generate one N-Triples line: <s> <p> <o> .
fn arb_triple_line() -> impl Strategy<Value = String> {
    (arb_iri(), arb_iri(), arb_iri()).prop_map(|(s, p, o)| {
        format!("<{s}> <{p}> <{o}> .\n")
    })
}

/// Generate one N-Triples line: <s> <p> "literal" .
fn arb_triple_literal_line() -> impl Strategy<Value = String> {
    (arb_iri(), arb_iri(), arb_literal()).prop_map(|(s, p, o)| {
        format!("<{s}> <{p}> \"{o}\" .\n")
    })
}

/// Generate a small N-Triples document (1–8 triples).
fn arb_ntriples_doc() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![arb_triple_line(), arb_triple_literal_line()],
        1..=8,
    )
    .prop_map(|lines| lines.concat())
}

/// Count triples parsed by rio_turtle.
fn count_rio(data: &str) -> usize {
    let mut count = 0usize;
    let mut parser = NTriplesParser::new(data.as_bytes());
    let _ = parser.parse_all(&mut |_| -> Result<(), TurtleError> {
        count += 1;
        Ok(())
    });
    count
}

/// Count triples parsed by oxigraph (GraphParser with NTriples format).
fn count_oxigraph(data: &str) -> usize {
    let mut count = 0usize;
    for result in GraphParser::from_format(GraphFormat::NTriples).read_triples(data.as_bytes()) {
        if result.is_ok() {
            count += 1;
        }
    }
    count
}

proptest! {
    /// rio_turtle and oxigraph must parse the same number of triples from any
    /// well-formed N-Triples document.
    #[test]
    fn ntriples_count_matches_oxigraph(doc in arb_ntriples_doc()) {
        let rio_count = count_rio(&doc);
        let ox_count  = count_oxigraph(&doc);
        prop_assert_eq!(
            rio_count, ox_count,
            "rio_turtle and oxigraph parsed different triple counts for:\n{}",
            doc
        );
        // Both parsers must find at least 1 triple (our generator always
        // produces valid N-Triples).
        prop_assert!(rio_count >= 1, "at least one triple expected");
    }

    /// An empty N-Triples document yields 0 triples in both parsers.
    #[test]
    fn empty_document_yields_zero(_ignored in Just(())) {
        prop_assert_eq!(count_rio(""), 0);
        prop_assert_eq!(count_oxigraph(""), 0);
    }

    /// N-Triples with comment lines only yields 0 triples.
    #[test]
    fn comment_only_yields_zero(suffix in "[a-z]{3,8}") {
        let doc = format!("# comment: {suffix}\n");
        prop_assert_eq!(count_rio(&doc), 0);
        prop_assert_eq!(count_oxigraph(&doc), 0);
    }
}
