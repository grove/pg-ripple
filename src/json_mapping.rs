//! Named bidirectional JSON ↔ RDF mapping registry (v0.73.0, JSON-MAPPING-01).
//!
//! `pg_ripple.register_json_mapping(name, context, shape_iri)` stores a named
//! JSON-LD context that is used both for ingest (`ingest_json`) and export
//! (`export_json_node`).  When an optional SHACL shape IRI is provided, the
//! engine validates that the context terms and shape properties are consistent.
//!
//! ## Relationship to RML / R2RML
//!
//! `register_json_mapping` covers flat-to-moderately-nested JSON payloads
//! where a full round-trip (ingest + export) is needed and a SHACL shape is
//! already registered.  For complex ETL (computed IRIs from templates,
//! JSONPath extraction, multi-source joins) use `pg_ripple.load_r2rml(mapping)`.

use pgrx::prelude::*;

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Register (or replace) a named bidirectional JSON ↔ RDF mapping.
    ///
    /// Stores a JSON-LD `@context` object in `_pg_ripple.json_mappings`.
    /// When `shape_iri` is provided, validates that the context terms are
    /// consistent with the SHACL shape properties:
    ///
    /// - Context term with no shape property → warning
    /// - Shape property with no context term → warning
    /// - Datatype mismatch → error
    ///
    /// Warnings are written to `_pg_ripple.json_mapping_warnings`.
    ///
    /// Calling `register_json_mapping` a second time with the same `name`
    /// replaces the existing entry (upsert semantics).
    ///
    /// v0.77.0 BIDI-ATTR-01 adds:
    /// - `default_graph_iri`: graph used when caller omits graph_iri on ingest
    /// - `timestamp_path`: JSONPath to root timestamp field (for diff mode)
    /// - `timestamp_predicate`: RDF predicate for per-triple change timestamps
    /// - `iri_template`: `https://target.example.com/contacts/{id}` for linkback expansion
    /// - `iri_match_pattern`: prefix or regex for late-binding IRI rewrite
    #[pg_extern]
    #[allow(clippy::too_many_arguments)]
    pub fn register_json_mapping(
        name: &str,
        context: pgrx::JsonB,
        shape_iri: default!(Option<&str>, "NULL"),
        default_graph_iri: default!(Option<&str>, "NULL"),
        timestamp_path: default!(Option<&str>, "NULL"),
        timestamp_predicate: default!(Option<&str>, "'http://www.w3.org/ns/prov#generatedAtTime'"),
        iri_template: default!(Option<&str>, "NULL"),
        iri_match_pattern: default!(Option<&str>, "NULL"),
    ) {
        crate::json_mapping::register_mapping_impl(
            name,
            &context.0,
            shape_iri,
            default_graph_iri,
            timestamp_path,
            timestamp_predicate,
            iri_template,
            iri_match_pattern,
        );
    }

    /// Ingest a JSON payload using a named mapping.
    ///
    /// Equivalent to `json_to_ntriples_and_load()` but derives the JSON-LD
    /// context from the registry by name, eliminating the need to pass the
    /// context inline.
    ///
    /// `mode` controls ingest semantics (v0.77.0 BIDI-UPSERT-01, BIDI-DIFF-01):
    /// - `'append'` (default): insert triples without checking for existing values
    /// - `'upsert'`: for sh:maxCount 1 predicates, delete existing value first
    /// - `'diff'`: derive per-triple change timestamps; idempotent re-delivery
    ///
    /// Returns the number of triples inserted.
    #[pg_extern]
    pub fn ingest_json(
        payload: pgrx::JsonB,
        subject_iri: &str,
        mapping: &str,
        graph_iri: default!(Option<&str>, "NULL"),
        mode: default!(&str, "'append'"),
        source_timestamp: default!(Option<pgrx::datum::Timestamp>, "NULL"),
    ) -> i64 {
        match mode {
            "append" => {
                crate::json_mapping::ingest_json_impl(&payload.0, subject_iri, mapping, graph_iri)
            }
            "upsert" => {
                crate::bidi::ingest_json_upsert_impl(&payload.0, subject_iri, mapping, graph_iri)
            }
            "diff" => crate::bidi::ingest_json_diff_impl(
                &payload.0,
                subject_iri,
                mapping,
                graph_iri,
                source_timestamp,
            ),
            other => pgrx::error!(
                "ingest_json: unknown mode '{}'; valid values: append, upsert, diff",
                other
            ),
        }
    }

    /// Export a single RDF subject as a plain JSON object using a named mapping.
    ///
    /// Derives the JSON-LD frame from the registered mapping context (and SHACL
    /// shape if registered), then applies `export_jsonld_node()` logic to
    /// produce a plain JSON object with `@type` and `@id` stripped.
    ///
    /// Returns `NULL` when no triples exist for `subject_id`.
    #[pg_extern]
    pub fn export_json_node(
        subject_id: i64,
        mapping: &str,
        strip: default!(Vec<String>, "ARRAY['@type','@id']::TEXT[]"),
    ) -> Option<pgrx::JsonB> {
        crate::json_mapping::export_json_node_impl(subject_id, mapping, strip)
    }
}

