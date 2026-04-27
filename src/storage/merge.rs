//! HTAP merge logic for pg_ripple v0.6.0.
//!
//! Each VP table is split into:
//! - `_pg_ripple.vp_{id}_delta`      — write inbox (B-tree indexed, small)
//! - `_pg_ripple.vp_{id}_main`       — read-optimised archive (BRIN indexed)
//! - `_pg_ripple.vp_{id}_tombstones` — pending deletes from main
//!
//! A VIEW `_pg_ripple.vp_{id}` exposes the union of main + delta minus
//! tombstones, maintaining backward compatibility with the SPARQL query engine.
//!
//! The merge cycle ("fresh-table generation merge"):
//! 1. Create `vp_{id}_main_new` from `(main − tombstones) UNION ALL delta ORDER BY s`
//! 2. Add BRIN index on `vp_{id}_main_new` (on i column — monotonic SID)
//! 3. Atomically rename `_main_new` to `_main` (drop previous main)  
//! 4. TRUNCATE delta and tombstones
//! 5. ANALYZE the new main table

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── Schema setup ─────────────────────────────────────────────────────────────

/// Create the `subject_patterns` and `object_patterns` tables if they are absent.
#[allow(dead_code)]
pub fn initialize_pattern_tables() {
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.subject_patterns ( \
             s       BIGINT   NOT NULL PRIMARY KEY, \
             pattern BIGINT[] NOT NULL \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("subject_patterns table creation error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_subject_patterns_gin \
         ON _pg_ripple.subject_patterns USING GIN (pattern)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("subject_patterns GIN index creation error: {e}"));

    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.object_patterns ( \
             o       BIGINT   NOT NULL PRIMARY KEY, \
             pattern BIGINT[] NOT NULL \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("object_patterns table creation error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_object_patterns_gin \
         ON _pg_ripple.object_patterns USING GIN (pattern)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("object_patterns GIN index creation error: {e}"));

    // v0.6.0: add `htap` flag to predicates catalog (idempotent).
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.predicates \
         ADD COLUMN IF NOT EXISTS htap BOOLEAN NOT NULL DEFAULT false",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("predicates.htap column migration error: {e}"));
}

// ─── HTAP table creation ──────────────────────────────────────────────────────

/// Create the HTAP triple partition for `pred_id`:
/// - `_pg_ripple.vp_{id}_delta`      (B-tree on s,o and o,s)
/// - `_pg_ripple.vp_{id}_main`       (BRIN on i — monotonic SID column)
/// - `_pg_ripple.vp_{id}_tombstones` (index on s,o,g)
/// - VIEW `_pg_ripple.vp_{id}`       = (main − tombstones) UNION ALL delta
///
/// Marks `predicates.htap = true` and updates `table_oid` to the view OID.
pub fn ensure_htap_tables(pred_id: i64) -> String {
    let view = format!("_pg_ripple.vp_{pred_id}");
    let delta = format!("_pg_ripple.vp_{pred_id}_delta");
    let main = format!("_pg_ripple.vp_{pred_id}_main");
    let tombs = format!("_pg_ripple.vp_{pred_id}_tombstones");

    // Delta table — write inbox.
    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {delta} ( \
                 s      BIGINT   NOT NULL, \
                 o      BIGINT   NOT NULL, \
                 g      BIGINT   NOT NULL DEFAULT 0, \
                 i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
                 source SMALLINT NOT NULL DEFAULT 0, \
                 UNIQUE (s, o, g) \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("delta table creation error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX IF NOT EXISTS idx_vp_{pred_id}_delta_s_o ON {delta} (s, o)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("delta index(s,o) error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX IF NOT EXISTS idx_vp_{pred_id}_delta_o_s ON {delta} (o, s)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("delta index(o,s) error: {e}"));

    // Main table — read-optimised.
    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {main} ( \
                 s      BIGINT   NOT NULL, \
                 o      BIGINT   NOT NULL, \
                 g      BIGINT   NOT NULL DEFAULT 0, \
                 i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
                 source SMALLINT NOT NULL DEFAULT 0 \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("main table creation error: {e}"));

    Spi::run_with_args(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_vp_{pred_id}_main_i_brin ON {main} USING BRIN (i)"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("main BRIN index error: {e}"));

    // Tombstones table — pending deletes from main.
    // Column `i` records the SID at insert time; used by merge_predicate to
    // delete only tombstones older than max_sid_at_snapshot (C-4 optimization).
    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {tombs} ( \
                 s BIGINT NOT NULL, \
                 o BIGINT NOT NULL, \
                 g BIGINT NOT NULL DEFAULT 0, \
                 i BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq') \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("tombstones table creation error: {e}"));

    Spi::run_with_args(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_vp_{pred_id}_tombs \
             ON {tombs} (s, o, g)"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("tombstones index error: {e}"));

    // View — UNION ALL of (main − tombstones) + delta, with dedup safety net (v0.22.0 H-6).
    // The DISTINCT ON (s, o, g) prevents a triple from appearing twice when it exists
    // in both main and delta (e.g., if an insert was already in main before the
    // delta UNIQUE constraint was added, or if a triple crossed a merge boundary
    // before the constraint existed). The UNIQUE (s, o, g) constraint on delta
    // ensures no duplicates within delta itself, and future merges will prevent
    // main+delta duplicates via the merging process. This view definition covers
    // historical data that may not have had the constraint when inserted.
    Spi::run_with_args(
        &format!(
            "CREATE OR REPLACE VIEW {view} AS \
             SELECT DISTINCT ON (s, o, g) s, o, g, i, source \
             FROM ( \
                 SELECT m.s, m.o, m.g, m.i, m.source \
                 FROM {main} m \
                 LEFT JOIN {tombs} t ON m.s = t.s AND m.o = t.o AND m.g = t.g \
                 WHERE t.s IS NULL \
                 UNION ALL \
                 SELECT d.s, d.o, d.g, d.i, d.source \
                 FROM {delta} d \
             ) merged \
             ORDER BY s, o, g, i ASC"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp view creation error: {e}"));

    // Update predicates catalog: set htap=true and table_oid = view OID.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count, htap) \
         VALUES ($1, $2::regclass::oid, 0, true) \
         ON CONFLICT (id) DO UPDATE \
             SET table_oid = EXCLUDED.table_oid, htap = true",
        &[
            DatumWithOid::from(pred_id),
            DatumWithOid::from(view.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("predicates htap upsert error: {e}"));

    view
}

/// Check whether a predicate has been split into HTAP partitions.
pub fn is_htap(pred_id: i64) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT htap FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Return the delta table name for a predicate, or `None` if not HTAP.
#[allow(dead_code)] // used by the ExecutorEnd hook introduced in v0.6.0
pub fn delta_table(pred_id: i64) -> Option<String> {
    if is_htap(pred_id) {
        Some(format!("_pg_ripple.vp_{pred_id}_delta"))
    } else {
        None
    }
}

// ─── Fresh-table generation merge ─────────────────────────────────────────────

/// Merge delta into main for a single predicate.
///
/// Uses the "fresh-table generation merge" to maintain BRIN effectiveness:
/// 1. Creates `vp_{id}_main_new` with rows ordered by `s`
/// 2. Adds BRIN index
/// 3. Atomically renames it to `vp_{id}_main`
/// 4. TRUNCATEs delta and tombstones
/// 5. ANALYZEs the new main table
///
/// Returns the number of rows in the new main table.
pub fn merge_predicate(pred_id: i64) -> i64 {
    if !is_htap(pred_id) {
        return 0;
    }

    // v0.37.0: Acquire a per-predicate exclusive advisory lock for the duration
    // of the merge transaction. The DELETE path acquires the same lock in share
    // mode (pg_advisory_xact_lock_shared) before inserting tombstones, so a
    // merge and a concurrent delete can never race on the delta→main swap.
    Spi::run_with_args(
        "SELECT pg_advisory_xact_lock($1)",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: advisory lock error: {e}"));

    let main = format!("_pg_ripple.vp_{pred_id}_main");
    let main_new = format!("_pg_ripple.vp_{pred_id}_main_new");
    let delta = format!("_pg_ripple.vp_{pred_id}_delta");
    let tombs = format!("_pg_ripple.vp_{pred_id}_tombstones");

    // Capture the max statement ID at merge-start (v0.22.0 C-4).
    // This prevents "tombstone resurrection": deletes that commit during the merge
    // will have statement IDs > max_sid_at_snapshot, so their tombstones will not
    // be truncated in this cycle, surviving to the next merge cycle where they can
    // correctly filter out the resurrected deletes.
    let max_sid_at_snapshot: i64 =
        Spi::get_one_with_args::<i64>("SELECT currval('_pg_ripple.statement_id_seq')", &[])
            .unwrap_or_else(|e| pgrx::error!("merge: capture max_sid error: {e}"))
            .unwrap_or(0);

    // Drop any leftover _main_new from a previous failed merge.
    Spi::run_with_args("SET LOCAL pg_ripple.maintenance_mode = 'on'", &[])
        .unwrap_or_else(|e| pgrx::error!("merge: set maintenance_mode (cleanup) error: {e}"));
    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {main_new}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: drop leftover main_new error: {e}"));

    // Step 1: create fresh main_new from (main − tombstones UNION ALL delta) ORDER BY s.
    // When dedup_on_merge is enabled, use DISTINCT ON (s,o,g) to deduplicate,
    // keeping the row with the lowest SID (oldest assertion) per logical triple.
    let dedup_on_merge = crate::DEDUP_ON_MERGE.get();
    let create_sql = if dedup_on_merge {
        format!(
            "CREATE TABLE {main_new} AS \
             SELECT DISTINCT ON (merged.s, merged.o, merged.g) \
                    merged.s, merged.o, merged.g, merged.i, merged.source \
             FROM ( \
                 SELECT m.s, m.o, m.g, m.i, m.source \
                 FROM {main} m \
                 LEFT JOIN {tombs} t ON m.s = t.s AND m.o = t.o AND m.g = t.g \
                 WHERE t.s IS NULL \
                 UNION ALL \
                 SELECT d.s, d.o, d.g, d.i, d.source \
                 FROM {delta} d \
             ) merged \
             ORDER BY merged.s, merged.o, merged.g, merged.i ASC"
        )
    } else {
        format!(
            "CREATE TABLE {main_new} AS \
             SELECT merged.s, merged.o, merged.g, merged.i, merged.source \
             FROM ( \
                 SELECT m.s, m.o, m.g, m.i, m.source \
                 FROM {main} m \
                 LEFT JOIN {tombs} t ON m.s = t.s AND m.o = t.o AND m.g = t.g \
                 WHERE t.s IS NULL \
                 UNION ALL \
                 SELECT d.s, d.o, d.g, d.i, d.source \
                 FROM {delta} d \
             ) merged \
             ORDER BY merged.s"
        )
    };
    Spi::run_with_args(&create_sql, &[])
        .unwrap_or_else(|e| pgrx::error!("merge: create main_new error: {e}"));

    // Step 2: BRIN index on new main (effective because rows arrive in SID (i) order —
    // monotonically increasing, giving BRIN strong correlation on the i column).
    // Drop any stale index from a previous merge cycle.
    Spi::run_with_args(
        &format!("DROP INDEX IF EXISTS _pg_ripple.idx_vp_{pred_id}_main_new_i_brin"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: drop stale BRIN index error: {e}"));
    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_main_new_i_brin ON {main_new} USING BRIN (i)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: BRIN index on main_new error: {e}"));

    // Count rows before rename (for return value).
    let row_count: i64 =
        Spi::get_one_with_args::<i64>(&format!("SELECT count(*)::bigint FROM {main_new}"), &[])
            .unwrap_or_else(|e| pgrx::error!("merge: count main_new error: {e}"))
            .unwrap_or(0);

    // Step 3: F7-1 (v0.60.0) — atomic rename-swap that never leaves the backing
    // relation non-existent.  The sequence:
    //   a. Rename old main → main_old  (view OID still resolves to the old table)
    //   b. Rename main_new → main      (view OID still resolves to old; queries work)
    //   c. CREATE OR REPLACE VIEW      (atomically repoints the view to new main)
    //   d. DROP old main_old           (removes old data; old index dropped with it)
    //   e. Rename new BRIN index to canonical name
    //
    // There is no window where the VP view lacks a backing relation.
    // Use lock_timeout to avoid blocking the query path for too long.
    Spi::run_with_args("SET LOCAL lock_timeout = '5s'", &[])
        .unwrap_or_else(|e| pgrx::error!("merge: set lock_timeout error: {e}"));

    let main_old = format!("_pg_ripple.vp_{pred_id}_main_old");

    // a. Rename current main → main_old (keeps the old OID in the view working).
    // If main_old already exists from a previously aborted merge, drop it first.
    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {main_old}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: drop stale main_old error: {e}"));
    Spi::run_with_args(
        &format!("ALTER TABLE {main} RENAME TO vp_{pred_id}_main_old"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: rename main → main_old error: {e}"));

    // b. Rename main_new → main (backing table for the refreshed view).
    Spi::run_with_args(
        &format!("ALTER TABLE {main_new} RENAME TO vp_{pred_id}_main"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: rename main_new → main error: {e}"));

    // c. CREATE OR REPLACE VIEW — atomically repoints to new main OID.
    // The view must exist for find_triples / SPARQL queries to work correctly.
    let view = format!("_pg_ripple.vp_{pred_id}");
    Spi::run_with_args(
        &format!(
            "CREATE OR REPLACE VIEW {view} AS \
             SELECT DISTINCT ON (s, o, g) s, o, g, i, source \
             FROM ( \
                 SELECT m.s, m.o, m.g, m.i, m.source \
                 FROM {main} m \
                 LEFT JOIN {tombs} t ON m.s = t.s AND m.o = t.o AND m.g = t.g \
                 WHERE t.s IS NULL \
                 UNION ALL \
                 SELECT d.s, d.o, d.g, d.i, d.source \
                 FROM {delta} d \
             ) merged \
             ORDER BY s, o, g, i ASC"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: recreate view error: {e}"));

    // d. Drop the old main table now that the view points to the new one.
    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {main_old}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: drop main_old error: {e}"));

    // e. Rename the BRIN index on the new main to the canonical name.
    //    The old index was dropped with main_old; the new one was created as
    //    idx_vp_{pred_id}_main_new_i_brin and needs to be renamed.
    Spi::run_with_args(
        &format!(
            "ALTER INDEX IF EXISTS _pg_ripple.idx_vp_{pred_id}_main_new_i_brin \
             RENAME TO idx_vp_{pred_id}_main_i_brin"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("merge: rename BRIN index error (non-fatal): {e}"));

    // Re-summarize BRIN index so page-range summaries are valid immediately
    // without waiting for the autovacuum BRIN worker.
    let brin_sql = format!(
        "SELECT brin_summarize_new_values(c.oid) \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' \
           AND c.relname = 'idx_vp_{pred_id}_main_i_brin' \
           AND c.relkind = 'i'"
    );
    // Best-effort: failure to re-summarize is non-fatal (BRIN self-heals on next vacuum).
    if let Err(e) = Spi::run_with_args(&brin_sql, &[]) {
        pgrx::debug1!(
            "merge: brin_summarize_new_values failed for vp_{pred_id}_main (non-fatal): {e}"
        );
    }

    // v0.37.0: Atomically update _pg_ripple.statements SID-range catalog in the
    // same transaction as the VP table swap. This prevents a race where the merge
    // worker is killed mid-update and leaves a stale SID→OID mapping for RDF-star
    // queries. DELETE then INSERT guarantees an atomic replacement.
    let new_sid_min: i64 =
        Spi::get_one_with_args::<i64>(&format!("SELECT COALESCE(MIN(i), 0) FROM {main}"), &[])
            .unwrap_or(None)
            .unwrap_or(0);
    let new_sid_max: i64 =
        Spi::get_one_with_args::<i64>(&format!("SELECT COALESCE(MAX(i), 0) FROM {main}"), &[])
            .unwrap_or(None)
            .unwrap_or(0);
    if new_sid_min > 0 && new_sid_max >= new_sid_min {
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.statements WHERE predicate_id = $1",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or_else(|e| pgrx::warning!("merge: statements delete error: {e}"));
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.statements (sid_min, sid_max, predicate_id, table_oid) \
             VALUES ($1, $2, $3, \
                 (SELECT c.oid FROM pg_class c \
                  JOIN pg_namespace n ON n.oid = c.relnamespace \
                  WHERE n.nspname = '_pg_ripple' AND c.relname = $4)) \
             ON CONFLICT (sid_min) DO UPDATE \
             SET sid_max = EXCLUDED.sid_max, \
                 predicate_id = EXCLUDED.predicate_id, \
                 table_oid = EXCLUDED.table_oid",
            &[
                DatumWithOid::from(new_sid_min),
                DatumWithOid::from(new_sid_max),
                DatumWithOid::from(pred_id),
                DatumWithOid::from(format!("vp_{pred_id}_main").as_str()),
            ],
        )
        .unwrap_or_else(|e| pgrx::warning!("merge: statements insert error: {e}"));
    }

    // Step 4: truncate delta; delete only older tombstones (v0.22.0 C-4).
    // Truncate the entire delta table (all rows have been merged into main_new).
    Spi::run_with_args(&format!("TRUNCATE {delta}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: truncate delta error: {e}"));

    // v0.55.0 F-2: when TOMBSTONE_RETENTION_SECONDS=0, TRUNCATE the entire tombstones
    // table (all entries were absorbed into main_new) for cheaper reclaim.
    // Otherwise, delete only tombstones with i <= max_sid_at_snapshot.  Newer
    // tombstones (from deletes that committed during this merge cycle) survive to
    // the next merge cycle, preventing the "tombstone resurrection" race where a
    // delete could be missed if it arrived after main_new was created.
    if crate::TOMBSTONE_RETENTION_SECONDS.get() == 0 {
        Spi::run_with_args(&format!("TRUNCATE {tombs}"), &[])
            .unwrap_or_else(|e| pgrx::error!("merge: truncate tombstones error: {e}"));
        // Record the GC timestamp in the predicates catalog (v0.55.0 migration col).
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET tombstones_cleared_at = now() WHERE id = $1",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or_else(|e| pgrx::warning!("merge: tombstones_cleared_at update error: {e}"));
    } else {
        Spi::run_with_args(
            &format!("DELETE FROM {tombs} WHERE i <= $1"),
            &[DatumWithOid::from(max_sid_at_snapshot)],
        )
        .unwrap_or_else(|e| pgrx::error!("merge: delete old tombstones error: {e}"));
    }

    // Step 5: ANALYZE so planner has fresh stats.
    // AUTO_ANALYZE GUC (v0.24.0): skip ANALYZE if the user has disabled it.
    if crate::AUTO_ANALYZE.get() {
        Spi::run_with_args(&format!("ANALYZE {main}"), &[])
            .unwrap_or_else(|e| pgrx::error!("merge: ANALYZE error: {e}"));
    }

    // Clear the bloom filter bit — delta is now empty.
    crate::shmem::clear_predicate_delta_bit(pred_id);

    // Update triple_count in predicates catalog.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET triple_count = $1 WHERE id = $2",
        &[DatumWithOid::from(row_count), DatumWithOid::from(pred_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: update triple_count error: {e}"));

    // v0.37.0: Tombstone GC — schedule VACUUM on the tombstones table when the
    // residual tombstone count exceeds tombstone_gc_threshold × main row count.
    if crate::TOMBSTONE_GC_ENABLED.get() && row_count > 0 {
        let threshold_str = crate::TOMBSTONE_GC_THRESHOLD_STR
            .get()
            .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
            .unwrap_or_else(|| "0.05".to_string());
        let threshold: f64 = threshold_str.parse().unwrap_or(0.05);
        let tombs_remaining: i64 =
            Spi::get_one_with_args::<i64>(&format!("SELECT count(*)::bigint FROM {tombs}"), &[])
                .unwrap_or(None)
                .unwrap_or(0);
        if (tombs_remaining as f64) / (row_count as f64) > threshold {
            // Schedule VACUUM — runs asynchronously outside our transaction.
            // Use VACUUM (not VACUUM FULL) to avoid table locks.
            if let Err(e) = Spi::run_with_args(&format!("VACUUM ANALYZE {tombs}"), &[]) {
                pgrx::warning!("merge: tombstone GC VACUUM on {tombs}: {e}");
            }
        }
    }

    // v0.53.0: Emit CDC lifecycle NOTIFY (best-effort, non-blocking).
    // Count remaining tombstones after GC for the payload.
    let tombs_remaining: i64 =
        Spi::get_one_with_args::<i64>(&format!("SELECT count(*)::bigint FROM {tombs}"), &[])
            .unwrap_or(None)
            .unwrap_or(0);
    notify_merge_lifecycle(pred_id, row_count, tombs_remaining);

    row_count
}

/// Emit a CDC lifecycle NOTIFY for a completed merge cycle.
///
/// Channel: `pg_ripple_cdc_lifecycle` (global lifecycle channel).
/// Payload: `{"op":"merge","predicate_id":N,"merged":M,"tombstones":T}`
///
/// This is best-effort: errors are logged as warnings and do not fail the merge.
pub(crate) fn notify_merge_lifecycle(pred_id: i64, merged: i64, tombstones: i64) {
    let channel = "pg_ripple_cdc_lifecycle";
    let payload = format!(
        r#"{{"op":"merge","predicate_id":{pred_id},"merged":{merged},"tombstones":{tombstones}}}"#
    );
    let _ = Spi::run_with_args(
        "SELECT pg_notify($1, $2)",
        &[
            pgrx::datum::DatumWithOid::from(channel),
            pgrx::datum::DatumWithOid::from(payload.as_str()),
        ],
    );
}

/// Merge all HTAP predicates.  Returns total rows across all merged main tables.
pub fn merge_all() -> i64 {
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("merge_all predicates SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    let mut total = 0i64;
    for p_id in pred_ids {
        // Only merge predicates that have rows in delta.
        let delta_rows: i64 = Spi::get_one_with_args::<i64>(
            &format!("SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_delta"),
            &[],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        if delta_rows > 0 {
            total += merge_predicate(p_id);
        }
    }
    total
}

// ─── Pattern tables ────────────────────────────────────────────────────────────

/// Rebuild `_pg_ripple.subject_patterns` from all VP tables.
///
/// For each subject, records the sorted array of all predicates it appears in.
/// Called by the merge worker after each generation merge.
pub fn rebuild_subject_patterns() {
    // Collect all HTAP predicate IDs (predicates with dedicated VP tables).
    // Predicate with table_oid IS NOT NULL are those promoted from vp_rare to have
    // their own dedicated vp_{id} table. We enumerate ONLY these, excluding vp_rare.
    // This prevents the "vp_rare double-count" bug (v0.22.0 H-7) where entries in
    // vp_rare would be counted twice: once via vp_rare itself and once via their
    // respective dedicated vp_{id} tables (if promoted).
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("rebuild_subject_patterns: predicates scan error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    if pred_ids.is_empty() {
        return;
    }

    // Build a union query across all dedicated VP tables (view name = _pg_ripple.vp_{id}).
    // Each dedicated VP table's view already incorporates merged main/delta/tombstones.
    // vp_rare is never scanned directly as a table in this aggregation.
    let union_parts: Vec<String> = pred_ids
        .iter()
        .map(|&p| format!("SELECT {p}::bigint AS p, s FROM _pg_ripple.vp_{p}"))
        .collect();

    let union_sql = union_parts.join(" UNION ALL ");

    // Rebuild subject_patterns as an aggregation: s → array_agg(DISTINCT p ORDER BY p).
    Spi::run_with_args(
        &format!(
            "INSERT INTO _pg_ripple.subject_patterns (s, pattern) \
             SELECT s, array_agg(DISTINCT p ORDER BY p) \
             FROM ({union_sql}) AS all_triples \
             GROUP BY s \
             ON CONFLICT (s) DO UPDATE \
                 SET pattern = EXCLUDED.pattern"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("rebuild_subject_patterns: upsert error: {e}"));
}

/// Rebuild `_pg_ripple.object_patterns` from all VP tables.
/// Rebuild `_pg_ripple.object_patterns` from all dedicated VP tables (v0.22.0 H-7).
///
/// For each object, records the sorted array of all predicates it appears in.
/// Only enumerates dedicated VP tables (table_oid IS NOT NULL), never scans vp_rare
/// directly to prevent double-counting. Entries in vp_rare are already reachable via
/// their associated dedicated vp_{id} tables after promotion.
pub fn rebuild_object_patterns() {
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("rebuild_object_patterns: predicates scan error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    if pred_ids.is_empty() {
        return;
    }

    let union_parts: Vec<String> = pred_ids
        .iter()
        .map(|&p| format!("SELECT {p}::bigint AS p, o FROM _pg_ripple.vp_{p}"))
        .collect();

    let union_sql = union_parts.join(" UNION ALL ");

    Spi::run_with_args(
        &format!(
            "INSERT INTO _pg_ripple.object_patterns (o, pattern) \
             SELECT o, array_agg(DISTINCT p ORDER BY p) \
             FROM ({union_sql}) AS all_triples \
             GROUP BY o \
             ON CONFLICT (o) DO UPDATE \
                 SET pattern = EXCLUDED.pattern"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("rebuild_object_patterns: upsert error: {e}"));
}

// ─── Full compact ─────────────────────────────────────────────────────────────

/// Trigger an immediate full merge of all HTAP VP tables.
///
/// After the merge, rebuild subject_patterns and object_patterns.
/// Called by `pg_ripple.compact()` SQL function.
pub fn compact() -> i64 {
    let merged = merge_all();
    rebuild_subject_patterns();
    rebuild_object_patterns();
    // Signal the shmem counter to zero.
    crate::shmem::reset_delta_count();
    // All deltas are now empty — reset the bloom filter entirely.
    crate::shmem::reset_bloom_filter();
    merged
}

// ─── Migrate flat table to HTAP ───────────────────────────────────────────────

/// Migrate an existing flat VP table `_pg_ripple.vp_{id}` to the HTAP split.
///
/// Called from the `ALTER EXTENSION pg_ripple UPDATE` migration script
/// via the `pg_ripple.htap_migrate_predicate(bigint)` function.
pub fn migrate_flat_to_htap(pred_id: i64) {
    let flat = format!("_pg_ripple.vp_{pred_id}");
    let backup = format!("_pg_ripple.vp_{pred_id}_pre_htap");
    let delta = format!("_pg_ripple.vp_{pred_id}_delta");
    let main = format!("_pg_ripple.vp_{pred_id}_main");
    let tombs = format!("_pg_ripple.vp_{pred_id}_tombstones");
    let view = format!("_pg_ripple.vp_{pred_id}");

    // Check if already migrated.
    if is_htap(pred_id) {
        return;
    }

    // Rename flat table → backup.
    Spi::run_with_args(
        &format!("ALTER TABLE IF EXISTS {flat} RENAME TO vp_{pred_id}_pre_htap"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: rename flat error: {e}"));

    // Create delta table (copy existing rows into it as the write inbox).
    Spi::run_with_args(
        &format!("CREATE TABLE {delta} AS SELECT * FROM {backup}"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: create delta error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_delta_s_o ON {delta} (s, o)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: delta index(s,o) error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_delta_o_s ON {delta} (o, s)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: delta index(o,s) error: {e}"));

    // Create empty main table.
    Spi::run_with_args(
        &format!(
            "CREATE TABLE {main} ( \
                 s      BIGINT   NOT NULL, \
                 o      BIGINT   NOT NULL, \
                 g      BIGINT   NOT NULL DEFAULT 0, \
                 i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
                 source SMALLINT NOT NULL DEFAULT 0 \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: create main error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_main_brin ON {main} USING BRIN (s)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: main BRIN index error: {e}"));

    // Create empty tombstones table.
    Spi::run_with_args(
        &format!(
            "CREATE TABLE {tombs} ( \
                 s BIGINT NOT NULL, \
                 o BIGINT NOT NULL, \
                 g BIGINT NOT NULL DEFAULT 0, \
                 i BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq') \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: create tombstones error: {e}"));

    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_tombs ON {tombs} (s, o, g)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: tombstones index error: {e}"));

    // Create the view.
    Spi::run_with_args(
        &format!(
            "CREATE VIEW {view} AS \
             SELECT m.s, m.o, m.g, m.i, m.source \
             FROM {main} m \
             LEFT JOIN {tombs} t ON m.s = t.s AND m.o = t.o AND m.g = t.g \
             WHERE t.s IS NULL \
             UNION ALL \
             SELECT d.s, d.o, d.g, d.i, d.source \
             FROM {delta} d"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: create view error: {e}"));

    // Update predicates catalog.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates \
         SET table_oid = $2::regclass::oid, htap = true \
         WHERE id = $1",
        &[
            DatumWithOid::from(pred_id),
            DatumWithOid::from(view.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("htap_migrate: predicates update error: {e}"));

    // Drop the backup table.
    Spi::run_with_args("SET LOCAL pg_ripple.maintenance_mode = 'on'", &[])
        .unwrap_or_else(|e| pgrx::error!("htap_migrate: set maintenance_mode error: {e}"));
    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {backup}"), &[])
        .unwrap_or_else(|e| pgrx::error!("htap_migrate: drop backup error: {e}"));
}
