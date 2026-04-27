//! pg_ripple SQL API — SPARQL CONSTRUCT writeback rules (v0.63.0)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── v0.63.0: SPARQL CONSTRUCT Writeback Rules ────────────────────────────

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
    #[pg_extern]
    fn create_construct_rule(
        name: &str,
        sparql: &str,
        target_graph: &str,
        mode: default!(&str, "'incremental'"),
    ) {
        crate::construct_rules::create_construct_rule(name, sparql, target_graph, mode);
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
    /// rule_order, last_refreshed}` objects.
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
}
