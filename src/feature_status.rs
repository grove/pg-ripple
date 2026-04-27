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
                "planned".to_string(),
                None,
                Some(
                    "current /sparql/stream endpoint fully materializes results before streaming; \
                     true incremental streaming deferred to v0.66.0"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/sparql.md".to_string()),
                None,
            ),
            // ── CONSTRUCT writeback ────────────────────────────────────────
            (
                "construct_writeback".to_string(),
                "manual_refresh".to_string(),
                None,
                Some(
                    "CONSTRUCT rules apply on manual invocation only; \
                     incremental delta maintenance deferred to v0.65.0"
                        .to_string(),
                ),
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
                     the derivation kernel; deferred to v0.65.0"
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
                    "SERVICE result shard pruning is planned but not integrated \
                     into the SPARQL-to-SQL translator; deferred to v0.66.0"
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
                    "COUNT(DISTINCT) via HyperLogLog is planned but SQL aggregate \
                     generation does not yet emit HLL calls; deferred to v0.66.0"
                        .to_string(),
                ),
                None,
                Some("docs/src/reference/scalability.md".to_string()),
                None,
            ),
            // ── Arrow Flight ───────────────────────────────────────────────
            (
                "arrow_flight".to_string(),
                "stub".to_string(),
                None,
                Some(
                    "Arrow Flight endpoint (/flight/do_get) returns a JSON stub; \
                     real Arrow IPC streaming deferred to v0.66.0"
                        .to_string(),
                ),
                None,
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
