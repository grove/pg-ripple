//! Per-backend plan cache for SPARQL→SQL translations.
//!
//! Caches the result of SPARQL→SQL translation keyed by the exact query
//! text.  Structurally identical queries have the same text, so the cache
//! avoids repeated translation overhead for repeated SPARQL invocations.
//!
//! The cache is thread-local (one entry per backend), consistent with the
//! backend-local dictionary cache used in v0.1.0–v0.5.1.  The shared-memory
//! plan cache is introduced in v0.6.0.
//!
//! # v0.13.0 — instrumentation
//!
//! Hit and miss counters are tracked per-backend and exposed via
//! `pg_ripple.plan_cache_stats()` for monitoring and benchmarking.

use lru::LruCache;
use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Cached translation: generated SQL + projected variable names + raw numeric variable names + raw text variable names + raw IRI variable names + raw double variable names + wcoj_preamble flag.
pub type CacheEntry = (
    String,
    Vec<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    bool,
);

const DEFAULT_CAPACITY: usize = 256;

thread_local! {
    // SAFETY: Capacity is initialised from the PLAN_CACHE_CAPACITY GUC at first use.
    // If the GUC is 0 or the process is not inside PostgreSQL (e.g. unit tests),
    // DEFAULT_CAPACITY is used as a safe fallback.
    #[allow(clippy::expect_used)]
    static PLAN_CACHE: RefCell<LruCache<String, CacheEntry>> = RefCell::new(
        // CACHE-CAP-01 (v0.82.0): initialise from GUC; fall back to DEFAULT_CAPACITY.
        // The GUC may not be readable before _PG_init sets it, so we catch panics.
        {
            let cap = std::panic::catch_unwind(|| crate::PLAN_CACHE_CAPACITY.get())
                .unwrap_or(DEFAULT_CAPACITY as i32)
                .max(1) as usize;
            LruCache::new(NonZeroUsize::new(cap).expect("capacity > 0"))
        }
    );
}

/// Process-wide hit counter (cumulative across all backends in this process).
static HIT_COUNT: AtomicU64 = AtomicU64::new(0);
/// Process-wide miss counter.
static MISS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Retrieve a cached translation for `query_text`, if available.
/// The cache key incorporates GUC values that affect SQL generation
/// (currently `max_path_depth`) so stale plans are never returned after
/// a GUC change.
pub fn get(query_text: &str) -> Option<CacheEntry> {
    let key = cache_key(query_text);
    let result = PLAN_CACHE.with(|c| c.borrow_mut().get(&key).cloned());
    if result.is_some() {
        HIT_COUNT.fetch_add(1, Ordering::Relaxed);
    } else {
        MISS_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// Store a translation in the cache.
pub fn put(query_text: &str, entry: CacheEntry) {
    let key = cache_key(query_text);
    PLAN_CACHE.with(|c| c.borrow_mut().put(key, entry));
}

/// Return `(hit_count, miss_count, current_cache_size, capacity)`.
pub fn stats() -> (u64, u64, usize, usize) {
    let hits = HIT_COUNT.load(Ordering::Relaxed);
    let misses = MISS_COUNT.load(Ordering::Relaxed);
    let (size, cap) = PLAN_CACHE.with(|c| {
        let borrowed = c.borrow();
        (borrowed.len(), borrowed.cap().get())
    });
    (hits, misses, size, cap)
}

/// Reset hit/miss counters and evict all cached entries.
pub fn reset() {
    HIT_COUNT.store(0, Ordering::Relaxed);
    MISS_COUNT.store(0, Ordering::Relaxed);
    PLAN_CACHE.with(|c| c.borrow_mut().clear());
}

/// Build the cache key: algebra digest (XXH3-128 of the normalised SPARQL IR)
/// plus GUC values that influence SQL generation.
///
/// Using the algebra IR (via `spargebra::Query`'s `Display` impl) instead of
/// the raw query text means whitespace variants and prefix-form variants share
/// the same cache slot.  Falls back to the raw text hash when parsing fails.
///
/// # CACHE-RLS-01 (v0.80.0)
/// The key also includes the current PostgreSQL role (user ID) and the
/// `inference_mode` GUC so that two roles with different RLS policies or
/// inference settings never share a cached plan.
fn cache_key(query_text: &str) -> String {
    let max_depth = crate::MAX_PATH_DEPTH.get();
    let bgp_reorder = crate::BGP_REORDER.get();
    // CACHE-RLS-01: include current role OID so cross-user plan leakage is
    // impossible.  GetUserId() is signal-safe and never fails.
    // SAFETY: GetUserId() is a pure accessor with no side effects; always safe.
    let role_oid: u32 = unsafe { pgrx::pg_sys::GetUserId().into() };
    // Include inference_mode GUC in key.
    let inference_mode = crate::INFERENCE_MODE
        .get()
        .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
        .unwrap_or_else(|| "off".to_string());
    // PLAN-CACHE-GUC-02 (v0.81.0): include additional GUCs that affect SQL generation.
    let normalize_iris = crate::NORMALIZE_IRIS.get();
    let wcoj_enabled = crate::WCOJ_ENABLED.get();
    let wcoj_min = crate::WCOJ_MIN_TABLES.get();
    let topn_pushdown = crate::TOPN_PUSHDOWN.get();
    let sparql_max_rows = crate::SPARQL_MAX_ROWS.get();
    let sparql_overflow = crate::SPARQL_OVERFLOW_ACTION
        .get()
        .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
        .unwrap_or_else(|| "error".to_string());
    let federation_timeout = crate::FEDERATION_TIMEOUT.get();
    let pgvector_enabled = crate::PGVECTOR_ENABLED.get();
    // Normalise via spargebra Display → canonical SPARQL → hash.
    let text_to_hash = match spargebra::SparqlParser::new().parse_query(query_text) {
        Ok(q) => format!("{q}"),
        Err(_) => query_text.to_owned(),
    };
    let digest = xxhash_rust::xxh3::xxh3_128(text_to_hash.as_bytes());
    format!(
        "{digest:x}\x00max_depth={max_depth}\x00bgp_reorder={bgp_reorder}\x00role={role_oid}\
         \x00inference_mode={inference_mode}\x00normalize_iris={normalize_iris}\
         \x00wcoj_enabled={wcoj_enabled}\x00wcoj_min={wcoj_min}\
         \x00topn_pushdown={topn_pushdown}\x00sparql_max_rows={sparql_max_rows}\
         \x00sparql_overflow={sparql_overflow}\x00federation_timeout={federation_timeout}\
         \x00pgvector_enabled={pgvector_enabled}"
    )
}
