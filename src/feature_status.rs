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
                     sparql_cursor_turtle and sparql_cursor_jsonld still materialize CONSTRUCT \
                     results (planned for full streaming in v0.67.0)"
                        .to_string(),
                ),
                Some("ci/regress: sparql_cursor.sql".to_string()),
                Some("docs/src/reference/sparql.md".to_string()),
                None,
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
                "planned".to_string(),
                None,
                Some(
                    "sh:SPARQLRule is parsed and stored but not executed through \
                     the derivation kernel; full routing deferred to v0.66.0 \
                     (CWB-FIX-09 derivation kernel foundation delivered in v0.65.0)"
                        .to_string(),
                ),
                None,
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
                "planned".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-01: SERVICE result shard pruning is not yet integrated into \
                     SPARQL-to-SQL translator; carry-forward set and pruning helpers exist \
                     but require translator-level wiring"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            (
                "citus_hll_distinct".to_string(),
                "planned".to_string(),
                Some("citus, hll".to_string()),
                Some(
                    "CITUS-02: COUNT(DISTINCT) via HyperLogLog is not yet wired into SQL \
                     aggregate generation; opt-in GUC pg_ripple.approx_distinct planned"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            (
                "citus_nonblocking_promotion".to_string(),
                "planned".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-03: VP promotion is currently synchronous (takes DDL lock); \
                     shadow-table non-blocking promotion requires schema changes planned for v0.67.0"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                None,
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
                     tests/integration/citus_rls_propagation.sh"
                        .to_string(),
                ),
                Some("ci/integration: citus_rls_propagation.sh".to_string()),
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            (
                "citus_multihop_pruning".to_string(),
                "planned".to_string(),
                Some("citus".to_string()),
                Some(
                    "CITUS-06: multi-hop carry-forward helpers exist in citus.rs but \
                     ShardPruneSet is not yet wired into property-path or BGP translation"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                None,
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
                "planner_hint".to_string(),
                None,
                Some(
                    "WCOJ is implemented as a cyclic-BGP planner hint that reorders joins; \
                     a true Leapfrog Triejoin executor is not implemented"
                        .to_string(),
                ),
                Some("ci/regress: sparql_wcoj.sql".to_string()),
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
