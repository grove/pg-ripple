use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::{StreamError, invalid};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryForm {
    Select,
    Ask,
    Construct,
    Describe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RdfBindingValue {
    Iri(String),
    BlankNode(String),
    Literal {
        lexical: String,
        datatype: Option<String>,
        language: Option<String>,
    },
    Triple(Box<QuotedTripleBinding>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotedTripleBinding {
    pub subject: Box<RdfBindingValue>,
    pub predicate: Box<RdfBindingValue>,
    pub object: Box<RdfBindingValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultBindingRow {
    pub values: BTreeMap<String, Option<RdfBindingValue>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultMetadata {
    pub query_form: QueryForm,
    pub variables: Vec<String>,
}

impl ResultMetadata {
    pub fn new(query_form: QueryForm, variables: Vec<String>) -> Result<Self, StreamError> {
        for variable in &variables {
            if variable.is_empty() || variable.starts_with('?') || variable.starts_with('$') {
                return Err(invalid(format!("invalid projected variable: {variable}")));
            }
        }
        let mut sorted = variables.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != variables.len() {
            return Err(invalid("projected variables must be unique"));
        }
        Ok(Self {
            query_form,
            variables,
        })
    }

    pub fn select(variables: Vec<String>) -> Result<Self, StreamError> {
        Self::new(QueryForm::Select, variables)
    }
}

impl ResultBindingRow {
    pub fn new(values: BTreeMap<String, Option<RdfBindingValue>>) -> Self {
        Self { values }
    }

    pub fn from_json(value: &Value) -> Result<Self, StreamError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("SPARQL result binding row must be an object"))?;
        object
            .iter()
            .map(|(variable, value)| {
                Ok((
                    variable.clone(),
                    (!value.is_null())
                        .then(|| RdfBindingValue::from_json(value))
                        .transpose()?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, StreamError>>()
            .map(Self::new)
    }

    pub fn value(&self, variable: &str) -> Option<&Option<RdfBindingValue>> {
        self.values.get(variable)
    }
}

impl RdfBindingValue {
    pub fn from_json(value: &Value) -> Result<Self, StreamError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("SPARQL result term must be an object"))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("SPARQL result term is missing a string type"))?;
        let value = object
            .get("value")
            .ok_or_else(|| invalid("SPARQL result term is missing a value"))?;

        match kind {
            "uri" => {
                let value = value
                    .as_str()
                    .ok_or_else(|| invalid("SPARQL result term value must be a string"))?;
                non_empty(value, "IRI").map(|value| Self::Iri(value.to_owned()))
            }
            "bnode" => {
                let value = value
                    .as_str()
                    .ok_or_else(|| invalid("SPARQL result term value must be a string"))?;
                let label = value.strip_prefix("_:").unwrap_or(value);
                valid_bnode(label)
                    .then(|| Self::BlankNode(label.to_owned()))
                    .ok_or_else(|| invalid(format!("invalid blank-node label: {value}")))
            }
            "literal" => {
                let value = value
                    .as_str()
                    .ok_or_else(|| invalid("SPARQL result term value must be a string"))?;
                let datatype = object
                    .get("datatype")
                    .map(|value| {
                        value
                            .as_str()
                            .ok_or_else(|| invalid("literal datatype must be a string"))
                            .and_then(|value| non_empty(value, "literal datatype"))
                            .map(str::to_owned)
                    })
                    .transpose()?;
                let language = object
                    .get("xml:lang")
                    .map(|value| {
                        let language = value
                            .as_str()
                            .ok_or_else(|| invalid("literal language must be a string"))?;
                        valid_language(language)
                            .then(|| language.to_ascii_lowercase())
                            .ok_or_else(|| invalid(format!("invalid language tag: {language}")))
                    })
                    .transpose()?;
                if datatype.is_some() && language.is_some() {
                    return Err(invalid("literal cannot have both datatype and language"));
                }
                Ok(Self::Literal {
                    lexical: value.to_owned(),
                    datatype,
                    language,
                })
            }
            "triple" => {
                let triple = object
                    .get("value")
                    .and_then(Value::as_object)
                    .ok_or_else(|| invalid("quoted triple value must be an object"))?;
                let term = |name| {
                    triple
                        .get(name)
                        .ok_or_else(|| invalid(format!("quoted triple is missing {name}")))
                        .and_then(Self::from_json)
                        .map(Box::new)
                };
                Ok(Self::Triple(Box::new(QuotedTripleBinding {
                    subject: term("subject")?,
                    predicate: term("predicate")?,
                    object: term("object")?,
                })))
            }
            other => Err(invalid(format!(
                "unsupported SPARQL result term type: {other}"
            ))),
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Iri(value) => json!({"type": "uri", "value": value}),
            Self::BlankNode(value) => json!({"type": "bnode", "value": value}),
            Self::Literal {
                lexical,
                datatype,
                language,
            } => {
                let mut object = Map::new();
                object.insert("type".into(), json!("literal"));
                object.insert("value".into(), json!(lexical));
                if let Some(datatype) = datatype {
                    object.insert("datatype".into(), json!(datatype));
                }
                if let Some(language) = language {
                    object.insert("xml:lang".into(), json!(language));
                }
                Value::Object(object)
            }
            Self::Triple(triple) => json!({
                "type": "triple",
                "value": {
                    "subject": triple.subject.to_json(),
                    "predicate": triple.predicate.to_json(),
                    "object": triple.object.to_json(),
                }
            }),
        }
    }

    pub fn to_ntriples(&self) -> Result<String, StreamError> {
        match self {
            Self::Iri(value) => {
                non_empty(value, "IRI")?;
                Ok(format!("<{}>", escape_iri(value)))
            }
            Self::BlankNode(label) => {
                if !valid_bnode(label) {
                    return Err(invalid(format!("invalid blank-node label: {label}")));
                }
                Ok(format!("_:{label}"))
            }
            Self::Literal {
                lexical,
                datatype,
                language,
            } => {
                if datatype.is_some() && language.is_some() {
                    return Err(invalid("literal cannot have both datatype and language"));
                }
                let mut result = format!("\"{}\"", escape_literal(lexical));
                if let Some(language) = language {
                    if !valid_language(language) {
                        return Err(invalid(format!("invalid language tag: {language}")));
                    }
                    result.push('@');
                    result.push_str(language);
                } else if let Some(datatype) = datatype {
                    non_empty(datatype, "literal datatype")?;
                    result.push_str("^^<");
                    result.push_str(&escape_iri(datatype));
                    result.push('>');
                }
                Ok(result)
            }
            Self::Triple(_) => Err(invalid("RDF-star quoted triples are not valid N-Triples")),
        }
    }

    fn to_delimited_value(&self) -> Result<String, StreamError> {
        match self {
            Self::Iri(value) => Ok(escape_iri(value)),
            Self::BlankNode(label) => {
                if !valid_bnode(label) {
                    return Err(invalid(format!("invalid blank-node label: {label}")));
                }
                Ok(format!("_:{label}"))
            }
            Self::Literal { lexical, .. } => Ok(lexical.clone()),
            Self::Triple(_) => Err(invalid(
                "RDF-star quoted triples are not valid delimited results",
            )),
        }
    }
}

fn non_empty<'a>(value: &'a str, kind: &str) -> Result<&'a str, StreamError> {
    (!value.is_empty())
        .then_some(value)
        .ok_or_else(|| invalid(format!("{kind} must not be empty")))
}

fn valid_bnode(label: &str) -> bool {
    let mut chars = label.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn valid_language(language: &str) -> bool {
    !language.is_empty()
        && language
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn escape_iri(value: &str) -> String {
    value.chars().map(escape_iri_char).collect()
}

fn escape_iri_char(c: char) -> String {
    if c.is_ascii_control()
        || c == ' '
        || matches!(c, '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\')
    {
        escape_codepoint(c)
    } else {
        c.to_string()
    }
}

fn escape_literal(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".into(),
            '"' => "\\\"".into(),
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            c if c.is_control() => escape_codepoint(c),
            c => c.to_string(),
        })
        .collect()
}

fn escape_codepoint(c: char) -> String {
    if (c as u32) <= 0xffff {
        format!("\\u{:04X}", c as u32)
    } else {
        format!("\\U{:08X}", c as u32)
    }
}

fn csv_escape(value: &str) -> String {
    if value.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub(crate) fn row_json(row: &ResultBindingRow, variables: &[String]) -> Value {
    let mut object = Map::new();
    for variable in variables {
        if let Some(Some(value)) = row.value(variable) {
            object.insert(variable.clone(), value.to_json());
        }
    }
    Value::Object(object)
}

pub(crate) fn row_delimited(
    row: &ResultBindingRow,
    variables: &[String],
    delimiter: char,
    csv_quote: bool,
) -> Result<String, StreamError> {
    let fields = variables
        .iter()
        .map(|variable| {
            row.value(variable)
                .and_then(Option::as_ref)
                .map(|value| {
                    if csv_quote {
                        value.to_delimited_value()
                    } else {
                        value.to_ntriples()
                    }
                })
                .transpose()
                .map(|value| {
                    let value = value.unwrap_or_default();
                    if csv_quote { csv_escape(&value) } else { value }
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("{}\n", fields.join(&delimiter.to_string())))
}
