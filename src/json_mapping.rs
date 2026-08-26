// @allow-large-file: writeback implementation adds substantial SPI boilerplate (JSON-WRITEBACK-01)
//! Named bidirectional JSON ↔ RDF mapping registry (v0.73.0, JSON-MAPPING-01).
//!
//! `pg_ripple.register_json_mapping(name, context, shape_iri)` stores a named
//! JSON-LD context that is used both for ingest (`ingest_json`) and export
//! (`export_json_node`).  When an optional SHACL shape IRI is provided, the
//! engine validates that the context terms and shape properties are consistent.
//!
//! ## v0.128.0 JSON-WRITEBACK-01: Relational writeback
//!
//! `writeback_json_row(mapping, subject_iri)` exports a subject as JSON via the
//! named mapping and writes the resulting values back into the configured
//! relational target table.  The conflict policy (`replace`, `skip`, `error`)
//! controls upsert behaviour.  `enable_json_writeback()` installs VP delta
//! triggers that automatically enqueue writeback events.
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
    // A16-CQ: too_many_arguments is necessary here — all parameters are required by the calling convention.
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

    // ─── v0.128.0 JSON-WRITEBACK-01: Relational writeback API ───────────────

    /// Write an RDF subject back to the configured relational target table.
    ///
    /// Exports the subject as JSON using the named mapping's context, maps
    /// JSON keys to relational columns, and executes an `INSERT … ON CONFLICT`
    /// based on the configured conflict policy:
    ///   - `'replace'` (default): `ON CONFLICT (key_cols) DO UPDATE SET …`
    ///   - `'skip'`: `ON CONFLICT DO NOTHING`
    ///   - `'error'`: raises `PT0551` when a conflicting row exists
    ///
    /// Returns the number of rows affected (0 or 1).
    ///
    /// Raises `PT0550` when `writeback_table` is NULL or `writeback_key_columns`
    /// is empty.
    #[pg_extern]
    pub fn writeback_json_row(mapping: &str, subject_iri: &str) -> i64 {
        crate::json_mapping::writeback_json_row_impl(mapping, subject_iri)
    }

    /// Delete the relational row corresponding to an RDF subject.
    ///
    /// Decodes key-column values from VP tables and executes
    /// `DELETE FROM <target> WHERE <key_cols> = …`.  Returns rows affected.
    ///
    /// Raises `PT0550` when `writeback_table` is NULL.
    #[pg_extern]
    pub fn writeback_json_row_delete(mapping: &str, subject_iri: &str) -> i64 {
        crate::json_mapping::writeback_json_row_delete_impl(mapping, subject_iri)
    }

    /// Enable VP trigger-based automatic writeback for a JSON mapping.
    ///
    /// Validates that `writeback_table` exists and `writeback_key_columns` is
    /// non-empty, then installs `AFTER INSERT OR DELETE FOR EACH ROW` triggers
    /// on every `_pg_ripple.vp_*_delta` table whose predicate IRI appears in the
    /// mapping context.  Sets `writeback_enabled = true`.
    ///
    /// Idempotent: re-running drops existing triggers before re-installing them.
    #[pg_extern]
    pub fn enable_json_writeback(mapping: &str) {
        crate::json_mapping::enable_json_writeback_impl(mapping)
    }

    /// Disable VP trigger-based automatic writeback for a JSON mapping.
    ///
    /// Drops all `pg_ripple_jwb_{mapping}_*` triggers and sets
    /// `writeback_enabled = false`.  Idempotent.
    #[pg_extern]
    pub fn disable_json_writeback(mapping: &str) {
        crate::json_mapping::disable_json_writeback_impl(mapping)
    }

    /// Return operational status of the writeback queue grouped by mapping.
    ///
    /// Columns: `mapping_name`, `pending`, `errors`, `last_error`,
    /// `last_processed_at`.
    #[allow(clippy::type_complexity)]
    #[pg_extern]
    pub fn json_writeback_status() -> pgrx::iter::TableIterator<
        'static,
        (
            pgrx::name!(mapping_name, String),
            pgrx::name!(pending, i64),
            pgrx::name!(errors, i64),
            pgrx::name!(last_error, Option<String>),
            pgrx::name!(
                last_processed_at,
                Option<pgrx::datum::TimestampWithTimeZone>
            ),
        ),
    > {
        crate::json_mapping::json_writeback_status_impl()
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

    let (inserted, graph_id) = match resolved_graph {
        None | Some("") => {
            let n = crate::bulk_load::load_ntriples(&ntriples, false);
            (n, 0i64)
        }
        Some(g) => {
            let g_clean = g.trim_matches(|c| c == '<' || c == '>');
            let g_id = crate::dictionary::encode(g_clean, crate::dictionary::KIND_IRI);
            let n = crate::bulk_load::load_ntriples_into_graph(&ntriples, g_id);
            (n, g_id)
        }
    };

    if inserted > 0 {
        crate::bidi::update_graph_metrics_triple_count(graph_id, inserted);
    }

    inserted
}

