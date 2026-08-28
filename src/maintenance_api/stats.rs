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

fn relation_exists(relation: &str) -> bool {
    pgrx::Spi::get_one_with_args::<bool>(
        "SELECT to_regclass($1) IS NOT NULL",
        &[pgrx::datum::DatumWithOid::from(relation)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

fn optional_count(relation: &str) -> i64 {
    if !relation_exists(relation) {
        return 0;
    }

    pgrx::Spi::get_one::<i64>(&format!("SELECT count(*)::bigint FROM {relation}"))
        .unwrap_or(None)
        .unwrap_or(0)
}

/// Return one stable JSONB document for operational health checks.
///
/// Optional catalogs are checked with `to_regclass()` first so this remains
/// callable during upgrades and against older installations.
#[pg_extern]
fn health() -> pgrx::JsonB {
    use serde_json::json;

    let extension_version: String = pgrx::Spi::get_one::<String>(
        "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple'",
    )
    .unwrap_or(None)
    .unwrap_or_else(|| "unknown".to_string());
    let compiled_version = env!("CARGO_PKG_VERSION");
    let schema_version: String = if relation_exists("_pg_ripple.schema_version") {
        pgrx::Spi::get_one::<String>(
            "SELECT version FROM _pg_ripple.schema_version ORDER BY installed_at DESC LIMIT 1",
        )
        .unwrap_or(None)
        .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };

    let migration_status = if extension_version == "unknown" || schema_version == "unknown" {
        "unknown"
    } else if extension_version == schema_version {
        "current"
    } else {
        "pending"
    };

    let (worker_status, last_heartbeat, restart_count, last_error) =
        if relation_exists("_pg_ripple.merge_worker_status") {
        let row = pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT max(last_heartbeat_at)::text, \
                            EXTRACT(EPOCH FROM (now() - max(last_heartbeat_at)))::float8 \
                            , COALESCE(sum(restart_count), 0)::bigint \
                            , bool_or(status = 'error') \
                            , bool_or(status = 'stopped') \
                            , (SELECT last_error FROM _pg_ripple.merge_worker_status \
                               WHERE last_error IS NOT NULL ORDER BY updated_at DESC LIMIT 1) \
                     FROM _pg_ripple.merge_worker_status",
                    None,
                    &[],
                )
                .ok()
                .and_then(|mut rows| rows.next())
                .map(|row| {
                    (
                        row.get::<String>(1).ok().flatten(),
                        row.get::<f64>(2).ok().flatten(),
                        row.get::<i64>(3).ok().flatten().unwrap_or(0),
                        row.get::<bool>(4).ok().flatten().unwrap_or(false),
                        row.get::<bool>(5).ok().flatten().unwrap_or(false),
                        row.get::<String>(6).ok().flatten(),
                    )
                })
        });
        let stale_after = (crate::MERGE_HEARTBEAT_INTERVAL_SECONDS.get() as f64 * 2.0).max(60.0);
        let status = match row.as_ref().and_then(|(_, age, _, _, _, _)| *age) {
            Some(age)
                if age <= stale_after
                    && !row
                        .as_ref()
                        .map(|(_, _, _, failed, _, _)| *failed)
                        .unwrap_or(false) =>
                "healthy",
            Some(age) if age <= stale_after => "error",
            Some(_) => "stale",
            None if row.as_ref().map(|(_, _, _, _, stopped, _)| *stopped).unwrap_or(false) => {
                "stopped"
            }
            None => "starting",
        };
        row.map(|(heartbeat, _, restarts, _, _, error)| (status, heartbeat, restarts, error))
            .unwrap_or(("starting", None, 0, None))
    } else {
        ("unknown", None, 0, None)
    };

    let merge_backlog: i64 = pgrx::Spi::get_one::<i64>(
        "SELECT COALESCE(SUM(GREATEST(c.reltuples, 0)::bigint), 0) \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname LIKE '%_delta' AND c.relkind = 'r'",
    )
    .unwrap_or(None)
    .unwrap_or(0);

    let (cache_hits, cache_misses, cache_evictions, cache_utilization) =
        crate::shmem::get_cache_stats();
    let hot_cache_hits =
        crate::dictionary::DICT_HOT_CACHE_HITS.load(std::sync::atomic::Ordering::Relaxed);
    let hot_cache_misses =
        crate::dictionary::DICT_HOT_CACHE_MISSES.load(std::sync::atomic::Ordering::Relaxed);
    let pending_replication: i64 = if relation_exists("_pg_ripple.replication_status") {
        pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.replication_status WHERE processed_at IS NULL",
        )
        .unwrap_or(None)
        .unwrap_or(0)
    } else {
        0
    };
    let (slot_count, replication_lag, replication_lsn): (i64, Option<i64>, Option<String>) =
        pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT count(*)::bigint, \
                            max(pg_current_wal_lsn() - confirmed_flush_lsn)::bigint, \
                            max(confirmed_flush_lsn::text) \
                     FROM pg_replication_slots WHERE slot_name LIKE 'pg_ripple%'",
                    None,
                    &[],
                )
                .ok()
                .and_then(|mut rows| rows.next())
                .map(|row| {
                    (
                        row.get::<i64>(1).ok().flatten().unwrap_or(0),
                        row.get::<i64>(2).ok().flatten(),
                        row.get::<String>(3).ok().flatten(),
                    )
                })
                .unwrap_or((0, None, None))
        });

    let dead_letters = optional_count("_pg_ripple.dead_letter_queue");
    let relay_dead_letters = optional_count("_pg_ripple.event_dead_letters");
    let degraded = matches!(worker_status, "stale" | "error" | "stopped")
        || dead_letters > 0
        || relay_dead_letters > 0;

    pgrx::JsonB(json!({
        "status": if degraded { "degraded" } else { "healthy" },
        "extension_version": extension_version,
        "compiled_version": compiled_version,
        "schema_version": schema_version,
        "worker": {
            "status": worker_status,
            "last_heartbeat_at": last_heartbeat,
            "restart_count": restart_count,
            "last_error": last_error
        },
        "merge": { "backlog_rows": merge_backlog },
        "cache": {
            "shared_memory": {
                "hits": cache_hits,
                "misses": cache_misses,
                "evictions": cache_evictions,
                "utilization": cache_utilization
            },
            "dictionary_hot": {
                "hits": hot_cache_hits,
                "misses": hot_cache_misses
            }
        },
        "dead_letter": {
            "shacl_count": dead_letters,
            "relay_count": relay_dead_letters
        },
        "replication": {
            "enabled": crate::REPLICATION_ENABLED.get(),
            "slot_count": slot_count,
            "lag_bytes": replication_lag,
            "last_applied_lsn": replication_lsn,
            "pending_batches": pending_replication
        },
        "migration": {
            "extension_version": extension_version,
            "schema_version": schema_version,
            "status": migration_status
        }
    }))
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

    // ── v0.87/v0.88 catalog: confidence + PageRank (OBS-05, v0.92.0) ──────
    // Guard each query: tables added in v0.87/v0.88 may not exist when
    // running against a pre-v0.87 schema (e.g. fresh pg_regress test DB).
    let has_confidence: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (
                SELECT 1 FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '_pg_ripple' AND c.relname = 'confidence'
            )",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    let confidence_count: i64 = if has_confidence {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.confidence")
            .unwrap_or(None)
            .unwrap_or(0)
    } else {
        0
    };
    rows.push((
        "confidence_row_count".to_string(),
        confidence_count.to_string(),
    ));

    let has_pagerank_scores: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (
                SELECT 1 FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '_pg_ripple' AND c.relname = 'pagerank_scores'
            )",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    let pagerank_last: String = if has_pagerank_scores {
        pgrx::Spi::get_one::<String>(
            "SELECT MAX(computed_at)::text FROM _pg_ripple.pagerank_scores",
        )
        .unwrap_or(None)
        .unwrap_or_else(|| "never".to_string())
    } else {
        "never".to_string()
    };
    rows.push(("pagerank_last_computed".to_string(), pagerank_last));

    let has_dirty_edges: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (
                SELECT 1 FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '_pg_ripple' AND c.relname = 'pagerank_dirty_edges'
            )",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    let pagerank_queue: i64 = if has_dirty_edges {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.pagerank_dirty_edges")
            .unwrap_or(None)
            .unwrap_or(0)
    } else {
        0
    };
    rows.push((
        "pagerank_queue_depth".to_string(),
        pagerank_queue.to_string(),
    ));

    let has_centrality: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (
                SELECT 1 FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '_pg_ripple' AND c.relname = 'centrality_scores'
            )",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    let centrality_metrics: String = if has_centrality {
        pgrx::Spi::get_one::<String>(
            "SELECT COALESCE(string_agg(DISTINCT metric, ', ' ORDER BY metric), 'none') \
                 FROM _pg_ripple.centrality_scores",
        )
        .unwrap_or(None)
        .unwrap_or_else(|| "none".to_string())
    } else {
        "none".to_string()
    };
    rows.push(("centrality_metrics".to_string(), centrality_metrics));

    TableIterator::new(rows)
}