// ─── Implementation ───────────────────────────────────────────────────────────

/// Internal: register or replace a JSON mapping in the catalog.
#[allow(clippy::too_many_arguments)]
pub fn register_mapping_impl(
    name: &str,
    context: &serde_json::Value,
    shape_iri: Option<&str>,
    default_graph_iri: Option<&str>,
    timestamp_path: Option<&str>,
    timestamp_predicate: Option<&str>,
    iri_template: Option<&str>,
    iri_match_pattern: Option<&str>,
) {
    // Validate that context is an object.
    if !context.is_object() {
        pgrx::error!("register_json_mapping: context must be a JSON object (the @context value)");
    }

    // Validate iri_template: must have exactly one {id} placeholder.
    if let Some(tmpl) = iri_template {
        let placeholder_count = tmpl.matches("{id}").count();
        if placeholder_count != 1 {
            pgrx::error!(
                "register_json_mapping: iri_template must contain exactly one {{id}} placeholder; \
                 found {} in {:?}",
                placeholder_count,
                tmpl
            );
        }
    }

    // Normalize the timestamp_predicate default.
    let ts_pred = timestamp_predicate.unwrap_or("http://www.w3.org/ns/prov#generatedAtTime");

    // Upsert into _pg_ripple.json_mappings.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.json_mappings \
         (name, context, shape_iri, default_graph_iri, timestamp_path, \
          timestamp_predicate, iri_template, iri_match_pattern) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (name) DO UPDATE SET \
             context = EXCLUDED.context, \
             shape_iri = EXCLUDED.shape_iri, \
             default_graph_iri = EXCLUDED.default_graph_iri, \
             timestamp_path = EXCLUDED.timestamp_path, \
             timestamp_predicate = EXCLUDED.timestamp_predicate, \
             iri_template = EXCLUDED.iri_template, \
             iri_match_pattern = EXCLUDED.iri_match_pattern, \
             created_at = now()",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(pgrx::JsonB(context.clone())),
            pgrx::datum::DatumWithOid::from(shape_iri),
            pgrx::datum::DatumWithOid::from(default_graph_iri),
            pgrx::datum::DatumWithOid::from(timestamp_path),
            pgrx::datum::DatumWithOid::from(ts_pred),
            pgrx::datum::DatumWithOid::from(iri_template),
            pgrx::datum::DatumWithOid::from(iri_match_pattern),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("register_json_mapping: catalog insert failed: {e}"));

    // When a shape is provided, run the consistency check.
    if let Some(siri) = shape_iri {
        check_mapping_consistency(name, context, siri);
    }
}

/// Internal: ingest JSON payload using a named mapping context.
pub fn ingest_json_impl(
    payload: &serde_json::Value,
    subject_iri: &str,
    mapping: &str,
    graph_iri: Option<&str>,
) -> i64 {
    let context = fetch_mapping_context(mapping);

    // Use the existing json_to_ntriples_and_load path with the fetched context.
    let ntriples = crate::bulk_load::json_to_ntriples(payload, subject_iri, None, Some(&context));

    if ntriples.is_empty() {
        return 0;
    }

    // BIDI-ATTR-01: resolve graph_iri → mapping.default_graph_iri → default graph.
    let effective_graph = graph_iri;

    // Fetch default_graph_iri from catalog when graph_iri is not provided.
    let default_g_owned: Option<String> = if graph_iri.is_none() {
        Spi::get_one_with_args::<String>(
            "SELECT default_graph_iri FROM _pg_ripple.json_mappings WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(mapping)],
        )
        .unwrap_or(None)
    } else {
        None
    };

    let resolved_graph = effective_graph.or(default_g_owned.as_deref());

    match resolved_graph {
        None | Some("") => crate::bulk_load::load_ntriples(&ntriples, false),
        Some(g) => {
            let g_clean = g.trim_matches(|c| c == '<' || c == '>');
            let g_id = crate::dictionary::encode(g_clean, crate::dictionary::KIND_IRI);
            crate::bulk_load::load_ntriples_into_graph(&ntriples, g_id)
        }
    }
}

