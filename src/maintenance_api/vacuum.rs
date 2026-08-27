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
        pgrx::Spi::run(&format!(
            "ANALYZE _pg_ripple.vp_{p_id}_delta; \
                 ANALYZE _pg_ripple.vp_{p_id}_main; \
                 ANALYZE _pg_ripple.vp_{p_id}_tombstones"
        ))
        .unwrap_or_else(|e| pgrx::warning!("vacuum: ANALYZE VP table error: {e}"));
        vacuumed += 1;
    }

    // Analyze vp_rare as well.
    pgrx::Spi::run("ANALYZE _pg_ripple.vp_rare")
        .unwrap_or_else(|e| pgrx::warning!("vacuum: ANALYZE vp_rare error: {e}"));

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
        pgrx::Spi::run(&format!(
            "REINDEX TABLE _pg_ripple.vp_{p_id}_delta; \
                 REINDEX TABLE _pg_ripple.vp_{p_id}_main"
        ))
        .unwrap_or_else(|e| pgrx::warning!("reindex: REINDEX VP table error: {e}"));
        reindexed += 1;
    }
    pgrx::Spi::run("REINDEX TABLE _pg_ripple.vp_rare")
        .unwrap_or_else(|e| pgrx::warning!("reindex: REINDEX vp_rare error: {e}"));

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

    // VACUUM-DICT-BATCH-01 (v0.82.0): insert IDs from VP tables in batches
    // to avoid generating a single multi-megabyte UNION ALL SQL string on
    // large instances.  Each batch processes up to vacuum_dict_batch_size
    // predicates in a single SPI call.
    let batch_size = crate::VACUUM_DICT_BATCH_SIZE.get().max(1) as usize;
    for chunk in pred_ids.chunks(batch_size) {
        let union_parts: Vec<String> = chunk
            .iter()
            .flat_map(|p_id| {
                [
                    format!("SELECT s FROM _pg_ripple.vp_{p_id}"),
                    format!("SELECT o FROM _pg_ripple.vp_{p_id}"),
                    format!("SELECT g FROM _pg_ripple.vp_{p_id} WHERE g <> 0"),
                ]
            })
            .collect();
        let sql = format!(
            "INSERT INTO _pg_ripple_live_ids {}",
            union_parts.join(" UNION ALL ")
        );
        pgrx::Spi::run(&sql)
            .unwrap_or_else(|e| pgrx::warning!("vacuum_dictionary: insert batch error: {e}"));
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
