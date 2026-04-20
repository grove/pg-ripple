//! pg_ripple SQL API — SPARQL/Datalog/ExtVP views, Framing views, CONSTRUCT/DESCRIBE/ASK views, Bulk loaders

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Register a named graph IRI (by encoded ID) in `_pg_ripple.named_graphs`.
    ///
    /// Called whenever a named graph is loaded, even if zero triples are inserted.
    /// This allows `GRAPH ?var { }` queries to enumerate empty named graphs.
    fn register_named_graph(graph_id: i64) {
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.named_graphs (graph_id) VALUES ($1) ON CONFLICT DO NOTHING",
            &[pgrx::datum::DatumWithOid::from(graph_id)],
        );
    }

    // ── v0.11.0: SPARQL Views, Datalog Views, ExtVP ───────────────────────────

    /// Return `true` when the pg_trickle extension is installed in the current database.
    ///
    /// SPARQL views, Datalog views, and ExtVP all require pg_trickle.
    /// Call this function to check availability before calling the view functions.
    #[pg_extern]
    fn pg_trickle_available() -> bool {
        crate::views::pg_trickle_available()
    }

    /// Create a named, incrementally-maintained SPARQL SELECT result table.
    ///
    /// Compiles the SPARQL query to SQL, registers a pg_trickle stream table
    /// under `pg_ripple.{name}`, and records the view in `_pg_ripple.sparql_views`.
    ///
    /// - `name` — view name (alphanumeric + underscores, ≤ 63 chars)
    /// - `sparql` — SPARQL SELECT query
    /// - `schedule` — pg_trickle schedule, e.g. `'1s'`, `'IMMEDIATE'`, `'30s'`
    /// - `decode` — when `false` (recommended), the stream table stores BIGINT IDs;
    ///              when `true`, stores decoded TEXT values
    ///
    /// Returns the number of projected variables (stream table columns).
    #[pg_extern]
    fn create_sparql_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_sparql_view(name, sparql, schedule, decode)
    }

    /// Drop a SPARQL view and its underlying pg_trickle stream table.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_sparql_view(name: &str) -> bool {
        crate::views::drop_sparql_view(name)
    }

    /// List all registered SPARQL views as a JSONB array.
    #[pg_extern]
    fn list_sparql_views() -> pgrx::JsonB {
        crate::views::list_sparql_views()
    }

    /// Create a Datalog view from inline rules and a SPARQL SELECT goal.
    ///
    /// The rules are parsed, stratified, and stored under `rule_set_name`.
    /// The goal query is compiled to SQL and registered as a pg_trickle stream
    /// table under `pg_ripple.{name}`.
    ///
    /// - `name` — view name
    /// - `rules` — Datalog rules in Turtle-flavoured Datalog syntax
    /// - `rule_set_name` — logical name for the stored rule set
    /// - `goal` — SPARQL SELECT query selecting from derived predicates
    /// - `schedule` — pg_trickle schedule
    /// - `decode` — as for `create_sparql_view`
    ///
    /// Returns the number of projected variables in the goal query.
    #[pg_extern]
    fn create_datalog_view(
        name: &str,
        rules: &str,
        goal: &str,
        rule_set_name: default!(&str, "'custom'"),
        schedule: default!(&str, "'10s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_datalog_view_from_rules(
            name,
            rules,
            rule_set_name,
            goal,
            schedule,
            decode,
        )
    }

    /// Create a Datalog view referencing an existing named rule set.
    ///
    /// The rule set must have been previously loaded with `load_rules`.
    /// The goal query is compiled to SQL and registered as a pg_trickle stream table.
    #[pg_extern]
    fn create_datalog_view_from_rule_set(
        name: &str,
        rule_set: &str,
        goal: &str,
        schedule: default!(&str, "'10s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_datalog_view_from_rule_set(name, rule_set, goal, schedule, decode)
    }

    /// Drop a Datalog view and its underlying pg_trickle stream table.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_datalog_view(name: &str) -> bool {
        crate::views::drop_datalog_view(name)
    }

    /// List all registered Datalog views as a JSONB array.
    #[pg_extern]
    fn list_datalog_views() -> pgrx::JsonB {
        crate::views::list_datalog_views()
    }

    // ── v0.17.0: Framing views ────────────────────────────────────────────────

    /// Create an incrementally-maintained JSON-LD framing view (requires pg_trickle).
    ///
    /// Translates `frame` to a SPARQL CONSTRUCT query and registers it as a
    /// pg_trickle stream table `pg_ripple.framing_view_{name}` with schema
    /// `(subject_id BIGINT, frame_tree JSONB, refreshed_at TIMESTAMPTZ)`.
    ///
    /// When `decode = TRUE` a thin IRI-decoding view is also created.
    #[pg_extern]
    fn create_framing_view(
        name: &str,
        frame: pgrx::JsonB,
        schedule: default!(&str, "'5s'"),
        decode: default!(bool, "false"),
        output_format: default!(&str, "'jsonld'"),
    ) {
        crate::views::create_framing_view(name, &frame.0, schedule, decode, output_format)
    }

    /// Drop a framing view stream table and its catalog entry.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_framing_view(name: &str) -> bool {
        crate::views::drop_framing_view(name)
    }

    /// List all registered framing views as a JSONB array.
    #[pg_extern]
    fn list_framing_views() -> pgrx::JsonB {
        crate::views::list_framing_views()
    }

    // ── v0.18.0: SPARQL CONSTRUCT, DESCRIBE & ASK Views ──────────────────────

    /// Create a CONSTRUCT view — an incrementally-maintained stream table
    /// `pg_ripple.construct_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
    /// whose rows reflect the CONSTRUCT template output at all times.
    ///
    /// When `decode = TRUE`, a thin TEXT-decoding view
    /// `pg_ripple.construct_view_{name}_decoded` is also created.
    ///
    /// Returns the number of template triples registered.
    ///
    /// Errors if `sparql` is not a CONSTRUCT query, if template variables are
    /// unbound, or if the template contains blank nodes.
    #[pg_extern]
    fn create_construct_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, "false"),
    ) -> i64 {
        crate::views::create_construct_view(name, sparql, schedule, decode)
    }

    /// Drop a CONSTRUCT view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_construct_view(name: &str) {
        crate::views::drop_construct_view(name)
    }

    /// List all registered CONSTRUCT views as a JSONB array.
    #[pg_extern]
    fn list_construct_views() -> pgrx::JsonB {
        crate::views::list_construct_views()
    }

    /// Create a DESCRIBE view — an incrementally-maintained stream table
    /// `pg_ripple.describe_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
    /// materialising the Concise Bounded Description (CBD) of the described resources.
    ///
    /// The `pg_ripple.describe_strategy` GUC controls CBD vs symmetric-CBD.
    ///
    /// When `decode = TRUE`, a thin TEXT-decoding view
    /// `pg_ripple.describe_view_{name}_decoded` is also created.
    ///
    /// Errors if `sparql` is not a DESCRIBE query.
    #[pg_extern]
    fn create_describe_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, "false"),
    ) {
        crate::views::create_describe_view(name, sparql, schedule, decode)
    }

    /// Drop a DESCRIBE view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_describe_view(name: &str) {
        crate::views::drop_describe_view(name)
    }

    /// List all registered DESCRIBE views as a JSONB array.
    #[pg_extern]
    fn list_describe_views() -> pgrx::JsonB {
        crate::views::list_describe_views()
    }

    /// Create an ASK view — an incrementally-maintained single-row stream table
    /// `pg_ripple.ask_view_{name}(result BOOLEAN, evaluated_at TIMESTAMPTZ)`
    /// that updates whenever the underlying pattern's satisfiability changes.
    ///
    /// Errors if `sparql` is not an ASK query.
    #[pg_extern]
    fn create_ask_view(name: &str, sparql: &str, schedule: default!(&str, "'1s'")) {
        crate::views::create_ask_view(name, sparql, schedule)
    }

    /// Drop an ASK view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_ask_view(name: &str) {
        crate::views::drop_ask_view(name)
    }

    /// List all registered ASK views as a JSONB array.
    #[pg_extern]
    fn list_ask_views() -> pgrx::JsonB {
        crate::views::list_ask_views()
    }

    ///
    /// Pre-computes subjects that appear in triples of both `pred1_iri` and
    /// `pred2_iri`.  The SPARQL query engine uses these tables to accelerate
    /// star-pattern queries that reference both predicates.
    ///
    /// - `name` — ExtVP name
    /// - `pred1_iri` — IRI of the first predicate
    /// - `pred2_iri` — IRI of the second predicate
    /// - `schedule` — pg_trickle schedule
    ///
    /// Returns the number of rows in the stream table after the first refresh.
    #[pg_extern]
    fn create_extvp(
        name: &str,
        pred1_iri: &str,
        pred2_iri: &str,
        schedule: default!(&str, "'10s'"),
    ) -> i64 {
        crate::views::create_extvp(name, pred1_iri, pred2_iri, schedule)
    }

    /// Drop an ExtVP table and remove it from the catalog.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_extvp(name: &str) -> bool {
        crate::views::drop_extvp(name)
    }

    /// List all registered ExtVP tables as a JSONB array.
    #[pg_extern]
    fn list_extvp() -> pgrx::JsonB {
        crate::views::list_extvp()
    }

    // ── v0.15.0: Graph-aware bulk loaders ─────────────────────────────────────

    /// Load N-Triples data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_ntriples_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        register_named_graph(g_id);
        crate::bulk_load::load_ntriples_into_graph(data, g_id)
    }

    /// Load Turtle data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_turtle_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        register_named_graph(g_id);
        crate::bulk_load::load_turtle_into_graph(data, g_id)
    }

    /// Load RDF/XML data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_rdfxml_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        register_named_graph(g_id);
        crate::bulk_load::load_rdfxml_into_graph(data, g_id)
    }

    /// Load N-Triples from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_ntriples_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        register_named_graph(g_id);
        crate::bulk_load::load_ntriples_file_into_graph(path, g_id)
    }

    /// Load Turtle from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_turtle_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        register_named_graph(g_id);
        crate::bulk_load::load_turtle_file_into_graph(path, g_id)
    }

    /// Load RDF/XML from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_rdfxml_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_rdfxml_file_into_graph(path, g_id)
    }

    /// Load RDF/XML from a server-side file path (superuser required).
    #[pg_extern]
    fn load_rdfxml_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_rdfxml_file(path, strict)
    }

    // ── v0.25.0 supplementary features ────────────────────────────────────────

    /// Load an OWL ontology file (Turtle / N-Triples / RDF/XML) from the server
    /// file system and insert all triples into the default graph.
    ///
    /// The format is detected from the file extension: `.ttl` → Turtle,
    /// `.nt` → N-Triples, `.xml` / `.rdf` / `.owl` → RDF/XML.
    /// Unrecognised extensions default to Turtle.
    ///
    /// Returns the number of triples loaded.
    #[pg_extern]
    fn load_owl_ontology(path: &str) -> i64 {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "nt" => crate::bulk_load::load_ntriples_file(path, false),
            "xml" | "rdf" | "owl" => crate::bulk_load::load_rdfxml_file(path, false),
            _ => crate::bulk_load::load_turtle_file(path, false),
        }
    }

    /// Apply an RDF Patch (W3C community standard) string to the store.
    ///
    /// Supported patch operations (one per line):
    /// - `A <s> <p> <o> .`  — add triple
    /// - `D <s> <p> <o> .`  — delete triple
    ///
    /// Lines beginning with `#`, `TX`, `TC`, `H` are treated as comments /
    /// transaction markers and silently ignored.  Returns the net change in
    /// triple count (additions minus deletions).
    #[pg_extern]
    fn apply_patch(patch: &str) -> i64 {
        let mut added = 0i64;
        let mut deleted = 0i64;
        for line in patch.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with("TX")
                || line.starts_with("TC")
                || line.starts_with('H')
            {
                continue;
            }
            if let Some(rest) = line.strip_prefix('A').map(|s| s.trim()) {
                // Parse as a single N-Triples statement
                let nt = format!("{rest}\n");
                added += crate::bulk_load::load_ntriples(&nt, false);
            } else if let Some(rest) = line.strip_prefix('D').map(|s| s.trim()) {
                // Delete via N-Triples term parser.
                if let Some((s, p, o)) = crate::parse_nt_triple(rest) {
                    deleted += crate::storage::delete_triple(&s, &p, &o, 0);
                }
            }
        }
        added - deleted
    }

    /// Register a custom SPARQL aggregate function name with the extension.
    ///
    /// This records the aggregate IRI in the `_pg_ripple.custom_aggregates`
    /// catalog table so the SPARQL-to-SQL translator can recognise it and
    /// delegate to the corresponding PostgreSQL aggregate.
    ///
    /// `sparql_iri`  — the full IRI of the custom aggregate function.
    /// `pg_function` — the PostgreSQL aggregate or function to call (schema-qualified
    ///                 if outside `pg_catalog`).
    #[pg_extern]
    fn register_aggregate(sparql_iri: &str, pg_function: &str) {
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.custom_aggregates (sparql_iri, pg_function)
             VALUES ($1, $2)
             ON CONFLICT (sparql_iri) DO UPDATE SET pg_function = EXCLUDED.pg_function",
            &[
                pgrx::datum::DatumWithOid::from(sparql_iri),
                pgrx::datum::DatumWithOid::from(pg_function),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("register_aggregate failed: {e}"));
    }

    // ── v0.15.0: Graph-aware triple deletion ──────────────────────────────────

    /// Delete a specific triple from a named graph.  Returns 0 or 1.
    #[pg_extern]
    fn delete_triple_from_graph(s: &str, p: &str, o: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::delete_triple(s, p, o, g_id)
    }

    /// Delete all triples in a named graph without unregistering it.
    /// Returns the number of triples deleted.
    #[pg_extern]
    fn clear_graph(graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::clear_graph_by_id(g_id)
    }

    // ── v0.15.0: SQL API completeness gaps ────────────────────────────────────

    /// Pattern-match triples within a specific named graph (or default graph if NULL).
    #[pg_extern]
    fn find_triples_in_graph(
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
        graph: Option<&str>,
    ) -> TableIterator<
        'static,
        (
            name!(s, String),
            name!(p, String),
            name!(o, String),
            name!(g, String),
        ),
    > {
        let g_id = graph.map(|g| crate::dictionary::encode(g, crate::dictionary::KIND_IRI));
        let rows = crate::storage::find_triples(s, p, o, g_id);
        TableIterator::new(rows)
    }

    /// Return the number of triples in a specific named graph.
    #[pg_extern]
    fn triple_count_in_graph(graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::triple_count_in_graph(g_id)
    }

    /// Decode a dictionary ID to its full structured representation as JSONB.
    /// Returns {"kind": ..., "value": ..., "language": null|"...", "datatype": null|"..."}.
    #[pg_extern]
    fn decode_id_full(id: i64) -> Option<pgrx::JsonB> {
        crate::dictionary::decode_full(id).map(|info| {
            let kind_label = match info.kind {
                0 => "iri",
                1 => "blank_node",
                2 => "literal",
                3 => "typed_literal",
                4 => "lang_literal",
                5 => "quoted_triple",
                _ => "unknown",
            };
            let mut obj = serde_json::Map::new();
            obj.insert(
                "kind".to_owned(),
                serde_json::Value::String(kind_label.to_owned()),
            );
            obj.insert("value".to_owned(), serde_json::Value::String(info.value));
            obj.insert(
                "datatype".to_owned(),
                info.datatype
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            obj.insert(
                "language".to_owned(),
                info.lang
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            pgrx::JsonB(serde_json::Value::Object(obj))
        })
    }

    /// Look up an IRI in the dictionary without encoding it.
    /// Returns the dictionary ID if the IRI exists, NULL otherwise.
    #[pg_extern]
    fn lookup_iri(iri: &str) -> Option<i64> {
        crate::dictionary::lookup_iri(iri)
    }
}
