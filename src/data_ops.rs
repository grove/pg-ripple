//! pg_ripple SQL API — CDC, Subject/Object pattern index, SHACL management, Async validation, Deduplication

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Change Data Capture (v0.6.0) ──────────────────────────────────────────

    /// Subscribe to triple change notifications on a NOTIFY channel.
    ///
    /// `pattern` is a predicate IRI (e.g. `<https://schema.org/name>`) or
    /// `'*'` for all predicates.  Notifications fire on INSERT and DELETE in
    /// the matching VP delta tables.  Returns the subscription ID.
    ///
    /// ```sql
    /// SELECT pg_ripple.subscribe('<https://schema.org/name>', 'my_channel');
    /// LISTEN my_channel;
    /// ```
    #[pg_extern]
    fn subscribe(pattern: &str, channel: &str) -> i64 {
        crate::cdc::subscribe(pattern, channel)
    }

    /// Remove all subscriptions for a notification channel.  Returns count removed.
    #[pg_extern]
    fn unsubscribe(channel: &str) -> i64 {
        crate::cdc::unsubscribe(channel)
    }

    // ── Subject / Object pattern index (v0.6.0) ───────────────────────────────

    /// Return the sorted array of predicate IDs for a given subject ID.
    ///
    /// Uses the `_pg_ripple.subject_patterns` index populated by the merge worker.
    /// Returns NULL if the subject has not been indexed yet (before first merge).
    #[pg_extern]
    fn subject_predicates(subject_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.subject_patterns WHERE s = $1",
            &[pgrx::datum::DatumWithOid::from(subject_id)],
        )
        .unwrap_or(None)
    }

    /// Return the sorted array of predicate IDs for a given object ID.
    ///
    /// Uses the `_pg_ripple.object_patterns` index populated by the merge worker.
    #[pg_extern]
    fn object_predicates(object_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.object_patterns WHERE o = $1",
            &[pgrx::datum::DatumWithOid::from(object_id)],
        )
        .unwrap_or(None)
    }

    // ── SHACL management (v0.7.0) ─────────────────────────────────────────────

    /// Load and store SHACL shapes from Turtle-formatted text.
    ///
    /// Parses the Turtle, extracts all NodeShape and PropertyShape definitions,
    /// and upserts them into `_pg_ripple.shacl_shapes`.  Returns the number of
    /// shapes loaded.
    #[pg_extern]
    fn load_shacl(data: &str) -> i32 {
        use crate::shacl::parse_and_store_shapes;
        parse_and_store_shapes(data)
    }

    /// Run a full SHACL validation report against all active shapes.
    ///
    /// `graph` selects which graph to validate:
    /// - NULL or empty string: default graph (id 0)
    /// - `'*'`: all graphs
    /// - An IRI: the named graph with that IRI
    ///
    /// Returns a SHACL validation report as JSONB with `conforms` and `violations`.
    #[pg_extern]
    fn validate(graph: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        crate::shacl::run_validate(graph)
    }

    /// Return a table of all loaded SHACL shapes.
    #[pg_extern]
    fn list_shapes() -> TableIterator<'static, (name!(shape_iri, String), name!(active, bool))> {
        let rows = pgrx::Spi::connect(|c| {
            let tup = c
                .select(
                    "SELECT shape_iri, active FROM _pg_ripple.shacl_shapes ORDER BY shape_iri",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_shapes SPI error: {e}"));
            let mut out: Vec<(String, bool)> = Vec::new();
            for row in tup {
                let iri: String = row.get::<&str>(1).unwrap_or(None).unwrap_or("").to_owned();
                let active: bool = row.get::<bool>(2).unwrap_or(None).unwrap_or(false);
                out.push((iri, active));
            }
            out
        });
        TableIterator::new(rows)
    }

    /// Deactivate and remove a SHACL shape by its IRI.
    ///
    /// Returns 1 if the shape was found and removed, 0 if not found.
    #[pg_extern]
    fn drop_shape(shape_uri: &str) -> i32 {
        // v0.38.0: remove associated query-planner hints first.
        crate::shacl::hints::remove_hints_for_shape(shape_uri);

        let rows_deleted = pgrx::Spi::get_one_with_args::<i64>(
            "DELETE FROM _pg_ripple.shacl_shapes WHERE shape_iri = $1 RETURNING 1",
            &[pgrx::datum::DatumWithOid::from(shape_uri)],
        )
        .unwrap_or(None)
        .unwrap_or(0);
        rows_deleted as i32
    }

    // ── Async validation pipeline (v0.8.0) ────────────────────────────────────

    /// Manually process up to `batch_size` items from the async validation queue.
    ///
    /// Violations found are moved to `_pg_ripple.dead_letter_queue`.
    /// Returns the number of items processed.
    ///
    /// Normally the background worker processes the queue automatically when
    /// `pg_ripple.shacl_mode = 'async'`.  This function is useful for testing
    /// or for draining the queue on demand.
    #[pg_extern]
    fn process_validation_queue(batch_size: default!(i64, "1000")) -> i64 {
        crate::shacl::process_validation_batch(batch_size)
    }

    /// Return the number of items currently pending in the async validation queue.
    #[pg_extern]
    fn validation_queue_length() -> i64 {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.validation_queue")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    /// Return the number of items in the dead-letter queue (async violations).
    #[pg_extern]
    fn dead_letter_count() -> i64 {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dead_letter_queue")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    /// Return all entries in the dead-letter queue as JSONB.
    ///
    /// Each row includes `s_id`, `p_id`, `o_id`, `g_id`, `stmt_id`,
    /// `violation` (JSONB), and `detected_at` (timestamptz).
    #[pg_extern]
    fn dead_letter_queue() -> pgrx::JsonB {
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            let tup = c
                .select(
                    "SELECT s_id, p_id, o_id, g_id, stmt_id, violation::text, detected_at::text \
                     FROM _pg_ripple.dead_letter_queue ORDER BY id ASC",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("dead_letter_queue SPI error: {e}"));
            let mut out: Vec<serde_json::Value> = Vec::new();
            for row in tup {
                let s_id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let p_id: i64 = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                let o_id: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                let g_id: i64 = row.get::<i64>(4).ok().flatten().unwrap_or(0);
                let stmt_id: i64 = row.get::<i64>(5).ok().flatten().unwrap_or(0);
                let violation_text: String =
                    row.get::<&str>(6).ok().flatten().unwrap_or("").to_owned();
                let detected_at: String =
                    row.get::<&str>(7).ok().flatten().unwrap_or("").to_owned();
                let violation_json: serde_json::Value =
                    serde_json::from_str(&violation_text).unwrap_or(serde_json::Value::Null);
                out.push(serde_json::json!({
                    "s_id": s_id,
                    "p_id": p_id,
                    "o_id": o_id,
                    "g_id": g_id,
                    "stmt_id": stmt_id,
                    "violation": violation_json,
                    "detected_at": detected_at
                }));
            }
            out
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Clear all entries from the dead-letter queue.
    ///
    /// Returns the number of rows deleted.
    #[pg_extern]
    fn drain_dead_letter_queue() -> i64 {
        pgrx::Spi::get_one::<i64>("DELETE FROM _pg_ripple.dead_letter_queue RETURNING 1")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    // ── Deduplication functions (v0.7.0) ──────────────────────────────────────

    /// Remove duplicate `(s, o, g)` rows for a single predicate, keeping the
    /// row with the lowest SID (oldest assertion).
    ///
    /// - In `_delta` tables: uses DELETE with a `ctid NOT IN (MIN(ctid) GROUP BY s,o,g)` pattern.
    /// - In `_main` tables: inserts tombstone rows for duplicate-SID rows so they are
    ///   filtered at query time and removed on the next merge cycle.
    /// - In `vp_rare`: uses the same MIN(ctid) pattern with a `p = pred_id` filter.
    ///
    /// Runs ANALYZE on all affected tables after deduplication.
    /// Returns the total count of rows removed.
    #[pg_extern]
    fn deduplicate_predicate(p_iri: &str) -> i64 {
        crate::storage::deduplicate_predicate(p_iri)
    }

    /// Remove duplicate `(s, o, g)` rows across all predicates and `vp_rare`.
    ///
    /// Applies `deduplicate_predicate` for each predicate with a dedicated VP table,
    /// then deduplicates `vp_rare`.
    /// Returns the total count of rows removed.
    #[pg_extern]
    fn deduplicate_all() -> i64 {
        crate::storage::deduplicate_all()
    }
}