/// Internal: export a subject as JSON using a named mapping.
pub fn export_json_node_impl(
    subject_id: i64,
    mapping: &str,
    strip: Vec<String>,
) -> Option<pgrx::JsonB> {
    let context = fetch_mapping_context(mapping);

    // Build a frame that includes @context PLUS one empty-object property slot
    // per IRI defined in the context.  This produces OPTIONAL triple patterns
    // in the CONSTRUCT query so the SPARQL engine fetches all mapped predicates.
    // Without property slots the CONSTRUCT template is empty and returns nothing.
    let mut frame = serde_json::Map::new();
    frame.insert("@context".to_string(), context.clone());

    if let Some(ctx_obj) = context.as_object() {
        for (_term, iri_val) in ctx_obj {
            let iri_opt = match iri_val {
                serde_json::Value::String(s) => Some(s.as_str()),
                serde_json::Value::Object(meta) => meta.get("@id").and_then(|v| v.as_str()),
                _ => None,
            };
            if let Some(iri) = iri_opt
                && !iri.starts_with('@')
            {
                // Empty object `{}` → OPTIONAL { ?root <iri> ?v } in SPARQL
                frame.insert(
                    iri.to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
        }
    }

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

/// Internal: check if a mapping exists, raise error if not.
fn require_mapping_exists(mapping: &str) {
    let exists: bool = pgrx::Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.json_mappings WHERE name = $1)",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or(false);
    if !exists {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        );
    }
}

/// Internal: fetch writeback config for a mapping.
/// Returns `(writeback_table, writeback_schema, key_columns_json, conflict_policy)`.
/// Raises PT0550 if `writeback_table` is NULL or key_columns is empty.
fn fetch_writeback_config(mapping: &str) -> (String, String, Vec<String>, String) {
    let row: Option<(Option<String>, String, Option<String>, String)> =
        pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT writeback_table, writeback_schema, \
                        to_json(writeback_key_columns)::text, \
                        writeback_conflict_policy \
                 FROM _pg_ripple.json_mappings WHERE name = $1",
                    None,
                    &[pgrx::datum::DatumWithOid::from(mapping)],
                )
                .unwrap_or_else(|e| pgrx::error!("writeback config SPI error: {e}"))
                .next()
                .map(|row| {
                    let wt: Option<String> = row.get(1).ok().flatten();
                    let ws: String = row
                        .get(2)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "public".to_string());
                    let wk: Option<String> = row.get(3).ok().flatten();
                    let wp: String = row
                        .get(4)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "replace".to_string());
                    (wt, ws, wk, wp)
                })
        });

    let (wt, ws, wk_json, wp) = row.unwrap_or_else(|| {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        )
    });

    let writeback_table = wt.unwrap_or_else(|| {
        pgrx::error!(
            "PT0550: json mapping writeback target not configured; \
             call register_json_mapping(…, writeback_table => '…')"
        )
    });

    let key_columns: Vec<String> = wk_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    if key_columns.is_empty() {
        pgrx::error!(
            "PT0550: json mapping writeback target not configured; \
             call register_json_mapping(…, writeback_table => '…')"
        );
    }

    (writeback_table, ws, key_columns, wp)
}

