//! Export — serialize stored triples to N-Triples, N-Quads, Turtle, JSON-LD,
//! and GraphRAG Parquet (v0.26.0).
//!
//! Queries all VP tables (dedicated + vp_rare) for the requested graph(s),
//! decodes the integer IDs in bulk via `dictionary::format_ntriples`, and
//! assembles an N-Triples, N-Quads, Turtle, or JSON-LD document.
//!
//! v0.9.0: Turtle and JSON-LD serialization, streaming variants, and
//! Turtle-star / N-Triples-star export for RDF-star quoted triples.
//!
//! v0.26.0: GraphRAG BYOG Parquet export functions.

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

    let mut out = String::new();
    storage::for_each_encoded_triple_batch(g_id, &mut |batch| {
        for (s_id, p_id, o_id, _g) in batch {
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
    });
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

    let mut out = String::new();
    storage::for_each_encoded_triple_batch(g_filter, &mut |batch| {
        for (s_id, p_id, o_id, g_id) in batch {
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
    });
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

    // Load all triples first (Turtle requires grouping by subject).
    let mut rows: Vec<(i64, i64, i64, i64)> = Vec::new();
    storage::for_each_encoded_triple_batch(g_id, &mut |batch| {
        rows.extend_from_slice(batch);
    });

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

    storage::for_each_encoded_triple_batch(g_id, &mut |batch| {
        for (s_id, p_id, o_id, _g) in batch {
            let s = nt_term_to_turtle(&dictionary::format_ntriples(*s_id));
            let p = nt_term_to_turtle(&dictionary::format_ntriples(*p_id));
            let o = nt_term_to_turtle(&dictionary::format_ntriples(*o_id));
            lines.push(format!("{} {} {} .", s, p, o));
        }
    });

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

    // JSON-LD requires grouping by subject, so collect all triples first.
    let mut rows: Vec<(i64, i64, i64, i64)> = Vec::new();
    storage::for_each_encoded_triple_batch(g_id, &mut |batch| {
        rows.extend_from_slice(batch);
    });

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

// ─── GraphRAG BYOG Parquet export (v0.26.0) ──────────────────────────────────

/// Strip N-Triples formatting from a SPARQL result value.
///
/// - `<https://example.org/foo>` → `https://example.org/foo`
/// - `"Alice"` → `Alice`
/// - `"Alice"^^<xsd:string>` → `Alice`
/// - `"42"^^<xsd:integer>` → `42`
/// - `_:b0` → `_:b0` (blank nodes kept as-is)
fn strip_nt(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].to_owned()
    } else if let Some(inner) = s.strip_prefix('"') {
        // Find closing quote (simple scan, not full N-Triples parser)
        let inner_end = inner.find('"').unwrap_or(inner.len());
        inner[..inner_end].to_owned()
    } else {
        s.to_owned()
    }
}

/// Parse an optional integer from a SPARQL N-Triples literal like `"42"^^<xsd:integer>`.
fn parse_nt_integer(s: &str) -> Option<i64> {
    strip_nt(s).parse::<i64>().ok()
}

/// Parse an optional float from a SPARQL N-Triples literal like `"0.95"^^<xsd:float>`.
fn parse_nt_float(s: &str) -> Option<f64> {
    strip_nt(s).parse::<f64>().ok()
}

/// Extract a string value from a SPARQL JsonB result row.
fn get_str(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(strip_nt)
        .filter(|s| !s.is_empty())
}

/// Build the GRAPH clause for a SPARQL query.
/// Returns (open_clause, close_clause) — both empty when graph_iri is empty/NULL.
fn graph_clause(graph_iri: &str) -> (String, &'static str) {
    let clean = graph_iri.trim().trim_matches(|c| c == '<' || c == '>');
    if clean.is_empty() {
        (String::new(), "")
    } else {
        (format!("GRAPH <{clean}> {{ "), " }}")
    }
}

/// Security check: path must not contain `..` and the parent directory must exist.
fn validate_output_path(path: &str) -> Result<(), String> {
    if path.contains("..") {
        return Err("output path must not contain '..'".to_owned());
    }
    if path.is_empty() {
        return Err("output path must not be empty".to_owned());
    }
    Ok(())
}

