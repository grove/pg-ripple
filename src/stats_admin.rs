//! pg_ripple SQL API — pg_trickle integration, Statistics (v0.6.0)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── pg_trickle integration (v0.6.0, optional) ────────────────────────────

    /// Enable live statistics via pg_trickle stream tables.
    ///
    /// Creates `_pg_ripple.predicate_stats` and `_pg_ripple.graph_stats` stream
    /// tables using pg_trickle.  These let `pg_ripple.stats()` return results
    /// instantly (no full VP table scan) when pg_trickle is installed and
    /// `enable_live_statistics()` has been called.
    ///
    /// Returns `true` if stream tables were created, `false` if pg_trickle is
    /// not installed (no error is raised — pg_trickle is optional).
    ///
    /// ```sql
    /// SELECT pg_ripple.enable_live_statistics();
    /// ```
    #[pg_extern]
    fn enable_live_statistics() -> bool {
        // Check if pg_trickle is installed.
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; live statistics are unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_live_statistics() to enable."
            );
            return false;
        }

        // Create _pg_ripple.predicate_stats stream table via pg_trickle.
        // Refreshed every 5 seconds; reads from the predicates catalog +
        // dedicated VP table reltuples (fast, planner-statistics-based).
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.predicate_stats',
                $$
                    SELECT
                        p.id          AS predicate_id,
                        d.value       AS predicate_iri,
                        p.triple_count,
                        CASE WHEN p.table_oid IS NOT NULL THEN 'dedicated'
                             ELSE 'rare' END AS storage_type
                    FROM _pg_ripple.predicates p
                    JOIN _pg_ripple.dictionary d ON d.id = p.id
                    ORDER BY p.triple_count DESC
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.predicate_stats stream table: {}",
                e
            );
        });

        // Create _pg_ripple.graph_stats stream table via pg_trickle.
        // Refreshed every 10 seconds.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.graph_stats',
                $$
                    SELECT
                        g.id       AS graph_id,
                        d.value    AS graph_iri,
                        g.triple_count
                    FROM _pg_ripple.graphs g
                    JOIN _pg_ripple.dictionary d ON d.id = g.id
                    ORDER BY g.triple_count DESC
                $$,
                '10s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.graph_stats stream table: {}",
                e
            );
        });

        // Create _pg_ripple.vp_cardinality stream table — per-predicate live
        // row counts for BGP join reordering without waiting for ANALYZE.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.vp_cardinality',
                $$
                    SELECT
                        p.id     AS predicate_id,
                        c.reltuples::bigint AS estimated_rows
                    FROM _pg_ripple.predicates p
                    JOIN pg_class c ON c.oid = p.table_oid
                    WHERE p.table_oid IS NOT NULL
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.vp_cardinality stream table: {}",
                e
            );
        });

        // Create _pg_ripple.rare_predicate_candidates stream table with
        // IMMEDIATE mode — replaces the merge-worker GROUP BY polling for
        // VP promotion detection.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.rare_predicate_candidates',
                $$
                    SELECT p AS predicate_id, count(*) AS triple_count
                    FROM _pg_ripple.vp_rare
                    GROUP BY p
                    HAVING count(*) >= current_setting('pg_ripple.vp_promotion_threshold')::bigint
                $$,
                'IMMEDIATE'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.rare_predicate_candidates stream table: {}",
                e
            );
        });

        true
    }

    // ── pg_trickle SHACL violation monitors (v0.7.0, optional) ──────────────

    /// Enable SHACL violation monitors via pg_trickle stream tables.
    ///
    /// Creates the `_pg_ripple.violation_summary` stream table that aggregates
    /// `_pg_ripple.dead_letter_queue` by shape IRI and severity.  This avoids
    /// full `GROUP BY` scans of a potentially large dead-letter queue when
    /// monitoring dashboards or Prometheus `/metrics` poll for violation counts.
    ///
    /// The stream table is refreshed every 5 seconds by pg_trickle's IVM engine.
    ///
    /// Returns `true` if the stream table was created, `false` if pg_trickle is
    /// not installed.  No error is raised — pg_trickle is optional.
    ///
    /// ```sql
    /// SELECT pg_ripple.enable_shacl_monitors();
    /// -- Then query the summary:
    /// SELECT * FROM _pg_ripple.violation_summary;
    /// ```
    #[pg_extern]
    fn enable_shacl_monitors() -> bool {
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; SHACL violation monitors are unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_shacl_monitors() to enable."
            );
            return false;
        }

        // violation_summary — aggregate dead_letter_queue by shape + severity + graph.
        // Refreshed every 5 seconds via pg_trickle incremental view maintenance.
        // Reading the summary is an index scan on a small table rather than a
        // full GROUP BY over potentially millions of violation rows.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.violation_summary',
                $$
                    SELECT
                        dlq.violation ->> 'shapeIRI'   AS shape_iri,
                        dlq.violation ->> 'severity'   AS severity,
                        dlq.g_id                       AS graph_id,
                        COUNT(*)                       AS violation_count,
                        MAX(dlq.detected_at)           AS last_seen
                    FROM _pg_ripple.dead_letter_queue dlq
                    GROUP BY 1, 2, 3
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.violation_summary stream table: {}",
                e
            );
        });

        true
    }

    // ── pg_trickle SHACL DAG monitors (v0.8.0, optional) ────────────────────

    /// Enable multi-shape DAG validation via pg_trickle stream tables.
    ///
    /// For each active, compilable SHACL shape in `_pg_ripple.shacl_shapes`,
    /// creates a per-shape violation-detection stream table named
    /// `_pg_ripple.shacl_viol_{shape_suffix}` (refreshed in `IMMEDIATE` mode
    /// so violations are detected within the same transaction).  Supported
    /// constraint types: `sh:minCount`, `sh:maxCount`, `sh:datatype`,
    /// `sh:class`.  Complex combinators (`sh:or`, `sh:and`, `sh:not`,
    /// `sh:qualifiedValueShape`) are not compiled to stream tables; shapes
    /// that use only those constraints are skipped.
    ///
    /// After creating all per-shape tables, creates
    /// `_pg_ripple.violation_summary_dag` — a pg_trickle stream table (5 s
    /// refresh) that aggregates per-shape violation counts.  Because it reads
    /// from the per-shape stream tables, pg_trickle refreshes them in
    /// topological order (per-shape first, summary last).  When violations are
    /// resolved the summary automatically drops to zero — unlike the
    /// dead-letter-queue-based `_pg_ripple.violation_summary` from v0.7.0,
    /// which requires manual cleanup.
    ///
    /// Returns the number of per-shape stream tables created.  Returns 0 with
    /// a warning when pg_trickle is not installed.  No error is raised.
    ///
    /// ```sql
    /// -- Load shapes, then enable DAG monitors:
    /// SELECT pg_ripple.load_shacl('...');
    /// SELECT pg_ripple.enable_shacl_dag_monitors();
    /// -- Query the live summary:
    /// SELECT * FROM _pg_ripple.violation_summary_dag;
    /// ```
    #[pg_extern]
    fn enable_shacl_dag_monitors() -> i64 {
        crate::shacl::compile_dag_monitors()
    }

    /// Disable SHACL DAG monitors by dropping all per-shape violation stream
    /// tables and the `violation_summary_dag` aggregate table.
    ///
    /// Also clears the `_pg_ripple.shacl_dag_monitors` catalog.  Returns the
    /// number of per-shape stream tables dropped.
    ///
    /// ```sql
    /// SELECT pg_ripple.disable_shacl_dag_monitors();
    /// ```
    #[pg_extern]
    fn disable_shacl_dag_monitors() -> i64 {
        crate::shacl::drop_dag_monitors()
    }

    /// List all active SHACL DAG monitor stream tables.
    ///
    /// Returns one row per compiled shape with:
    /// - `shape_iri` — the shape's IRI
    /// - `stream_table` — fully-qualified name of the violation stream table
    /// - `constraints` — human-readable summary of compiled constraints
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.list_shacl_dag_monitors();
    /// ```
    #[pg_extern]
    fn list_shacl_dag_monitors() -> TableIterator<
        'static,
        (
            name!(shape_iri, String),
            name!(stream_table, String),
            name!(constraints, String),
        ),
    > {
        let rows = crate::shacl::list_dag_monitors();
        TableIterator::new(rows)
    }

    // ── Statistics (v0.6.0) ───────────────────────────────────────────────────

    /// Return extension statistics as JSONB.
    ///
    /// Includes total triple count, per-predicate storage sizes, delta/main
    /// split counts, and (when shared_preload_libraries is set) cache hit ratio.
    /// When pg_trickle is installed and `enable_live_statistics()` has been
    /// called, reads per-predicate counts from the `_pg_ripple.predicate_stats`
    /// stream table (instant, no full scan) instead of re-scanning VP tables.
    ///
    /// ```sql
    /// SELECT pg_ripple.stats();
    /// ```
    #[pg_extern]
    fn stats() -> pgrx::JsonB {
        // When pg_trickle live statistics are enabled, the total triple count
        // is read from the predicate_stats stream table (sum of triple_count
        // across all predicates) — this avoids a full VP table scan and
        // returns instantly.  Fall back to the full scan otherwise.
        let use_live_stats = crate::has_live_statistics();

        let total: i64 = if use_live_stats {
            pgrx::Spi::get_one::<i64>(
                "SELECT COALESCE(sum(triple_count), 0)::bigint \
                 FROM _pg_ripple.predicate_stats",
            )
            .unwrap_or(None)
            .unwrap_or_else(crate::storage::total_triple_count)
        } else {
            crate::storage::total_triple_count()
        };

        let pred_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let rare_count: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.vp_rare")
                .unwrap_or(None)
                .unwrap_or(0);

        let htap_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE htap = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let delta_rows: i64 =
            if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                crate::shmem::TOTAL_DELTA_ROWS
                    .get()
                    .load(std::sync::atomic::Ordering::Relaxed)
            } else {
                -1 // shmem not available (loaded without shared_preload_libraries)
            };

        let merge_pid: i32 = if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire)
        {
            crate::shmem::MERGE_WORKER_PID
                .get()
                .load(std::sync::atomic::Ordering::Relaxed)
        } else {
            0
        };

        let mut obj = serde_json::Map::new();
        obj.insert("total_triples".to_string(), serde_json::json!(total));
        obj.insert(
            "dedicated_predicates".to_string(),
            serde_json::json!(pred_count),
        );
        obj.insert("htap_predicates".to_string(), serde_json::json!(htap_count));
        obj.insert("rare_triples".to_string(), serde_json::json!(rare_count));
        obj.insert(
            "unmerged_delta_rows".to_string(),
            serde_json::json!(delta_rows),
        );
        obj.insert("merge_worker_pid".to_string(), serde_json::json!(merge_pid));
        obj.insert(
            "live_statistics_enabled".to_string(),
            serde_json::json!(use_live_stats),
        );

        // v0.22.0: encode cache statistics (4-way set-associative).
        let (hits, misses, evictions, utilisation) = crate::shmem::get_cache_stats();
        let cache_capacity = crate::shmem::ENCODE_CACHE_CAPACITY as i64;
        let cache_utilization_pct = (utilisation * 100.0) as i64;
        obj.insert(
            "encode_cache_capacity".to_string(),
            serde_json::json!(cache_capacity),
        );
        obj.insert(
            "encode_cache_utilization_pct".to_string(),
            serde_json::json!(cache_utilization_pct),
        );
        obj.insert("encode_cache_hits".to_string(), serde_json::json!(hits));
        obj.insert("encode_cache_misses".to_string(), serde_json::json!(misses));
        obj.insert(
            "encode_cache_evictions".to_string(),
            serde_json::json!(evictions),
        );

        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Health check function (v0.25.0).
    ///
    /// Returns a JSONB object with key health indicators for operations dashboards:
    /// - `merge_worker`: `"ok"` if the merge worker PID is recorded in shared memory,
    ///   `"stalled"` otherwise.
    /// - `cache_hit_rate`: fraction of dictionary encode lookups that hit the
    ///   backend-local LRU cache (0.0–1.0).
    /// - `catalog_consistent`: `true` if the number of VP tables in `pg_class` matches
    ///   the number of promoted predicates in `_pg_ripple.predicates`.
    /// - `orphaned_rare_rows`: number of `vp_rare` rows whose predicate has a dedicated
    ///   VP table (should be 0 after a healthy promotion cycle).
    #[pg_extern]
    fn canary() -> pgrx::JsonB {
        use serde_json::{Map, Number, Value as Json};

        // merge_worker: check PID in shared memory.
        let merge_worker_pid =
            if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                crate::shmem::MERGE_WORKER_PID
                    .get()
                    .load(std::sync::atomic::Ordering::Relaxed)
            } else {
                0
            };
        let merge_worker_status = if merge_worker_pid > 0 {
            "ok"
        } else {
            "stalled"
        };

        // cache_hit_rate: from shmem stats.
        let (hits, misses, _, _) = crate::shmem::get_cache_stats();
        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64) / (total as f64)
        } else {
            1.0_f64
        };

        // catalog_consistent: VP table count == promoted predicate count.
        let pg_table_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relname LIKE 'vp_%_delta'",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let predicate_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE htap = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let catalog_consistent = pg_table_count == predicate_count;

        // orphaned_rare_rows: vp_rare rows for promoted predicates.
        let orphaned: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint \
             FROM _pg_ripple.vp_rare r \
             WHERE EXISTS ( \
               SELECT 1 FROM _pg_ripple.predicates p WHERE p.id = r.p AND p.htap = true \
             )",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let mut obj = Map::new();
        obj.insert(
            "merge_worker".to_owned(),
            Json::String(merge_worker_status.to_owned()),
        );
        obj.insert(
            "cache_hit_rate".to_owned(),
            Json::Number(Number::from_f64(hit_rate).unwrap_or(Number::from(0))),
        );
        obj.insert(
            "catalog_consistent".to_owned(),
            Json::Bool(catalog_consistent),
        );
        obj.insert(
            "orphaned_rare_rows".to_owned(),
            Json::Number(Number::from(orphaned)),
        );

        pgrx::JsonB(Json::Object(obj))
    }

    // ── v0.51.0: predicate workload statistics ────────────────────────────────

    /// Return per-predicate workload statistics from `_pg_ripple.predicate_stats`.
    ///
    /// Columns:
    /// - `predicate_iri TEXT`  — decoded IRI of the predicate
    /// - `query_count BIGINT`  — number of SPARQL queries that touched this predicate
    /// - `merge_count BIGINT`  — number of HTAP merge cycles involving this predicate
    /// - `last_merged TIMESTAMPTZ` — timestamp of the most recent merge for this predicate
    ///
    /// Returns an empty set if `_pg_ripple.predicate_stats` is empty or unpopulated.
    #[pg_extern]
    fn predicate_workload_stats() -> TableIterator<
        'static,
        (
            name!(predicate_iri, String),
            name!(query_count, i64),
            name!(merge_count, i64),
            name!(last_merged, Option<pgrx::datum::TimestampWithTimeZone>),
        ),
    > {
        let rows: Vec<(String, i64, i64, Option<pgrx::datum::TimestampWithTimeZone>)> =
            Spi::connect(|c| {
                let results = c.select(
                    "SELECT d.value, ps.query_count, ps.merge_count, ps.last_merged
                     FROM _pg_ripple.predicate_stats ps
                     JOIN _pg_ripple.dictionary d ON d.id = ps.predicate_id
                     ORDER BY ps.query_count DESC",
                    None,
                    &[],
                );
                match results {
                    Ok(tup) => {
                        let mut out = Vec::new();
                        for row in tup {
                            let iri = row.get::<&str>(1).ok().flatten().unwrap_or("").to_owned();
                            let qc = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                            let mc = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                            let lm = row
                                .get::<pgrx::datum::TimestampWithTimeZone>(4)
                                .ok()
                                .flatten();
                            out.push((iri, qc, mc, lm));
                        }
                        out
                    }
                    Err(_) => vec![],
                }
            });
        TableIterator::new(rows.into_iter().map(|(iri, qc, mc, lm)| (iri, qc, mc, lm)))
    }
}
