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

/// Cached translation: generated SQL + projected variable names in SELECT order.
pub type CacheEntry = (String, Vec<String>);

const DEFAULT_CAPACITY: usize = 256;

thread_local! {
    static PLAN_CACHE: RefCell<LruCache<String, CacheEntry>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(DEFAULT_CAPACITY).expect("capacity > 0"))
    );
}

/// Retrieve a cached translation for `query_text`, if available.
pub fn get(query_text: &str) -> Option<CacheEntry> {
    PLAN_CACHE.with(|c| c.borrow_mut().get(query_text).cloned())
}

/// Store a translation in the cache.
pub fn put(query_text: &str, entry: CacheEntry) {
    PLAN_CACHE.with(|c| c.borrow_mut().put(query_text.to_owned(), entry));
}
