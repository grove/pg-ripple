//! pg_ripple SQL API — Uncertain Knowledge Engine (v0.87.0)
//!
//! Exposes the following SQL functions:
//! - `pg_ripple.load_triples_with_confidence(data, confidence, format, graph_uri)` — bulk load with confidence
//! - `pg_ripple.vacuum_confidence()` — purge orphaned confidence rows
//! - `pg_ripple.shacl_score(graph_iri)` — weighted SHACL quality score
//! - `pg_ripple.shacl_report_scored(graph_iri)` — per-violation scored report
//! - `pg_ripple.log_shacl_score(graph_iri)` — log the score to the history table

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── LOAD-CONF-01: confidence-aware bulk loader ────────────────────────────

    /// Load triples with a uniform confidence score attached to all inserted SIDs.
    ///
    /// `format` may be `'ntriples'` (default), `'nquads'`, or `'turtle'`.
    /// `graph_uri` routes all triples to a named graph when provided.
    ///
    /// Returns the number of triples loaded.
    ///
    /// ```sql
    /// SELECT pg_ripple.load_triples_with_confidence(
    ///   '<ex:alice> <ex:knows> <ex:bob> .',
    ///   confidence => 0.85
    /// );
    /// ```
    #[pg_extern]
    fn load_triples_with_confidence(
        data: &str,
        confidence: default!(f64, 1.0),
        format: default!(&str, "'ntriples'"),
        graph_uri: default!(Option<&str>, "NULL"),
    ) -> i64 {
        crate::bulk_load::load_triples_with_confidence(data, confidence, format, graph_uri)
    }

    // ── CONF-GC-01d: vacuum_confidence ────────────────────────────────────────

    /// Purge orphaned confidence rows — rows whose statement_id does not exist
    /// in any VP table.  Called manually or by the maintenance hook.
    ///
    /// Returns the number of rows deleted.
    ///
    /// ```sql
    /// SELECT pg_ripple.vacuum_confidence();
    /// ```
    #[pg_extern]
    fn vacuum_confidence() -> i64 {
        // Ensure the confidence table exists (idempotent).
        crate::bulk_load::ensure_confidence_catalog();
        // Collect all VP tables and build a multi-UNION EXISTS check.
        let pred_rows: Vec<(i64, bool)> = Spi::connect(|c| {
            c.select(
                "SELECT id, (table_oid IS NOT NULL) AS dedicated \
                 FROM _pg_ripple.predicates ORDER BY id",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("vacuum_confidence: predicates scan: {e}"))
            .map(|row| {
                let id = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let dedicated = row.get::<bool>(2).ok().flatten().unwrap_or(false);
                (id, dedicated)
            })
            .collect()
        });

        // Build an EXISTS subquery across all VP tables.
        let mut exists_parts: Vec<String> = Vec::new();
        exists_parts.push("SELECT 1 FROM _pg_ripple.vp_rare WHERE i = c.statement_id".to_owned());
        for (pred_id, dedicated) in &pred_rows {
            if *dedicated {
                exists_parts.push(format!(
                    "SELECT 1 FROM _pg_ripple.vp_{pred_id} WHERE i = c.statement_id"
                ));
            }
        }

        let exists_sql = exists_parts.join(" UNION ALL ");
        let delete_sql = format!(
            "WITH deleted AS (\
               DELETE FROM _pg_ripple.confidence c \
               WHERE NOT EXISTS ({exists_sql}) \
               RETURNING 1 \
             ) SELECT COUNT(*)::bigint FROM deleted"
        );

        Spi::get_one::<i64>(&delete_sql)
            .unwrap_or(None)
            .unwrap_or(0)
    }

    // ── SOFT-SHACL-01b: shacl_score ──────────────────────────────────────────

    /// Return a weighted SHACL data-quality score for a named graph.
    ///
    /// Score formula:
    ///   `score = 1 − (Σᵢ wᵢ × violationsᵢ) / (Σᵢ wᵢ × applicableᵢ)`
    ///
    /// Shapes with `sh:severityWeight xsd:decimal` use that weight; others default to 1.0.
    /// Returns 1.0 when there are no applicable constraints.
    ///
    /// ```sql
    /// SELECT pg_ripple.shacl_score('http://example.org/data');
    /// ```
    #[pg_extern]
    fn shacl_score(graph_iri: &str) -> f64 {
        crate::shacl_scoring::compute_shacl_score(graph_iri)
    }

    // ── SOFT-SHACL-01c: shacl_report_scored ─────────────────────────────────

    /// Return SHACL validation results with per-violation severity scores.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.shacl_report_scored('http://example.org/data');
    /// ```
    #[pg_extern]
    fn shacl_report_scored(
        graph_iri: &str,
    ) -> TableIterator<
        'static,
        (
            name!(focus_node, String),
            name!(shape_iri, String),
            name!(result_severity, String),
            name!(result_severity_score, f64),
            name!(message, String),
        ),
    > {
        crate::shacl_scoring::shacl_report_scored(graph_iri)
    }

    // ── SOFT-SHACL-01d: log_shacl_score ─────────────────────────────────────

    /// Log the current SHACL quality score to `_pg_ripple.shacl_score_log`.
    ///
    /// ```sql
    /// SELECT pg_ripple.log_shacl_score('http://example.org/data');
    /// ```
    #[pg_extern]
    fn log_shacl_score(graph_iri: &str) {
        let score = crate::shacl_scoring::compute_shacl_score(graph_iri);
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.shacl_score_log (graph_iri, score) VALUES ($1, $2)",
            &[
                pgrx::datum::DatumWithOid::from(graph_iri),
                pgrx::datum::DatumWithOid::from(score),
            ],
        )
        .unwrap_or_else(|e| pgrx::warning!("log_shacl_score insert error: {e}"));
    }

    // ── CONF-EXPORT-01a: export_turtle_with_confidence ───────────────────────

    /// Export a graph as Turtle with RDF* confidence annotations.
    ///
    /// Triples that have confidence rows in `_pg_ripple.confidence` are annotated
    /// with `<< s p o >> pg:confidence "score"^^xsd:float .`
    ///
    /// Controlled by `pg_ripple.export_confidence` GUC (bool, default off).
    /// When the GUC is off, this function behaves identically to `export_turtle()`.
    ///
    /// ```sql
    /// SELECT pg_ripple.export_turtle_with_confidence('http://example.org/data');
    /// ```
    #[pg_extern]
    fn export_turtle_with_confidence(graph: default!(Option<&str>, "NULL")) -> String {
        if !crate::EXPORT_CONFIDENCE.get() {
            // Fall back to plain export when GUC is off.
            return crate::export::export_turtle_impl(graph);
        }
        crate::export::export_turtle_with_confidence_impl(graph)
    }
}
