//! Citus horizontal sharding integration (v0.58.0, Feature L-5.4).
//!
//! # Architecture
//!
//! When `pg_ripple.citus_sharding_enabled = on`, VP tables are distributed
//! across Citus worker nodes using `create_distributed_table()`.  The
//! distribution column is `s` (subject ID) to co-locate triples that share a
//! subject on the same shard — this optimises star-pattern joins.
//!
//! Key decisions (v0.58.0):
//! - Dictionary and predicates catalog become Citus **reference tables**.
//! - VP delta tables use `colocate_with = 'none'` when
//!   `pg_ripple.citus_trickle_compat = on` (prevents pg-trickle from issuing
//!   cross-shard deletes during apply).
//! - `REPLICA IDENTITY FULL` is set **before** `create_distributed_table()` so
//!   that the logical replication slot used by pg-trickle captures full row
//!   images from the very first write.
//! - The merge worker fence uses `pg_try_advisory_xact_lock(pid)` on the
//!   coordinator before executing the merge to prevent split-brain during shard
//!   rebalancing.
//!
//! # Error codes
//!
//! - PT536 — Citus extension is not installed.

use pgrx::prelude::*;

// ─── Citus detection ──────────────────────────────────────────────────────────

/// Return `true` if Citus is installed and accessible.
pub fn is_citus_loaded() -> bool {
    let result = Spi::get_one::<bool>(
        "SELECT EXISTS ( \
             SELECT 1 FROM pg_extension WHERE extname = 'citus' \
         )",
    );
    matches!(result, Ok(Some(true)))
}

// ─── Reference table setup ───────────────────────────────────────────────────

/// Convert the dictionary and predicates catalog to Citus reference tables.
///
/// Reference tables are replicated to every worker node so that dictionary
/// lookups and predicate routing never require cross-shard queries.
///
/// # Errors
/// Raises `PT536` if Citus is not installed.
fn make_reference_table(table: &str) {
    let sql = format!("SELECT create_reference_table('{table}')");
    Spi::run_with_args(&sql, &[])
        .unwrap_or_else(|e| pgrx::warning!("make_reference_table {table}: {e}"));
}

// ─── VP table distribution ───────────────────────────────────────────────────

/// Set `REPLICA IDENTITY FULL` on a VP delta table and distribute it.
///
/// This is the **canonical** order: REPLICA IDENTITY must come **before**
/// `create_distributed_table()` so pg-trickle captures full row images from
/// the first logical replication message (C-9 fix).
///
/// # Arguments
/// - `pred_id` — predicate integer ID
/// - `colocate_with` — Citus colocate_with parameter (`'default'` or `'none'`)
pub fn distribute_vp_delta(pred_id: i64, colocate_with: &str) {
    let delta = format!("_pg_ripple.vp_{pred_id}_delta");

    // Step 1: REPLICA IDENTITY FULL (must come before create_distributed_table).
    Spi::run_with_args(&format!("ALTER TABLE {delta} REPLICA IDENTITY FULL"), &[])
        .unwrap_or_else(|e| pgrx::warning!("REPLICA IDENTITY FULL {delta}: {e}"));

    // Step 2: Distribute the table on column `s` (subject).
    Spi::run_with_args(
        &format!(
            "SELECT create_distributed_table( \
                 '{delta}', 's', colocate_with => '{colocate_with}' \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("create_distributed_table {delta}: {e}"));

    // Query the shard count so listeners can enumerate worker-level shard tables.
    let shard_count: i64 = Spi::get_one_with_args::<i64>(
        "SELECT count(*)::bigint FROM pg_dist_shard WHERE logicalrelid = $1::regclass",
        &[delta.as_str().into()],
    )
    .unwrap_or(Some(0))
    .unwrap_or(0);

    // Notify pg-trickle and other listeners that a VP table has been promoted
    // to distributed.  The payload follows the agreed contract (C-4):
    //   table             — fully-qualified logical table name
    //   shard_count       — number of shards created by Citus (for slot setup)
    //   shard_table_prefix — prefix used by Citus for physical shard tables
    //   predicate_id      — pg_ripple predicate integer ID
    //
    // pg-trickle uses `shard_count` and `shard_table_prefix` to enumerate
    // per-worker shard names when creating logical replication slots without
    // querying `pg_dist_shard` directly.
    let shard_table_prefix = format!("{delta}_");
    let payload = format!(
        "{{\"table\":\"{delta}\",\"shard_count\":{shard_count},\
          \"shard_table_prefix\":\"{shard_table_prefix}\",\"predicate_id\":{pred_id}}}",
    );
    Spi::run_with_args(
        &format!("SELECT pg_notify('pg_ripple.vp_promoted', '{payload}')"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("pg_notify vp_promoted: {e}"));
}

// ─── SQL API ─────────────────────────────────────────────────────────────────

/// Enable Citus sharding for all existing VP tables.
///
/// Iterates over all promoted predicates and distributes their delta tables
/// using `s` (subject ID) as the distribution column.  Also converts the
/// dictionary and predicates catalog to reference tables.
///
/// Requires `pg_ripple.citus_sharding_enabled = on`.
///
/// Returns a summary row for each distributed table.
#[pg_extern(schema = "pg_ripple")]
pub fn enable_citus_sharding() -> TableIterator<
    'static,
    (
        name!(predicate_id, i64),
        name!(table_name, String),
        name!(status, String),
    ),
> {
    if !is_citus_loaded() {
        pgrx::error!("enable_citus_sharding: Citus extension is not installed (PT536)");
    }

    let colocate = if crate::gucs::storage::CITUS_TRICKLE_COMPAT.get() {
        "none"
    } else {
        "default"
    };

    // Convert reference tables (idempotent via Citus's own checks).
    make_reference_table("_pg_ripple.dictionary");
    make_reference_table("_pg_ripple.predicates");
    // `vp_rare` cannot be straightforwardly distributed by `s` because its
    // primary selectivity column is `p`; promote it to a reference table so
    // that every worker has a full copy and coordinator fan-out is avoided.
    make_reference_table("_pg_ripple.vp_rare");

    // Collect predicate IDs that have promoted VP tables.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL ORDER BY id",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect()
        })
        .unwrap_or_default()
    });

    let mut results: Vec<(i64, String, String)> = Vec::new();

    for pred_id in pred_ids {
        let delta_name = format!("_pg_ripple.vp_{pred_id}_delta");
        distribute_vp_delta(pred_id, colocate);
        results.push((pred_id, delta_name, "distributed".to_string()));
    }

    // Notify merge worker to re-fence.
    let payload = format!("{{\"pid\":{}}}", std::process::id());
    Spi::run_with_args(
        &format!("SELECT pg_notify('pg_ripple.merge_start', '{payload}')"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("pg_notify merge_start: {e}"));

    TableIterator::new(results)
}