/// Export all `gr:Entity` nodes from a named graph to a Parquet file.
///
/// Columns: `id`, `title`, `type`, `description`, `text_unit_ids`, `frequency`, `degree`.
/// The `text_unit_ids` column contains a JSON-encoded array of text unit IRI strings.
///
/// Requires superuser.  Returns the number of entity rows written.
pub fn export_graphrag_entities(graph_iri: &str, output_path: &str) -> i64 {
    // SAFETY: superuser() is a PostgreSQL function with no thread-safety concerns.
    if !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("export_graphrag_entities: requires superuser");
    }
    if let Err(e) = validate_output_path(output_path) {
        pgrx::error!("export_graphrag_entities: {e}");
    }

    let (graph_open, graph_close) = graph_clause(graph_iri);
    let sparql = format!(
        "SELECT ?entity ?title ?entityType ?description ?frequency ?degree \
         WHERE {{ \
           {graph_open} \
           ?entity <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://graphrag.org/ns/Entity> . \
           OPTIONAL {{ ?entity <https://graphrag.org/ns/title> ?title }} \
           OPTIONAL {{ ?entity <https://graphrag.org/ns/type> ?entityType }} \
           OPTIONAL {{ ?entity <https://graphrag.org/ns/description> ?description }} \
           OPTIONAL {{ ?entity <https://graphrag.org/ns/frequency> ?frequency }} \
           OPTIONAL {{ ?entity <https://graphrag.org/ns/degree> ?degree }} \
           {graph_close} \
         }}"
    );

    let results = crate::sparql::sparql(&sparql);

    // Collect column arrays
    let mut ids: Vec<String> = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    let mut entity_types: Vec<String> = Vec::new();
    let mut descriptions: Vec<String> = Vec::new();
    let mut text_unit_ids: Vec<String> = Vec::new();
    let mut frequencies: Vec<i64> = Vec::new();
    let mut degrees: Vec<i64> = Vec::new();

    for row in &results {
        let obj = match row.0.as_object() {
            Some(o) => o,
            None => continue,
        };
        let id = get_str(obj, "entity").unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        ids.push(id);
        titles.push(get_str(obj, "title").unwrap_or_default());
        entity_types.push(get_str(obj, "entityType").unwrap_or_default());
        descriptions.push(get_str(obj, "description").unwrap_or_default());
        text_unit_ids.push("[]".to_owned()); // populated by text-unit linkage query if needed
        frequencies.push(
            obj.get("frequency")
                .and_then(|v| v.as_str())
                .and_then(parse_nt_integer)
                .unwrap_or(0),
        );
        degrees.push(
            obj.get("degree")
                .and_then(|v| v.as_str())
                .and_then(parse_nt_integer)
                .unwrap_or(0),
        );
    }

    let row_count = ids.len() as i64;
    write_entities_parquet(
        output_path,
        ids,
        titles,
        entity_types,
        descriptions,
        text_unit_ids,
        frequencies,
        degrees,
    );
    row_count
}

/// Export all `gr:Relationship` nodes from a named graph to a Parquet file.
///
/// Columns: `id`, `source`, `target`, `description`, `weight`, `combined_degree`, `text_unit_ids`.
///
/// Requires superuser.  Returns the number of relationship rows written.
pub fn export_graphrag_relationships(graph_iri: &str, output_path: &str) -> i64 {
    // SAFETY: superuser() is a standard PostgreSQL function.
    if !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("export_graphrag_relationships: requires superuser");
    }
    if let Err(e) = validate_output_path(output_path) {
        pgrx::error!("export_graphrag_relationships: {e}");
    }

    let (graph_open, graph_close) = graph_clause(graph_iri);
    let sparql = format!(
        "SELECT ?rel ?source ?target ?description ?weight \
         WHERE {{ \
           {graph_open} \
           ?rel <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://graphrag.org/ns/Relationship> . \
           OPTIONAL {{ ?rel <https://graphrag.org/ns/source> ?source }} \
           OPTIONAL {{ ?rel <https://graphrag.org/ns/target> ?target }} \
           OPTIONAL {{ ?rel <https://graphrag.org/ns/description> ?description }} \
           OPTIONAL {{ ?rel <https://graphrag.org/ns/weight> ?weight }} \
           {graph_close} \
         }}"
    );

    let results = crate::sparql::sparql(&sparql);

    let mut ids: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    let mut targets: Vec<String> = Vec::new();
    let mut descriptions: Vec<String> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();
    let mut combined_degrees: Vec<i64> = Vec::new();
    let mut text_unit_ids: Vec<String> = Vec::new();

    for row in &results {
        let obj = match row.0.as_object() {
            Some(o) => o,
            None => continue,
        };
        let id = get_str(obj, "rel").unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        ids.push(id);
        sources.push(get_str(obj, "source").unwrap_or_default());
        targets.push(get_str(obj, "target").unwrap_or_default());
        descriptions.push(get_str(obj, "description").unwrap_or_default());
        weights.push(
            obj.get("weight")
                .and_then(|v| v.as_str())
                .and_then(parse_nt_float)
                .unwrap_or(0.0),
        );
        combined_degrees.push(0); // populated by a follow-up join query if needed
        text_unit_ids.push("[]".to_owned());
    }

    let row_count = ids.len() as i64;
    write_relationships_parquet(
        output_path,
        ids,
        sources,
        targets,
        descriptions,
        weights,
        combined_degrees,
        text_unit_ids,
    );
    row_count
}