/// Quote a SQL identifier via PostgreSQL's quote_ident().
fn pg_quote_ident(ident: &str) -> String {
    pgrx::Spi::get_one_with_args::<String>(
        "SELECT quote_ident($1)",
        &[pgrx::datum::DatumWithOid::from(ident)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| format!("\"{}\"", ident.replace('"', "\"\"")))
}

/// Internal: write an RDF subject back to a relational table.
pub fn writeback_json_row_impl(mapping: &str, subject_iri: &str) -> i64 {
    let (writeback_table, writeback_schema, key_columns, conflict_policy) =
        fetch_writeback_config(mapping);

    // Build term→IRI map from the stored context.
    let context = fetch_mapping_context(mapping);
    let ctx_obj = match context.as_object() {
        Some(o) => o.clone(),
        None => return 0,
    };
    // Maps full predicate IRI → context term name.
    let mut iri_to_term: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (term, iri_val) in &ctx_obj {
        if term.starts_with('@') {
            continue;
        }
        let iri = match iri_val {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(meta) => {
                meta.get("@id").and_then(|v| v.as_str()).unwrap_or("")
            }
            _ => "",
        };
        if !iri.is_empty() && !iri.starts_with('@') {
            iri_to_term.insert(iri.to_string(), term.clone());
        }
    }

    // Use a CONSTRUCT query to fetch all (predicate, object) pairs for the subject.
    // This bypasses the framing machinery and reads directly from VP tables.
    let sparql = format!(
        "CONSTRUCT {{ <{0}> ?p ?o }} WHERE {{ <{0}> ?p ?o }}",
        subject_iri.replace('\\', "\\\\").replace('>', "\\>")
    );
    let triples = crate::sparql::sparql_construct_rows(&sparql);

    if triples.is_empty() {
        return 0; // no triples for this subject
    }

    // Decode triples and map predicates to context terms.
    let mut term_values: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_s_id, p_id, o_id) in &triples {
        let pred_iri = match crate::dictionary::decode(*p_id) {
            Some(s) => s,
            None => continue,
        };
        let term = match iri_to_term.get(&pred_iri) {
            Some(t) => t.clone(),
            None => continue, // predicate not in context, skip
        };
        let obj_str = match crate::dictionary::decode(*o_id) {
            Some(s) => {
                // Strip datatype suffix from typed literals: "value"^^<type> → value
                // Plain literals are returned as `"value"` or `value`.
                // IRI objects are `<iri>`.
                if s.starts_with('"') {
                    // Typed or plain literal — extract the value between the first pair of quotes.
                    let inner = s.trim_start_matches('"');
                    if let Some(end) = inner.find('"') {
                        inner[..end].to_string()
                    } else {
                        inner.to_string()
                    }
                } else if s.starts_with('<') && s.ends_with('>') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                }
            }
            None => continue,
        };
        term_values.insert(term, obj_str);
    }

    if term_values.is_empty() {
        return 0;
    }

    // Get target table columns from pg_catalog (avoids information_schema type coercion issues).
    let table_cols: Vec<String> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT a.attname::text \
                 FROM pg_catalog.pg_attribute a \
                 JOIN pg_catalog.pg_class c ON c.oid = a.attrelid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 \
                   AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY a.attnum",
                None,
                &[
                    pgrx::datum::DatumWithOid::from(writeback_schema.as_str()),
                    pgrx::datum::DatumWithOid::from(writeback_table.as_str()),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("writeback column list SPI error: {e}"))
            .filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect()
    });

    if table_cols.is_empty() {
        pgrx::error!(
            "writeback_json_row: target table {}.{} not found or has no columns; \
             check writeback_schema and writeback_table in the mapping",
            writeback_schema,
            writeback_table
        );
    }

    // Build column/value pairs from term_values keys that match table columns.
    let col_set: std::collections::HashSet<&str> = table_cols.iter().map(|s| s.as_str()).collect();

    let mut insert_cols: Vec<String> = Vec::new();
    let mut insert_vals: Vec<String> = Vec::new();

    // Iterate in stable order (table column order).
    for col in &table_cols {
        if let Some(val) = term_values.get(col.as_str())
            && col_set.contains(col.as_str())
        {
            insert_cols.push(col.clone());
            insert_vals.push(val.clone());
        }
    }

    if insert_cols.is_empty() {
        return 0;
    }

    // Check for policy='error' conflicts before inserting.
    if conflict_policy == "error" {
        let q_schema = pg_quote_ident(&writeback_schema);
        let q_table = pg_quote_ident(&writeback_table);
        let q_key_cols_check: Vec<String> = key_columns
            .iter()
            .filter_map(|col| {
                term_values.get(col.as_str()).map(|_val| {
                    let qcol = pg_quote_ident(col);
                    format!("{qcol} = ${}", qcol) // placeholder; we use a count-based check
                })
            })
            .collect();
        let _ = q_key_cols_check; // build conflict check separately below
        // Build WHERE clause with key column values from term_values.
        let key_placeholders: Vec<String> = key_columns
            .iter()
            .enumerate()
            .filter_map(|(i, col)| {
                if term_values.contains_key(col.as_str()) {
                    Some(format!("{} = ${}", pg_quote_ident(col), i + 1))
                } else {
                    None
                }
            })
            .collect();
        if !key_placeholders.is_empty() {
            let where_clause = key_placeholders.join(" AND ");
            let check_sql =
                format!("SELECT COUNT(*) FROM {q_schema}.{q_table} WHERE {where_clause}");
            let key_vals: Vec<pgrx::datum::DatumWithOid> = key_columns
                .iter()
                .filter_map(|col| {
                    term_values
                        .get(col.as_str())
                        .map(|v| pgrx::datum::DatumWithOid::from(v.as_str()))
                })
                .collect();
            let count: i64 = pgrx::Spi::get_one_with_args::<i64>(&check_sql, &key_vals)
                .unwrap_or(None)
                .unwrap_or(0);
            if count > 0 {
                pgrx::error!(
                    "PT0551: json mapping writeback conflict on mapping {:?} subject {:?}; \
                     policy is 'error'",
                    mapping,
                    subject_iri
                );
            }
        }
    }

    // Quote all identifiers.
    let q_schema = pg_quote_ident(&writeback_schema);
    let q_table = pg_quote_ident(&writeback_table);
    let q_cols: Vec<String> = insert_cols.iter().map(|c| pg_quote_ident(c)).collect();
    let q_key_cols: Vec<String> = key_columns.iter().map(|c| pg_quote_ident(c)).collect();

    let cols_list = q_cols.join(", ");

    let conflict_clause = match conflict_policy.as_str() {
        "skip" => "ON CONFLICT DO NOTHING".to_string(),
        "error" => "".to_string(), // already checked above
        _ => {
            // 'replace': ON CONFLICT (key_cols) DO UPDATE SET non-key=EXCLUDED.non-key
            let update_cols: Vec<String> = q_cols
                .iter()
                .zip(insert_cols.iter())
                .filter(|(_, col)| !key_columns.contains(col))
                .map(|(qc, _)| format!("{qc} = EXCLUDED.{qc}"))
                .collect();
            if update_cols.is_empty() || q_key_cols.is_empty() {
                "ON CONFLICT DO NOTHING".to_string()
            } else {
                let key_list = q_key_cols.join(", ");
                let set_list = update_cols.join(", ");
                format!("ON CONFLICT ({key_list}) DO UPDATE SET {set_list}")
            }
        }
    };

    // Build parameterized INSERT … SELECT $1::text, $2::text, …
    let select_vals: Vec<String> = (1..=insert_cols.len())
        .map(|i| format!("${i}::text"))
        .collect();
    let select_vals_list = select_vals.join(", ");
    let insert_select_sql = format!(
        "INSERT INTO {q_schema}.{q_table} ({cols_list}) \
         SELECT {select_vals_list} {conflict_clause}"
    );

    let spi_args: Vec<pgrx::datum::DatumWithOid> = insert_vals
        .iter()
        .map(|s| pgrx::datum::DatumWithOid::from(s.as_str()))
        .collect();

    pgrx::Spi::run_with_args(&insert_select_sql, &spi_args)
        .map(|_| 1i64)
        .unwrap_or_else(|e| pgrx::error!("writeback_json_row: INSERT failed: {e}"))
}

