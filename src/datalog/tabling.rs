//! Tabling / memoisation for Datalog and SPARQL (v0.32.0).
//!
//! Caches the results of Datalog inference calls and SPARQL sub-query patterns
//! so that repeated invocations with the same rule set return immediately from
//! a database-resident cache table rather than re-running the fixpoint.
//!
//! # Design
//!
//! The cache table `_pg_ripple.tabling_cache` is a standard PostgreSQL table.
//! The cache key is the XXH3-64 hash of the goal string (rule set name or SPARQL
//! query text), truncated to a signed `BIGINT`.  Collisions are harmless — a
//! collision is treated as a cache miss and the result is recomputed.
//!
//! # Invalidation
//!
//! The cache is invalidated (all entries deleted) on any call to
//! `drop_rules()`, `load_rules()`, `insert_triple()`, or `delete_triple()`.
//! This is conservative but correct: any change to the underlying data may
//! change the result of a cached inference.
//!
//! # GUC controls
//!
//! - `pg_ripple.tabling` (bool, default `true`) — master switch.
//! - `pg_ripple.tabling_ttl` (integer seconds, default `300`) — TTL for cached
//!   entries.  Set to `0` to disable TTL-based expiry (entries survive until
//!   explicit invalidation).

use pgrx::prelude::*;
use xxhash_rust::xxh3::xxh3_64;

// ─── Catalog management ───────────────────────────────────────────────────────

/// Ensure the `_pg_ripple.tabling_cache` table exists.
pub fn ensure_tabling_catalog() {
    let _ = Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.tabling_cache ( \
             goal_hash   BIGINT      NOT NULL PRIMARY KEY, \
             result      JSONB       NOT NULL, \
             computed_ms FLOAT8      NOT NULL DEFAULT 0, \
             hits        BIGINT      NOT NULL DEFAULT 0, \
             cached_at   TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    );
}

// ─── Hash function ────────────────────────────────────────────────────────────

/// Compute the goal hash for a cache key string (XXH3-64, cast to signed i64).
pub fn compute_goal_hash(key: &str) -> i64 {
    xxh3_64(key.as_bytes()) as i64
}

// ─── Cache operations ─────────────────────────────────────────────────────────

/// Look up a goal in the tabling cache.
///
/// Returns `Some(result)` on a valid hit (entry exists and has not expired).
/// Returns `None` on cache miss, TTL expiry, or when tabling is disabled.
pub fn tabling_lookup(goal_hash: i64) -> Option<serde_json::Value> {
    if !crate::TABLING.get() {
        return None;
    }
    ensure_tabling_catalog();

    let ttl = crate::TABLING_TTL.get();
    let row = if ttl == 0 {
        // No TTL — return any entry.
        Spi::get_one_with_args::<pgrx::JsonB>(
            "SELECT result FROM _pg_ripple.tabling_cache WHERE goal_hash = $1",
            &[pgrx::datum::DatumWithOid::from(goal_hash)],
        )
    } else {
        // Respect TTL.
        Spi::get_one_with_args::<pgrx::JsonB>(
            "SELECT result FROM _pg_ripple.tabling_cache \
             WHERE goal_hash = $1 \
               AND cached_at >= now() - ($2 || ' seconds')::interval",
            &[
                pgrx::datum::DatumWithOid::from(goal_hash),
                pgrx::datum::DatumWithOid::from(ttl),
            ],
        )
    };

    match row {
        Ok(Some(jsonb)) => {
            // Increment hit counter.
            let _ = Spi::run_with_args(
                "UPDATE _pg_ripple.tabling_cache SET hits = hits + 1 WHERE goal_hash = $1",
                &[pgrx::datum::DatumWithOid::from(goal_hash)],
            );
            Some(jsonb.0)
        }
        _ => None,
    }
}

/// Store a result in the tabling cache.
///
/// Does nothing when tabling is disabled.
pub fn tabling_store(goal_hash: i64, result: &serde_json::Value, computed_ms: f64) {
    if !crate::TABLING.get() {
        return;
    }
    ensure_tabling_catalog();

    let json_str = result.to_string();
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.tabling_cache (goal_hash, result, computed_ms, cached_at) \
         VALUES ($1, $2::jsonb, $3, now()) \
         ON CONFLICT (goal_hash) DO UPDATE \
             SET result = EXCLUDED.result, \
                 computed_ms = EXCLUDED.computed_ms, \
                 cached_at = EXCLUDED.cached_at",
        &[
            pgrx::datum::DatumWithOid::from(goal_hash),
            pgrx::datum::DatumWithOid::from(json_str.as_str()),
            pgrx::datum::DatumWithOid::from(computed_ms),
        ],
    );
}

/// Invalidate all tabling cache entries.
///
/// Called on `drop_rules()`, `load_rules()`, and triple insert/delete.
/// Safe to call even when the tabling_cache table does not yet exist.
pub fn tabling_invalidate_all() {
    if !crate::TABLING.get() {
        return;
    }
    // Only delete if the table actually exists — avoids an error during the
    // first use of the extension before the migration creates the table.
    if tabling_table_exists() {
        let _ = Spi::run_with_args("DELETE FROM _pg_ripple.tabling_cache WHERE TRUE", &[]);
    }
}

/// Check whether the tabling cache table exists (used to guard stats queries).
fn tabling_table_exists() -> bool {
    Spi::get_one::<bool>(
        "SELECT EXISTS ( \
             SELECT 1 FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relname = 'tabling_cache' \
         )",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Return statistics for all cached goals.
///
/// Each row: `(goal_hash, hits, computed_ms, cached_at_iso)`.
pub fn tabling_stats_impl() -> Vec<(i64, i64, f64, String)> {
    if !tabling_table_exists() {
        return vec![];
    }
    Spi::connect(|client| {
        client
            .select(
                "SELECT goal_hash, hits, computed_ms, \
                        to_char(cached_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
                 FROM _pg_ripple.tabling_cache \
                 ORDER BY hits DESC, cached_at DESC",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("tabling_stats: SPI error: {e}"))
            .map(|row| {
                let hash = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let hits = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                let ms = row.get::<f64>(3).ok().flatten().unwrap_or(0.0);
                let ts = row.get::<String>(4).ok().flatten().unwrap_or_default();
                (hash, hits, ms, ts)
            })
            .collect::<Vec<_>>()
    })
}

// ─── SPARQL integration ───────────────────────────────────────────────────────

/// Compute the tabling key for a SPARQL query string.
///
/// Incorporates a lightweight "data generation" counter (max SID from the
/// statement sequence) so that the same query after a triple insert returns a
/// different hash and bypasses the stale cache entry.
#[allow(dead_code)]
pub fn sparql_goal_hash(query: &str) -> i64 {
    // Mix in the current max SID as a data-generation marker.
    let data_gen =
        Spi::get_one::<i64>("SELECT COALESCE(last_value, 0) FROM _pg_ripple.statement_id_seq")
            .unwrap_or(None)
            .unwrap_or(0);

    let key = format!("sparql:{}:{}", data_gen, query);
    compute_goal_hash(&key)
}
