//! pg_ripple SQL API — SPARQL CONSTRUCT writeback rules (v0.63.0, v0.65.0)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── v0.63.0 + v0.65.0: SPARQL CONSTRUCT Writeback Rules ─────────────────

    /// Register a SPARQL CONSTRUCT writeback rule.
    ///
    /// The query must be a SPARQL CONSTRUCT statement.  The derived triples are
    /// written directly into the VP storage layer in `target_graph` (with
    /// `source = 1`) and are immediately queryable from SPARQL.
    ///
    /// # Arguments
    /// - `name`         — unique rule name (ASCII alphanumeric + underscore, ≤ 63 chars)
    /// - `sparql`       — SPARQL CONSTRUCT query text
    /// - `target_graph` — IRI of the named graph that will receive derived triples
    /// - `mode`         — `'incremental'` (default) or `'full'`
    ///
    /// On success the rule is registered and an initial full recompute is run.
    /// Returns `NULL` on success; throws on error.
    #[pg_extern]
    fn create_construct_rule(
        name: &str,
        sparql: &str,
        target_graph: &str,
        mode: default!(&str, "'incremental'"),
    ) -> Option<String> {
        crate::construct_rules::create_construct_rule(name, sparql, target_graph, mode);
        None // NULL = success; elog::Error propagates on failure
    }

    /// Drop a SPARQL CONSTRUCT writeback rule.
    ///
    /// - `name`    — rule name
    /// - `retract` — when `true` (default), remove derived triples from VP tables
    ///               that are exclusively owned by this rule; when `false`, leave
    ///               derived triples in place.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_construct_rule(name: &str, retract: default!(bool, true)) -> bool {
        crate::construct_rules::drop_construct_rule(name, retract);
        true
    }

    /// Force a full recompute of a SPARQL CONSTRUCT writeback rule.
    ///
    /// Clears all derived triples owned exclusively by this rule in the target
    /// graph, then re-runs the CONSTRUCT query from scratch.
    ///
    /// Returns the number of triples written.
    #[pg_extern]
    fn refresh_construct_rule(name: &str) -> i64 {
        crate::construct_rules::refresh_construct_rule(name)
    }

    /// List all registered SPARQL CONSTRUCT writeback rules.
    ///
    /// Returns a JSONB array of `{name, sparql, target_graph, mode, source_graphs,
    /// rule_order, last_refreshed, last_incremental_run, successful_run_count,
    /// failed_run_count, derived_triple_count, last_error}` objects (v0.65.0).
    #[pg_extern]
    fn list_construct_rules() -> pgrx::JsonB {
        crate::construct_rules::list_construct_rules()
    }

    /// Return explain output for a SPARQL CONSTRUCT writeback rule.
    ///
    /// Returns rows of `(section TEXT, content TEXT)` with sections:
    /// `delta_insert_sql`, `source_graphs`, `rule_order`.
    #[pg_extern]
    fn explain_construct_rule(
        name: &str,
    ) -> TableIterator<'static, (name!(section, String), name!(content, String))> {
        let rows = crate::construct_rules::explain_construct_rule(name);
        TableIterator::new(rows)
    }

    /// Return the canonical pipeline status for all construct rules (v0.65.0).
    ///
    /// Returns a JSONB object with `rule_count` and a `rules` array containing
    /// per-rule: dependency graph, last run state, pending deltas, derived
    /// triple counts, failed run count, stale flag, and health observability.
    ///
    /// This is the API foundation for a future canonical graph pipeline UI.
    #[pg_extern]
    fn construct_pipeline_status() -> pgrx::JsonB {
        crate::construct_rules::construct_pipeline_status()
    }

    /// Apply all construct rules that source from `graph_iri` (v0.65.0).
    ///
    /// Runs incremental maintenance for every registered rule whose
    /// `source_graphs` list contains `graph_iri`.  This is called automatically
    /// by the extension write path; it can also be called manually to trigger
    /// maintenance without inserting new source triples.
    ///
    /// Returns the total number of newly derived triples.
    #[pg_extern]
    fn apply_construct_rules_for_graph(graph_iri: &str) -> i64 {
        crate::construct_rules::apply_for_graph(graph_iri)
    }
}
