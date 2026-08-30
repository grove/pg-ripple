//! Typed initial bindings for the application query API.
//!
//! The public JSON shape is deliberately small: one object maps variable names
//! to URI or literal descriptors. It becomes one algebra-level VALUES row.

use std::collections::HashSet;

use serde_json::Value;
use spargebra::algebra::GraphPattern;
use spargebra::term::{GroundTerm, Literal, NamedNode, Variable};

fn error(code: &str, message: impl Into<String>) -> String {
    format!("{code}: {}", message.into())
}

pub(crate) fn with_bindings(pattern: GraphPattern, input: &Value) -> Result<GraphPattern, String> {
    let (variables, row) = parse(input)?;
    attach_parsed_bindings(pattern, variables, row)
}

pub(crate) fn attach_parsed_bindings(
    pattern: GraphPattern,
    variables: Vec<Variable>,
    row: Vec<Option<GroundTerm>>,
) -> Result<GraphPattern, String> {
    if variables.is_empty() {
        return Ok(pattern);
    }

    let scope = pattern_variables(&pattern);
    for variable in &variables {
        if !scope.contains(variable.as_str()) {
            return Err(error(
                "PT0576",
                format!(
                    "binding variable ?{} is not in query scope",
                    variable.as_str()
                ),
            ));
        }
    }

    Ok(attach_values(
        pattern,
        GraphPattern::Values {
            variables,
            bindings: vec![row],
        },
    ))
}