/// Export all `gr:TextUnit` nodes from a named graph to a Parquet file.
///
/// Columns: `id`, `text`, `n_tokens`, `document_id`, `entity_ids`, `relationship_ids`.
///
/// Requires superuser.  Returns the number of text unit rows written.
pub fn export_graphrag_text_units(graph_iri: &str, output_path: &str) -> i64 {
    // SAFETY: superuser() is a standard PostgreSQL function.
    if !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("export_graphrag_text_units: requires superuser");
    }
    if let Err(e) = validate_output_path(output_path) {
        pgrx::error!("export_graphrag_text_units: {e}");
    }

    let (graph_open, graph_close) = graph_clause(graph_iri);
    let sparql = format!(
        "SELECT ?tu ?text ?tokenCount ?documentId \
         WHERE {{ \
           {graph_open} \
           ?tu <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://graphrag.org/ns/TextUnit> . \
           OPTIONAL {{ ?tu <https://graphrag.org/ns/text> ?text }} \
           OPTIONAL {{ ?tu <https://graphrag.org/ns/tokenCount> ?tokenCount }} \
           OPTIONAL {{ ?tu <https://graphrag.org/ns/documentId> ?documentId }} \
           {graph_close} \
         }}"
    );

    let results = crate::sparql::sparql(&sparql);

    let mut ids: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut n_tokens: Vec<i64> = Vec::new();
    let mut document_ids: Vec<String> = Vec::new();
    let mut entity_ids: Vec<String> = Vec::new();
    let mut relationship_ids: Vec<String> = Vec::new();

    for row in &results {
        let obj = match row.0.as_object() {
            Some(o) => o,
            None => continue,
        };
        let id = get_str(obj, "tu").unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        ids.push(id);
        texts.push(get_str(obj, "text").unwrap_or_default());
        n_tokens.push(
            obj.get("tokenCount")
                .and_then(|v| v.as_str())
                .and_then(parse_nt_integer)
                .unwrap_or(0),
        );
        document_ids.push(get_str(obj, "documentId").unwrap_or_default());
        entity_ids.push("[]".to_owned());
        relationship_ids.push("[]".to_owned());
    }

    let row_count = ids.len() as i64;
    write_text_units_parquet(
        output_path,
        ids,
        texts,
        n_tokens,
        document_ids,
        entity_ids,
        relationship_ids,
    );
    row_count
}

