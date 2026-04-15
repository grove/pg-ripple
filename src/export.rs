//! Export — serialize stored triples to N-Triples, N-Quads, Turtle, and JSON-LD.
//!
//! Queries all VP tables (dedicated + vp_rare) for the requested graph(s),
//! decodes the integer IDs in bulk via `dictionary::format_ntriples`, and
//! assembles an N-Triples, N-Quads, Turtle, or JSON-LD document.
//!
//! v0.9.0: Turtle and JSON-LD serialization, streaming variants, and
//! Turtle-star / N-Triples-star export for RDF-star quoted triples.

use crate::{dictionary, storage};
use std::collections::BTreeMap;

// ─── N-Triples ────────────────────────────────────────────────────────────────

/// Export triples as N-Triples text.
///
/// If `graph` is `None`, export the default graph (g = 0).
/// N-Triples format does not include graph information.
pub fn export_ntriples(graph: Option<&str>) -> String {
    let g_id: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => Some(0), // default graph
    };

    let rows = storage::all_encoded_triples(g_id);

    let mut out = String::with_capacity(rows.len() * 80);
    for (s_id, p_id, o_id, _g) in &rows {
        let s = dictionary::format_ntriples(*s_id);
        let p = dictionary::format_ntriples(*p_id);
        let o = dictionary::format_ntriples(*o_id);
        out.push_str(&s);
        out.push(' ');
        out.push_str(&p);
        out.push(' ');
        out.push_str(&o);
        out.push_str(" .\n");
    }
    out
}

// ─── N-Quads ─────────────────────────────────────────────────────────────────

/// Export triples as N-Quads text.
///
/// If `graph` is `None`, all graphs are exported.  A graph-column value of 0
/// (default graph) is omitted from the quad line (yielding a triple-like line).
/// A named-graph value is included as the fourth field.
pub fn export_nquads(graph: Option<&str>) -> String {
    let g_filter: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => None, // all graphs
    };

    let rows = storage::all_encoded_triples(g_filter);

    let mut out = String::with_capacity(rows.len() * 100);
    for (s_id, p_id, o_id, g_id) in &rows {
        let s = dictionary::format_ntriples(*s_id);
        let p = dictionary::format_ntriples(*p_id);
        let o = dictionary::format_ntriples(*o_id);
        out.push_str(&s);
        out.push(' ');
        out.push_str(&p);
        out.push(' ');
        out.push_str(&o);
        if *g_id > 0 {
            out.push(' ');
            out.push_str(&dictionary::format_ntriples(*g_id));
        }
        out.push_str(" .\n");
    }
    out
}

// ─── Turtle serialization ─────────────────────────────────────────────────────

/// Convert an N-Triples–formatted term to Turtle format.
///
/// IRIs: `<iri>` — unchanged
/// Blank nodes: `_:b<id>` — unchanged
/// Literals: `"value"`, `"value"^^<dt>`, `"value"@lang` — escape for Turtle
/// Quoted triples: `<< s p o >>` — Turtle-star notation (unchanged)
fn nt_term_to_turtle(nt: &str) -> String {
    // Quoted triple — pass through (Turtle-star syntax is identical to N-Triples-star)
    if nt.starts_with("<<") {
        return nt.to_owned();
    }
    // IRI or blank node — pass through unchanged
    if nt.starts_with('<') || nt.starts_with("_:") {
        return nt.to_owned();
    }
    // Literal — already in N-Triples form which is valid Turtle
    nt.to_owned()
}

/// Emit Turtle prefix declarations from the prefix registry.
fn emit_turtle_prefixes() -> String {
    let rows: Vec<(String, String)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT prefix, expansion FROM _pg_ripple.prefixes ORDER BY prefix",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("export_turtle prefix SPI error: {e}"))
            .filter_map(|row| {
                let prefix: String = row.get::<String>(1).ok().flatten()?;
                let expansion: String = row.get::<String>(2).ok().flatten()?;
                Some((prefix, expansion))
            })
            .collect()
    });

    let mut out = String::new();
    for (prefix, expansion) in &rows {
        out.push_str(&format!("@prefix {}: <{}> .\n", prefix, expansion));
    }
    if !rows.is_empty() {
        out.push('\n');
    }
    out
}

