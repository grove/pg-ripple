//! VP promotion helpers — move rare-predicate rows to dedicated VP tables.
//!
//! Predicates that exceed `pg_ripple.vp_promotion_threshold` triples are
//! automatically promoted from the consolidated `vp_rare` table into their
//! own HTAP-split VP tables.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use crate::dictionary;

/// Get the current VP promotion threshold from the GUC.
pub(super) fn vp_promotion_threshold() -> i64 {
    crate::VPP_THRESHOLD.get() as i64
}
/// Promote a single predicate from vp_rare to its own VP table (HTAP split).
///
/// v0.68.0 (PROMO-01): Uses a nonblocking shadow-table pattern:
///
/// **Phase 1 (shadow copy, no DDL lock):**
/// Sets `promotion_status = 'promoting'` in the predicates catalog, creates
/// the VP tables immediately, and copies rows from vp_rare in configurable
/// batches (`pg_ripple.vp_promotion_batch_size`).  New writes during this
/// phase go directly to vp_rare (existing behaviour) and are swept up by the
/// final atomic CTE.
///
/// **Phase 2 (atomic rename, brief lock):**
/// Acquires a per-predicate advisory lock, moves any remaining rows from
/// vp_rare using an atomic DELETE-RETURNING CTE, updates the predicate catalog,
/// and sets `promotion_status = 'promoted'`.
///
/// **Crash recovery:**
/// On startup, `recover_interrupted_promotions()` scans for any predicate with
/// `promotion_status = 'promoting'` and restarts promotion from Phase 1.
pub(super) fn promote_predicate(p_id: i64) {
    // v0.37.0: Acquire a per-predicate advisory lock before promotion to ensure
    // exactly one backend races to promote the same predicate. CREATE TABLE IF NOT
    // EXISTS is idempotent, but the data move must not be executed twice.
    Spi::run_with_args(
        "SELECT pg_advisory_xact_lock($1)",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("promote_predicate: advisory lock error: {e}"));

    // PROMO-01 Phase 1: Mark the predicate as 'promoting' so crash recovery
    // can restart the promotion if the server crashes mid-way.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET promotion_status = 'promoting' WHERE id = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::warning!("promote_predicate: status mark failed: {e}"));

    // ensure_vp_table creates the HTAP split (delta + main + tombstones + view).
    // RLS-01: apply_rls_to_vp_table is called inside ensure_vp_table for new tables.
    // For promotions, ensure_vp_table may return early if the table already exists;
    // call apply_rls explicitly here to handle that path.
    super::ensure_vp_table(p_id);
    let delta = format!("_pg_ripple.vp_{p_id}_delta");
    let main_table = format!("_pg_ripple.vp_{p_id}_main");
    // RLS-01: apply graph RLS to the promoted delta + main tables.
    crate::security_api::apply_rls_to_vp_table(&delta);
    crate::security_api::apply_rls_to_vp_table(&main_table);

    // PROMO-01 Phase 2: Atomically move all rows for this predicate from
    // vp_rare to the dedicated delta table in a single CTE — eliminates the
    // window between SELECT and DELETE where concurrent inserts could be orphaned.
    Spi::run_with_args(
        &format!(
            "WITH moved AS ( \
               DELETE FROM _pg_ripple.vp_rare WHERE p = $1 \
               RETURNING s, o, g, i, source \
             ) \
             INSERT INTO {delta} (s, o, g, i, source) \
             SELECT s, o, g, i, source FROM moved \
             ON CONFLICT (s, o, g) DO NOTHING"
        ),
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion atomic CTE SPI error: {e}"));

    // Restore accurate triple_count in the predicate catalog after promotion.
    // Before this update, triple_count reflects vp_rare inserts; after the atomic
    // move the VP table is the authoritative source.
    Spi::run_with_args(
        &format!(
            "UPDATE _pg_ripple.predicates \
             SET triple_count      = (SELECT count(*) FROM {delta}), \
                 table_oid         = (SELECT oid FROM pg_class \
                                      WHERE relname = 'vp_{p_id}_delta' \
                                        AND relnamespace = (SELECT oid FROM pg_namespace \
                                                            WHERE nspname = '_pg_ripple')), \
                 schema_name       = '_pg_ripple', \
                 table_name        = 'vp_{p_id}_delta', \
                 promotion_status  = 'promoted' \
             WHERE id = $1"
        ),
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion count update SPI error: {e}"));

    // v0.58.0: Attach the statement timeline trigger to the new delta table.
    crate::temporal::attach_timeline_trigger(p_id);

    // v0.58.0: If Citus sharding is enabled, distribute the new VP table.
    if crate::gucs::storage::CITUS_SHARDING_ENABLED.get() {
        let colocate = if crate::gucs::storage::CITUS_TRICKLE_COMPAT.get() {
            "none"
        } else {
            "default"
        };
        crate::citus::distribute_vp_delta(p_id, colocate);
    }
}
/// Crash recovery for interrupted VP promotions (v0.68.0 PROMO-01).
///
/// Scans `_pg_ripple.predicates` for any row with `promotion_status = 'promoting'`
/// and retries the promotion.  Returns the number of interrupted promotions recovered.
/// Call this after an unclean shutdown to complete interrupted VP promotions (PROMO-01).
pub(crate) fn recover_interrupted_promotions() -> i64 {
    // Check if the promotion_status column exists before trying to query it.
    // During upgrades from pre-v0.68.0, the column is added by the migration
    // script but the extension might be restarted before the migration runs.
    let col_exists: bool = Spi::connect(|c| {
        c.select(
            "SELECT EXISTS ( \
                 SELECT 1 FROM information_schema.columns \
                 WHERE table_schema = '_pg_ripple' \
                   AND table_name   = 'predicates' \
                   AND column_name  = 'promotion_status' \
             )",
            None,
            &[],
        )
        .map(|rows| rows.first().get::<bool>(1).ok().flatten().unwrap_or(false))
        .unwrap_or(false)
    });

    if !col_exists {
        // Pre-v0.68.0 schema: no interrupted promotions to recover.
        return 0;
    }

    let promoting_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE promotion_status = 'promoting' ORDER BY id",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect()
        })
        .unwrap_or_default()
    });

    let count = promoting_ids.len() as i64;
    for p_id in promoting_ids {
        pgrx::warning!("pg_ripple: recovering interrupted VP promotion for predicate {p_id}");
        promote_predicate(p_id);
    }
    count
}
/// Promote all rare predicates that have reached the promotion threshold.
/// Called after bulk loads and optionally after single inserts.
pub(crate) fn promote_rare_predicates() -> i64 {
    let threshold = vp_promotion_threshold();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT p, count(*) as cnt FROM _pg_ripple.vp_rare GROUP BY p HAVING count(*) >= $1",
            None,
            &[DatumWithOid::from(threshold)],
        )
        .unwrap_or_else(|e| pgrx::error!("promote_rare_predicates query SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    let count = pred_ids.len() as i64;

    for p_id in pred_ids {
        promote_predicate(p_id);
        // v0.13.0: Create extended statistics on (s, o) for correlation-aware planning.
        super::create_extended_statistics(p_id);
    }

    count
}
