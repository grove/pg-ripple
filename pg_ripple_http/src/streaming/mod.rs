//! Pull-oriented primitives for standards-valid SPARQL result streaming.
//!
//! This module owns no task, channel, connection, or producer.  An HTTP body
//! can call `start`, `encode_row`/`encode_triple`, and `finish` as it polls its
//! database row stream, passing each returned byte vector through
//! [`ChunkCoalescer`].

mod terms;

pub mod coalesce;
pub mod encoder;

pub use coalesce::ChunkCoalescer;
pub use encoder::{StreamFormat, StreamingEncoder};
pub use terms::{
    QueryForm, QuotedTripleBinding, RdfBindingValue, ResultBindingRow, ResultMetadata,
};

/// Error returned when a result term or stream output cannot be represented
/// by the requested standards format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamError(pub String);

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StreamError {}

impl From<serde_json::Error> for StreamError {
    fn from(error: serde_json::Error) -> Self {
        Self(format!("invalid SPARQL result JSON: {error}"))
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> StreamError {
    StreamError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_terms_round_trip_through_json_and_nt() {
        let value = RdfBindingValue::Literal {
            lexical: "a\"b\n".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        };
        let json = value.to_json();
        assert_eq!(RdfBindingValue::from_json(&json).unwrap(), value);
        assert_eq!(
            value.to_ntriples().unwrap(),
            r#""a\"b\n"^^<http://www.w3.org/2001/XMLSchema#string>"#
        );
    }

    #[test]
    fn json_row_parser_preserves_unbound_and_rdf_star_terms() {
        let row = ResultBindingRow::from_json(&json!({
            "iri": {"type": "uri", "value": "https://example.test/a"},
            "missing": null,
            "quoted": {"type": "triple", "value": {
                "subject": {"type": "bnode", "value": "b1"},
                "predicate": {"type": "uri", "value": "https://example.test/p"},
                "object": {"type": "literal", "value": "ok", "xml:lang": "en"}
            }}
        }))
        .unwrap();

        assert!(matches!(row.value("missing"), Some(None)));
        assert!(matches!(
            row.value("quoted"),
            Some(Some(RdfBindingValue::Triple(_)))
        ));
    }

    #[test]
    fn formats_are_parseable_and_json_has_no_trailing_comma() {
        let metadata = ResultMetadata::select(vec!["name".into(), "iri".into()]).unwrap();
        let row = ResultBindingRow::from_json(&json!({
            "name": {"type": "literal", "value": "A, B"},
            "iri": {"type": "uri", "value": "https://example.test/a"}
        }))
        .unwrap();

        let mut json_encoder =
            StreamingEncoder::new(StreamFormat::SparqlJson, metadata.clone()).unwrap();
        let mut json_body = json_encoder.start().unwrap();
        json_body.extend(json_encoder.encode_row(&row).unwrap());
        json_body.extend(json_encoder.finish().unwrap());
        let parsed: serde_json::Value = serde_json::from_slice(&json_body).unwrap();
        assert_eq!(parsed["results"]["bindings"][0]["name"]["value"], "A, B");

        let mut csv = StreamingEncoder::new(StreamFormat::Csv, metadata.clone()).unwrap();
        let mut csv_body = csv.start().unwrap();
        csv_body.extend(csv.encode_row(&row).unwrap());
        csv_body.extend(csv.finish().unwrap());
        assert_eq!(
            String::from_utf8(csv_body).unwrap(),
            "name,iri\n\"A, B\",https://example.test/a\n"
        );

        let mut tsv = StreamingEncoder::new(StreamFormat::Tsv, metadata).unwrap();
        let mut tsv_body = tsv.start().unwrap();
        tsv_body.extend(tsv.encode_row(&row).unwrap());
        tsv_body.extend(tsv.finish().unwrap());
        assert_eq!(
            String::from_utf8(tsv_body).unwrap(),
            "?name\t?iri\n\"A, B\"\t<https://example.test/a>\n"
        );
    }

    #[test]
    fn coalescer_never_returns_a_chunk_over_its_bound() {
        let mut coalescer = ChunkCoalescer::new(4).unwrap();
        let mut chunks = coalescer.push(b"abcdef");
        chunks.extend(coalescer.push(b"gh"));
        chunks.extend(coalescer.finish());
        assert_eq!(chunks, vec![b"abcd".to_vec(), b"efgh".to_vec()]);
    }

    #[test]
    fn ntriples_encoder_rejects_rdf_star_and_emits_canonical_lines() {
        let metadata = ResultMetadata::new(QueryForm::Construct, Vec::new()).unwrap();
        let mut encoder = StreamingEncoder::new(StreamFormat::NTriples, metadata).unwrap();
        assert!(encoder.start().unwrap().is_empty());
        let subject = RdfBindingValue::BlankNode("b1".into());
        let predicate = RdfBindingValue::Iri("https://example.test/p".into());
        let object = RdfBindingValue::Literal {
            lexical: "line\nvalue".into(),
            datatype: None,
            language: Some("EN-us".into()),
        };
        assert_eq!(
            String::from_utf8(
                encoder
                    .encode_triple(&subject, &predicate, &object)
                    .unwrap()
            )
            .unwrap(),
            "_:b1 <https://example.test/p> \"line\\nvalue\"@EN-us .\n"
        );
        let quoted = RdfBindingValue::Triple(Box::new(QuotedTripleBinding {
            subject: Box::new(subject.clone()),
            predicate: Box::new(predicate.clone()),
            object: Box::new(object.clone()),
        }));
        assert!(quoted.to_ntriples().is_err());
    }
}
