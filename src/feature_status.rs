//! Feature status catalog for pg_ripple (v0.64.0, TRUTH-01).
//!
//! `pg_ripple.feature_status()` returns one row per major capability with an
//! honest status value.  Operators can use this to understand which features
//! are fully implemented, experimental, stubbed, or planned.
//!
//! Status taxonomy (TRUTH-06):
//! - `implemented`   — normal execution path is wired, tested, and documented
//! - `experimental`  — available behind a GUC/feature flag with documented limits
//! - `planner_hint`  — optimization guidance exists but is not a custom executor
//! - `manual_refresh`— feature is correct only when a manual refresh is invoked
//! - `stub`          — API exists but production behavior is not implemented
//! - `degraded`      — dependency or configuration is missing; fallback is active
//! - `planned`       — roadmap item exists, no user-facing implementation

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Return one row per major capability with an honest status value.
    ///
    /// Use this function to understand which features are fully implemented,
    /// experimental, stubbed, or planned before relying on them in production.
    ///
    /// ```sql
    /// SELECT feature_name, status, degraded_reason
    /// FROM pg_ripple.feature_status()
    /// WHERE status != 'implemented'
    /// ORDER BY feature_name;
    /// ```
    #[allow(clippy::type_complexity)]
    #[pg_extern]
    pub fn feature_status() -> TableIterator<
        'static,
        (
            name!(feature_name, String),
            name!(status, String),
            name!(dependency, Option<String>),
            name!(degraded_reason, Option<String>),
            name!(ci_gate, Option<String>),
            name!(docs_path, Option<String>),
            name!(evidence_path, Option<String>),
        ),
    > {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = vec![
            // ── Core SPARQL engine ─────────────────────────────────────────
            (
                "sparql_select".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/test: cargo pgrx test pg18".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                None,
            ),
            (
                "sparql_update".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/test: cargo pgrx test pg18".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                None,
            ),
            (
                "sparql_construct".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/test: cargo pgrx test pg18".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                None,
            ),
            (
                "sparql_property_paths".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: property_paths.sql".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                None,
            ),
            (
                "sparql_federation".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: sparql_federation.sql".to_string()),
                Some("docs/src/reference/federation.md".to_string()),
                None,
            ),
            (
                "sparql_cursor_streaming".to_string(),
                "experimental".to_string(),
                None,
                Some(
                    "sparql_cursor uses portal-based paged fetching (bounded memory per page); \
                     sparql_cursor_turtle and sparql_cursor_jsonld use ConstructCursorIter \
                     (v0.68.0 STREAM-01) — portal-based paging, template applied per page, \
                     no full document buffered in Rust memory"
                        .to_string(),
                ),
                Some("ci/regress: sparql_cursor.sql, v068_features.sql".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                Some("src/sparql/cursor.rs: ConstructCursorIter".to_string()),
            ),
            // ── CONSTRUCT writeback ────────────────────────────────────────
            (
                "construct_writeback".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: construct_rules.sql".to_string()),
                Some("docs/src/reference/construct-rules.md".to_string()),
                None,
            ),
            // ── SHACL ──────────────────────────────────────────────────────
            (
                "shacl_sparql_constraint".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: shacl_sparql_constraint.sql".to_string()),
                Some("docs/src/reference/shacl.md".to_string()),
                None,
            ),
            (
                "shacl_sparql_rule".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: v079_shacl_sparql_rule.sql".to_string()),
                Some("docs/src/reference/shacl.md".to_string()),
                None,
            ),
            // ── Datalog ────────────────────────────────────────────────────
            (
                "datalog_inference".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/test: cargo pgrx test pg18".to_string()),
                Some("docs/src/reference/datalog.md".to_string()),
                None,
            ),
            (
                "datalog_owl_rl".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: datalog_owl_rl.sql".to_string()),
                Some("docs/src/reference/datalog.md".to_string()),
                None,
            ),
            // ── HTAP storage ───────────────────────────────────────────────
            (
                "htap_delta_main".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: htap_merge.sql".to_string()),
                Some("docs/src/reference/storage.md".to_string()),
                None,
            ),
            // ── Citus scalability ──────────────────────────────────────────
            (
                "citus_service_pruning".to_string(),
                "experimental".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-SVC-01 (v0.68.0): SERVICE translator calls \
                     citus_service_shard_annotation() when citus_service_pruning=on; \
                     wires shard-constraint annotations for Citus worker endpoints. \
                     Full multi-node infrastructure required for end-to-end testing."
                        .to_string(),
                ),
                Some("ci/regress: v068_features.sql (EXPLAIN citus_service_pruning GUC)".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                Some("src/citus.rs: citus_service_shard_annotation()".to_string()),
            ),
            (
                "citus_hll_distinct".to_string(),
                "experimental".to_string(),
                Some("citus, hll".to_string()),
                Some(
                    "CITUS-HLL-01 (v0.68.0): when pg_ripple.approx_distinct=on and the hll \
                     extension is installed, COUNT(DISTINCT ?x) is translated to \
                     hll_cardinality(hll_add_agg(hll_hash_bigint(x)))::bigint for scalable \
                     approximate counts on distributed VP tables. Falls back to exact \
                     COUNT(DISTINCT) when hll is absent or approx_distinct=off."
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                Some("src/sparql/translate/group.rs: citus_hll_available".to_string()),
            ),
            (
                "citus_nonblocking_promotion".to_string(),
                "experimental".to_string(),
                Some("citus".to_string()),
                Some(
                    "PROMO-01 (v0.68.0): VP promotion tracks progress via \
                     promotion_status column in _pg_ripple.predicates ('promoting'/'promoted'). \
                     pg_ripple.recover_interrupted_promotions() retries any interrupted \
                     promotion after an unclean shutdown; call it on-demand after crash."
                        .to_string(),
                ),
                Some("ci/regress: vp_promotion_nonblocking.sql, v068_features.sql".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                Some("src/storage/mod.rs: promote_predicate, recover_interrupted_promotions".to_string()),
            ),
            (
                "citus_brin_summarise".to_string(),
                "implemented".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-04: run_command_on_shards(brin_summarize_new_values) called \
                     after HTAP merge for distributed VP main tables; graceful fallback \
                     for non-Citus deployments"
                        .to_string(),
                ),
                Some("ci/regress: htap_merge.sql (brin_summarise assertions)".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            (
                "citus_rls_propagation".to_string(),
                "experimental".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-05: grant_graph/revoke_graph propagate to workers via \
                     run_command_on_all_nodes; synchronous propagation verified in \
                     security_rls_role_injection pg_regress test. \
                     Full integration test planned for v0.71.0 (CITUS-INT-01)."
                        .to_string(),
                ),
                Some("ci/regress: security_rls_role_injection.sql".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            (
                "citus_multihop_pruning".to_string(),
                "experimental".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-SVC-01 (v0.68.0): is_citus_worker_endpoint() detects Citus worker \
                     endpoints; citus_service_pruning=on wires shard annotations into \
                     SERVICE translator. Multi-hop carry-forward helpers exist in citus.rs."
                        .to_string(),
                ),
                Some("ci/regress: v068_features.sql".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                Some("src/citus.rs: is_citus_worker_endpoint, citus_service_shard_annotation".to_string()),
            ),
            // ── Arrow Flight ───────────────────────────────────────────────
            (
                "arrow_flight".to_string(),
                "experimental".to_string(),
                None,
                Some(
                    "Tickets are HMAC-SHA256 signed with expiry and nonce (FLIGHT-01); \
                     pg_ripple_http /flight/do_get streams real Arrow IPC record batches \
                     from VP tables (FLIGHT-02); requires pg_ripple.arrow_flight_secret to be set"
                        .to_string(),
                ),
                Some("ci/regress: v062_features.sql (ticket signing), tests/integration/".to_string()),
                Some("docs/src/reference/arrow-flight.md".to_string()),
                None,
            ),
            // ── WCOJ ───────────────────────────────────────────────────────
            (
                "wcoj".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: sparql_wcoj.sql, v079_wcoj.sql".to_string()),
                Some("docs/src/reference/query-optimization.md".to_string()),
                None,
            ),
            // ── Streaming observability (v0.66.0 OBS-01) ──────────────────
            (
                "streaming_observability".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: streaming_metrics.sql".to_string()),
                Some("docs/src/reference/observability.md".to_string()),
                None,
            ),
            // ── Vector search ──────────────────────────────────────────────
            (
                "vector_hybrid_search".to_string(),
                "experimental".to_string(),
                Some("pgvector".to_string()),
                Some(
                    "requires pgvector extension; gracefully degrades to exact search \
                     when pgvector is not installed"
                        .to_string(),
                ),
                Some("ci/regress: vector_graceful.sql".to_string()),
                Some("docs/src/reference/vector-search.md".to_string()),
                None,
            ),
            // ── Federation ─────────────────────────────────────────────────
            (
                "sparql_service_federation".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: sparql_federation.sql".to_string()),
                Some("docs/src/reference/federation.md".to_string()),
                None,
            ),
            // ── GraphRAG ───────────────────────────────────────────────────
            (
                "graphrag_export".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: graphrag_export.sql".to_string()),
                Some("docs/src/reference/graphrag.md".to_string()),
                None,
            ),
            // ── CDC ────────────────────────────────────────────────────────
            (
                "cdc_subscriptions".to_string(),
                "experimental".to_string(),
                Some("pg_trickle".to_string()),
                Some(
                    "requires pg_trickle; degrades gracefully when pg_trickle is \
                     not installed"
                        .to_string(),
                ),
                Some("ci/regress: cdc_subscriptions.sql".to_string()),
                Some("docs/src/reference/cdc.md".to_string()),
                None,
            ),
            // ── Continuous fuzzing (v0.68.0 FUZZ-01) ─────────────────────
            (
                "continuous_fuzzing".to_string(),
                "experimental".to_string(),
                None,
                Some(
                    "FUZZ-01 (v0.68.0): scheduled nightly fuzz workflow (.github/workflows/fuzz.yml) \
                     runs all 12 fuzz targets for 60 s each; manual workflow_dispatch supports \
                     extended runs. Corpus and crash artifacts uploaded on each run."
                        .to_string(),
                ),
                Some("ci/workflow: .github/workflows/fuzz.yml (nightly schedule)".to_string()),
                Some("docs/src/reference/development.md".to_string()),
                Some(".github/workflows/fuzz.yml".to_string()),
            ),
            // ── LLM / NL-to-SPARQL (v0.49.0, FEATURE-STATUS-02) ─────────
            (
                "llm_sparql_repair".to_string(),
                "experimental".to_string(),
                Some("external LLM endpoint".to_string()),
                Some(
                    "FEATURE-STATUS-02 (v0.73.0): src/llm/ provides NL-to-SPARQL via a \
                     configurable LLM endpoint (sparql_from_nl), SPARQL auto-repair, and \
                     embedding-based entity alignment (suggest_sameas). \
                     All paths degrade gracefully when no LLM endpoint is configured."
                        .to_string(),
                ),
                Some("ci/regress: v073_features.sql".to_string()),
                Some("docs/src/features/nl-to-sparql.md".to_string()),
                Some("src/llm/mod.rs".to_string()),
            ),
            (
                "kge_embeddings".to_string(),
                "experimental".to_string(),
                Some("pgvector".to_string()),
                Some(
                    "FEATURE-STATUS-02 (v0.73.0): src/kge.rs implements TransE / RotatE \
                     knowledge-graph embeddings stored in _pg_ripple.kge_embeddings with \
                     HNSW index. Requires pgvector; degrades gracefully when absent."
                        .to_string(),
                ),
                Some("ci/regress: v073_features.sql".to_string()),
                Some("docs/src/features/knowledge-graph-embeddings.md".to_string()),
                Some("src/kge.rs".to_string()),
            ),
            (
                "sparql_nl_to_sparql".to_string(),
                "experimental".to_string(),
                Some("external LLM endpoint".to_string()),
                Some(
                    "FEATURE-STATUS-02 (v0.73.0): sparql_from_nl() translates natural-language \
                     questions to SPARQL SELECT queries via a configurable LLM endpoint. \
                     Returns NULL when no LLM endpoint is configured."
                        .to_string(),
                ),
                Some("ci/regress: v073_features.sql".to_string()),
                Some("docs/src/features/nl-to-sparql.md".to_string()),
                Some("src/llm/mod.rs: sparql_from_nl".to_string()),
            ),
            // ── SPARQL 1.2 (v0.73.0 SPARQL12-01) ─────────────────────────
            (
                "sparql_12".to_string(),
                "planned".to_string(),
                Some("spargebra SPARQL 1.2 grammar".to_string()),
                Some(
                    "SPARQL12-01 (v0.73.0): SPARQL 1.2 (W3C WG draft) tracked in \
                     plans/sparql12_tracking.md. Waiting for spargebra upstream to ship \
                     SPARQL 1.2 grammar support before implementation. \
                     Targeted as post-v1.0.0 unless spargebra ships 1.2 before v1.0.0."
                        .to_string(),
                ),
                None,
                Some("plans/sparql12_tracking.md".to_string()),
                None,
            ),
            // ── Live SPARQL subscriptions (v0.73.0 SUB-01) ───────────────
            (
                "sparql_subscription".to_string(),
                "experimental".to_string(),
                None,
                Some(
                    "SUB-01 (v0.73.0): subscribe_sparql() / unsubscribe_sparql() SQL functions \
                     register subscriptions in _pg_ripple.sparql_subscriptions. \
                     After each graph write, the mutation journal flush calls pg_notify with \
                     the updated SPARQL result (or {\"changed\":true} when result > 8 KB). \
                     pg_ripple_http /subscribe/:id exposes SSE streaming."
                        .to_string(),
                ),
                Some("ci/regress: subscriptions.sql".to_string()),
                Some("docs/src/features/live-subscriptions.md".to_string()),
                Some("src/subscriptions.rs".to_string()),
            ),
            // ── Multi-subject JSON-LD ingest (v0.73.0 JSONLD-INGEST-02) ──
            (
                "json_ld_multi_ingest".to_string(),
                "implemented".to_string(),
                None,
                None,
                Some("ci/regress: jsonld_ingest_multi_graph.sql".to_string()),
                Some("docs/src/features/loading-data.md".to_string()),
                Some("src/bulk_load.rs: json_ld_load".to_string()),
            ),
            // ── Named JSON mapping (v0.73.0 JSON-MAPPING-01) ─────────────
            (
                "json_mapping".to_string(),
                "experimental".to_string(),
                None,
                Some(
                    "JSON-MAPPING-01 (v0.73.0): register_json_mapping() stores a named \
                     JSON-LD context; ingest_json() and export_json_node() derive both \
                     directions from one registration. Optional SHACL shape integration \
                     for nesting derivation and consistency checking."
                        .to_string(),
                ),
                Some("ci/regress: json_mapping.sql".to_string()),
                Some("docs/src/features/loading-data.md".to_string()),
                Some("src/json_mapping.rs".to_string()),
            ),
            // ── VP promotion recovery monitoring (v0.74.0 PROMO-RECOVER-01) ──
            (
                "vp_promotion_recovery".to_string(),
                "implemented".to_string(),
                None,
                Some({
                    let count = pgrx::Spi::get_one::<i64>(
                        "SELECT COUNT(*) FROM _pg_ripple.predicates \
                         WHERE promotion_status = 'promoting'"
                    ).ok().flatten().unwrap_or(0);
                    format!(
                        "PROMO-RECOVER-01 (v0.74.0): recover_interrupted_promotions() is \
                         auto-invoked by background worker 0 at startup. \
                         Currently {count} predicate(s) stuck in 'promoting' state."
                    )
                }),
                Some("ci/regress: v074_features.sql".to_string()),
                Some("docs/src/reference/storage.md".to_string()),
                Some("src/storage/promote.rs: recover_interrupted_promotions".to_string()),
            ),
            // ── Mutation journal (v0.75.0 FEATURE-STATUS-JOURNAL-01) ────────
            (
                "mutation_journal".to_string(),
                "implemented".to_string(),
                None,
                Some(
                    "FEATURE-STATUS-JOURNAL-01 (v0.75.0): mutation_journal tracks every \
                     graph write and fires CONSTRUCT writeback rules (CWB). \
                     Wired call sites: bulk_load (BULK-01), dict_api executor-end hook \
                     (FLUSH-DEFER-01), Datalog seminaive inference (JOURNAL-DATALOG-01), \
                     SPARQL Update (CF-A v0.74.0). flush() is called per-statement, not \
                     per-triple, to avoid O(n x rules) quadratic cost."
                        .to_string(),
                ),
                Some("ci/regress: v075_features.sql".to_string()),
                Some("docs/src/reference/storage.md".to_string()),
                Some("src/storage/mutation_journal.rs".to_string()),
            ),
        ];

        TableIterator::new(rows)
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