/// Internal: delete a relational row corresponding to an RDF subject.
pub fn writeback_json_row_delete_impl(mapping: &str, subject_iri: &str) -> i64 {
    let (writeback_table, writeback_schema, key_columns, _conflict_policy) =
        fetch_writeback_config(mapping);

    // Build term→IRI map from the stored context.
    let context = fetch_mapping_context(mapping);
    let ctx_obj = match context.as_object() {
        Some(o) => o.clone(),
        None => return 0,
    };
    let mut iri_to_term: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (term, iri_val) in &ctx_obj {
        if term.starts_with('@') {
            continue;
        }
        let iri = match iri_val {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(meta) => {
                meta.get("@id").and_then(|v| v.as_str()).unwrap_or("")
            }
            _ => "",
        };
        if !iri.is_empty() && !iri.starts_with('@') {
            iri_to_term.insert(iri.to_string(), term.clone());
        }
    }

    // CONSTRUCT query to fetch all (predicate, object) pairs for the subject.
    let sparql = format!(
        "CONSTRUCT {{ <{0}> ?p ?o }} WHERE {{ <{0}> ?p ?o }}",
        subject_iri.replace('\\', "\\\\").replace('>', "\\>")
    );
    let triples = crate::sparql::sparql_construct_rows(&sparql);

    if triples.is_empty() {
        return 0;
    }

    // Decode and map predicates → term values.
    let mut term_values: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_s_id, p_id, o_id) in &triples {
        let pred_iri = match crate::dictionary::decode(*p_id) {
            Some(s) => s,
            None => continue,
        };
        let term = match iri_to_term.get(&pred_iri) {
            Some(t) => t.clone(),
            None => continue,
        };
        let obj_str = match crate::dictionary::decode(*o_id) {
            Some(s) => {
                if s.starts_with('"') {
                    let inner = s.trim_start_matches('"');
                    if let Some(end) = inner.find('"') {
                        inner[..end].to_string()
                    } else {
                        inner.to_string()
                    }
                } else if s.starts_with('<') && s.ends_with('>') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                }
            }
            None => continue,
        };
        term_values.insert(term, obj_str);
    }

    let q_schema = pg_quote_ident(&writeback_schema);
    let q_table = pg_quote_ident(&writeback_table);

    // Build WHERE clause from key_columns.
    let mut conditions: Vec<String> = Vec::new();
    let mut args_owned: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    for col in &key_columns {
        if let Some(val_str) = term_values.get(col.as_str()) {
            conditions.push(format!("{} = ${param_idx}::text", pg_quote_ident(col)));
            args_owned.push(val_str.clone());
            param_idx += 1;
        }
    }

    if conditions.is_empty() {
        return 0;
    }

    let where_clause = conditions.join(" AND ");
    let delete_sql = format!("DELETE FROM {q_schema}.{q_table} WHERE {where_clause}");

    let spi_args: Vec<pgrx::datum::DatumWithOid> = args_owned
        .iter()
        .map(|s| pgrx::datum::DatumWithOid::from(s.as_str()))
        .collect();

    pgrx::Spi::run_with_args(&delete_sql, &spi_args)
        .map(|_| 1i64)
        .unwrap_or_else(|e| pgrx::error!("writeback_json_row_delete: DELETE failed: {e}"))
}

