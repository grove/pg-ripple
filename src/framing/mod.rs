//! JSON-LD Framing Engine — v0.17.0.
//!
//! Translates a W3C JSON-LD 1.1 frame document into a SPARQL CONSTRUCT query,
//! executes it via the existing SPARQL engine, and reshapes the flat result into
//! a nested JSON-LD tree matching the frame's structure.
//!
//! # Public entry points
//!
//! - [`frame_to_sparql`] — translate a frame document to a SPARQL CONSTRUCT string
//! - [`frame_and_execute`] — translate, execute, embed, and compact in one call
//! - [`frame_jsonld`] — apply the embedding algorithm to any pre-expanded JSON-LD value
//! - [`execute_framed_stream`] — streaming variant: one JSON object per root node

pub mod compactor;
pub mod embedder;
pub mod frame_translator;

use serde_json::Value;

/// Translate a JSON-LD frame to a SPARQL CONSTRUCT query string.
///
/// `frame` must be a JSON object. `graph_iri` restricts the query to a named
/// graph when provided; `None` operates over the merged/default graph.
pub fn frame_to_sparql(frame: &Value, graph_iri: Option<&str>) -> Result<String, String> {
    frame_translator::translate(frame, graph_iri)
}

/// Primary end-user function: translate the frame to CONSTRUCT, execute,
/// embed, compact, and return the framed JSON-LD document.
pub fn frame_and_execute(
    frame: &Value,
    graph_iri: Option<&str>,
    embed: &str,
    explicit: bool,
    ordered: bool,
) -> Result<Value, String> {
    let sparql = frame_translator::translate(frame, graph_iri)?;

    // Execute via the SPARQL engine (reuses plan cache).
    let triples = crate::sparql::sparql_construct_rows(&sparql);

    // Decode each triple to N-Triples strings.
    let decoded = decode_rows(triples);

    // Embed into nested JSON-LD tree.
    let embedded = embedder::embed(&decoded, frame, embed, explicit, ordered)?;

    // Compact IRIs using @context from the frame.
    let compacted = compactor::compact(embedded, frame);

    Ok(compacted)
}

/// General-purpose framing primitive: apply the embedding algorithm to any
/// already-expanded JSON-LD value (not necessarily from pg_ripple storage).
///
/// `input` is expected to be a JSON-LD array of expanded node objects.
/// `frame` is the framing document.
pub fn frame_jsonld(
    input: &Value,
    frame: &Value,
    embed: &str,
    explicit: bool,
    ordered: bool,
) -> Result<Value, String> {
    // Convert the expanded JSON-LD into (s, p, o) N-Triples rows.
    let triples = expanded_jsonld_to_triples(input);
    let embedded = embedder::embed(&triples, frame, embed, explicit, ordered)?;
    let compacted = compactor::compact(embedded, frame);
    Ok(compacted)
}

/// Streaming variant: execute framing and return one JSON object per root node.
pub fn execute_framed_stream(
    frame: &Value,
    graph_iri: Option<&str>,
) -> Result<Vec<String>, String> {
    let sparql = frame_translator::translate(frame, graph_iri)?;
    let triples = crate::sparql::sparql_construct_rows(&sparql);
    let decoded = decode_rows(triples);

    let embedded = embedder::embed(&decoded, frame, "@once", false, false)?;

    // Return one line per root node in @graph (or the single node if no @graph wrapper).
    let lines = match &embedded {
        Value::Object(obj) => {
            if let Some(Value::Array(nodes)) = obj.get("@graph") {
                nodes
                    .iter()
                    .map(|n| serde_json::to_string(n).unwrap_or_else(|_| "{}".to_owned()))
                    .collect()
            } else {
                vec![serde_json::to_string(&embedded).unwrap_or_else(|_| "{}".to_owned())]
            }
        }
        Value::Array(nodes) => nodes
            .iter()
            .map(|n| serde_json::to_string(n).unwrap_or_else(|_| "{}".to_owned()))
            .collect(),
        other => vec![serde_json::to_string(other).unwrap_or_else(|_| "{}".to_owned())],
    };

    Ok(lines)
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Decode `(s_id, p_id, o_id)` integer rows to N-Triples string triples.
fn decode_rows(rows: Vec<(i64, i64, i64)>) -> Vec<(String, String, String)> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut all_ids: Vec<i64> = Vec::with_capacity(rows.len() * 3);
    for (s, p, o) in &rows {
        all_ids.push(*s);
        all_ids.push(*p);
        all_ids.push(*o);
    }
    all_ids.sort_unstable();
    all_ids.dedup();
    let map = crate::sparql::batch_decode(&all_ids);

    rows.into_iter()
        .filter_map(|(s, p, o)| {
            let s_str = map.get(&s).cloned()?;
            let p_str = map.get(&p).cloned()?;
            let o_str = map.get(&o).cloned()?;
            Some((s_str, p_str, o_str))
        })
        .collect()
}

/// Convert an expanded JSON-LD array to `(s_nt, p_nt, o_nt)` triples.
///
/// Handles the standard expanded JSON-LD form:
/// `[{"@id": "...", "predIRI": [{"@id": "..."}], ...}]`
fn expanded_jsonld_to_triples(input: &Value) -> Vec<(String, String, String)> {
    let nodes = match input {
        Value::Array(arr) => arr,
        _ => return Vec::new(),
    };

    let mut triples = Vec::new();

    for node in nodes {
        let obj = match node.as_object() {
            Some(o) => o,
            None => continue,
        };

        let s_nt = match obj.get("@id").and_then(Value::as_str) {
            Some(iri) => {
                if iri.starts_with("_:") {
                    iri.to_owned()
                } else {
                    format!("<{iri}>")
                }
            }
            None => continue,
        };

        for (key, values) in obj {
            if key.starts_with('@') {
                continue;
            }
            let p_nt = format!("<{key}>");
            let arr = match values.as_array() {
                Some(a) => a,
                None => continue,
            };
            for obj_val in arr {
                let o_nt = jsonld_value_to_nt(obj_val);
                triples.push((s_nt.clone(), p_nt.clone(), o_nt));
            }
        }
    }

    triples
}

/// Convert a JSON-LD object value back to an N-Triples string.
fn jsonld_value_to_nt(val: &Value) -> String {
    match val {
        Value::Object(obj) => {
            if let Some(id) = obj.get("@id").and_then(Value::as_str) {
                if id.starts_with("_:") {
                    id.to_owned()
                } else {
                    format!("<{id}>")
                }
            } else if let Some(v) = obj.get("@value") {
                let raw = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let escaped = raw
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                if let Some(dt) = obj.get("@type").and_then(Value::as_str) {
                    format!("\"{escaped}\"^^<{dt}>")
                } else if let Some(lang) = obj.get("@language").and_then(Value::as_str) {
                    format!("\"{escaped}\"@{lang}")
                } else {
                    format!("\"{escaped}\"")
                }
            } else {
                "\"\"".to_owned()
            }
        }
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => format!("\"{n}\""),
        Value::Bool(b) => format!(
            "\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
            if *b { "true" } else { "false" }
        ),
        _ => "\"\"".to_owned(),
    }
}
