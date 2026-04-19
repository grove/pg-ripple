//! pg_ripple SQL API — Dictionary, Triple CRUD, Rare-predicate, Bulk loaders, Named graph, IRI prefix

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Dictionary ────────────────────────────────────────────────────────────

    /// Encode a text IRI/blank-node/literal to its dictionary `i64` identifier.
    #[pg_extern]
    fn encode_term(term: &str, kind: i16) -> i64 {
        crate::dictionary::encode(term, kind)
    }

    /// Decode a dictionary `i64` back to its original text value.
    #[pg_extern]
    fn decode_id(id: i64) -> Option<String> {
        crate::dictionary::decode(id)
    }

    // ── RDF-star: quoted triple encoding (v0.4.0) ──────────────────────────

    /// Encode a quoted triple `(s, p, o)` into the dictionary.
    ///
    /// All three arguments must be N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or nested `<< … >>` quoted triples).
    /// Returns the dictionary ID of the quoted triple.
    #[pg_extern]
    fn encode_triple(s: &str, p: &str, o: &str) -> i64 {
        let s_id = crate::storage::encode_rdf_term(s);
        let p_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(p),
            crate::dictionary::KIND_IRI,
        );
        let o_id = crate::storage::encode_rdf_term(o);
        crate::dictionary::encode_quoted_triple(s_id, p_id, o_id)
    }

    /// Decode a quoted triple dictionary ID to its component terms as JSONB.
    ///
    /// Returns `{"s": "...", "p": "...", "o": "..."}` with N-Triples–formatted
    /// values, or NULL if `id` is not a quoted triple.
    #[pg_extern]
    fn decode_triple(id: i64) -> Option<pgrx::JsonB> {
        let (s_id, p_id, o_id) = crate::dictionary::decode_quoted_triple_components(id)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "s".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(s_id)),
        );
        obj.insert(
            "p".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(p_id)),
        );
        obj.insert(
            "o".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(o_id)),
        );
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    // ── Triple CRUD ───────────────────────────────────────────────────────────

    /// Insert a triple into the appropriate VP table.
    ///
    /// `s`, `p`, and `o` accept N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or `<< … >>` quoted triples).
    /// `g` is an optional named graph IRI; NULL uses the default graph.
    /// Returns the globally-unique statement identifier (SID).
    #[pg_extern]
    fn insert_triple(s: &str, p: &str, o: &str, g: default!(Option<&str>, "NULL")) -> i64 {
        let g_id = g.map_or(0_i64, |iri| {
            crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(iri),
                crate::dictionary::KIND_IRI,
            )
        });

        // ── v0.7.0: SHACL sync validation ──────────────────────────────────
        let shacl_mode = crate::SHACL_MODE.get();
        let shacl_mode_str = shacl_mode
            .as_ref()
            .and_then(|c| c.to_str().ok())
            .unwrap_or("off");

        if shacl_mode_str == "sync" {
            // Pre-encode the triple terms to check constraints.
            let s_id = crate::storage::encode_rdf_term(s);
            let p_id = crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(p),
                crate::dictionary::KIND_IRI,
            );
            let o_id = crate::storage::encode_rdf_term(o);
            if let Err(msg) = crate::shacl::validate_sync(s_id, p_id, o_id, g_id) {
                pgrx::error!("{msg}");
            }
        }

        let sid = crate::storage::insert_triple(s, p, o, g_id);

        // ── v0.32.0: Tabling cache invalidation ────────────────────────────
        if sid > 0 {
            crate::datalog::tabling_invalidate_all();
        }

        // ── v0.7.0: SHACL async queue ───────────────────────────────────────
        if shacl_mode_str == "async" && sid > 0 {
            let s_id = crate::storage::encode_rdf_term(s);
            let p_id = crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(p),
                crate::dictionary::KIND_IRI,
            );
            let o_id = crate::storage::encode_rdf_term(o);
            let _ = pgrx::Spi::run_with_args(
                "INSERT INTO _pg_ripple.validation_queue (s_id, p_id, o_id, g_id, stmt_id) \
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    pgrx::datum::DatumWithOid::from(s_id),
                    pgrx::datum::DatumWithOid::from(p_id),
                    pgrx::datum::DatumWithOid::from(o_id),
                    pgrx::datum::DatumWithOid::from(g_id),
                    pgrx::datum::DatumWithOid::from(sid),
                ],
            );
        }

        sid
    }

    /// Look up a statement by its globally-unique statement identifier (SID).
    ///
    /// Returns `{"s": "...", "p": "...", "o": "...", "g": "..."}` as JSONB,
    /// or NULL if the SID is not found.
    #[pg_extern]
    fn get_statement(i: i64) -> Option<pgrx::JsonB> {
        let (s, p, o, g) = crate::storage::get_statement_by_sid(i)?;
        let mut obj = serde_json::Map::new();
        obj.insert("s".to_owned(), serde_json::Value::String(s));
        obj.insert("p".to_owned(), serde_json::Value::String(p));
        obj.insert("o".to_owned(), serde_json::Value::String(o));
        obj.insert("g".to_owned(), serde_json::Value::String(g));
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    /// Delete a triple.  Returns the number of rows removed (0 or 1).
    #[pg_extern]
    fn delete_triple(s: &str, p: &str, o: &str) -> i64 {
        let deleted = crate::storage::delete_triple(s, p, o, 0_i64);
        // Invalidate tabling cache on data change (v0.32.0).
        if deleted > 0 {
            crate::datalog::tabling_invalidate_all();
        }
        deleted
    }

    /// Return the total number of triples across all VP tables and vp_rare.
    #[pg_extern]
    fn triple_count() -> i64 {
        crate::storage::total_triple_count()
    }

    /// Flush the backend-local predicate OID cache (v0.38.0).
    ///
    /// Forces the next SPARQL query to re-query `_pg_ripple.predicates` for
    /// all predicates.  Useful after DDL changes or when debugging cache
    /// behaviour with `pg_ripple.predicate_cache_enabled = off`.
    #[pg_extern]
    fn invalidate_catalog_cache() {
        crate::storage::catalog::invalidate_predicate_cache();
    }

    /// Pattern-match triples; any argument may be NULL to act as a wildcard.
    /// Queries both dedicated VP tables and vp_rare.
    /// Returns N-Triples–formatted `(s, p, o, g)` tuples.
    #[pg_extern]
    fn find_triples(
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
    ) -> TableIterator<
        'static,
        (
            name!(s, String),
            name!(p, String),
            name!(o, String),
            name!(g, String),
        ),
    > {
        let rows = crate::storage::find_triples(s, p, o, None);
        TableIterator::new(rows)
    }

    // ── Rare-predicate promotion ──────────────────────────────────────────────

    /// Promote all rare predicates that have reached the promotion threshold.
    /// Returns the number of predicates promoted.
    #[pg_extern]
    fn promote_rare_predicates() -> i64 {
        crate::storage::promote_rare_predicates()
    }

    // ── Bulk loaders ──────────────────────────────────────────────────────────

    /// Load N-Triples data from a text string.  Returns the number of triples loaded.
    /// Also accepts N-Triples-star (quoted triples as objects or subjects).
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_ntriples(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_ntriples(data, strict)
    }

    /// Load N-Quads data from a text string (supports named graphs).
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_nquads(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_nquads(data, strict)
    }

    /// Load Turtle data from a text string.
    /// Also accepts Turtle-star (quoted triples) using oxttl with rdf-12 support.
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_turtle(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_turtle(data, strict)
    }

    /// Load TriG data (Turtle with named graph blocks) from a text string.
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_trig(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_trig(data, strict)
    }

    /// Load N-Triples from a server-side file path (superuser required).
    #[pg_extern]
    fn load_ntriples_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_ntriples_file(path, strict)
    }

    /// Load N-Quads from a server-side file path (superuser required).
    #[pg_extern]
    fn load_nquads_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_nquads_file(path, strict)
    }

    /// Load Turtle from a server-side file path (superuser required).
    #[pg_extern]
    fn load_turtle_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_turtle_file(path, strict)
    }

    /// Load TriG from a server-side file path (superuser required).
    #[pg_extern]
    fn load_trig_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_trig_file(path, strict)
    }

    /// Load RDF/XML data from a text string.  Returns the number of triples loaded.
    ///
    /// Parses conformant RDF/XML using `rio_xml`.  All triples are loaded into the
    /// default graph (RDF/XML does not support named graphs).
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_rdfxml(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_rdfxml(data, strict)
    }

    // ── Named graph management ────────────────────────────────────────────────

    /// Register a named graph IRI.  Returns its dictionary id.
    /// This is idempotent — safe to call multiple times.
    #[pg_extern]
    fn create_graph(graph_iri: &str) -> i64 {
        crate::storage::create_graph(graph_iri)
    }

    /// Delete all triples in a named graph.  Returns the number of triples deleted.
    #[pg_extern]
    fn drop_graph(graph_iri: &str) -> i64 {
        crate::storage::drop_graph(graph_iri)
    }

    /// List all named graph IRIs (excludes the default graph).
    #[pg_extern]
    fn list_graphs() -> TableIterator<'static, (name!(graph_iri, String),)> {
        let graphs = crate::storage::list_graphs();
        TableIterator::new(graphs.into_iter().map(|g| (g,)))
    }

    // ── IRI prefix management ─────────────────────────────────────────────────

    /// Register (or update) an IRI prefix abbreviation.
    #[pg_extern]
    fn register_prefix(prefix: &str, expansion: &str) {
        crate::storage::register_prefix(prefix, expansion);
    }

    /// Return all registered prefix → expansion mappings.
    #[pg_extern]
    fn prefixes() -> TableIterator<'static, (name!(prefix, String), name!(expansion, String))> {
        let pfxs = crate::storage::list_prefixes();
        TableIterator::new(pfxs)
    }
}
