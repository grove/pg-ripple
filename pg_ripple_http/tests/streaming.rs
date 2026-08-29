use pg_ripple_http::streaming::{
    ChunkCoalescer, QueryForm, RdfBindingValue, ResultBindingRow, ResultMetadata, StreamFormat,
    StreamingEncoder,
};
use serde_json::json;

#[test]
fn empty_results_keep_valid_headers_and_footers() {
    let metadata = ResultMetadata::select(vec!["person".into()]).unwrap();
    let mut encoder = StreamingEncoder::new(StreamFormat::SparqlJson, metadata).unwrap();
    let mut body = encoder.start().unwrap();
    body.extend(encoder.finish().unwrap());
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["head"]["vars"], json!(["person"]));
    assert!(parsed["results"]["bindings"].as_array().unwrap().is_empty());
}

#[test]
fn typed_terms_round_trip_through_json_and_ntriples() {
    let row = ResultBindingRow::from_json(&json!({
        "iri": {"type": "uri", "value": "https://example.test/a"},
        "blank": {"type": "bnode", "value": "b1"},
        "label": {"type": "literal", "value": "A,\tB", "xml:lang": "en"},
        "missing": null
    }))
    .unwrap();
    assert!(matches!(row.value("missing"), Some(None)));
    assert_eq!(
        RdfBindingValue::from_json(&json!({
            "type": "literal", "value": "42",
            "datatype": "http://www.w3.org/2001/XMLSchema#integer"
        }))
        .unwrap()
        .to_ntriples()
        .unwrap(),
        "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
}

#[test]
fn delimited_output_and_coalescing_are_bounded() {
    let metadata = ResultMetadata::select(vec!["label".into()]).unwrap();
    let row = ResultBindingRow::from_json(&json!({
        "label": {"type": "literal", "value": "A,B"}
    }))
    .unwrap();
    let mut encoder = StreamingEncoder::new(StreamFormat::Csv, metadata).unwrap();
    let mut output = encoder.start().unwrap();
    output.extend(encoder.encode_row(&row).unwrap());
    output.extend(encoder.finish().unwrap());
    assert_eq!(String::from_utf8(output).unwrap(), "label\n\"A,B\"\n");

    let mut coalescer = ChunkCoalescer::new(3).unwrap();
    let mut chunks = coalescer.push(b"abcdef");
    chunks.extend(coalescer.finish());
    assert_eq!(chunks, vec![b"abc".to_vec(), b"def".to_vec()]);
}

#[test]
fn ntriples_rejects_rdf_star_terms() {
    let metadata = ResultMetadata::new(QueryForm::Construct, Vec::new()).unwrap();
    let mut encoder = StreamingEncoder::new(StreamFormat::NTriples, metadata).unwrap();
    encoder.start().unwrap();
    let quoted =
        RdfBindingValue::Triple(Box::new(pg_ripple_http::streaming::QuotedTripleBinding {
            subject: Box::new(RdfBindingValue::Iri("https://example.test/s".into())),
            predicate: Box::new(RdfBindingValue::Iri("https://example.test/p".into())),
            object: Box::new(RdfBindingValue::Iri("https://example.test/o".into())),
        }));
    assert!(
        encoder
            .encode_triple(
                &RdfBindingValue::Iri("https://example.test/s".into()),
                &RdfBindingValue::Iri("https://example.test/p".into()),
                &quoted,
            )
            .is_err()
    );
}
