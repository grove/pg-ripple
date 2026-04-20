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

/// Cached translation: generated SQL + projected variable names + raw numeric variable names + raw text variable names.
pub type CacheEntry = (String, Vec<String>, std::collections::HashSet<String>, std::collections::HashSet<String>);

const DEFAULT_CAPACITY: usize = 256;

thread_local! {
    // SAFETY: DEFAULT_CAPACITY is a compile-time non-zero literal (256).
    #[allow(clippy::expect_used)]
    static PLAN_CACHE: RefCell<LruCache<String, CacheEntry>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(DEFAULT_CAPACITY).expect("capacity > 0"))
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
fn cache_key(query_text: &str) -> String {
    let max_depth = crate::MAX_PATH_DEPTH.get();
    let bgp_reorder = crate::BGP_REORDER.get();
    // Normalise via spargebra Display → canonical SPARQL → hash.
    let text_to_hash = match spargebra::SparqlParser::new().parse_query(query_text) {
        Ok(q) => format!("{q}"),
        Err(_) => query_text.to_owned(),
    };
    let digest = xxhash_rust::xxh3::xxh3_128(text_to_hash.as_bytes());
    format!("{digest:x}\x00max_depth={max_depth}\x00bgp_reorder={bgp_reorder}")
}
