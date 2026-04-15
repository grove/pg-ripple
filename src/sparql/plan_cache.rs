//! Per-backend plan cache for SPARQL→SQL translations.
//!
//! Caches the result of SPARQL→SQL translation keyed by the exact query
//! text.  Structurally identical queries have the same text, so the cache
//! avoids repeated translation overhead for repeated SPARQL invocations.
//!
//! The cache is thread-local (one entry per backend), consistent with the
//! backend-local dictionary cache used in v0.1.0–v0.5.1.  The shared-memory
//! plan cache is introduced in v0.6.0.

use lru::LruCache;
use std::cell::RefCell;
use std::num::NonZeroUsize;

/// Cached translation: generated SQL + projected variable names + raw numeric variable names.
pub type CacheEntry = (String, Vec<String>, std::collections::HashSet<String>);

const DEFAULT_CAPACITY: usize = 256;

thread_local! {
    static PLAN_CACHE: RefCell<LruCache<String, CacheEntry>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(DEFAULT_CAPACITY).expect("capacity > 0"))
    );
}

/// Retrieve a cached translation for `query_text`, if available.
/// The cache key incorporates GUC values that affect SQL generation
/// (currently `max_path_depth`) so stale plans are never returned after
/// a GUC change.
pub fn get(query_text: &str) -> Option<CacheEntry> {
    let key = cache_key(query_text);
    PLAN_CACHE.with(|c| c.borrow_mut().get(&key).cloned())
}

/// Store a translation in the cache.
pub fn put(query_text: &str, entry: CacheEntry) {
    let key = cache_key(query_text);
    PLAN_CACHE.with(|c| c.borrow_mut().put(key, entry));
}

/// Build the cache key: query text + GUC values that influence SQL generation.
fn cache_key(query_text: &str) -> String {
    let max_depth = crate::MAX_PATH_DEPTH.get();
    format!("{query_text}\x00max_depth={max_depth}")
}