/// Export triples as Turtle text.
///
/// Groups triples by subject, then by predicate, emitting compact Turtle blocks.
/// Includes all `@prefix` declarations from the prefix registry.
/// RDF-star quoted triples are serialized in Turtle-star `<< s p o >>` notation.
///
/// If `graph` is `None`, export the default graph (g = 0).
pub fn export_turtle(graph: Option<&str>) -> String {
    let g_id: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => Some(0),
    };

    let rows = storage::all_encoded_triples(g_id);

    // Group: subject → predicate → [object]
    let mut subjects: BTreeMap<i64, BTreeMap<i64, Vec<i64>>> = BTreeMap::new();
    for (s_id, p_id, o_id, _g) in &rows {
        subjects
            .entry(*s_id)
            .or_default()
            .entry(*p_id)
            .or_default()
            .push(*o_id);
    }

    let mut out = emit_turtle_prefixes();

    for (s_id, predicates) in &subjects {
        let s = nt_term_to_turtle(&dictionary::format_ntriples(*s_id));
        let pred_count = predicates.len();
        let mut pred_idx = 0;
        out.push_str(&s);
        out.push('\n');

        for (p_id, objects) in predicates {
            pred_idx += 1;
            let p = nt_term_to_turtle(&dictionary::format_ntriples(*p_id));
            let obj_count = objects.len();
            out.push_str(&format!("    {} ", p));
            for (obj_idx, o_id) in objects.iter().enumerate() {
                let o = nt_term_to_turtle(&dictionary::format_ntriples(*o_id));
                out.push_str(&o);
                if obj_idx + 1 < obj_count {
                    out.push_str(" ,\n        ");
                }
            }
            if pred_idx < pred_count {
                out.push_str(" ;\n");
            } else {
                out.push_str(" .\n");
            }
        }
        out.push('\n');
    }

    out
}

/// Streaming Turtle export — yields one `TEXT` line per triple.
///
/// Returns triples in `subject predicate object .` form (flat, one-triple-per-line
/// Turtle).  This avoids buffering the full document in memory and is suitable
/// for large graphs.  Prefix declarations are yielded first.
pub fn export_turtle_stream(graph: Option<&str>) -> Vec<String> {
    let g_id: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => Some(0),
    };

    let prefix_block = emit_turtle_prefixes();
    let mut lines: Vec<String> = Vec::new();
    if !prefix_block.is_empty() {
        for line in prefix_block.lines() {
            lines.push(line.to_owned());
        }
    }

    let rows = storage::all_encoded_triples(g_id);
    for (s_id, p_id, o_id, _g) in &rows {
        let s = nt_term_to_turtle(&dictionary::format_ntriples(*s_id));
        let p = nt_term_to_turtle(&dictionary::format_ntriples(*p_id));
        let o = nt_term_to_turtle(&dictionary::format_ntriples(*o_id));
        lines.push(format!("{} {} {} .", s, p, o));
    }

    lines
}

// ─── JSON-LD serialization ────────────────────────────────────────────────────

