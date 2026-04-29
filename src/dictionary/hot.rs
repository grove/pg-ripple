//! Tiered hot/cold dictionary (v0.10.0).
//!
//! `_pg_ripple.dictionary_hot` (UNLOGGED) holds IRIs ≤512 bytes and all
//! predicate/prefix IRIs — the working set that fits in shared buffers.
//! The full `dictionary` table is unchanged; the encoder checks the hot
//! table first, dramatically reducing random I/O at large scale.
//!
//! The hot table is populated at extension load via `pg_prewarm` and updated
//! whenever a new predicate or prefix IRI is encoded.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

/// Create the hot dictionary table if it does not already exist.
pub fn ensure_hot_table() {
    // UNLOGGED for max performance; crash-recovery is handled by rebuilding
    // from the main dictionary table on startup.
    // DICT-01 (v0.74.0): hash_hi/hash_lo BIGINT pair replaces hash BYTEA.
    Spi::run_with_args(
        "CREATE UNLOGGED TABLE IF NOT EXISTS _pg_ripple.dictionary_hot ( \
             id       BIGINT   NOT NULL PRIMARY KEY, \
             hash_hi  BIGINT   NOT NULL, \
             hash_lo  BIGINT   NOT NULL, \
             value    TEXT     NOT NULL, \
             kind     SMALLINT NOT NULL DEFAULT 0 \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary_hot creation error: {e}"));

    Spi::run_with_args(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hot_hash_split \
         ON _pg_ripple.dictionary_hot (hash_hi, hash_lo)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary_hot hash index error: {e}"));
}

/// Prewarm the hot table: copy all short IRIs (≤512 bytes) and predicate
/// IRIs from the main dictionary into `dictionary_hot`.
///
/// This is idempotent and safe to call multiple times.
pub fn prewarm_hot_table() {
    // Insert all IRI terms whose value fits in 512 bytes.
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.dictionary_hot (id, hash_hi, hash_lo, value, kind) \
         SELECT id, hash_hi, hash_lo, value, kind \
         FROM _pg_ripple.dictionary \
         WHERE kind = 0 AND octet_length(value) <= 512 \
         ON CONFLICT (id) DO NOTHING",
        &[],
    );

    // Also insert all predicate IRIs regardless of length.
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.dictionary_hot (id, hash_hi, hash_lo, value, kind) \
         SELECT d.id, d.hash_hi, d.hash_lo, d.value, d.kind \
         FROM _pg_ripple.predicates p \
         JOIN _pg_ripple.dictionary d ON d.id = p.id \
         ON CONFLICT (id) DO NOTHING",
        &[],
    );

    // Attempt pg_prewarm to load the hot table into shared buffers.
    // pg_prewarm is optional; ignore errors if the extension is not installed.
    let _ = Spi::run_with_args(
        "SELECT pg_prewarm('_pg_ripple.dictionary_hot') \
         WHERE EXISTS ( \
             SELECT 1 FROM pg_proc WHERE proname = 'pg_prewarm' \
         )",
        &[],
    );
}

/// Add a term to the hot table when it qualifies (IRI ≤512 bytes).
///
/// Called after encoding a new predicate or prefix IRI.
/// DICT-01 (v0.74.0): takes (hash_hi, hash_lo) instead of hash_bytes.
#[allow(dead_code)]
pub fn add_to_hot(id: i64, hash_hi: i64, hash_lo: i64, value: &str, kind: i16) {
    if kind != 0 {
        return; // Only IRIs go into the hot table.
    }
    if value.len() > 512 {
        return; // Too large for hot table.
    }
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.dictionary_hot (id, hash_hi, hash_lo, value, kind) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (id) DO NOTHING",
        &[
            DatumWithOid::from(id),
            DatumWithOid::from(hash_hi),
            DatumWithOid::from(hash_lo),
            DatumWithOid::from(value),
            DatumWithOid::from(kind),
        ],
    );
}

/// Lookup a term in the hot table by its (hash_hi, hash_lo) BIGINT pair.
///
/// Returns the dictionary `id` if found, or `None`.
#[allow(dead_code)]
pub fn lookup_hot(hash_hi: i64, hash_lo: i64) -> Option<i64> {
    Spi::get_one_with_args::<i64>(
        "SELECT id FROM _pg_ripple.dictionary_hot WHERE hash_hi = $1 AND hash_lo = $2",
        &[DatumWithOid::from(hash_hi), DatumWithOid::from(hash_lo)],
    )
    .ok()
    .flatten()
}
