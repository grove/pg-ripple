//! Tree-embedding algorithm — implements W3C JSON-LD 1.1 Framing §4.1.
//!
//! Takes a flat list of `(subject_nt, predicate_nt, object_nt)` triple rows
//! (already decoded to N-Triples strings) and the original frame document,
//! then produces a nested JSON-LD tree matching the frame structure.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Embed options passed through the recursive walk.
#[derive(Clone)]
pub struct EmbedOptions {
    pub embed: EmbedMode,
    pub explicit: bool,
    pub omit_default: bool,
    pub ordered: bool,
}

/// `@embed` flag values.
#[derive(Clone, PartialEq)]
pub enum EmbedMode {
    Once,
    Always,
    Never,
}

impl EmbedMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "@once" | "once" => Ok(EmbedMode::Once),
            "@always" | "always" => Ok(EmbedMode::Always),
            "@never" | "never" => Ok(EmbedMode::Never),
            other => Err(format!(
                "PT711: unrecognised @embed value: {other:?}; expected @once, @always, or @never"
            )),
        }
    }
}

/// Build a subject node map from flat triple rows.
///
/// Returns `HashMap<subject_id_string, BTreeMap<predicate_iri, Vec<Value>>>`.
/// Subject and predicate keys are plain IRI strings (no angle brackets).
/// Object values are converted to JSON-LD value objects via
/// [`crate::export::nt_term_to_jsonld_value`].
fn build_node_map(
    triples: &[(String, String, String)],
) -> HashMap<String, BTreeMap<String, Vec<Value>>> {
    let mut map: HashMap<String, BTreeMap<String, Vec<Value>>> = HashMap::new();

    for (s_nt, p_nt, o_nt) in triples {
        let s_key = nt_to_id_string(s_nt);
        let p_key = nt_to_iri_string(p_nt);
        let o_val = nt_term_to_jsonld_value(o_nt);
        map.entry(s_key)
            .or_default()
            .entry(p_key)
            .or_default()
            .push(o_val);
    }

    map
}

/// Apply the W3C JSON-LD Framing §4.1 embedding algorithm.
///
/// Returns a JSON-LD document (possibly with `@graph` wrapper or a single
/// node object if only one root matches).
pub fn embed(
    triples: &[(String, String, String)],
    frame: &Value,
    embed_str: &str,
    explicit: bool,
    ordered: bool,
) -> Result<Value, String> {
    let embed_mode = EmbedMode::parse(embed_str)?;
    let opts = EmbedOptions {
        embed: embed_mode,
        explicit,
        omit_default: false,
        ordered,
    };

    let node_map = build_node_map(triples);
    let frame_obj = match frame.as_object() {
        Some(o) => o,
        None => return Err("PT710: frame must be a JSON object".to_owned()),
    };

    let mut embedded_set: HashSet<String> = HashSet::new();
    let roots = embed_frame_level(&node_map, frame_obj, &opts, &mut embedded_set);

    if roots.is_empty() {
        // Empty result per W3C framing spec.
        return Ok(Value::Object({
            let mut m = Map::new();
            m.insert("@graph".to_owned(), Value::Array(vec![]));
            m
        }));
    }

    // @omitGraph: single root node → return without @graph wrapper.
    let omit_graph = frame_obj
        .get("@omitGraph")
        .and_then(Value::as_bool)
        .unwrap_or(roots.len() == 1);

    if omit_graph && roots.len() == 1 {
        Ok(roots.into_iter().next().unwrap())
    } else {
        Ok(Value::Object({
            let mut m = Map::new();
            m.insert("@graph".to_owned(), Value::Array(roots));
            m
        }))
    }
}