/// Convert an N-Triples term string to a JSON-LD value node.
fn nt_term_to_jsonld_value(nt: &str) -> serde_json::Value {
    if nt.starts_with("<<") {
        // Quoted triple: represent as a JSON-LD value string (RDF-star is not
        // yet in the JSON-LD spec; emit the N-Triples-star form as a string).
        return serde_json::json!({"@value": nt, "@type": "rdf:Statement"});
    }
    if nt.starts_with('<') && nt.ends_with('>') {
        let iri = &nt[1..nt.len() - 1];
        return serde_json::json!({"@id": iri});
    }
    if nt.starts_with("_:") {
        return serde_json::json!({"@id": nt});
    }
    // Literal: parse "value"^^<dt> or "value"@lang or "value"
    if nt.starts_with('"') {
        // Find end of quoted string
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

/// Export triples as JSON-LD (expanded form).
///
/// Returns a JSON-LD document as `[{"@id": "...", "predIRI": [...], ...}, ...]`.
/// This is the JSON-LD "expanded" form — suitable for use in REST APIs and
/// Linked Data Platform contexts.
///
/// If `graph` is `None`, export the default graph (g = 0).
pub fn export_jsonld(graph: Option<&str>) -> serde_json::Value {
    let g_id: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => Some(0),
    };

    let rows = storage::all_encoded_triples(g_id);

    // Group: subject_nt → predicate_nt → [object_value]
    let mut subjects: BTreeMap<String, BTreeMap<String, Vec<serde_json::Value>>> = BTreeMap::new();

    for (s_id, p_id, o_id, _g) in &rows {
        let s = dictionary::format_ntriples(*s_id);
        let p = dictionary::format_ntriples(*p_id);
        let o_val = nt_term_to_jsonld_value(&dictionary::format_ntriples(*o_id));
        subjects
            .entry(s)
            .or_default()
            .entry(p)
            .or_default()
            .push(o_val);
    }

    let mut array: Vec<serde_json::Value> = Vec::with_capacity(subjects.len());
    for (s_nt, predicates) in subjects {
        let mut node = serde_json::Map::new();
        // @id: strip angle brackets from IRI or use _:b... for blank
        if s_nt.starts_with('<') && s_nt.ends_with('>') {
            node.insert("@id".to_owned(), serde_json::json!(s_nt[1..s_nt.len() - 1]));
        } else {
            node.insert("@id".to_owned(), serde_json::json!(s_nt));
        }
        for (p_nt, objects) in predicates {
            // Strip angle brackets from predicate IRI
            let p_key = if p_nt.starts_with('<') && p_nt.ends_with('>') {
                p_nt[1..p_nt.len() - 1].to_owned()
            } else {
                p_nt
            };
            node.insert(p_key, serde_json::Value::Array(objects));
        }
        array.push(serde_json::Value::Object(node));
    }

    serde_json::Value::Array(array)
}

/// Streaming JSON-LD export — yields one `TEXT` line per subject-block (NDJSON).
///
/// Each line is a complete JSON object for one subject.
pub fn export_jsonld_stream(graph: Option<&str>) -> Vec<String> {
    let doc = export_jsonld(graph);
    if let serde_json::Value::Array(nodes) = doc {
        nodes
            .iter()
            .map(|n| serde_json::to_string(n).unwrap_or_else(|_| "{}".to_owned()))
            .collect()
    } else {
        vec![]
    }
}

// ─── CONSTRUCT / DESCRIBE result serialization ────────────────────────────────

/// Serialize a list of `(s, p, o)` triple rows (N-Triples format) to Turtle text.
///
/// Used by `sparql_construct_turtle` and `sparql_describe_turtle` to convert
/// the JSONB result rows from the SPARQL engine into a Turtle document.
pub fn triples_to_turtle(triples: &[(String, String, String)]) -> String {
    let mut subjects: BTreeMap<&str, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    for (s, p, o) in triples {
        subjects
            .entry(s.as_str())
            .or_default()
            .entry(p.as_str())
            .or_default()
            .push(o.as_str());
    }

    let mut out = String::new();
    for (s, predicates) in &subjects {
        let pred_count = predicates.len();
        let mut pred_idx = 0;
        out.push_str(s);
        out.push('\n');
        for (p, objects) in predicates {
            pred_idx += 1;
            let obj_count = objects.len();
            out.push_str(&format!("    {} ", p));
            for (obj_idx, o) in objects.iter().enumerate() {
                out.push_str(o);
                if obj_idx + 1 < obj_count {
                    out.push_str(" ,\n        ");
                }
            }
            if pred_idx < pred_count {
                out.push_str(" ;\n");
            } else {
                out.push_str(" .\n");
            }
        }
        out.push('\n');
    }
    out
}

/// Serialize a list of `(s, p, o)` triple rows (N-Triples format) to JSON-LD.
///
/// Returns a `serde_json::Value` array (JSON-LD expanded form).
pub fn triples_to_jsonld(triples: &[(String, String, String)]) -> serde_json::Value {
    let mut subjects: BTreeMap<&str, BTreeMap<&str, Vec<serde_json::Value>>> = BTreeMap::new();
    for (s, p, o) in triples {
        let o_val = nt_term_to_jsonld_value(o.as_str());
        subjects
            .entry(s.as_str())
            .or_default()
            .entry(p.as_str())
            .or_default()
            .push(o_val);
    }

    let mut array: Vec<serde_json::Value> = Vec::with_capacity(subjects.len());
    for (s_nt, predicates) in subjects {
        let mut node = serde_json::Map::new();
        if s_nt.starts_with('<') && s_nt.ends_with('>') {
            node.insert("@id".to_owned(), serde_json::json!(s_nt[1..s_nt.len() - 1]));
        } else {
            node.insert("@id".to_owned(), serde_json::json!(s_nt));
        }
        for (p_nt, objects) in predicates {
            let p_key = if p_nt.starts_with('<') && p_nt.ends_with('>') {
                p_nt[1..p_nt.len() - 1].to_owned()
            } else {
                p_nt.to_owned()
            };
            node.insert(p_key, serde_json::Value::Array(objects));
        }
        array.push(serde_json::Value::Object(node));
    }

    serde_json::Value::Array(array)
}