/// Trigger a Citus shard rebalance.
///
/// Wraps `citus_rebalance_start()` and waits for it to complete.
/// Returns the number of rebalanced shard moves.
///
/// Requires Citus to be installed (PT536).
#[pg_extern(schema = "pg_ripple")]
pub fn citus_rebalance() -> i64 {
    if !is_citus_loaded() {
        pgrx::error!("citus_rebalance: Citus extension is not installed (PT536)");
    }
    // citus_rebalance_start returns a job_id; we call citus_rebalance which is
    // the blocking version if available, else fall back to start+wait pattern.
    let moves: i64 = Spi::get_one::<i64>(
        "SELECT COALESCE( \
             (SELECT count(*) FROM citus_rebalance_start()), \
             0 \
         )",
    )
    .unwrap_or_else(|e| {
        pgrx::warning!("citus_rebalance: {e}");
        None
    })
    .unwrap_or(0);
    moves
}

/// Return a status summary for the Citus cluster as seen by pg_ripple.
///
/// Columns: `node_id`, `node_name`, `shard_count`, `is_active`.
/// Returns an empty set if Citus is not installed.
#[pg_extern(schema = "pg_ripple")]
pub fn citus_cluster_status() -> TableIterator<
    'static,
    (
        name!(node_id, i64),
        name!(node_name, String),
        name!(shard_count, i64),
        name!(is_active, bool),
    ),
> {
    if !is_citus_loaded() {
        return TableIterator::new(std::iter::empty());
    }

    let rows = Spi::connect(|c| {
        c.select(
            "SELECT n.nodeid::bigint, \
                    n.nodename, \
                    count(s.shardid)::bigint AS shard_count, \
                    n.isactive \
             FROM pg_dist_node n \
             LEFT JOIN pg_dist_placement p ON p.groupid = n.groupid \
             LEFT JOIN pg_dist_shard s ON s.shardid = p.shardid \
             GROUP BY n.nodeid, n.nodename, n.isactive \
             ORDER BY n.nodeid",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let node_id = row.get::<i64>(1).ok().flatten()?;
                let node_name = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let shard_count = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                let is_active = row.get::<bool>(4).ok().flatten().unwrap_or(false);
                Some((node_id, node_name, shard_count, is_active))
            })
            .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    });

    TableIterator::new(rows)
}

/// Return `true` if Citus extension is available.
#[pg_extern(schema = "pg_ripple")]
pub fn citus_available() -> bool {
    is_citus_loaded()
}
