//! SPARQL CONSTRUCT execution (M15-13, v0.96.0).
//! Moved from execute/mod.rs lines 163-439.

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::SparqlParser;

use super::super::decode::batch_decode;
use super::super::sqlgen;
use crate::dictionary;

// ─── SPARQL CONSTRUCT ─────────────────────────────────────────────────────────

/// Execute a SPARQL CONSTRUCT query; returns raw `(s_id, p_id, o_id)` integer rows.
///
/// Used by the framing engine to obtain encoded triples that are then decoded
/// in a single batch SPI round-trip.
pub(crate) fn sparql_construct_rows(query_text: &str) -> Vec<(i64, i64, i64)> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => pgrx::error!("sparql_construct_rows() requires a CONSTRUCT query"),
    };

    let trans = sqlgen::translate_select(&pattern, None);
    let (sql, variables) = (trans.sql, trans.variables);

    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("SPARQL CONSTRUCT SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<i64>> = Vec::with_capacity(variables.len());
            for i in 1..=(variables.len() as i64) {
                row_vals.push(row.get::<i64>(i as _).ok().flatten());
            }
            raw_rows.push(row_vals);
        }
    });

    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();
    let resolve_idx = |var: &str| variables.iter().position(|v| v == var);

    let mut result = Vec::new();
    for row_vals in &raw_rows {
        for triple in &template {
            let s_id = match &triple.subject {
                spargebra::term::TermPattern::NamedNode(nn) => Some(crate::dictionary::encode(
                    nn.as_str(),
                    crate::dictionary::KIND_IRI,
                )),
                spargebra::term::TermPattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                _ => None,
            };
            let p_id = match &triple.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => Some(
                    crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                ),
                spargebra::term::NamedNodePattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                _ => None,
            };
            let o_id = match &triple.object {
                spargebra::term::TermPattern::NamedNode(nn) => Some(crate::dictionary::encode(
                    nn.as_str(),
                    crate::dictionary::KIND_IRI,
                )),
                spargebra::term::TermPattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                spargebra::term::TermPattern::Triple(inner) => {
                    // v0.24.0: ground quoted-triple in CONSTRUCT template.
                    let ts_str = match &inner.subject {
                        spargebra::term::TermPattern::NamedNode(nn) => Some(nn.as_str()),
                        _ => None,
                    };
                    let tp_id = match &inner.predicate {
                        spargebra::term::NamedNodePattern::NamedNode(nn) => Some(
                            crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                        ),
                        _ => None,
                    };
                    let to_id_opt = match &inner.object {
                        spargebra::term::TermPattern::NamedNode(nn) => Some(
                            crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                        ),
                        spargebra::term::TermPattern::Variable(v)
                            if var_set.contains(v.as_str()) =>
                        {
                            resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                        }
                        _ => None,
                    };
                    match (ts_str, tp_id, to_id_opt) {
                        (Some(ts_str), Some(tp_id), Some(to_id)) => {
                            let ts_id =
                                crate::dictionary::encode(ts_str, crate::dictionary::KIND_IRI);
                            Some(crate::dictionary::encode_quoted_triple(ts_id, tp_id, to_id))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let (Some(s), Some(p), Some(o)) = (s_id, p_id, o_id) {
                result.push((s, p, o));
            }
        }
    }
    result
}

/// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
///
/// Each row is `{"s": "<iri>", "p": "<iri>", "o": "..."}`.
pub(crate) fn sparql_construct(query_text: &str) -> Vec<pgrx::JsonB> {
    sparql_construct_impl(query_text, None)
}

pub(crate) fn sparql_construct_with_bindings(
    query_text: &str,
    bindings: &serde_json::Value,
) -> Vec<pgrx::JsonB> {
    sparql_construct_impl(query_text, Some(bindings))
}

fn sparql_construct_impl(
    query_text: &str,
    bindings: Option<&serde_json::Value>,
) -> Vec<pgrx::JsonB> {
    let query_text = super::super::parse::preprocess_query(query_text);
    let query = SparqlParser::new()
        .parse_query(&query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => pgrx::error!("sparql_construct() requires a CONSTRUCT query"),
    };

    let pattern = if let Some(bindings) = bindings {
        super::super::bindings::with_bindings(pattern, bindings)
            .unwrap_or_else(|e| pgrx::error!("invalid SPARQL bindings: {e}"))
    } else {
        pattern
    };

    // Translate the WHERE clause as a SELECT over all template variables.
    let trans = if bindings.is_some() {
        sqlgen::translate_select_parameterized(&pattern, None)
    } else {
        sqlgen::translate_select(&pattern, None)
    };
    let (sql, variables) = (trans.sql, trans.variables);
    let params = trans.parameters;
    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();

    // Execute and collect raw rows.
    let mut all_ids: Vec<i64> = Vec::new();
    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();
    Spi::connect(|client| {
        let args: Vec<pgrx::datum::DatumWithOid> = params
            .iter()
            .copied()
            .map(pgrx::datum::DatumWithOid::from)
            .collect();
        let rows = client
            .select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("SPARQL CONSTRUCT SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<i64>> = Vec::with_capacity(variables.len());
            for i in 1..=(variables.len() as i64) {
                let val = row.get::<i64>(i as _).ok().flatten();
                if let Some(id) = val {
                    all_ids.push(id);
                }
                row_vals.push(val);
            }
            raw_rows.push(row_vals);
        }
    });

    all_ids.sort_unstable();
    all_ids.dedup();
    let decode_map = batch_decode(&all_ids);

    // Build a var → decoded-value map helper.
    let resolve = |row_vals: &[Option<i64>], var: &str| -> Option<String> {
        let idx = variables.iter().position(|v| v == var)?;
        let id = row_vals.get(idx).copied().flatten()?;
        decode_map.get(&id).cloned()
    };

    // Instantiate the CONSTRUCT template for each result row.
    let mut result = Vec::new();
    for row_vals in &raw_rows {
        for triple in &template {
            // Resolve subject (TermPattern).
            let s_val = match &triple.subject {
                spargebra::term::TermPattern::NamedNode(nn) => Some(format!("<{}>", nn.as_str())),
                spargebra::term::TermPattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            // Resolve predicate (NamedNodePattern).
            let p_val = match &triple.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => {
                    Some(format!("<{}>", nn.as_str()))
                }
                spargebra::term::NamedNodePattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
            };
            // Resolve object.
            let o_val = match &triple.object {
                spargebra::term::TermPattern::NamedNode(nn) => Some(format!("<{}>", nn.as_str())),
                spargebra::term::TermPattern::Literal(lit) => {
                    let lang = lit.language();
                    let dt = lit.datatype().as_str();
                    let kind = if lang.is_some() {
                        dictionary::KIND_LANG_LITERAL
                    } else {
                        dictionary::KIND_TYPED_LITERAL
                    };
                    Some(dictionary::format_ntriples_term(
                        lit.value(),
                        kind,
                        Some(dt),
                        lang,
                        0,
                    ))
                }
                spargebra::term::TermPattern::BlankNode(_) => None,
                spargebra::term::TermPattern::Triple(inner) => {
                    // v0.51.0: emit ground quoted triples as N-Triples-star notation
                    // `<< s p o >>` (S2-6 / N5-5).  Only ground (all-IRI) inner
                    // triples are supported; variable-containing inner triples are
                    // handled via the outer variable bindings.
                    let ts_val = match &inner.subject {
                        spargebra::term::TermPattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
                        _ => None,
                    };
                    let tp_val = match &inner.predicate {
                        spargebra::term::NamedNodePattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
                        _ => None,
                    };
                    let to_val = match &inner.object {
                        spargebra::term::TermPattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
                        spargebra::term::TermPattern::Literal(lit) => {
                            let lang = lit.language();
                            let dt = lit.datatype().as_str();
                            let kind = if lang.is_some() {
                                dictionary::KIND_LANG_LITERAL
                            } else {
                                dictionary::KIND_TYPED_LITERAL
                            };
                            Some(dictionary::format_ntriples_term(
                                lit.value(),
                                kind,
                                Some(dt),
                                lang,
                                0,
                            ))
                        }
                        _ => None,
                    };
                    match (ts_val, tp_val, to_val) {
                        (Some(s), Some(p), Some(o)) => Some(format!("<< {s} {p} {o} >>")),
                        _ => None,
                    }
                }
                spargebra::term::TermPattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
            };

            // Only emit the triple if all three components are bound.
            if let (Some(s), Some(p), Some(o)) = (s_val, p_val, o_val) {
                let mut obj = Map::new();
                obj.insert("s".to_owned(), Json::String(s));
                obj.insert("p".to_owned(), Json::String(p));
                obj.insert("o".to_owned(), Json::String(o));
                result.push(pgrx::JsonB(Json::Object(obj)));
            }
        }
    }

    result
}