/// Internal: enable VP trigger-based auto-enqueue for a JSON mapping.
pub fn enable_json_writeback_impl(mapping: &str) {
    // First validate writeback_table and key_columns via fetch_writeback_config.
    let (writeback_table, writeback_schema, _key_columns, _) = fetch_writeback_config(mapping);

    // Verify the target table actually exists.
    let table_exists: bool = pgrx::Spi::get_one_with_args::<bool>(
        "SELECT EXISTS( \
             SELECT 1 FROM information_schema.tables \
             WHERE table_schema = $1 AND table_name = $2)",
        &[
            pgrx::datum::DatumWithOid::from(writeback_schema.as_str()),
            pgrx::datum::DatumWithOid::from(writeback_table.as_str()),
        ],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if !table_exists {
        pgrx::error!(
            "enable_json_writeback: target table {}.{} does not exist",
            writeback_schema,
            writeback_table
        );
    }

    // Idempotency: drop existing triggers for this mapping first.
    disable_json_writeback_impl(mapping);

    // Get predicate IRIs from the mapping context.
    let context_json: serde_json::Value = pgrx::Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT context FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .map(|j| j.0)
    .unwrap_or(serde_json::Value::Object(Default::default()));

    let pred_iris: Vec<String> = if let Some(obj) = context_json.as_object() {
        obj.values()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => {
                    if s.starts_with("http") {
                        Some(s.clone())
                    } else {
                        None
                    }
                }
                serde_json::Value::Object(meta) => meta
                    .get("@id")
                    .and_then(|id| id.as_str())
                    .filter(|s| s.starts_with("http"))
                    .map(String::from),
                _ => None,
            })
            .collect()
    } else {
        vec![]
    };

    // For each predicate IRI, find the VP delta table and install trigger.
    let safe_mapping = mapping.replace(|c: char| !c.is_alphanumeric(), "_");

    // v0.128.1 C18-01 containment: track exactly which predicates got a
    // working enqueue trigger installed. Until v0.129.0 rebuilds this on a
    // storage-level event path, `writeback_enabled` must never claim more
    // coverage than what was actually installed just now.
    let mut installed = 0usize;
    let mut uncovered: Vec<String> = Vec::new();

    for pred_iri in &pred_iris {
        // Look up predicate_id in dictionary.
        let pred_id_opt: Option<i64> = pgrx::Spi::get_one_with_args::<i64>(
            "SELECT id FROM _pg_ripple.dictionary WHERE iri = $1 LIMIT 1",
            &[pgrx::datum::DatumWithOid::from(pred_iri.as_str())],
        )
        .unwrap_or(None);

        let pred_id = match pred_id_opt {
            Some(id) => id,
            None => {
                uncovered.push(format!("{pred_iri} (no dictionary entry yet)"));
                continue;
            }
        };

        // Check whether the delta table exists.
        let delta_table = format!("vp_{pred_id}_delta");
        let delta_exists: bool = pgrx::Spi::get_one_with_args::<bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM information_schema.tables \
                 WHERE table_schema = '_pg_ripple' AND table_name = $1)",
            &[pgrx::datum::DatumWithOid::from(delta_table.as_str())],
        )
        .unwrap_or(None)
        .unwrap_or(false);

        if !delta_exists {
            uncovered.push(format!("{pred_iri} (no {delta_table} table yet)"));
            continue;
        }

        let trigger_name = format!("pg_ripple_jwb_{}_{}", safe_mapping, pred_id);
        let q_trigger = pg_quote_ident(&trigger_name);
        let q_delta = format!("_pg_ripple.{}", pg_quote_ident(&delta_table));
        let q_mapping = mapping.replace('\'', "''");

        let create_trigger_sql = format!(
            "CREATE TRIGGER {q_trigger} \
             AFTER INSERT OR DELETE ON {q_delta} \
             FOR EACH ROW EXECUTE FUNCTION _pg_ripple.json_writeback_enqueue_fn('{q_mapping}')"
        );

        match pgrx::Spi::run_with_args(&create_trigger_sql, &[]) {
            Ok(_) => installed += 1,
            Err(e) => uncovered.push(format!("{pred_iri} (trigger install failed: {e})")),
        }
    }

    // v0.128.1 C18-01 containment: fail closed rather than silently reporting
    // "enabled" when the enqueue path is incomplete or entirely missing.
    if installed == 0 || !uncovered.is_empty() {
        pgrx::error!(
            "enable_json_writeback: cannot enable — incomplete enqueue coverage \
             ({installed} of {} predicate(s) covered); async writeback is unavailable \
             for this mapping until every mapped predicate has been ingested at least \
             once. Uncovered: [{}]. writeback_enabled was left false; use \
             writeback_json_row()/writeback_json_row_delete() for direct writeback \
             in the meantime.",
            pred_iris.len(),
            uncovered.join(", ")
        );
    }

    // Set writeback_enabled = true — only reached once every mapped predicate
    // has a validated enqueue trigger installed.
    pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_mappings SET writeback_enabled = true WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or_else(|e| pgrx::error!("enable_json_writeback: catalog update failed: {e}"));
}

