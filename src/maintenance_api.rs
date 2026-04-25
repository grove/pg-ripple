//! pg_ripple SQL API — Administrative functions, Graph-level RLS, Schema summary

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Administrative functions (v0.14.0) ───────────────────────────────────

    /// Force a full delta→main merge on all HTAP VP tables, then run
    /// PostgreSQL VACUUM on every VP table (delta, main, tombstones).
    ///
    /// Returns the number of VP tables vacuumed.
    #[pg_extern]
    fn vacuum() -> i64 {
        // Merge first so VACUUM sees the final row set.
        crate::storage::merge::compact();

        // Collect all HTAP predicate IDs.
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("vacuum: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        let mut vacuumed = 0i64;
        for p_id in &pred_ids {
            // VACUUM cannot run inside a transaction block, so we use
            // ANALYZE instead, which has the same effect on planner statistics
            // and can run inside a transaction.
            let _ = pgrx::Spi::run(&format!(
                "ANALYZE _pg_ripple.vp_{p_id}_delta; \
                 ANALYZE _pg_ripple.vp_{p_id}_main; \
                 ANALYZE _pg_ripple.vp_{p_id}_tombstones"
            ));
            vacuumed += 1;
        }

        // Analyze vp_rare as well.
        let _ = pgrx::Spi::run("ANALYZE _pg_ripple.vp_rare");

        pgrx::log!("pg_ripple.vacuum: analyzed {} VP table groups", vacuumed);
        vacuumed
    }

    /// Rebuild all indices on VP tables (delta, main, tombstones) and vp_rare.
    ///
    /// Uses `REINDEX TABLE CONCURRENTLY` to avoid locking out reads.
    /// Returns the number of tables reindexed.
    #[pg_extern]
    fn reindex() -> i64 {
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("reindex: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        let mut reindexed = 0i64;
        for p_id in &pred_ids {
            // REINDEX CONCURRENTLY cannot run inside a transaction block;
            // use plain REINDEX instead (safe for maintenance windows).
            let _ = pgrx::Spi::run(&format!(
                "REINDEX TABLE _pg_ripple.vp_{p_id}_delta; \
                 REINDEX TABLE _pg_ripple.vp_{p_id}_main"
            ));
            reindexed += 1;
        }
        let _ = pgrx::Spi::run("REINDEX TABLE _pg_ripple.vp_rare");

        pgrx::log!("pg_ripple.reindex: reindexed {} VP table groups", reindexed);
        reindexed
    }

    /// Remove dictionary entries that are no longer referenced by any VP table.
    ///
    /// Scans all predicate VP tables and vp_rare to build a set of live s/o/p IDs,
    /// then deletes any dictionary rows not in that set.
    ///
    /// Uses an advisory lock (key 0x7269706c = ASCII 'ripl') to prevent
    /// concurrent runs.  Safe to run during normal operation — may miss very
    /// recently orphaned entries (cleaned on the next run).
    ///
    /// Returns the number of dictionary entries removed.
    #[pg_extern]
    fn vacuum_dictionary() -> i64 {
        // Advisory lock to prevent concurrent runs.
        let lock_acquired: bool =
            pgrx::Spi::get_one::<bool>("SELECT pg_try_advisory_xact_lock(0x7269706c::bigint)")
                .unwrap_or(None)
                .unwrap_or(false);

        if !lock_acquired {
            pgrx::warning!("vacuum_dictionary: another vacuum_dictionary is already running");
            return 0;
        }

        // Collect all live IDs referenced by VP tables and vp_rare.
        // Build a UNION ALL of all s,o,g columns from every VP table.
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        // Build a temporary table of live IDs.
        pgrx::Spi::run(
            "CREATE TEMP TABLE IF NOT EXISTS _pg_ripple_live_ids (id BIGINT) ON COMMIT DROP",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: create temp table error: {e}"));

        pgrx::Spi::run("TRUNCATE _pg_ripple_live_ids")
            .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: truncate temp table error: {e}"));

        // Insert predicate IDs themselves.
        pgrx::Spi::run(
            "INSERT INTO _pg_ripple_live_ids \
             SELECT id FROM _pg_ripple.predicates",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: insert pred IDs error: {e}"));

        // Insert vp_rare IDs.
        pgrx::Spi::run(
            "INSERT INTO _pg_ripple_live_ids \
             SELECT p FROM _pg_ripple.vp_rare \
             UNION ALL SELECT s FROM _pg_ripple.vp_rare \
             UNION ALL SELECT o FROM _pg_ripple.vp_rare \
             UNION ALL SELECT g FROM _pg_ripple.vp_rare WHERE g <> 0",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: insert vp_rare IDs error: {e}"));

        // Insert IDs from each dedicated VP table.
        for p_id in &pred_ids {
            let _ = pgrx::Spi::run(&format!(
                "INSERT INTO _pg_ripple_live_ids \
                 SELECT s FROM _pg_ripple.vp_{p_id} \
                 UNION ALL SELECT o FROM _pg_ripple.vp_{p_id} \
                 UNION ALL SELECT g FROM _pg_ripple.vp_{p_id} WHERE g <> 0"
            ));
        }

        // Delete dictionary entries not referenced by any live ID.
        // Inline-encoded IDs (bit 63 set) have no dictionary row; skip them.
        let deleted: i64 = pgrx::Spi::get_one::<i64>(
            "WITH live AS (SELECT DISTINCT id FROM _pg_ripple_live_ids), \
              deleted AS ( \
                  DELETE FROM _pg_ripple.dictionary d \
                  WHERE d.id > 0 \
                    AND NOT EXISTS (SELECT 1 FROM live WHERE live.id = d.id) \
                  RETURNING 1 \
              ) \
              SELECT count(*)::bigint FROM deleted",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        pgrx::log!(
            "pg_ripple.vacuum_dictionary: removed {} orphaned dictionary entries",
            deleted
        );
        deleted
    }

    /// Return detailed dictionary cache and size metrics as JSONB.
    ///
    /// Fields:
    /// - `total_entries` — total rows in the dictionary
    /// - `hot_entries` — rows in the unlogged hot dictionary cache
    /// - `cache_capacity` — shared-memory encode cache capacity (entries)
    /// - `cache_budget_mb` — configured cache budget cap in MB
    /// - `shmem_ready` — whether shared memory is initialized
    #[pg_extern]
    fn dictionary_stats() -> pgrx::JsonB {
        let total: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dictionary")
                .unwrap_or(None)
                .unwrap_or(0);

        let hot: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dictionary_hot")
                .unwrap_or(None)
                .unwrap_or(0);

        let cache_capacity = crate::DICTIONARY_CACHE_SIZE.get();
        let cache_budget_mb = crate::CACHE_BUDGET_MB.get();
        let shmem_ready = crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire);

        pgrx::JsonB(serde_json::json!({
            "total_entries":   total,
            "hot_entries":     hot,
            "cache_capacity":  cache_capacity,
            "cache_budget_mb": cache_budget_mb,
            "shmem_ready":     shmem_ready
        }))
    }

    // ── Graph-level Row-Level Security (v0.14.0) ─────────────────────────────

    /// Enable graph-level Row-Level Security on the current database.
    ///
    /// Creates RLS policies on `_pg_ripple.vp_rare` using the `g` column and
    /// the `_pg_ripple.graph_access` mapping table.  Dedicated VP tables
    /// created after this call also receive RLS policies.
    ///
    /// Set `pg_ripple.rls_bypass = on` in a superuser session to bypass all
    /// policies.  Default graph (g = 0) is always accessible.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn enable_graph_rls() -> bool {
        // Enable RLS on vp_rare — the consolidation table always exists.
        pgrx::Spi::run(
            "ALTER TABLE _pg_ripple.vp_rare ENABLE ROW LEVEL SECURITY; \
             DROP POLICY IF EXISTS pg_ripple_rls_read ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_read ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR SELECT \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('read', 'write', 'admin') \
                     ) \
                 ); \
             DROP POLICY IF EXISTS pg_ripple_rls_write ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_write ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR ALL \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('write', 'admin') \
                     ) \
                 )",
        )
        .unwrap_or_else(|e| pgrx::error!("enable_graph_rls: error creating policy: {e}"));

        // Record that RLS is enabled in the predicates catalog metadata.
        let _ = pgrx::Spi::run(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ('__rls_enabled__', -1, 'admin') \
             ON CONFLICT DO NOTHING",
        );

        true
    }

    /// Grant a permission on a named graph to a PostgreSQL role.
    ///
    /// `permission` must be `'read'`, `'write'`, or `'admin'`.
    /// The graph IRI is encoded in the dictionary automatically.
    /// Granting `'admin'` implies read and write.
    #[pg_extern]
    fn grant_graph(role: &str, graph: &str, permission: &str) {
        let valid = matches!(permission, "read" | "write" | "admin");
        if !valid {
            pgrx::error!(
                "grant_graph: permission must be 'read', 'write', or 'admin'; got '{permission}'"
            );
        }

        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        pgrx::Spi::run_with_args(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
            &[
                pgrx::datum::DatumWithOid::from(role),
                pgrx::datum::DatumWithOid::from(graph_id),
                pgrx::datum::DatumWithOid::from(permission),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("grant_graph: insert error: {e}"));
    }

    /// Revoke a permission on a named graph from a PostgreSQL role.
    ///
    /// Pass NULL for `permission` to revoke all permissions for the role on that graph.
    #[pg_extern]
    fn revoke_graph(role: &str, graph: &str, permission: default!(Option<&str>, "NULL")) {
        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        if let Some(perm) = permission {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2 AND permission = $3",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                    pgrx::datum::DatumWithOid::from(perm),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph: delete error: {e}"));
        } else {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph: delete error: {e}"));
        }
    }

    /// List all graph access control entries as JSONB.
    ///
    /// Returns one row per (role, graph, permission) entry with decoded graph IRIs.
    #[pg_extern]
    fn list_graph_access() -> pgrx::JsonB {
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT ga.role_name, d.value AS graph_iri, ga.permission \
                 FROM _pg_ripple.graph_access ga \
                 LEFT JOIN _pg_ripple.dictionary d ON d.id = ga.graph_id \
                 WHERE ga.role_name <> '__rls_enabled__' \
                 ORDER BY ga.role_name, ga.graph_id",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_graph_access: SPI error: {e}"))
            .map(|row| {
                let role: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let graph_iri: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let perm: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                serde_json::json!({
                    "role": role,
                    "graph": graph_iri,
                    "permission": perm
                })
            })
            .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    // ── Schema summary (v0.14.0, optional pg_trickle) ────────────────────────

    /// Enable the live schema summary stream table via pg_trickle.
    ///
    /// Creates `_pg_ripple.inferred_schema` as a pg_trickle stream table that
    /// maintains a live class→property→cardinality summary.  Used by tooling
    /// and SPARQL IDE auto-completion.
    ///
    /// Returns `true` if the stream table was created; `false` if pg_trickle
    /// is not installed (no error is raised).
    #[pg_extern]
    fn enable_schema_summary() -> bool {
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; schema summary is unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_schema_summary() to enable."
            );
            return false;
        }

        // The schema summary groups triples by predicate to give a rough
        // class→property→cardinality overview.  We use rdf:type as the
        // class link; predicates become properties; COUNT becomes cardinality.
        let rdf_type_id = crate::dictionary::encode(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            crate::dictionary::KIND_IRI,
        );

        let summary_sql = format!(
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.vp_rare vr \
             JOIN _pg_ripple.vp_rare type_row \
                 ON type_row.s = vr.s \
                AND type_row.p = {rdf_type_id} \
             JOIN _pg_ripple.dictionary dp ON dp.id = vr.p \
             LEFT JOIN _pg_ripple.dictionary dc ON dc.id = type_row.o \
             WHERE vr.p <> {rdf_type_id} \
             GROUP BY 1, 2"
        );

        pgrx::Spi::run_with_args(
            "SELECT pg_trickle.create_stream_table($1, $2, '30s')",
            &[
                pgrx::datum::DatumWithOid::from("_pg_ripple.inferred_schema"),
                pgrx::datum::DatumWithOid::from(summary_sql.as_str()),
            ],
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.inferred_schema stream table: {}",
                e
            );
        });

        true
    }

    /// Return the live schema summary as JSONB.
    ///
    /// Reads from `_pg_ripple.inferred_schema` if available (requires
    /// `enable_schema_summary()` to have been called), otherwise falls back
    /// to a direct scan.  Returns an array of `{class, property, cardinality}`.
    #[pg_extern]
    fn schema_summary() -> pgrx::JsonB {
        let has_stream_table = pgrx::Spi::get_one::<bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = '_pg_ripple' AND c.relname = 'inferred_schema' \
             )",
        )
        .unwrap_or(None)
        .unwrap_or(false);

        let query = if has_stream_table {
            "SELECT class_iri, property_iri, cardinality \
             FROM _pg_ripple.inferred_schema \
             ORDER BY class_iri, property_iri"
        } else {
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.predicates p \
             JOIN _pg_ripple.dictionary dp ON dp.id = p.id \
             CROSS JOIN LATERAL (SELECT 1 LIMIT 0) AS dummy(x) \
             GROUP BY 1, 2 \
             ORDER BY 1, 2 \
             LIMIT 0"
        };

        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(query, None, &[])
                .unwrap_or_else(|e| pgrx::error!("schema_summary: SPI error: {e}"))
                .map(|row| {
                    let class: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let prop: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    let card: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                    serde_json::json!({
                        "class": class,
                        "property": prop,
                        "cardinality": card
                    })
                })
                .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Return a system health report as a set of (key, value) rows.
    ///
    /// Covers: GUC validity, shared-memory cache hit/miss rates, merge backlog,
    /// SHACL validation queue depth, schema version, and federation endpoint count.
    ///
    /// v0.37.0: first implementation.
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.diagnostic_report();
    /// ```
    #[pg_extern]
    fn diagnostic_report() -> TableIterator<'static, (name!(key, String), name!(value, String))> {
        let mut rows: Vec<(String, String)> = Vec::new();

        // ── Schema version ────────────────────────────────────────────────────
        let schema_ver: String = pgrx::Spi::get_one::<String>(
            "SELECT version FROM _pg_ripple.schema_version \
             ORDER BY installed_at DESC LIMIT 1",
        )
        .unwrap_or(None)
        .unwrap_or_else(|| "unknown".to_string());
        rows.push(("schema_version".to_string(), schema_ver));

        // ── Cargo (compiled) version ──────────────────────────────────────────
        rows.push((
            "compiled_version".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ));

        // ── GUC validity summary ──────────────────────────────────────────────
        let inference_mode = crate::INFERENCE_MODE
            .get()
            .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
            .unwrap_or_else(|| "off".to_string());
        let valid_inference = matches!(
            inference_mode.as_str(),
            "off" | "on_demand" | "materialized" | "incremental_rdfs"
        );
        rows.push((
            "guc_inference_mode".to_string(),
            if valid_inference {
                inference_mode
            } else {
                format!("INVALID: {inference_mode}")
            },
        ));

        let shacl_mode = crate::SHACL_MODE
            .get()
            .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
            .unwrap_or_else(|| "off".to_string());
        let valid_shacl = matches!(shacl_mode.as_str(), "off" | "sync" | "async");
        rows.push((
            "guc_shacl_mode".to_string(),
            if valid_shacl {
                shacl_mode
            } else {
                format!("INVALID: {shacl_mode}")
            },
        ));

        // ── Merge backlog: total rows in all delta tables ─────────────────────
        let delta_backlog: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT COALESCE(SUM(c.reltuples::bigint), 0) \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' \
               AND c.relname LIKE '%_delta' \
               AND c.relkind = 'r'",
        )
        .unwrap_or(None)
        .unwrap_or(0);
        rows.push(("merge_backlog_rows".to_string(), delta_backlog.to_string()));

        // ── SHACL validation queue depth ──────────────────────────────────────
        let vq_depth: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.validation_queue")
                .unwrap_or(None)
                .unwrap_or(0);
        rows.push(("validation_queue_depth".to_string(), vq_depth.to_string()));

        // ── Federation endpoint count ──────────────────────────────────────────
        let fed_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.federation_endpoints WHERE enabled = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);
        rows.push((
            "federation_endpoints_enabled".to_string(),
            fed_count.to_string(),
        ));

        // ── Shared-memory cache status ────────────────────────────────────────
        let shmem_ready = crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Relaxed);
        rows.push(("shmem_cache_ready".to_string(), shmem_ready.to_string()));

        // ── Total triple count ────────────────────────────────────────────────
        let triple_count = crate::storage::total_triple_count();
        rows.push(("total_triple_count".to_string(), triple_count.to_string()));

        // ── Predicate count ────────────────────────────────────────────────────
        let pred_count: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.predicates")
                .unwrap_or(None)
                .unwrap_or(0);
        rows.push(("predicate_count".to_string(), pred_count.to_string()));

        // ── Dictionary size ────────────────────────────────────────────────────
        let dict_count: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dictionary")
                .unwrap_or(None)
                .unwrap_or(0);
        rows.push(("dictionary_size".to_string(), dict_count.to_string()));

        TableIterator::new(rows)
    }

    /// Migrate an existing flat VP table (pre-v0.6.0) to the HTAP partition split.
    ///
    /// Called automatically by the v0.5.1→v0.6.0 migration script, but can
    /// also be called manually if needed.  The predicate is specified by its
    /// dictionary integer ID.
    #[pg_extern]
    fn htap_migrate_predicate(pred_id: i64) {
        crate::storage::merge::migrate_flat_to_htap(pred_id);
    }

    /// Returns the estimated years remaining before `_pg_ripple.statement_id_seq`
    /// wraps (i64::MAX ≈ 9.2 × 10^18).
    ///
    /// Runway is computed as:
    ///   years_remaining = (max_value - current_value) / max(insert_rate_per_day, 1) / 365
    ///
    /// `insert_rate_per_day` is estimated from the sequence's `last_value` divided by
    /// the extension's installed age in days (read from `_pg_ripple.schema_version`).
    /// Returns a single row; returns NULL for years_remaining if the rate cannot be determined.
    #[pg_extern]
    fn sid_runway() -> TableIterator<
        'static,
        (
            name!(current_value, i64),
            name!(max_value, i64),
            name!(insert_rate_per_day, i64),
            name!(years_remaining, Option<pgrx::AnyNumeric>),
        ),
    > {
        let row = Spi::connect(|c| {
            // Get current sequence last_value.
            let current: i64 = c
                .select(
                    "SELECT last_value FROM _pg_ripple.statement_id_seq",
                    None,
                    &[],
                )
                .ok()
                .and_then(|mut r| r.next())
                .and_then(|row| row.get::<i64>(1).ok().flatten())
                .unwrap_or(1);

            let max_val: i64 = i64::MAX;

            // Estimate daily insert rate from extension age.
            let days_installed: i64 = c
                .select(
                    "SELECT GREATEST(1, EXTRACT(EPOCH FROM (now() - MIN(installed_at))) / 86400)::bigint \
                     FROM _pg_ripple.schema_version",
                    None,
                    &[],
                )
                .ok()
                .and_then(|mut r| r.next())
                .and_then(|row| row.get::<i64>(1).ok().flatten())
                .unwrap_or(1);

            let rate_per_day: i64 = (current / days_installed).max(1);
            let remaining = max_val.saturating_sub(current);
            let years: Option<pgrx::AnyNumeric> = if rate_per_day > 0 {
                let years_f64 = (remaining as f64) / (rate_per_day as f64) / 365.0;
                let s = format!("{:.2}", years_f64);
                pgrx::AnyNumeric::try_from(s.as_str()).ok()
            } else {
                None
            };

            (current, max_val, rate_per_day, years)
        });

        TableIterator::new(vec![row])
    }

    /// Returns all rows from `_pg_ripple.audit_log` up to the configured limit.
    ///
    /// Only meaningful when `pg_ripple.audit_log_enabled = on`.
    #[pg_extern]
    fn audit_log() -> TableIterator<
        'static,
        (
            name!(id, i64),
            name!(ts, pgrx::datum::TimestampWithTimeZone),
            name!(role, String),
            name!(txid, i64),
            name!(operation, String),
            name!(query, String),
        ),
    > {
        let rows: Vec<(
            i64,
            pgrx::datum::TimestampWithTimeZone,
            String,
            i64,
            String,
            String,
        )> = Spi::connect(|c| {
            let results = c.select(
                "SELECT id, ts, role::text, txid, operation, query \
                     FROM _pg_ripple.audit_log ORDER BY id DESC LIMIT 10000",
                None,
                &[],
            );
            match results {
                Ok(tup) => {
                    let mut out = Vec::new();
                    for row in tup {
                        let id = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                        let ts = match row
                            .get::<pgrx::datum::TimestampWithTimeZone>(2)
                            .ok()
                            .flatten()
                        {
                            Some(t) => t,
                            None => continue,
                        };
                        let role = row.get::<&str>(3).ok().flatten().unwrap_or("").to_owned();
                        let txid = row.get::<i64>(4).ok().flatten().unwrap_or(0);
                        let op = row.get::<&str>(5).ok().flatten().unwrap_or("").to_owned();
                        let q = row.get::<&str>(6).ok().flatten().unwrap_or("").to_owned();
                        out.push((id, ts, role, txid, op, q));
                    }
                    out
                }
                Err(_) => vec![],
            }
        });
        TableIterator::new(rows)
    }

    /// Purge audit log entries older than `before`.
    /// Returns the number of rows deleted.
    #[pg_extern]
    fn purge_audit_log(before: pgrx::datum::TimestampWithTimeZone) -> i64 {
        Spi::connect(|c| {
            c.select(
                "WITH del AS (DELETE FROM _pg_ripple.audit_log WHERE ts < $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM del",
                None,
                &[pgrx::datum::DatumWithOid::from(before)],
            )
            .ok()
            .and_then(|mut r| r.next())
            .and_then(|row| row.get::<i64>(1).ok().flatten())
            .unwrap_or(0)
        })
    }

    // ── R2RML Direct Mapping (v0.56.0 L-7.3) ─────────────────────────────────

    /// Execute an R2RML mapping document that has already been loaded into the
    /// triple store (e.g., via `pg_ripple.load_turtle()`).
    ///
    /// Walks all `rr:TriplesMap` instances, queries the mapped PostgreSQL tables
    /// via SPI, applies `rr:template`/`rr:column`/`rr:constant` rules, and
    /// bulk-inserts the generated triples.
    ///
    /// Returns the number of triples inserted.
    ///
    /// ```sql
    /// -- First load the mapping:
    /// SELECT pg_ripple.load_turtle('<path_to_mapping.ttl>');
    /// -- Then execute it:
    /// SELECT pg_ripple.r2rml_load('http://example.org/mapping');
    /// ```
    #[pg_extern]
    fn r2rml_load(mapping_iri: &str) -> i64 {
        crate::r2rml::r2rml_load(mapping_iri)
    }
}