/// Build the plan-cache shape without putting application values in the key.
pub(crate) fn shape(input: &Value) -> String {
    let Some(object) = input.as_object() else {
        return "invalid".to_owned();
    };
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.into_iter()
        .map(|key| {
            let term = object.get(key).and_then(Value::as_object);
            format!(
                "{key}:{}:{}:{}",
                term.and_then(|v| v.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("?"),
                term.is_some_and(|v| v.contains_key("datatype")),
                term.is_some_and(|v| v.contains_key("xml:lang")),
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Parse the public one-object, one-row binding shape in stable variable order.
pub(crate) fn parse(input: &Value) -> Result<(Vec<Variable>, Vec<Option<GroundTerm>>), String> {
    let object = input
        .as_object()
        .ok_or_else(|| error("PT0570", "bindings must be a JSON object"))?;
    let max_vars = crate::SPARQL_MAX_INITIAL_BINDINGS.get().max(0) as usize;
    if object.len() > max_vars {
        return Err(error(
            "PT0571",
            format!("too many binding variables; limit is {max_vars}"),
        ));
    }

    let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
    names.sort_unstable_by_key(|name| name.strip_prefix('?').unwrap_or(name));
    let mut variables = Vec::with_capacity(names.len());
    let mut row = Vec::with_capacity(names.len());
    let mut seen = HashSet::new();
    for raw_name in names {
        let name = raw_name.strip_prefix('?').unwrap_or(raw_name);
        let variable = Variable::new(name)
            .map_err(|_| error("PT0571", format!("invalid binding variable ?{name}")))?;
        if !seen.insert(variable.as_str().to_owned()) {
            return Err(error(
                "PT0571",
                format!("duplicate binding variable ?{name}"),
            ));
        }
        variables.push(variable);
        let value = object
            .get(raw_name)
            .ok_or_else(|| error("PT0572", "binding term is missing"))?;
        row.push(parse_term(value)?);
    }
    Ok((variables, row))
}

fn parse_term(value: &Value) -> Result<Option<GroundTerm>, String> {
    if value.is_null() {
        return Err(error("PT0572", "binding terms cannot be null"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| error("PT0572", "binding terms must be objects"))?;
    const ALLOWED: &[&str] = &["type", "value", "datatype", "xml:lang"];
    if let Some(field) = object
        .keys()
        .find(|field| !ALLOWED.contains(&field.as_str()))
    {
        return Err(error(
            "PT0572",
            format!("unknown binding term field '{field}'"),
        ));
    }
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| error("PT0572", "binding term is missing string type"))?;
    let text = object
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| error("PT0572", "binding term is missing string value"))?;
    let max_bytes = crate::SPARQL_MAX_BINDING_VALUE_BYTES.get().max(0) as usize;
    if text.len() > max_bytes {
        return Err(error(
            "PT0574",
            format!("binding term exceeds {max_bytes} bytes"),
        ));
    }

    match kind {
        "uri" => {
            if object.contains_key("datatype") || object.contains_key("xml:lang") {
                return Err(error(
                    "PT0573",
                    "URI bindings cannot have datatype or xml:lang",
                ));
            }
            let iri = NamedNode::new(text)
                .map_err(|_| error("PT0573", "binding uri is not a valid absolute IRI"))?;
            if !is_absolute_iri(text) {
                return Err(error("PT0573", "binding uri is not a valid absolute IRI"));
            }
            Ok(Some(iri.into()))
        }
        "literal" => {
            if object.contains_key("datatype") && object.contains_key("xml:lang") {
                return Err(error(
                    "PT0573",
                    "literal cannot contain both xml:lang and datatype",
                ));
            }
            if let Some(language) = object.get("xml:lang") {
                let language = language
                    .as_str()
                    .ok_or_else(|| error("PT0573", "literal xml:lang must be a string"))?;
                return Ok(Some(
                    Literal::new_language_tagged_literal(text, language)
                        .map_err(|_| {
                            error("PT0573", "binding literal has an invalid language tag")
                        })?
                        .into(),
                ));
            }
            let datatype = object
                .get("datatype")
                .map(|value| {
                    let datatype = value
                        .as_str()
                        .ok_or_else(|| error("PT0573", "literal datatype must be a string"))?;
                    let iri = NamedNode::new(datatype).map_err(|_| {
                        error("PT0573", "literal datatype is not a valid absolute IRI")
                    })?;
                    if !is_absolute_iri(datatype) {
                        return Err(error(
                            "PT0573",
                            "literal datatype is not a valid absolute IRI",
                        ));
                    }
                    Ok(iri)
                })
                .transpose()?;
            Ok(Some(match datatype {
                Some(datatype) => Literal::new_typed_literal(text, datatype).into(),
                None => Literal::new_simple_literal(text).into(),
            }))
        }
        "bnode" => Err(error("PT0575", "blank-node bindings are not supported")),
        "triple" => Err(error("PT0575", "RDF-star bindings are not supported")),
        _ => Err(error("PT0572", "binding term type must be uri or literal")),
    }
}

fn is_absolute_iri(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(i, byte)| {
            byte.is_ascii_alphabetic()
                || (i > 0 && (byte.is_ascii_digit() || b"+.-".contains(&byte)))
        })
}

fn attach_values(pattern: GraphPattern, values: GraphPattern) -> GraphPattern {
    match pattern {
        GraphPattern::Project { inner, variables } => GraphPattern::Project {
            inner: Box::new(attach_values(*inner, values)),
            variables,
        },
        GraphPattern::Distinct { inner } => GraphPattern::Distinct {
            inner: Box::new(attach_values(*inner, values)),
        },
        GraphPattern::Reduced { inner } => GraphPattern::Reduced {
            inner: Box::new(attach_values(*inner, values)),
        },
        GraphPattern::Slice {
            inner,
            start,
            length,
        } => GraphPattern::Slice {
            inner: Box::new(attach_values(*inner, values)),
            start,
            length,
        },
        GraphPattern::OrderBy { inner, expression } => GraphPattern::OrderBy {
            inner: Box::new(attach_values(*inner, values)),
            expression,
        },
        GraphPattern::Filter { expr, inner } => GraphPattern::Filter {
            expr,
            inner: Box::new(attach_values(*inner, values)),
        },
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => GraphPattern::Extend {
            inner: Box::new(attach_values(*inner, values)),
            variable,
            expression,
        },
        pattern => GraphPattern::Join {
            left: Box::new(pattern),
            right: Box::new(values),
        },
    }
}

fn pattern_variables(pattern: &GraphPattern) -> HashSet<String> {
    let mut output = HashSet::new();
    collect_variables(pattern, &mut output);
    output
}

fn collect_variables(pattern: &GraphPattern, output: &mut HashSet<String>) {
    use spargebra::algebra::GraphPattern::*;
    match pattern {
        Bgp { patterns } => patterns.iter().for_each(|triple| {
            for term in [&triple.subject, &triple.object] {
                if let spargebra::term::TermPattern::Variable(variable) = term {
                    output.insert(variable.as_str().to_owned());
                }
            }
            if let spargebra::term::NamedNodePattern::Variable(variable) = &triple.predicate {
                output.insert(variable.as_str().to_owned());
            }
        }),
        Path {
            subject, object, ..
        } => {
            for term in [subject, object] {
                if let spargebra::term::TermPattern::Variable(variable) = term {
                    output.insert(variable.as_str().to_owned());
                }
            }
        }
        Join { left, right } | Union { left, right } | Minus { left, right } => {
            collect_variables(left, output);
            collect_variables(right, output);
        }
        LeftJoin { left, right, .. } | Lateral { left, right } => {
            collect_variables(left, output);
            collect_variables(right, output);
        }
        Filter { inner, .. }
        | OrderBy { inner, .. }
        | Distinct { inner }
        | Reduced { inner }
        | Slice { inner, .. }
        | Group { inner, .. } => collect_variables(inner, output),
        Project { inner, variables } => {
            output.extend(variables.iter().map(|v| v.as_str().to_owned()));
            collect_variables(inner, output);
        }
        Graph { name, inner } => {
            if let spargebra::term::NamedNodePattern::Variable(variable) = name {
                output.insert(variable.as_str().to_owned());
            }
            collect_variables(inner, output);
        }
        Extend {
            inner, variable, ..
        } => {
            output.insert(variable.as_str().to_owned());
            collect_variables(inner, output);
        }
        Values { variables, .. } => output.extend(variables.iter().map(|v| v.as_str().to_owned())),
        Service { name, inner, .. } => {
            if let spargebra::term::NamedNodePattern::Variable(variable) = name {
                output.insert(variable.as_str().to_owned());
            }
            collect_variables(inner, output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_public_one_row_shape() {
        let input = serde_json::json!({
            "?x": {"type": "uri", "value": "https://example.test/x"},
            "label": {"type": "literal", "value": "hello", "xml:lang": "en"}
        });
        let (variables, row) = parse(&input).unwrap();
        assert_eq!(
            variables.iter().map(Variable::as_str).collect::<Vec<_>>(),
            ["label", "x"]
        );
        assert_eq!(row.len(), 2);
    }

    #[test]
    fn rejects_non_w3c_terms() {
        let input = serde_json::json!({"x": {"type": "bnode", "value": "b"}});
        assert!(parse(&input).unwrap_err().starts_with("PT0575:"));
    }
}