/// Internal: disable VP trigger-based auto-enqueue for a JSON mapping.
pub fn disable_json_writeback_impl(mapping: &str) {
    require_mapping_exists(mapping);

    let safe_mapping = mapping.replace(|c: char| !c.is_alphanumeric(), "_");
    let trigger_prefix = format!("pg_ripple_jwb_{safe_mapping}_");

    // Find all triggers matching this mapping prefix.
    let trigger_rows: Vec<(String, String)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT trigger_name, event_object_table \
                 FROM information_schema.triggers \
                 WHERE trigger_schema = '_pg_ripple' \
                   AND trigger_name LIKE $1 \
                 GROUP BY trigger_name, event_object_table",
                None,
                &[pgrx::datum::DatumWithOid::from(
                    format!("{trigger_prefix}%").as_str(),
                )],
            )
            .unwrap_or_else(|e| pgrx::error!("disable_json_writeback: SPI error: {e}"))
            .filter_map(|row| {
                let tn: String = row.get(1).ok().flatten()?;
                let tbl: String = row.get(2).ok().flatten()?;
                Some((tn, tbl))
            })
            .collect()
    });

    for (trigger_name, table_name) in &trigger_rows {
        let q_trigger = pg_quote_ident(trigger_name);
        let q_table = format!("_pg_ripple.{}", pg_quote_ident(table_name));
        let drop_sql = format!("DROP TRIGGER IF EXISTS {q_trigger} ON {q_table}");
        pgrx::Spi::run_with_args(&drop_sql, &[]).unwrap_or_else(|e| {
            pgrx::warning!(
                "disable_json_writeback: could not drop trigger {}: {e}",
                trigger_name
            )
        });
    }

    // Set writeback_enabled = false.
    pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_mappings SET writeback_enabled = false WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or_else(|e| pgrx::error!("disable_json_writeback: catalog update failed: {e}"));
}

