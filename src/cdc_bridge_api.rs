//! pg_ripple SQL API — CDC bridge, JSON→RDF, vocabulary templates, trickle checks (v0.52.0)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── v0.52.0: pg-trickle runtime detection ─────────────────────────────────

    /// Return `true` when the pg-trickle extension is installed and integration
    /// is enabled (`pg_ripple.trickle_integration = on`).
    ///
    /// All CDC bridge functions require pg-trickle; call this to check
    /// availability before using bridge features.
    #[pg_extern]
    fn trickle_available() -> bool {
        crate::TRICKLE_INTEGRATION.get() && crate::has_pg_trickle()
    }

    // ── v0.52.0: CDC bridge trigger management ────────────────────────────────

    /// Install a CDC bridge trigger on the VP delta table for `predicate`.
    ///
    /// When a triple for `predicate` is inserted into the delta table, the
    /// trigger decodes the `(s, p, o)` dictionary IDs and writes a JSON-LD
    /// event with a dedup key to `outbox` within the same transaction.
    ///
    /// Raises PT800 when pg-trickle is absent or `trickle_integration = off`.
    #[pg_extern]
    fn enable_cdc_bridge_trigger(name: &str, predicate: &str, outbox: &str) {
        crate::storage::cdc_bridge::enable_cdc_bridge_trigger(name, predicate, outbox);
    }

    /// Drop a CDC bridge trigger previously installed by `enable_cdc_bridge_trigger`.
    #[pg_extern]
    fn disable_cdc_bridge_trigger(name: &str) {
        crate::storage::cdc_bridge::disable_cdc_bridge_trigger(name);
    }

    /// List all registered CDC bridge triggers.
    ///
    /// Returns one row per registered trigger with columns
    /// `(name TEXT, predicate TEXT, outbox TEXT, active BOOL)`.
    #[pg_extern]
    fn cdc_bridge_triggers() -> TableIterator<
        'static,
        (
            name!(name, String),
            name!(predicate, String),
            name!(outbox, String),
            name!(active, bool),
        ),
    > {
        let rows = crate::storage::cdc_bridge::list_cdc_bridge_triggers();
        TableIterator::new(
            rows.into_iter()
                .map(|r| (r.name, r.predicate, r.outbox, r.active)),
        )
    }

    // ── v0.52.0: Outbox dedup key ─────────────────────────────────────────────

    /// Return a relay-compatible dedup key for the given `(s, p, o)` triple.
    ///
    /// Looks up the statement ID (`i` column) for the triple and returns
    /// `'ripple:{statement_id}'`.  Returns `NULL` when the triple does not
    /// exist in the store.
    #[pg_extern]
    fn statement_dedup_key(s: i64, p: i64, o: i64) -> Option<String> {
        crate::storage::statement_id_for_triple(s, p, o).map(|sid| format!("ripple:{sid}"))
    }

    // ── v0.52.0: JSON → N-Triples helpers ────────────────────────────────────

    /// Convert a JSON object payload to an N-Triples string.
    ///
    /// - `payload` — JSONB object; each key becomes a predicate IRI.
    /// - `subject_iri` — IRI for the RDF subject (without angle brackets).
    /// - `type_iri` — optional `rdf:type` IRI; prepends one type triple.
    /// - `context` — optional JSONB `{"key": "iri", "@vocab": "prefix/", …}`
    ///   mapping that resolves short keys to full IRIs.
    ///
    /// Nested objects become blank nodes.  Arrays produce one triple per element.
    /// `null` values are skipped.
    #[pg_extern]
    fn json_to_ntriples(
        payload: pgrx::JsonB,
        subject_iri: &str,
        type_iri: default!(Option<&str>, "NULL"),
        context: default!(Option<pgrx::JsonB>, "NULL"),
    ) -> String {
        let ctx_val = context.as_ref().map(|c| &c.0);
        crate::bulk_load::json_to_ntriples(&payload.0, subject_iri, type_iri, ctx_val)
    }

    /// Convert a JSON object to N-Triples and immediately load the triples
    /// into the store.
    ///
    /// Returns the number of triples inserted.  Equivalent to calling
    /// `json_to_ntriples()` and then `load_ntriples()`, but in one step.
    #[pg_extern]
    fn json_to_ntriples_and_load(
        payload: pgrx::JsonB,
        subject_iri: &str,
        type_iri: default!(Option<&str>, "NULL"),
        context: default!(Option<pgrx::JsonB>, "NULL"),
    ) -> i64 {
        let ctx_val = context.as_ref().map(|c| &c.0);
        crate::bulk_load::json_to_ntriples_and_load(&payload.0, subject_iri, type_iri, ctx_val)
    }

    // ── v0.73.0: Multi-subject JSON-LD document ingest (JSONLD-INGEST-02) ─────

    /// Ingest a full JSON-LD document that may contain multiple top-level subjects.
    ///
    /// Handles both the `@graph` form (multiple top-level nodes) and the
    /// single-node form (object with `@id`).  Each top-level node must have an
    /// `@id` key.
    ///
    /// - `document`      — JSONB value representing the JSON-LD document.
    /// - `default_graph` — named graph IRI to use when the document has no outer
    ///   named graph.  `NULL` means the default graph.
    ///
    /// Returns the total number of triples loaded.
    ///
    /// Use this function in relay triggers when the inbound JSON-LD payload
    /// contains multiple subjects, instead of looping over
    /// `json_to_ntriples_and_load()`.
    ///
    /// **Deprecated (v0.83.0 API-RENAME-01)**: use `load_jsonld()` instead.
    /// This function emits a NOTICE and delegates to `load_jsonld()`.
    /// It will be removed in v1.0.0.
    #[pg_extern]
    fn json_ld_load(document: pgrx::JsonB, default_graph: default!(Option<&str>, "NULL")) -> i64 {
        pgrx::warning!(
            "json_ld_load is deprecated; use load_jsonld() instead. \
             json_ld_load will be removed in v1.0.0 (API-RENAME-01)"
        );
        crate::bulk_load::json_ld_load(&document.0, default_graph)
    }

    /// Load a JSON-LD document and store all triples in the RDF graph store.
    ///
    /// This is the canonical name for the JSON-LD bulk loader.
    /// `json_ld_load()` is the deprecated alias.
    ///
    /// `document` is a JSONB value containing a JSON-LD document.
    /// `graph_uri` optionally specifies the named graph to load into;
    /// when NULL the default graph (g = 0) is used.
    ///
    /// Returns the number of triples loaded.  (API-RENAME-01, v0.83.0)
    #[pg_extern]
    fn load_jsonld(document: pgrx::JsonB, graph_uri: default!(Option<&str>, "NULL")) -> i64 {
        crate::bulk_load::json_ld_load(&document.0, graph_uri)
    }

    // ── v0.52.0: Vocabulary template loader ───────────────────────────────────

    /// Load a named vocabulary alignment template from `sql/vocab/`.
    ///
    /// Returns the number of Datalog rules loaded.  Available templates:
    /// - `'schema_to_saref'` — Schema.org ↔ SAREF IoT sensor data
    /// - `'schema_to_fhir'` — Schema.org ↔ FHIR R4 basic resources
    /// - `'schema_to_provo'` — Schema.org ↔ PROV-O provenance ontology
    /// - `'generic_to_schema'` — generic JSON key → Schema.org property heuristics
    #[pg_extern]
    fn load_vocab_template(name: &str) -> i64 {
        let rules = match name {
            "schema_to_saref" => include_str!("../sql/vocab/schema_to_saref.pl"),
            "schema_to_fhir" => include_str!("../sql/vocab/schema_to_fhir.pl"),
            "schema_to_provo" => include_str!("../sql/vocab/schema_to_provo.pl"),
            "generic_to_schema" => include_str!("../sql/vocab/generic_to_schema.pl"),
            other => pgrx::error!(
                "load_vocab_template: unknown template '{}'; valid options are \
                 schema_to_saref, schema_to_fhir, schema_to_provo, generic_to_schema",
                other
            ),
        };
        crate::datalog::builtins::register_standard_prefixes();
        crate::datalog::cache::invalidate(name);
        crate::datalog::tabling_invalidate_all();
        match crate::datalog::parse_rules(rules, name) {
            Ok(rs) => crate::datalog::store_rules(name, &rs.rules),
            Err(e) => pgrx::error!("load_vocab_template '{}': rule parse error: {}", name, e),
        }
    }
}
