//! pg_ripple SQL API — PageRank & Graph Analytics (v0.88.0)
//!
//! Exposes the following SQL functions in the `pg_ripple` schema:
//! - `pg_ripple.pagerank_run(...)` — run full PageRank computation
//! - `pg_ripple.pagerank_run_topics(topics)` — run multiple topic runs
//! - `pg_ripple.vacuum_pagerank_dirty()` — drain processed dirty-edge queue entries
//! - `pg_ripple.pagerank_queue_stats()` — IVM queue metrics
//! - `pg_ripple.explain_pagerank(node_iri, top_k)` — score explanation tree
//! - `pg_ripple.export_pagerank(format, top_k, topic)` — export to Turtle/JSON-LD/CSV/N-Triples
//! - `pg_ripple.centrality_run(metric, ...)` — alternative centrality measures
//! - `pg_ripple.pagerank_find_duplicates(...)` — centrality-guided entity deduplication

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── PR-SQL-FN-01: pagerank_run ────────────────────────────────────────────

    /// Run iterative PageRank and store results in `_pg_ripple.pagerank_scores`.
    ///
    /// Returns one row per node with node IRI, score, bounds, iteration count,
    /// convergence flag, staleness flag, and topic.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.pagerank_run() ORDER BY score DESC LIMIT 10;
    /// SELECT * FROM pg_ripple.pagerank_run(damping => 0.85, max_iterations => 50);
    /// ```
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    #[pg_extern]
    fn pagerank_run(
        edge_predicates: default!(Option<Vec<String>>, "NULL"),
        damping: default!(f64, 0.85),
        max_iterations: default!(i32, 100),
        convergence_delta: default!(f64, 0.0001),
        graph_uri: default!(Option<String>, "NULL"),
        direction: default!(String, "'forward'"),
        edge_weight_predicate: default!(Option<String>, "NULL"),
        topic: default!(Option<String>, "NULL"),
        decay_rate: default!(f64, 0.0),
        temporal_predicate: default!(Option<String>, "NULL"),
        seed_iris: default!(Option<Vec<String>>, "NULL"),
        bias: default!(f64, 0.15),
        predicate_filter: default!(Option<Vec<String>>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(node_iri, String),
            name!(score, f64),
            name!(score_lower, f64),
            name!(score_upper, f64),
            name!(iterations, i32),
            name!(converged, bool),
            name!(stale, bool),
            name!(topic, String),
        ),
    > {
        let params = crate::pagerank::PageRankParams {
            edge_predicates,
            damping,
            max_iterations,
            convergence_delta,
            graph_uri,
            direction,
            edge_weight_predicate,
            topic,
            decay_rate,
            temporal_predicate,
            seed_iris,
            bias,
            predicate_filter,
        };
        let rows = crate::pagerank::run_pagerank(params);
        TableIterator::new(rows.into_iter().map(|r| {
            (
                r.node_iri,
                r.score,
                r.score_lower,
                r.score_upper,
                r.iterations,
                r.converged,
                r.stale,
                r.topic,
            )
        }))
    }

    // ── PR-TOPIC-01: pagerank_run_topics ─────────────────────────────────────

    /// Run multiple topic-specific PageRank runs in sequence.
    ///
    /// ```sql
    /// SELECT pg_ripple.pagerank_run_topics(ARRAY[ARRAY['science', NULL], ARRAY['politics', NULL]]);
    /// ```
    #[pg_extern]
    fn pagerank_run_topics(topics: Vec<String>) {
        for topic in topics {
            let params = crate::pagerank::PageRankParams {
                topic: Some(topic),
                ..Default::default()
            };
            crate::pagerank::run_pagerank(params);
        }
    }

    // ── PR-TRICKLE-01: vacuum_pagerank_dirty ─────────────────────────────────

    /// Drain processed entries from the dirty-edges queue.
    ///
    /// Returns the number of rows deleted.
    ///
    /// ```sql
    /// SELECT pg_ripple.vacuum_pagerank_dirty();
    /// ```
    #[pg_extern]
    fn vacuum_pagerank_dirty() -> i64 {
        crate::pagerank::vacuum_pagerank_dirty()
    }

    // ── PR-IVM-METRICS-01: pagerank_queue_stats ───────────────────────────────

    /// Return IVM queue metrics.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.pagerank_queue_stats();
    /// ```
    #[pg_extern]
    fn pagerank_queue_stats() -> TableIterator<
        'static,
        (
            name!(queued_edges, i64),
            name!(max_delta, f64),
            name!(oldest_enqueue, Option<pgrx::datum::TimestampWithTimeZone>),
            name!(estimated_drain_seconds, f64),
        ),
    > {
        let stats = crate::pagerank::pagerank_queue_stats();
        TableIterator::new(std::iter::once((
            stats.queued_edges,
            stats.max_delta,
            stats.oldest_enqueue,
            stats.estimated_drain_seconds,
        )))
    }

    // ── PR-EXPLAIN-SCORE-01: explain_pagerank ─────────────────────────────────

    /// Return the top-K incoming contributor chain for a node's PageRank score.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.explain_pagerank('http://example.org/alice', 5);
    /// ```
    #[pg_extern]
    fn explain_pagerank(
        node_iri: &str,
        top_k: default!(i32, 5),
    ) -> TableIterator<
        'static,
        (
            name!(depth, i32),
            name!(contributor_iri, String),
            name!(contribution, f64),
            name!(path, String),
        ),
    > {
        let rows = crate::pagerank::explain_pagerank(node_iri, top_k);
        TableIterator::new(
            rows.into_iter()
                .map(|r| (r.depth, r.contributor_iri, r.contribution, r.path)),
        )
    }

    // ── PR-EXPORT-01: export_pagerank ─────────────────────────────────────────

    /// Export PageRank scores in the requested format.
    ///
    /// Supported formats: `'turtle'`, `'jsonld'`, `'csv'`, `'ntriples'`.
    ///
    /// ```sql
    /// SELECT pg_ripple.export_pagerank('csv', 100, NULL);
    /// ```
    #[pg_extern]
    fn export_pagerank(
        format: default!(&str, "'turtle'"),
        top_k: default!(Option<i32>, "NULL"),
        topic: default!(Option<String>, "NULL"),
    ) -> String {
        crate::pagerank::export_pagerank(format, top_k, topic.as_deref())
    }

    // ── PR-CENTRALITY-01: centrality_run ─────────────────────────────────────

    /// Run a centrality measure and return results.
    ///
    /// Supported metrics: `'betweenness'`, `'closeness'`, `'eigenvector'`, `'katz'`.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.centrality_run('betweenness');
    /// ```
    #[pg_extern]
    fn centrality_run(
        metric: &str,
        edge_predicates: default!(Option<Vec<String>>, "NULL"),
        graph_uri: default!(Option<String>, "NULL"),
    ) -> TableIterator<'static, (name!(node_iri, String), name!(score, f64))> {
        let rows = crate::pagerank::centrality_run(metric, edge_predicates, graph_uri.as_deref());
        TableIterator::new(rows.into_iter().map(|r| (r.node_iri, r.score)))
    }

    // ── PR-ENTITY-RESOLUTION-01: pagerank_find_duplicates ────────────────────

    /// Find candidate duplicate nodes via centrality + fuzzy matching.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.pagerank_find_duplicates();
    /// ```
    #[pg_extern]
    fn pagerank_find_duplicates(
        metric: default!(&str, "'betweenness'"),
        centrality_threshold: default!(f64, 0.1),
        fuzzy_threshold: default!(f64, 0.85),
    ) -> TableIterator<
        'static,
        (
            name!(node_a_iri, String),
            name!(node_b_iri, String),
            name!(centrality_score, f64),
            name!(fuzzy_score, f64),
        ),
    > {
        let rows = crate::pagerank::find_pagerank_duplicates(
            metric,
            centrality_threshold,
            fuzzy_threshold,
        );
        TableIterator::new(rows.into_iter().map(|r| {
            (
                r.node_a_iri,
                r.node_b_iri,
                r.centrality_score,
                r.fuzzy_score,
            )
        }))
    }
}