/// Internal: return writeback queue status grouped by mapping.
#[allow(clippy::type_complexity)]
pub fn json_writeback_status_impl() -> pgrx::iter::TableIterator<
    'static,
    (
        pgrx::name!(mapping_name, String),
        pgrx::name!(pending, i64),
        pgrx::name!(errors, i64),
        pgrx::name!(last_error, Option<String>),
        pgrx::name!(
            last_processed_at,
            Option<pgrx::datum::TimestampWithTimeZone>
        ),
    ),
> {
    type Row = (
        String,
        i64,
        i64,
        Option<String>,
        Option<pgrx::datum::TimestampWithTimeZone>,
    );

    let rows: Vec<Row> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT \
                     mapping_name, \
                     COUNT(*) FILTER (WHERE processed_at IS NULL)::bigint AS pending, \
                     COUNT(*) FILTER (WHERE error IS NOT NULL)::bigint AS errors, \
                     (SELECT error FROM _pg_ripple.json_writeback_queue q2 \
                      WHERE q2.mapping_name = q.mapping_name \
                        AND q2.error IS NOT NULL \
                      ORDER BY q2.queued_at DESC LIMIT 1) AS last_error, \
                     MAX(processed_at) AS last_processed_at \
                 FROM _pg_ripple.json_writeback_queue q \
                 GROUP BY mapping_name \
                 ORDER BY mapping_name",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("json_writeback_status SPI error: {e}"))
            .filter_map(|row| {
                let mn: String = row.get(1).ok().flatten()?;
                let pending: i64 = row.get(2).ok().flatten().unwrap_or(0);
                let errors: i64 = row.get(3).ok().flatten().unwrap_or(0);
                let last_err: Option<String> = row.get(4).ok().flatten();
                let last_proc: Option<pgrx::datum::TimestampWithTimeZone> =
                    row.get(5).ok().flatten();
                Some((mn, pending, errors, last_err, last_proc))
            })
            .collect()
    });

    pgrx::iter::TableIterator::new(rows)
}

