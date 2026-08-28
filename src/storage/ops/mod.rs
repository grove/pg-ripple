//! Storage operations -- extracted from storage/mod.rs (MOD-01, v0.72.0).
//! v0.90.0 CQ-02: pre-emptive split sub-modules
//!
//! Insert, delete, query, graph management, prefix registry, SID API, dedup.

// v0.90.0 CQ-02 / M15-13 v0.96.0: split sub-modules
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod delete;
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod insert;
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod merge;
pub mod scan;

pub use scan::{
    all_graph_ids, clear_graph_by_id, create_graph, current_load_generation, deduplicate_all,
    deduplicate_predicate, drop_graph, find_triples, for_each_encoded_triple_batch,
    get_statement_by_sid, list_graphs, list_prefixes, register_prefix, statement_id_for_triple,
    total_triple_count, triple_count_in_graph, triples_for_object, triples_for_subject,
};

pub(crate) use scan::{delete_triple_by_ids, insert_triple_by_ids};

use super::dictionary_io::{encode_rdf_term, strip_angle_brackets};

/// Session-local cache of the current load generation (avoids repeated SPI roundtrips).
/// Updated by `next_load_generation()` and read by `scan::current_load_generation()`.
pub(super) static LOAD_GEN_CACHE: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

use super::vp_rare_io::{get_dedicated_vp_table, insert_into_vp_rare};
use super::{mutation_journal, promote};
use crate::dictionary;
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

/// Allocate and return the next load generation ID (for blank node scoping).
pub fn next_load_generation() -> i64 {
    let new_gen = Spi::get_one::<i64>("SELECT nextval('_pg_ripple.load_generation_seq')")
        .unwrap_or_else(|e| pgrx::error!("load_generation_seq SPI error: {e}"))
        .unwrap_or(1);
    // Update the session cache so current_load_generation() reflects the new value.
    LOAD_GEN_CACHE.store(new_gen, std::sync::atomic::Ordering::Relaxed);
    new_gen
}

/// Insert a triple `(s, p, o)` into graph `g`.
///
/// Routes to vp_rare for new/rare predicates; promotes when threshold is crossed.
/// Returns the globally-unique statement identifier (SID).
///
/// # v0.107.0 CDC Integration
///
/// When `pg_ripple.temporal_cdc_enabled` is `on` (default) and the predicate is
/// registered as temporal, this function also inserts into
/// `_pg_ripple.temporal_facts` with `valid_from = transaction_timestamp()`.
/// The data model (snapshot vs versioned) follows the predicate's registration.
pub fn insert_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = encode_rdf_term(s);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = encode_rdf_term(o);

    // Fast path: dedicated VP table (HTAP split) already exists — insert to delta.
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        crate::error::fault_injection::hit("vp_delta_write");
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        // Use ON CONFLICT DO UPDATE to get the existing row's ID if it already exists.
        // This handles UNIQUE (s, o, g) constraint (v0.22.0 H-6).
        // If the triple already exists in delta, we return its existing statement ID.
        // This prevents duplicate triples across main+delta merge boundaries.
        let sid = Spi::get_one_with_args::<i64>(
            &format!(
                "INSERT INTO {delta} (s, o, g) VALUES ($1, $2, $3) \
                 ON CONFLICT (s, o, g) DO UPDATE SET i = EXCLUDED.i \
                 RETURNING i"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("triple insert SPI error: {e}"))
        .unwrap_or(0);

        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + 1 WHERE id = $1",
            &[DatumWithOid::from(p_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));

        // Update shmem delta counter for merge worker triggering.
        crate::shmem::record_delta_inserts(1);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);

        // v0.107.0: CDC → temporal_facts wiring.
        maybe_insert_temporal_cdc(s_id, p_id, o_id, g);

        return sid;
    }

    // Slow path: insert into vp_rare, check for promotion.
    let sid = insert_into_vp_rare(p_id, s_id, o_id, g);

    // Check if threshold crossed — promote immediately for single inserts.
    let new_count: i64 = Spi::get_one_with_args::<i64>(
        "SELECT triple_count FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    if new_count >= promote::vp_promotion_threshold() {
        promote::promote_predicate(p_id);
    }

    // v0.107.0: CDC → temporal_facts wiring.
    maybe_insert_temporal_cdc(s_id, p_id, o_id, g);

    sid
}

