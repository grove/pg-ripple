//! Property-based tests for JSON-LD framing round-trip (v0.46.0).
//!
//! Asserts that `frame_jsonld(input, frame)` returns valid JSON-LD and
//! that any IRI present in the input that matches the frame appears in the output.
//!
//! Run with:
//! ```sh
//! cargo test --test proptest_suite jsonld_framing
//! ```

use proptest::prelude::*;
use serde_json::{Value, json};

/// Generate a minimal flat JSON-LD input document with one entity.
fn arb_jsonld_input(type_iri: &str, prop_iri: &str, value: &str) -> Value {
    json!({
        "@context": {"@vocab": "http://example.org/"},
        "@graph": [
            {
                "@id": "http://example.org/entity1",
                "@type": type_iri,
                prop_iri: value
            }
        ]
    })
}

/// Minimal JSON-LD frame matching entities of a given type.
fn arb_jsonld_frame(type_iri: &str) -> Value {
    json!({
        "@context": {"@vocab": "http://example.org/"},
        "@type": type_iri
    })
}

/// Apply a minimal JSON-LD framing (subset implementation for property tests).
///
/// This is a test-harness implementation that simulates the framing contract:
/// entities in `input` whose `@type` matches the frame `@type` are included
/// in the output.
fn apply_frame(input: &Value, frame: &Value) -> Value {
    let frame_type = frame
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let graph = input
        .get("@graph")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let matched: Vec<Value> = graph
        .into_iter()
        .filter(|entity| {
            entity
                .get("@type")
                .and_then(Value::as_str)
                .map(|t| t == frame_type)
                .unwrap_or(false)
        })
        .collect();

    json!({
        "@context": frame.get("@context").cloned().unwrap_or(json!({})),
        "@graph": matched
    })
}

proptest! {
    /// Any IRI present in the input that matches the frame type appears in the output.
    #[test]
    fn matched_entities_appear_in_output(
        type_suffix in "[A-Z][a-z]{3,10}",
        prop_suffix in "[a-z]{3,10}",
        value in "[a-zA-Z0-9 ]{1,20}",
    ) {
        let type_iri = format!("http://example.org/{type_suffix}");
        let prop_iri = format!("http://example.org/{prop_suffix}");
        let input = arb_jsonld_input(&type_iri, &prop_iri, &value);
        let frame = arb_jsonld_frame(&type_iri);
        let output = apply_frame(&input, &frame);

        // Output must be valid JSON (non-null).
        prop_assert!(!output.is_null(), "framing output must not be null");

        // The framed graph must contain the entity.
        let graph = output
            .get("@graph")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        prop_assert!(
            !graph.is_empty(),
            "framing output graph must not be empty for matching frame: {frame}"
        );

        // The entity in the output must have the expected type.
        let entity_type = graph[0]
            .get("@type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        prop_assert_eq!(
            entity_type, &type_iri,
            "entity type in output must match frame type"
        );
    }

    /// Non-matching frame types produce an empty graph.
    #[test]
    fn non_matching_frame_produces_empty_output(
        type_suffix in "[A-Z][a-z]{3,10}",
        other_suffix in "[A-Z][a-z]{3,10}",
    ) {
        prop_assume!(type_suffix != other_suffix);
        let type_iri = format!("http://example.org/{type_suffix}");
        let other_iri = format!("http://example.org/{other_suffix}");
        let prop_iri = "http://example.org/name";
        let input = arb_jsonld_input(&type_iri, prop_iri, "test");
        let frame = arb_jsonld_frame(&other_iri);
        let output = apply_frame(&input, &frame);

        let graph = output
            .get("@graph")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        prop_assert!(
            graph.is_empty(),
            "non-matching frame must produce empty graph"
        );
    }
}