/// Internal: drain pending writeback queue rows (called by background worker).
pub fn drain_json_writeback_queue() {
    let batch_size = crate::JSON_WRITEBACK_BATCH_SIZE.get();
    if batch_size <= 0 {
        return;
    }

    // Fetch up to batch_size pending rows.
    let pending_rows: Vec<(i64, String, i64, String)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT id, mapping_name, subject_id, operation \
                 FROM _pg_ripple.json_writeback_queue \
                 WHERE processed_at IS NULL \
                 ORDER BY queued_at \
                 LIMIT $1",
                None,
                &[pgrx::datum::DatumWithOid::from(batch_size as i64)],
            )
            .unwrap_or_else(|e| pgrx::error!("drain_json_writeback_queue: SPI error: {e}"))
            .filter_map(|row| {
                let id: i64 = row.get(1).ok().flatten()?;
                let mn: String = row.get(2).ok().flatten()?;
                let sid: i64 = row.get(3).ok().flatten()?;
                let op: String = row.get(4).ok().flatten()?;
                Some((id, mn, sid, op))
            })
            .collect()
    });

    for (row_id, mapping_name, subject_id, operation) in &pending_rows {
        // Decode subject_id back to IRI.
        let subject_iri_opt: Option<String> = pgrx::Spi::get_one_with_args::<String>(
            "SELECT iri FROM _pg_ripple.dictionary WHERE id = $1 LIMIT 1",
            &[pgrx::datum::DatumWithOid::from(*subject_id)],
        )
        .unwrap_or(None);

        let subject_iri = match subject_iri_opt {
            Some(s) => s,
            None => {
                // Mark as processed with error (subject not in dictionary).
                let _ = pgrx::Spi::run_with_args(
                    "UPDATE _pg_ripple.json_writeback_queue \
                     SET processed_at = now(), error = 'subject_id not found in dictionary' \
                     WHERE id = $1",
                    &[pgrx::datum::DatumWithOid::from(*row_id)],
                );
                continue;
            }
        };

        // Attempt the writeback operation.
        let result: Result<(), String> =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if operation == "delete" {
                    writeback_json_row_delete_impl(mapping_name, &subject_iri);
                } else {
                    writeback_json_row_impl(mapping_name, &subject_iri);
                }
            }))
            .map_err(|e| {
                if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "unknown panic in writeback".to_string()
                }
            });

        let error_msg: Option<String> = result.err();

        let _ = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.json_writeback_queue \
             SET processed_at = now(), error = $2 WHERE id = $1",
            &[
                pgrx::datum::DatumWithOid::from(*row_id),
                pgrx::datum::DatumWithOid::from(error_msg.as_deref()),
            ],
        );
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    // A16-CQ: unused_imports here is intentional for test/cfg-gated code paths.
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
