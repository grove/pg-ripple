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
//! 2. Add BRIN index on `vp_{id}_main_new`
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
/// - `_pg_ripple.vp_{id}_main`       (BRIN on s)
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
        &format!("CREATE INDEX IF NOT EXISTS idx_vp_{pred_id}_main_brin ON {main} USING BRIN (s)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("main BRIN index error: {e}"));

    // Tombstones table — pending deletes from main.
    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {tombs} ( \
                 s BIGINT NOT NULL, \
                 o BIGINT NOT NULL, \
                 g BIGINT NOT NULL DEFAULT 0 \
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

    // Step 2: BRIN index on new main (effective because rows arrive ordered by s).
    // Drop any stale index from a previous merge cycle (the index name survives
    // across table renames: when main_new is renamed to main, its index keeps
    // the original name).
    Spi::run_with_args(
        &format!("DROP INDEX IF EXISTS _pg_ripple.idx_vp_{pred_id}_main_new_brin"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: drop stale BRIN index error: {e}"));
    Spi::run_with_args(
        &format!("CREATE INDEX idx_vp_{pred_id}_main_new_brin ON {main_new} USING BRIN (s)"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: BRIN index on main_new error: {e}"));

    // Count rows before rename (for return value).
    let row_count: i64 =
        Spi::get_one_with_args::<i64>(&format!("SELECT count(*)::bigint FROM {main_new}"), &[])
            .unwrap_or_else(|e| pgrx::error!("merge: count main_new error: {e}"))
            .unwrap_or(0);

    // Step 3: atomic rename — drop old main, rename new → main.
    // Use lock_timeout to avoid blocking query path for too long.
    Spi::run_with_args("SET LOCAL lock_timeout = '5s'", &[])
        .unwrap_or_else(|e| pgrx::error!("merge: set lock_timeout error: {e}"));

    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {main} CASCADE"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: drop old main error: {e}"));

    Spi::run_with_args(
        &format!("ALTER TABLE {main_new} RENAME TO vp_{pred_id}_main"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: rename main_new error: {e}"));

    // Step 4: truncate delta; delete only older tombstones (v0.22.0 C-4).
    // Truncate the entire delta table (all rows have been merged into main_new).
    Spi::run_with_args(&format!("TRUNCATE {delta}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: truncate delta error: {e}"));

    // Delete only tombstones with i <= max_sid_at_snapshot. Newer tombstones
    // (from deletes that committed during this merge cycle) survive to the next
    // merge cycle, preventing the "tombstone resurrection" race condition where
    // a delete could be missed if it happened after main_new was created but
    // before this merge cycle started.
    Spi::run_with_args(
        &format!("DELETE FROM {tombs} WHERE i <= $1"),
        &[DatumWithOid::from(max_sid_at_snapshot)],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: delete old tombstones error: {e}"));

    // Step 5: ANALYZE so planner has fresh stats.
    // NOTE: Do NOT recreate the view after rename (v0.22.0 C-3).
    // The view vp_{pred_id} is created once when the predicate is promoted
    // to have its own VP table. After renaming vp_{pred_id}_main_new to
    // vp_{pred_id}_main, PostgreSQL's name resolution automatically routes
    // queries through the renamed table, closing the atomicity window.
    // Recreating the view would introduce a gap where concurrent queries
    // could fail with "table not found" errors.
    Spi::run_with_args(&format!("ANALYZE {main}"), &[])
        .unwrap_or_else(|e| pgrx::error!("merge: ANALYZE error: {e}"));

    // Clear the bloom filter bit — delta is now empty.
    crate::shmem::clear_predicate_delta_bit(pred_id);

    // Update triple_count in predicates catalog.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET triple_count = $1 WHERE id = $2",
        &[DatumWithOid::from(row_count), DatumWithOid::from(pred_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("merge: update triple_count error: {e}"));

    row_count
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
    // Collect all HTAP predicate IDs.
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

    // Build a union query across all VP tables (view name = _pg_ripple.vp_{id}).
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
///
/// For each object, records the sorted array of all predicates it appears in.
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
            "CREATE TABLE {tombs} (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0)"
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
    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {backup}"), &[])
        .unwrap_or_else(|e| pgrx::error!("htap_migrate: drop backup error: {e}"));
}