/// Walk the frame at one level and return the matched + embedded node objects.
fn embed_frame_level(
    node_map: &HashMap<String, BTreeMap<String, Vec<Value>>>,
    frame_obj: &serde_json::Map<String, Value>,
    opts: &EmbedOptions,
    embedded_set: &mut HashSet<String>,
) -> Vec<Value> {
    // Gather matching subjects.
    let mut candidates: Vec<&String> = node_map.keys().collect();

    // Filter by @type.
    if let Some(type_val) = frame_obj.get("@type") {
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let expected: Vec<String> = match type_val {
            Value::String(s) => vec![strip_brackets(s).to_owned()],
            Value::Array(arr) => arr
                .iter()
                .filter_map(Value::as_str)
                .map(|s| strip_brackets(s).to_owned())
                .collect(),
            _ => vec![],
        };
        if !expected.is_empty() {
            candidates.retain(|subj| {
                let props = match node_map.get(*subj) {
                    Some(p) => p,
                    None => return false,
                };
                let types: Vec<String> = props
                    .get(rdf_type)
                    .map(|vals| {
                        vals.iter()
                            .filter_map(|v| v.as_object()?.get("@id")?.as_str())
                            .map(|s| s.to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                expected.iter().any(|e| types.contains(e))
            });
        }
    }

    // Filter by @id.
    if let Some(id_val) = frame_obj.get("@id") {
        let ids: Vec<String> = match id_val {
            Value::String(s) => vec![strip_brackets(s).to_owned()],
            Value::Array(arr) => arr
                .iter()
                .filter_map(Value::as_str)
                .map(|s| strip_brackets(s).to_owned())
                .collect(),
            _ => vec![],
        };
        if !ids.is_empty() {
            candidates.retain(|subj| ids.contains(subj));
        }
    }

    if opts.ordered {
        candidates.sort();
    }

    let mut result = Vec::new();

    for subj_key in candidates {
        let node = match build_output_node(subj_key, node_map, frame_obj, opts, embedded_set) {
            Some(n) => n,
            None => continue,
        };
        result.push(node);
    }

    result
}

/// Build the output JSON-LD node object for a single subject.
fn build_output_node(
    subj_key: &str,
    node_map: &HashMap<String, BTreeMap<String, Vec<Value>>>,
    frame_obj: &serde_json::Map<String, Value>,
    opts: &EmbedOptions,
    embedded_set: &mut HashSet<String>,
) -> Option<Value> {
    let props = node_map.get(subj_key)?;

    // Handle @embed policy.
    match opts.embed {
        EmbedMode::Never => {
            return Some(Value::Object({
                let mut m = Map::new();
                m.insert("@id".to_owned(), Value::String(subj_key.to_owned()));
                m
            }));
        }
        EmbedMode::Once => {
            if embedded_set.contains(subj_key) {
                return Some(Value::Object({
                    let mut m = Map::new();
                    m.insert("@id".to_owned(), Value::String(subj_key.to_owned()));
                    m
                }));
            }
            embedded_set.insert(subj_key.to_owned());
        }
        EmbedMode::Always => {}
    }

    let mut output = Map::new();
    output.insert("@id".to_owned(), Value::String(subj_key.to_owned()));

    // Copy properties — either all (non-explicit) or only those in the frame (@explicit).
    let frame_keys: HashSet<&str> = frame_obj
        .keys()
        .filter(|k| !k.starts_with('@'))
        .map(|k| k.as_str())
        .collect();

    // Iterate over stored properties; if @explicit only include frame-listed ones.
    for (pred_iri, values) in props {
        if opts.explicit && !frame_keys.contains(pred_iri.as_str()) {
            continue;
        }

        // Check if the frame has a nested frame for this predicate.
        let child_frame = frame_obj.get(pred_iri);

        let output_values: Vec<Value> = values
            .iter()
            .map(|v| {
                // Try to recurse into nested frame.
                if let Some(child_val) = child_frame
                    && let Some(child_obj) = child_val.as_object()
                    && !child_obj.is_empty()
                {
                    // Get the @id of the value if it's a node.
                    if let Some(id) = v
                        .as_object()
                        .and_then(|o| o.get("@id"))
                        .and_then(Value::as_str)
                        && let Some(embedded) =
                            build_output_node(id, node_map, child_obj, opts, embedded_set)
                    {
                        return embedded;
                    }
                }
                v.clone()
            })
            .collect();

        output.insert(pred_iri.clone(), Value::Array(output_values));
    }

    // Handle @default for absent frame properties when @omitDefault is false.
    if !opts.omit_default {
        for (frame_prop, frame_val) in frame_obj {
            if frame_prop.starts_with('@') {
                continue;
            }
            if output.contains_key(frame_prop) {
                continue;
            }
            // Check if the frame has a @default.
            if let Some(Value::Object(child_obj)) = Some(frame_val)
                && let Some(default_val) = child_obj.get("@default")
            {
                output.insert(frame_prop.clone(), Value::Array(vec![default_val.clone()]));
            }
        }
    }

    // Handle @reverse in frame: collect subjects whose predicate points to current.
    if let Some(rev_frame) = frame_obj.get("@reverse")
        && let Some(rev_obj) = rev_frame.as_object()
    {
        let mut rev_output = Map::new();
        for (pred_iri, child_frame_val) in rev_obj {
            let clean_pred = strip_brackets(pred_iri);
            let mut rev_nodes: Vec<Value> = Vec::new();
            for (other_subj, other_props) in node_map {
                if let Some(values) = other_props.get(clean_pred) {
                    let points_here = values.iter().any(|v| {
                        v.as_object()
                            .and_then(|o| o.get("@id"))
                            .and_then(Value::as_str)
                            == Some(subj_key)
                    });
                    if points_here {
                        let child_frame_obj = child_frame_val
                            .as_object()
                            .map(|o| o as &serde_json::Map<String, Value>);
                        let embedded = if let Some(cfo) = child_frame_obj {
                            build_output_node(other_subj, node_map, cfo, opts, embedded_set)
                        } else {
                            Some(Value::Object({
                                let mut m = Map::new();
                                m.insert("@id".to_owned(), Value::String(other_subj.clone()));
                                m
                            }))
                        };
                        if let Some(n) = embedded {
                            rev_nodes.push(n);
                        }
                    }
                }
            }
            if !rev_nodes.is_empty() {
                rev_output.insert(pred_iri.clone(), Value::Array(rev_nodes));
            }
        }
        if !rev_output.is_empty() {
            output.insert("@reverse".to_owned(), Value::Object(rev_output));
        }
    }

    Some(Value::Object(output))
}

// ─── Utility helpers ──────────────────────────────────────────────────────────

/// Extract the subject identifier string from an N-Triples subject term.
/// IRIs → bare IRI string; blank nodes → `_:xxx`.
fn nt_to_id_string(nt: &str) -> String {
    if nt.starts_with('<') && nt.ends_with('>') {
        nt[1..nt.len() - 1].to_owned()
    } else {
        nt.to_owned()
    }
}

/// Extract a bare IRI string from an N-Triples IRI term.
fn nt_to_iri_string(nt: &str) -> String {
    if nt.starts_with('<') && nt.ends_with('>') {
        nt[1..nt.len() - 1].to_owned()
    } else {
        nt.to_owned()
    }
}

/// Strip surrounding `<>` from an IRI string.
fn strip_brackets(s: &str) -> &str {
    if s.starts_with('<') && s.ends_with('>') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Convert an N-Triples term to a JSON-LD value object.
/// This mirrors `nt_term_to_jsonld_value` in `src/export.rs`.
fn nt_term_to_jsonld_value(nt: &str) -> Value {
    if nt.starts_with('<') && nt.ends_with('>') {
        let iri = &nt[1..nt.len() - 1];
        return serde_json::json!({"@id": iri});
    }
    if nt.starts_with("_:") {
        return serde_json::json!({"@id": nt});
    }
    // Literal
    if nt.starts_with('"') {
        let bytes = nt.as_bytes();
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                break;
            } else {
                i += 1;
            }
        }
        let raw_value = &nt[1..i];
        let value = raw_value
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t");
        let rest = if i + 1 < nt.len() { &nt[i + 1..] } else { "" };
        if let Some(dt_rest) = rest.strip_prefix("^^<") {
            let end = dt_rest.find('>').unwrap_or(dt_rest.len());
            let dt = &dt_rest[..end];
            return serde_json::json!({"@value": value, "@type": dt});
        } else if let Some(lang_rest) = rest.strip_prefix('@') {
            return serde_json::json!({"@value": value, "@language": lang_rest});
        } else {
            return serde_json::json!({"@value": value});
        }
    }
    serde_json::json!({"@value": nt})
}