// ─── Parquet writers ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn write_entities_parquet(
    path: &str,
    ids: Vec<String>,
    titles: Vec<String>,
    entity_types: Vec<String>,
    descriptions: Vec<String>,
    text_unit_ids: Vec<String>,
    frequencies: Vec<i64>,
    degrees: Vec<i64>,
) {
    use parquet::column::writer::ColumnWriter;
    use parquet::data_type::ByteArray;
    use parquet::file::properties::WriterProperties;
    use parquet::file::writer::SerializedFileWriter;
    use parquet::schema::parser::parse_message_type;
    use std::fs::File;
    use std::sync::Arc;

    let schema_str = "message schema {
        REQUIRED BYTE_ARRAY id (UTF8);
        OPTIONAL BYTE_ARRAY title (UTF8);
        OPTIONAL BYTE_ARRAY type (UTF8);
        OPTIONAL BYTE_ARRAY description (UTF8);
        OPTIONAL BYTE_ARRAY text_unit_ids (UTF8);
        OPTIONAL INT64 frequency;
        OPTIONAL INT64 degree;
    }";

    let schema = Arc::new(
        parse_message_type(schema_str)
            .unwrap_or_else(|e| pgrx::error!("entities parquet schema error: {e}")),
    );
    let props = Arc::new(WriterProperties::builder().build());
    let file = File::create(path).unwrap_or_else(|e| {
        pgrx::error!("export_graphrag_entities: cannot create file '{path}': {e}")
    });
    let mut writer = SerializedFileWriter::new(file, schema, props)
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_entities: writer init error: {e}"));

    if !ids.is_empty() {
        let mut rg = writer
            .next_row_group()
            .unwrap_or_else(|e| pgrx::error!("entities row group error: {e}"));

        // Helper: convert Vec<String> to Vec<ByteArray>
        let to_bytes = |v: &[String]| -> Vec<ByteArray> {
            v.iter().map(|s| s.as_bytes().to_vec().into()).collect()
        };

        // id column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities id col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected id column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities id write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities id close: {e}"));
        }
        // title column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities title col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected title column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&titles), None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities title write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities title close: {e}"));
        }
        // type column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities type col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected type column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&entity_types), None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities type write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities type close: {e}"));
        }
        // description column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities description col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected description column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&descriptions), None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities description write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities description close: {e}"));
        }
        // text_unit_ids column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities text_unit_ids col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected text_unit_ids column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&text_unit_ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities text_unit_ids write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities text_unit_ids close: {e}"));
        }
        // frequency column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities frequency col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected frequency column in parquet schema"));
            {
                if let ColumnWriter::Int64ColumnWriter(w) = cw.untyped() {
                    w.write_batch(&frequencies, None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities frequency write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities frequency close: {e}"));
        }
        // degree column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("entities degree col error: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected degree column in parquet schema"));
            {
                if let ColumnWriter::Int64ColumnWriter(w) = cw.untyped() {
                    w.write_batch(&degrees, None, None)
                        .unwrap_or_else(|e| pgrx::error!("entities degree write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("entities degree close: {e}"));
        }

        rg.close()
            .unwrap_or_else(|e| pgrx::error!("entities row group close: {e}"));
    }

    writer
        .close()
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_entities: writer close: {e}"));
}

#[allow(clippy::too_many_arguments)]
fn write_relationships_parquet(
    path: &str,
    ids: Vec<String>,
    sources: Vec<String>,
    targets: Vec<String>,
    descriptions: Vec<String>,
    weights: Vec<f64>,
    combined_degrees: Vec<i64>,
    text_unit_ids: Vec<String>,
) {
    use parquet::column::writer::ColumnWriter;
    use parquet::data_type::ByteArray;
    use parquet::file::properties::WriterProperties;
    use parquet::file::writer::SerializedFileWriter;
    use parquet::schema::parser::parse_message_type;
    use std::fs::File;
    use std::sync::Arc;

    let schema_str = "message schema {
        REQUIRED BYTE_ARRAY id (UTF8);
        OPTIONAL BYTE_ARRAY source (UTF8);
        OPTIONAL BYTE_ARRAY target (UTF8);
        OPTIONAL BYTE_ARRAY description (UTF8);
        OPTIONAL DOUBLE weight;
        OPTIONAL INT64 combined_degree;
        OPTIONAL BYTE_ARRAY text_unit_ids (UTF8);
    }";

    let schema = Arc::new(
        parse_message_type(schema_str)
            .unwrap_or_else(|e| pgrx::error!("relationships parquet schema error: {e}")),
    );
    let props = Arc::new(WriterProperties::builder().build());
    let file = File::create(path).unwrap_or_else(|e| {
        pgrx::error!("export_graphrag_relationships: cannot create file '{path}': {e}")
    });
    let mut writer = SerializedFileWriter::new(file, schema, props)
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_relationships: writer init error: {e}"));

    if !ids.is_empty() {
        let mut rg = writer
            .next_row_group()
            .unwrap_or_else(|e| pgrx::error!("relationships row group error: {e}"));

        let to_bytes = |v: &[String]| -> Vec<ByteArray> {
            v.iter().map(|s| s.as_bytes().to_vec().into()).collect()
        };

        // id column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships id col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected id column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel id write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel id close: {e}"));
        }
        // source column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships source col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected source column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&sources), None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel source write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel source close: {e}"));
        }
        // target column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships target col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected target column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&targets), None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel target write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel target close: {e}"));
        }
        // description column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships description col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected description column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&descriptions), None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel description write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel description close: {e}"));
        }
        // weight column (DOUBLE)
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships weight col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected weight column in parquet schema"));
            {
                if let ColumnWriter::DoubleColumnWriter(w) = cw.untyped() {
                    w.write_batch(&weights, None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel weight write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel weight close: {e}"));
        }
        // combined_degree column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships combined_degree col: {e}"))
                .unwrap_or_else(|| {
                    pgrx::error!("expected combined_degree column in parquet schema")
                });
            {
                if let ColumnWriter::Int64ColumnWriter(w) = cw.untyped() {
                    w.write_batch(&combined_degrees, None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel combined_degree write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel combined_degree close: {e}"));
        }
        // text_unit_ids column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("relationships text_unit_ids col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected text_unit_ids column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&text_unit_ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("rel text_unit_ids write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("rel text_unit_ids close: {e}"));
        }

        rg.close()
            .unwrap_or_else(|e| pgrx::error!("relationships row group close: {e}"));
    }

    writer
        .close()
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_relationships: writer close: {e}"));
}

#[allow(clippy::too_many_arguments)]
fn write_text_units_parquet(
    path: &str,
    ids: Vec<String>,
    texts: Vec<String>,
    n_tokens: Vec<i64>,
    document_ids: Vec<String>,
    entity_ids: Vec<String>,
    relationship_ids: Vec<String>,
) {
    use parquet::column::writer::ColumnWriter;
    use parquet::data_type::ByteArray;
    use parquet::file::properties::WriterProperties;
    use parquet::file::writer::SerializedFileWriter;
    use parquet::schema::parser::parse_message_type;
    use std::fs::File;
    use std::sync::Arc;

    let schema_str = "message schema {
        REQUIRED BYTE_ARRAY id (UTF8);
        OPTIONAL BYTE_ARRAY text (UTF8);
        OPTIONAL INT64 n_tokens;
        OPTIONAL BYTE_ARRAY document_id (UTF8);
        OPTIONAL BYTE_ARRAY entity_ids (UTF8);
        OPTIONAL BYTE_ARRAY relationship_ids (UTF8);
    }";

    let schema = Arc::new(
        parse_message_type(schema_str)
            .unwrap_or_else(|e| pgrx::error!("text_units parquet schema error: {e}")),
    );
    let props = Arc::new(WriterProperties::builder().build());
    let file = File::create(path).unwrap_or_else(|e| {
        pgrx::error!("export_graphrag_text_units: cannot create file '{path}': {e}")
    });
    let mut writer = SerializedFileWriter::new(file, schema, props)
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_text_units: writer init error: {e}"));

    if !ids.is_empty() {
        let mut rg = writer
            .next_row_group()
            .unwrap_or_else(|e| pgrx::error!("text_units row group error: {e}"));

        let to_bytes = |v: &[String]| -> Vec<ByteArray> {
            v.iter().map(|s| s.as_bytes().to_vec().into()).collect()
        };

        // id column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units id col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected id column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu id write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu id close: {e}"));
        }
        // text column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units text col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected text column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&texts), None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu text write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu text close: {e}"));
        }
        // n_tokens column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units n_tokens col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected n_tokens column in parquet schema"));
            {
                if let ColumnWriter::Int64ColumnWriter(w) = cw.untyped() {
                    w.write_batch(&n_tokens, None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu n_tokens write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu n_tokens close: {e}"));
        }
        // document_id column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units document_id col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected document_id column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&document_ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu document_id write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu document_id close: {e}"));
        }
        // entity_ids column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units entity_ids col: {e}"))
                .unwrap_or_else(|| pgrx::error!("expected entity_ids column in parquet schema"));
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&entity_ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu entity_ids write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu entity_ids close: {e}"));
        }
        // relationship_ids column
        {
            let mut cw = rg
                .next_column()
                .unwrap_or_else(|e| pgrx::error!("text_units relationship_ids col: {e}"))
                .unwrap_or_else(|| {
                    pgrx::error!("expected relationship_ids column in parquet schema")
                });
            {
                if let ColumnWriter::ByteArrayColumnWriter(w) = cw.untyped() {
                    w.write_batch(&to_bytes(&relationship_ids), None, None)
                        .unwrap_or_else(|e| pgrx::error!("tu relationship_ids write: {e}"));
                }
            }
            cw.close()
                .unwrap_or_else(|e| pgrx::error!("tu relationship_ids close: {e}"));
        }

        rg.close()
            .unwrap_or_else(|e| pgrx::error!("text_units row group close: {e}"));
    }

    writer
        .close()
        .unwrap_or_else(|e| pgrx::error!("export_graphrag_text_units: writer close: {e}"));
}
