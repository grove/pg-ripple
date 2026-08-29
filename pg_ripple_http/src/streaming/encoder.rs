use super::terms::{
    QueryForm, RdfBindingValue, ResultBindingRow, ResultMetadata, row_delimited, row_json,
};
use super::{StreamError, invalid};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFormat {
    SparqlJson,
    Csv,
    Tsv,
    NTriples,
}

impl StreamFormat {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::SparqlJson => "application/sparql-results+json",
            Self::Csv => "text/csv",
            Self::Tsv => "text/tab-separated-values",
            Self::NTriples => "application/n-triples",
        }
    }
}

/// Stateful encoder for one result stream.  It emits fragments on demand;
/// callers decide when and how to poll or flush them.
#[derive(Debug)]
pub struct StreamingEncoder {
    format: StreamFormat,
    metadata: ResultMetadata,
    started: bool,
    finished: bool,
    emitted_row: bool,
    emitted_boolean: bool,
}

impl StreamingEncoder {
    pub fn new(format: StreamFormat, metadata: ResultMetadata) -> Result<Self, StreamError> {
        match (metadata.query_form, format) {
            (QueryForm::Select, StreamFormat::NTriples)
            | (QueryForm::Ask, StreamFormat::NTriples)
            | (QueryForm::Construct | QueryForm::Describe, StreamFormat::SparqlJson)
            | (QueryForm::Construct | QueryForm::Describe, StreamFormat::Csv)
            | (QueryForm::Construct | QueryForm::Describe, StreamFormat::Tsv) => Err(invalid(
                "result format is not valid for this SPARQL query form",
            )),
            _ => Ok(Self {
                format,
                metadata,
                started: false,
                finished: false,
                emitted_row: false,
                emitted_boolean: false,
            }),
        }
    }

    pub fn start(&mut self) -> Result<Vec<u8>, StreamError> {
        self.ensure_not_started()?;
        self.started = true;
        let bytes = match (self.metadata.query_form, self.format) {
            (QueryForm::Select, StreamFormat::SparqlJson) => format!(
                "{{\"head\":{{\"vars\":{}}},\"results\":{{\"bindings\":[",
                serde_json::to_string(&self.metadata.variables)?
            )
            .into_bytes(),
            (QueryForm::Ask, StreamFormat::SparqlJson) => b"{\"head\":{},\"boolean\":".to_vec(),
            (QueryForm::Select, StreamFormat::Csv) => {
                format!("{}\n", self.metadata.variables.join(",")).into_bytes()
            }
            (QueryForm::Select, StreamFormat::Tsv) => format!(
                "{}\n",
                self.metadata
                    .variables
                    .iter()
                    .map(|variable| format!("?{variable}"))
                    .collect::<Vec<_>>()
                    .join("\t")
            )
            .into_bytes(),
            (QueryForm::Ask, StreamFormat::Csv) => b"boolean\n".to_vec(),
            (QueryForm::Ask, StreamFormat::Tsv) => b"?boolean\n".to_vec(),
            (QueryForm::Construct | QueryForm::Describe, StreamFormat::NTriples) => Vec::new(),
            _ => unreachable!("validated in StreamingEncoder::new"),
        };
        Ok(bytes)
    }

    pub fn encode_row(&mut self, row: &ResultBindingRow) -> Result<Vec<u8>, StreamError> {
        self.ensure_started()?;
        if self.metadata.query_form != QueryForm::Select {
            return Err(invalid("row bindings are only valid for SELECT streams"));
        }
        match self.format {
            StreamFormat::SparqlJson => {
                let prefix = if self.emitted_row { "," } else { "" };
                self.emitted_row = true;
                Ok(format!(
                    "{prefix}{}",
                    serde_json::to_string(&row_json(row, &self.metadata.variables))?
                )
                .into_bytes())
            }
            StreamFormat::Csv => {
                self.emitted_row = true;
                row_delimited(row, &self.metadata.variables, ',', true).map(String::into_bytes)
            }
            StreamFormat::Tsv => {
                self.emitted_row = true;
                row_delimited(row, &self.metadata.variables, '\t', false).map(String::into_bytes)
            }
            StreamFormat::NTriples => unreachable!("validated in StreamingEncoder::new"),
        }
    }

    /// Encode the one boolean value of an ASK result.
    pub fn encode_boolean(&mut self, value: bool) -> Result<Vec<u8>, StreamError> {
        self.ensure_started()?;
        if self.metadata.query_form != QueryForm::Ask || self.emitted_boolean {
            return Err(invalid("stream does not accept another ASK boolean"));
        }
        self.emitted_boolean = true;
        match self.format {
            StreamFormat::SparqlJson => Ok(format!("{value}").into_bytes()),
            StreamFormat::Csv | StreamFormat::Tsv => Ok(format!("{value}\n").into_bytes()),
            StreamFormat::NTriples => unreachable!("validated in StreamingEncoder::new"),
        }
    }

    /// Encode one graph triple.  N-Triples has no document header or footer.
    pub fn encode_triple(
        &mut self,
        subject: &RdfBindingValue,
        predicate: &RdfBindingValue,
        object: &RdfBindingValue,
    ) -> Result<Vec<u8>, StreamError> {
        self.ensure_started()?;
        if self.metadata.query_form != QueryForm::Construct
            && self.metadata.query_form != QueryForm::Describe
        {
            return Err(invalid("triples are only valid for graph result streams"));
        }
        if !matches!(
            subject,
            RdfBindingValue::Iri(_) | RdfBindingValue::BlankNode(_)
        ) {
            return Err(invalid("N-Triples subject must be an IRI or blank node"));
        }
        if !matches!(predicate, RdfBindingValue::Iri(_)) {
            return Err(invalid("N-Triples predicate must be an IRI"));
        }
        Ok(format!(
            "{} {} {} .\n",
            subject.to_ntriples()?,
            predicate.to_ntriples()?,
            object.to_ntriples()?
        )
        .into_bytes())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        self.ensure_started()?;
        if self.finished {
            return Err(invalid("stream encoder is already finished"));
        }
        let result = match (self.metadata.query_form, self.format) {
            (QueryForm::Select, StreamFormat::SparqlJson) => Ok(b"]}}".to_vec()),
            (QueryForm::Ask, StreamFormat::SparqlJson) if self.emitted_boolean => Ok(b"}".to_vec()),
            (QueryForm::Ask, StreamFormat::SparqlJson) => {
                Err(invalid("ASK stream is missing its boolean result"))
            }
            (QueryForm::Select | QueryForm::Ask, StreamFormat::Csv | StreamFormat::Tsv)
            | (QueryForm::Construct | QueryForm::Describe, StreamFormat::NTriples) => {
                Ok(Vec::new())
            }
            _ => unreachable!("validated in StreamingEncoder::new"),
        }?;
        self.finished = true;
        Ok(result)
    }

    pub fn format(&self) -> StreamFormat {
        self.format
    }

    fn ensure_started(&self) -> Result<(), StreamError> {
        if !self.started {
            Err(invalid("stream encoder must be started before encoding"))
        } else if self.finished {
            Err(invalid("stream encoder is already finished"))
        } else {
            Ok(())
        }
    }

    fn ensure_not_started(&self) -> Result<(), StreamError> {
        if self.started {
            Err(invalid("stream encoder is already started"))
        } else {
            Ok(())
        }
    }
}