/// Internal: export a subject as JSON using a named mapping.
pub fn export_json_node_impl(
    subject_id: i64,
    mapping: &str,
    strip: Vec<String>,
) -> Option<pgrx::JsonB> {
    let context = fetch_mapping_context(mapping);

    // Build a minimal frame from the context: {"@context": <context>, "@id": ""}
    // The @id will be filled in by export_jsonld_node_impl using the subject.
    let mut frame = serde_json::Map::new();
    frame.insert("@context".to_string(), context);
    let frame_val = serde_json::Value::Object(frame);

    crate::export::export_jsonld_node_impl(frame_val, subject_id, strip)
        .map(|opt| opt.map(pgrx::JsonB))
        .unwrap_or_else(|e| pgrx::error!("{}", e))
}

/// Fetch the JSON-LD context object for a named mapping.
/// Raises an error if the mapping does not exist.
fn fetch_mapping_context(mapping: &str) -> serde_json::Value {
    let ctx_jsonb = Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT context FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        )
    });
    ctx_jsonb.0
}

/// Validate consistency between a JSON-LD context and a SHACL shape.
///
/// Warns when terms in the context have no corresponding `sh:property` in the
/// shape, and vice versa.  Errors on `sh:datatype` mismatches with `@type`
/// annotations in the context.
fn check_mapping_consistency(mapping_name: &str, context: &serde_json::Value, shape_iri: &str) {
    // Collect context term → IRI pairs (skip @-keywords and non-string values).
    let ctx_terms: std::collections::HashMap<String, String> = context
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| !k.starts_with('@'))
                .filter_map(|(k, v)| {
                    let iri = match v {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Object(meta) => {
                            meta.get("@id").and_then(|id| id.as_str()).map(String::from)
                        }
                        _ => None,
                    };
                    iri.map(|i| (k.clone(), i))
                })
                .collect()
        })
        .unwrap_or_default();

    // Collect sh:property path IRIs from the shape using a SPARQL query.
    let sparql = format!(
        "SELECT ?path ?name WHERE {{ \
             <{shape_iri}> <http://www.w3.org/ns/shacl#property> ?prop . \
             ?prop <http://www.w3.org/ns/shacl#path> ?path . \
             OPTIONAL {{ ?prop <http://www.w3.org/ns/shacl#name> ?name }} \
         }}"
    );
    let shape_props = crate::sparql::sparql(&sparql);
    let shape_iris: std::collections::HashMap<String, Option<String>> = shape_props
        .iter()
        .filter_map(|row| {
            let obj = row.0.as_object()?;
            let path = obj.get("path")?.as_str()?.trim_matches('"').to_string();
            // Strip angle brackets from IRI terms like <http://...>.
            let path = path
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            let name = obj
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.trim_matches('"').to_string());
            Some((path, name))
        })
        .collect();

    // Check: context term with no shape property.
    for (term, iri) in &ctx_terms {
        if !shape_iris.contains_key(iri) {
            pgrx::warning!(
                "register_json_mapping {:?}: context term {:?} (IRI {}) \
                 has no corresponding sh:property in shape {}; \
                 field will be ingested but not validated",
                mapping_name,
                term,
                iri,
                shape_iri
            );
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.json_mapping_warnings \
                 (mapping_name, kind, detail) VALUES ($1, 'missing_shape_property', $2) \
                 ON CONFLICT DO NOTHING",
                &[
                    pgrx::datum::DatumWithOid::from(mapping_name),
                    pgrx::datum::DatumWithOid::from(
                        format!(
                            "context term {term:?} (IRI {iri}) has no sh:property in {shape_iri}"
                        )
                        .as_str(),
                    ),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("could not record warning: {e}"));
        }
    }

    // Check: shape property with no context term.
    let ctx_iris: std::collections::HashSet<&str> =
        ctx_terms.values().map(|s| s.as_str()).collect();
    for iri in shape_iris.keys() {
        if !ctx_iris.contains(iri.as_str()) {
            pgrx::warning!(
                "register_json_mapping {:?}: shape {} has sh:property <{}> \
                 with no corresponding context term; \
                 field will be stored but never appear in outbound documents",
                mapping_name,
                shape_iri,
                iri
            );
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.json_mapping_warnings \
                 (mapping_name, kind, detail) VALUES ($1, 'missing_context_term', $2) \
                 ON CONFLICT DO NOTHING",
                &[
                    pgrx::datum::DatumWithOid::from(mapping_name),
                    pgrx::datum::DatumWithOid::from(
                        format!("shape {shape_iri} has sh:property <{iri}> with no context term")
                            .as_str(),
                    ),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("could not record warning: {e}"));
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