/// v0.107.0 — Conditionally insert into `_pg_ripple.temporal_facts` when CDC
/// integration is enabled and the predicate is registered as temporal.
///
/// This is called from `insert_triple()` after the triple has been written to
/// the VP storage layer.  It is a no-op when:
/// - `pg_ripple.temporal_cdc_enabled` is `off`, or
/// - the predicate is not registered in `_pg_ripple.temporal_predicates`.
fn maybe_insert_temporal_cdc(s_id: i64, p_id: i64, o_id: i64, g: i64) {
    if !crate::TEMPORAL_CDC_ENABLED.get() {
        return;
    }
    // Check whether this predicate is registered as temporal.
    let data_model: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT data_model FROM _pg_ripple.temporal_predicates WHERE predicate_id = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or(None);

    let data_model = match data_model {
        Some(m) => m,
        None => return, // not a temporal predicate — nothing to do
    };

    // For 'snapshot' model: close any existing open-ended row for (s, p, g).
    if data_model == "snapshot" {
        Spi::run_with_args(
            "UPDATE _pg_ripple.temporal_facts \
             SET valid_to = transaction_timestamp() \
             WHERE s = $1 AND p = $2 AND g = $3 AND valid_to IS NULL",
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(p_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| {
            pgrx::warning!("temporal_cdc: snapshot close error: {e}");
        });
    }

    // Insert new open-ended temporal fact with valid_from = transaction_timestamp().
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.temporal_facts (s, p, o, g, valid_from, valid_to) \
         VALUES ($1, $2, $3, $4, transaction_timestamp(), NULL) \
         ON CONFLICT DO NOTHING",
        &[
            DatumWithOid::from(s_id),
            DatumWithOid::from(p_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| {
        pgrx::warning!("temporal_cdc: insert temporal fact error: {e}");
    });
}

/// Insert a triple that was pre-encoded (used by bulk loader for performance).
///
/// Routes to vp_rare or dedicated table based on current predicate state.
/// Does NOT check/trigger promotion (bulk load calls promote_rare_predicates at end).
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn insert_encoded_triple(s_id: i64, p_id: i64, o_id: i64, g: i64) -> i64 {
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        // Route insert to delta table (HTAP write inbox).
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        // Use ON CONFLICT DO UPDATE for UNIQUE (s, o, g) constraint (v0.22.0 H-6).
        let sid = Spi::get_one_with_args::<i64>(
            &format!(
                "INSERT INTO {delta} (s, o, g) VALUES ($1, $2, $3) \
                 ON CONFLICT (s, o, g) DO UPDATE SET i = EXCLUDED.i \
                 RETURNING i"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("bulk insert SPI error: {e}"))
        .unwrap_or(0);

        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + 1 WHERE id = $1",
            &[DatumWithOid::from(p_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));

        crate::shmem::record_delta_inserts(1);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);
        return sid;
    }

    insert_into_vp_rare(p_id, s_id, o_id, g)
}

/// Batch insert pre-encoded triples for a single predicate (bulk load performance).
///
/// Uses a VALUES-list INSERT to reduce SPI round-trips.
/// All values are i64 integers — no SQL injection risk.
/// H15-05 (v0.94.0): Shared VP insert helper using UNNEST arrays.
///
/// When `pg_ripple.bulk_load_use_copy = on`, this function is called instead of
/// the multi-row VALUES batch insert to reduce SQL string allocation overhead.
/// It passes triple arrays as PostgreSQL BIGINT[] parameters via UNNEST, which
/// avoids per-row string formatting and benefits from plan caching.
///
/// Used by bulk_load, R2RML, and CDC paths.
pub(crate) fn copy_into_vp(delta: &str, rows: &[(i64, i64, i64)]) {
    if rows.is_empty() {
        return;
    }
    let s_ids: Vec<i64> = rows.iter().map(|&(s, _, _)| s).collect();
    let o_ids: Vec<i64> = rows.iter().map(|&(_, o, _)| o).collect();
    let g_ids: Vec<i64> = rows.iter().map(|&(_, _, g)| g).collect();
    let sql = format!(
        "INSERT INTO {delta} (s, o, g) \
         SELECT s, o, g FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[]) AS t(s, o, g) \
         ON CONFLICT (s, o, g) DO NOTHING"
    );
    crate::error::fault_injection::hit("vp_delta_write");
    Spi::run_with_args(
        &sql,
        &[
            DatumWithOid::from(s_ids.as_slice()),
            DatumWithOid::from(o_ids.as_slice()),
            DatumWithOid::from(g_ids.as_slice()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("copy_into_vp: UNNEST insert error: {e}"));
}

/// Records unique graph IDs in the mutation journal so that CWB writeback fires
/// when `mutation_journal::flush()` is called at the end of the enclosing load_*.
/// The flush is the caller's responsibility; this function does NOT flush.
/// # Callers
/// Direct callers must be bulk-load functions only.
pub(crate) fn batch_insert_encoded(p_id: i64, rows: &[(i64, i64, i64)]) -> i64 {
    if rows.is_empty() {
        return 0;
    }

    let table_opt = get_dedicated_vp_table(p_id);

    if let Some(_view) = table_opt {
        crate::error::fault_injection::hit("vp_delta_write");
        // Route batch insert to delta table.
        let delta = format!("_pg_ripple.vp_{p_id}_delta");

        if crate::BULK_LOAD_USE_COPY.get() {
            // H15-05 (v0.94.0): COPY-style path via UNNEST arrays.
            copy_into_vp(&delta, rows);
        } else {
            // Build a multi-row VALUES insert (all i64 integers — injection-safe).
            let values: Vec<String> = rows
                .iter()
                .map(|(s, o, g)| format!("({},{},{})", s, o, g))
                .collect();
            let sql = format!(
                "INSERT INTO {delta} (s, o, g) VALUES {} ON CONFLICT (s, o, g) DO NOTHING",
                values.join(","),
            );
            Spi::run_with_args(&sql, &[])
                .unwrap_or_else(|e| pgrx::error!("batch VP delta insert SPI error: {e}"));
        }

        let cnt = rows.len() as i64;
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + $2 WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(cnt)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count batch update SPI error: {e}"));

        crate::shmem::record_delta_inserts(cnt);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);
    } else {
        // Insert into vp_rare in bulk.
        // Deduplicate within this batch first (set semantics within a single load).
        let mut seen = std::collections::HashSet::new();
        let unique_rows: Vec<(i64, i64, i64)> = rows
            .iter()
            .filter(|&&(s, o, g)| seen.insert((s, o, g)))
            .copied()
            .collect();
        if unique_rows.is_empty() {
            return 0;
        }
        // Insert only rows not already present — use a NOT EXISTS guard for
        // cross-statement deduplication (UNIQUE constraint enforces the rest).
        let values: Vec<String> = unique_rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{},{})", p_id, s, o, g))
            .collect();
        let sql = format!(
            "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
             SELECT p, s, o, g FROM (VALUES {}) AS v(p, s, o, g) \
             WHERE NOT EXISTS (SELECT 1 FROM _pg_ripple.vp_rare r WHERE r.p=v.p AND r.s=v.s AND r.o=v.o AND r.g=v.g)",
            values.join(",")
        );
        Spi::run_with_args(&sql, &[])
            .unwrap_or_else(|e| pgrx::error!("batch vp_rare insert SPI error: {e}"));

        let cnt = rows.len() as i64;
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
             VALUES ($1, NULL, $2) \
             ON CONFLICT (id) DO UPDATE \
             SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
            &[DatumWithOid::from(p_id), DatumWithOid::from(cnt)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count batch upsert SPI error: {e}"));
    }

    // BULK-01: record unique graph IDs in the mutation journal so CWB
    // writeback fires when the caller calls mutation_journal::flush().
    if !crate::construct_rules::has_no_rules() {
        for &(_s, _o, g) in rows {
            mutation_journal::record_write(g);
        }
    }

    rows.len() as i64
}

/// Direct-shard bulk-load path (v0.61.0 CITUS-21).
///
/// When `citus_sharding_enabled = on` and Citus is installed, bypasses the
/// coordinator routing by writing triples directly to the physical Citus shard
/// tables (`vp_{pred_id}_delta_{shard_id}`).
///
/// Triples are grouped by shard before emitting SQL to minimise round trips.
/// Falls back to `batch_insert_encoded` (coordinator path) when:
/// - Citus is not installed or sharding is disabled
/// - The predicate is in `vp_rare` (reference table — no sharding)
/// - The shard count cannot be determined
///
/// # Safety
///
/// All values are `i64` integers; no string-format interpolation of user data.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn batch_insert_encoded_shard_direct(p_id: i64, rows: &[(i64, i64, i64)]) -> i64 {
    if rows.is_empty() {
        return 0;
    }

    // Check if Citus sharding is enabled and applicable.
    if !crate::gucs::storage::CITUS_SHARDING_ENABLED.get() || !crate::citus::is_citus_loaded() {
        return batch_insert_encoded(p_id, rows);
    }

    // Only dedicated VP tables support direct-shard writes (vp_rare is a reference table).
    let table_opt = get_dedicated_vp_table(p_id);
    if table_opt.is_none() {
        return batch_insert_encoded(p_id, rows);
    }

    let delta = format!("_pg_ripple.vp_{p_id}_delta");

    // Determine the shard count from pg_dist_shard.
    let shard_count: i64 = Spi::get_one_with_args::<i64>(
        "SELECT count(*)::bigint FROM pg_dist_shard WHERE logicalrelid = $1::regclass",
        &[pgrx::datum::DatumWithOid::from(delta.as_str())],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    if shard_count <= 0 {
        // Not yet distributed — fall back to coordinator path.
        return batch_insert_encoded(p_id, rows);
    }

    // Group triples by physical shard.
    use std::collections::HashMap;
    let mut by_shard: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    for &(s, o, g) in rows {
        // Look up the actual Citus shard ID from pg_dist_shard.
        let physical_shard: i64 = Spi::get_one_with_args::<i64>(
            "SELECT s.shardid::bigint \
             FROM pg_dist_shard s \
             WHERE s.logicalrelid = $1::regclass \
               AND hashint8($2) BETWEEN s.shardminvalue::bigint AND s.shardmaxvalue::bigint \
             LIMIT 1",
            &[
                pgrx::datum::DatumWithOid::from(delta.as_str()),
                pgrx::datum::DatumWithOid::from(s),
            ],
        )
        .unwrap_or(None)
        .unwrap_or_else(|| crate::citus::compute_shard_id(s, shard_count));
        by_shard.entry(physical_shard).or_default().push((s, o, g));
    }

    let mut total: i64 = 0;
    for (shard_id, shard_rows) in &by_shard {
        let shard_table = format!("{delta}_{shard_id}");
        let values: Vec<String> = shard_rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{})", s, o, g))
            .collect();
        let sql = format!(
            "INSERT INTO {shard_table} (s, o, g) VALUES {} ON CONFLICT (s, o, g) DO NOTHING",
            values.join(","),
        );
        if let Err(e) = Spi::run_with_args(&sql, &[]) {
            pgrx::warning!("direct-shard insert failed for shard {shard_id} (falling back): {e}");
            // Fall back individual rows via coordinator.
            batch_insert_encoded(p_id, shard_rows);
        } else {
            total += shard_rows.len() as i64;
        }
    }

    // Update predicate counter once for the whole batch.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET triple_count = triple_count + $2 WHERE id = $1",
        &[
            pgrx::datum::DatumWithOid::from(p_id),
            pgrx::datum::DatumWithOid::from(total),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate count batch update SPI error: {e}"));

    crate::shmem::record_delta_inserts(total);
    crate::shmem::set_predicate_delta_bit(p_id);
    total
}

/// Delete a triple.  Returns the number of rows removed.
pub fn delete_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = encode_rdf_term(s);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = encode_rdf_term(o);

    let mut deleted = 0i64;

    // Try dedicated VP table (HTAP split).
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");

        // 1. Try to delete from delta first (fast path).
        let d = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE s=$1 AND o=$2 AND g=$3 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("triple delete delta SPI error: {e}"))
        .unwrap_or(0);

        if d > 0 {
            deleted += d;
        } else {
            // 2. Not in delta — add a tombstone to suppress it from main.
            crate::error::fault_injection::hit("tombstone_application");
            // v0.37.0: Acquire the per-predicate advisory lock in shared mode before
            // inserting a tombstone. The merge worker acquires the exclusive form
            // (pg_advisory_xact_lock) so a merge and a concurrent delete never race.
            Spi::run_with_args(
                "SELECT pg_advisory_xact_lock_shared($1)",
                &[DatumWithOid::from(p_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("delete_triple: advisory lock error: {e}"));

            Spi::run_with_args(
                &format!(
                    "INSERT INTO {tombs} (s, o, g) VALUES ($1, $2, $3) \
                     ON CONFLICT DO NOTHING"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("tombstone insert SPI error: {e}"));

            // M15-05 (v0.96.0): atomically increment tombstone_count.
            // If the prior count was 0 the view may be in tombstone-skip form
            // (rebuilt by merge_predicate after a clean compact).  Switch it back
            // to tombstone-aware (LEFT JOIN) so that new tombstones are honoured.
            let prev_count: i64 = Spi::get_one_with_args::<i64>(
                "UPDATE _pg_ripple.predicates \
                 SET tombstone_count = tombstone_count + 1 \
                 WHERE id = $1 \
                 RETURNING tombstone_count - 1",
                &[DatumWithOid::from(p_id)],
            )
            .unwrap_or(None)
            .unwrap_or(1);
            if prev_count == 0 {
                crate::storage::merge::rebuild_htap_view(p_id, true);
            }

            // Check if the triple actually existed in main.
            let in_main = Spi::get_one_with_args::<i64>(
                &format!(
                    "SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_main \
                     WHERE s = $1 AND o = $2 AND g = $3"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            deleted += in_main;
        }

        if deleted > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(deleted)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        }
    }

    // Also try vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE p=$1 AND s=$2 AND o=$3 AND g=$4 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare delete SPI error: {e}"))
    .unwrap_or(0);

    if d > 0 {
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates \
             SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        deleted += d;
    }

    deleted
}
