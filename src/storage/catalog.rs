// ─── PredicateCatalog — backend-local VP table cache (v0.38.0) ───────────────
//
// Reduces per-atom SPI lookups in the SPARQL query translator from one per
// predicate atom to zero after the first translation of a predicate.
//
// The cache is stored in a `thread_local!` (each PostgreSQL backend is a single
// OS thread) and is invalidated on explicit `invalidate()` calls.
//
// # GUC
// `pg_ripple.predicate_cache_enabled` (bool, default `true`):
//   disabling it causes every `resolve()` call to go straight to SPI.

use std::cell::RefCell;
use std::collections::HashMap;

use pgrx::prelude::*;

// ─── Public API types ─────────────────────────────────────────────────────────

/// Descriptor for a single VP table (or the rare-predicate table).
#[derive(Debug, Clone)]
pub struct TableDesc {
    /// Whether a dedicated `_pg_ripple.vp_{pred_id}` table exists.
    /// When `false` the predicate's triples live in `_pg_ripple.vp_rare`.
    pub dedicated: bool,
}

/// Abstraction over the predicate catalog; callers use this trait so that
/// a test double or alternative storage back-end can be substituted.
pub trait PredicateCatalog {
    /// Look up the [`TableDesc`] for `pred_id`.
    ///
    /// Returns `None` when the predicate is unknown (never stored in any VP
    /// table or vp_rare entry).
    fn resolve(&self, pred_id: i64) -> Option<TableDesc>;
}

// ─── Thread-local cache ───────────────────────────────────────────────────────

thread_local! {
    /// Per-backend predicate OID cache.  Maps `pred_id` → `Option<TableDesc>`.
    /// `None` in the map means "looked up and not found"; absent means "not yet looked up".
    static CACHE: RefCell<HashMap<i64, Option<TableDesc>>> = RefCell::new(HashMap::new());
}

/// Flush all cached predicate entries for this backend.
pub fn invalidate_predicate_cache() {
    CACHE.with(|c| c.borrow_mut().clear());
}

/// The public singleton that sqlgen.rs calls.
pub struct LocalPredicateCache;

impl PredicateCatalog for LocalPredicateCache {
    fn resolve(&self, pred_id: i64) -> Option<TableDesc> {
        let enabled = crate::PREDICATE_CACHE_ENABLED.get();

        if enabled {
            // Fast path: check cache.
            let cached = CACHE.with(|c| c.borrow().get(&pred_id).cloned());
            if let Some(entry) = cached {
                return entry;
            }
        }

        // Slow path: query SPI.
        let desc = spi_resolve(pred_id);

        if enabled {
            CACHE.with(|c| c.borrow_mut().insert(pred_id, desc.clone()));
        }

        desc
    }
}

/// Look up one predicate via SPI (bypasses cache).
fn spi_resolve(pred_id: i64) -> Option<TableDesc> {
    match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_oid)) => Some(TableDesc { dedicated: true }),
        Ok(None) => Some(TableDesc { dedicated: false }),
        Err(_) => None,
    }
}

/// Process-wide (backend-local) predicate catalog singleton.
pub static PREDICATE_CACHE: LocalPredicateCache = LocalPredicateCache;

// ─── Relcache invalidation callback (v0.51.0) ────────────────────────────────

/// Register a PostgreSQL relcache invalidation callback that flushes the
/// predicate-OID thread-local cache whenever any relation is rebuilt by
/// `VACUUM FULL` (which assigns a new OID to the replacement heap).
///
/// Called once from `_PG_init`.  Safe to call in both `shared_preload_libraries`
/// and direct `CREATE EXTENSION` contexts.
pub fn register_relcache_callback() {
    unsafe {
        // SAFETY: `CacheRegisterRelcacheCallback` is a standard PostgreSQL
        // extension point for cache invalidation notifications.  The callback
        // `relcache_inval_cb` is a C-compatible `extern "C"` function that
        // performs only safe Rust (clearing a thread_local HashMap).
        // The `arg` Datum is never dereferenced by our code — we pass 0.
        pgrx::pg_sys::CacheRegisterRelcacheCallback(
            Some(relcache_inval_cb),
            pgrx::pg_sys::Datum::from(0_usize),
        );
    }
}

/// C-compatible relcache callback: called by PostgreSQL when any relation is
/// invalidated (rebuilt by VACUUM FULL, DDL, etc.).
///
/// We conservatively flush the entire predicate-OID cache so that subsequent
/// SPARQL queries re-resolve VP table OIDs via SPI rather than using a stale
/// mapping.
#[allow(non_snake_case)]
unsafe extern "C-unwind" fn relcache_inval_cb(
    _arg: pgrx::pg_sys::Datum,
    _rel_id: pgrx::pg_sys::Oid,
) {
    invalidate_predicate_cache();
}
