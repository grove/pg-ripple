//! Rule plan cache for Datalog inference (v0.30.0).
//!
//! Caches the SQL strings generated for each rule set so that repeated
//! `infer()` and `infer_agg()` calls on the same rule set skip the parse +
//! compile step.
//!
//! # Design
//!
//! The cache is a process-local `Mutex<HashMap<String, PlanEntry>>` keyed on
//! rule set name.  It resets on backend restart (PostgreSQL process restart).
//! Cache invalidation is triggered by `drop_rules()` or `load_rules()`.
//!
//! # GUC controls
//!
//! - `pg_ripple.rule_plan_cache` (bool, default `true`) — master switch.
//! - `pg_ripple.rule_plan_cache_size` (int, default 64) — max entries.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// ─── Cache state ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct PlanEntry {
    sqls: Vec<String>,
    hits: i64,
    misses: i64,
}

struct CacheState {
    /// rule_set_name → plan entry (SQL strings + hit/miss counters).
    entries: HashMap<String, PlanEntry>,
}

static CACHE: OnceLock<Mutex<CacheState>> = OnceLock::new();

fn get_cache() -> &'static Mutex<CacheState> {
    CACHE.get_or_init(|| {
        Mutex::new(CacheState {
            entries: HashMap::new(),
        })
    })
}

// ─── Per-rule-set separate cache for aggregate rules ─────────────────────────

static AGG_CACHE: OnceLock<Mutex<CacheState>> = OnceLock::new();

fn get_agg_cache() -> &'static Mutex<CacheState> {
    AGG_CACHE.get_or_init(|| {
        Mutex::new(CacheState {
            entries: HashMap::new(),
        })
    })
}

// ─── Cache operations ─────────────────────────────────────────────────────────

/// Look up the compiled SQL for a rule set.
///
/// Returns `Some(sqls)` on a cache hit (and increments the hit counter).
/// Returns `None` on a miss (and increments the miss counter, creating the entry
/// if it doesn't exist yet so the miss is tracked).
#[allow(dead_code)]
pub fn lookup(rule_set: &str) -> Option<Vec<String>> {
    if !crate::RULE_PLAN_CACHE.get() {
        return None;
    }
    let Ok(mut state) = get_cache().lock() else {
        return None;
    };
    match state.entries.get_mut(rule_set) {
        Some(entry) if !entry.sqls.is_empty() => {
            entry.hits += 1;
            Some(entry.sqls.clone())
        }
        _ => {
            // Record a miss.
            state.entries.entry(rule_set.to_owned()).or_default().misses += 1;
            None
        }
    }
}

/// Store the compiled SQL for a rule set.
///
/// Evicts the entry with the lowest hit count when the cache is full.
#[allow(dead_code)]
pub fn store(rule_set: &str, sqls: &[String]) {
    if !crate::RULE_PLAN_CACHE.get() {
        return;
    }
    let Ok(mut state) = get_cache().lock() else {
        return;
    };
    let max_entries = crate::RULE_PLAN_CACHE_SIZE.get().max(1) as usize;
    if state.entries.len() >= max_entries && !state.entries.contains_key(rule_set) {
        // Simple eviction: remove the entry with the fewest hits.
        if let Some(victim) = state
            .entries
            .iter()
            .min_by_key(|(_, v)| v.hits)
            .map(|(k, _)| k.clone())
        {
            state.entries.remove(&victim);
        }
    }
    let entry = state.entries.entry(rule_set.to_owned()).or_default();
    entry.sqls = sqls.to_vec();
}

/// Invalidate the cache entry for a rule set (called on drop_rules / load_rules).
pub fn invalidate(rule_set: &str) {
    if let Ok(mut state) = get_cache().lock() {
        state.entries.remove(rule_set);
    }
    if let Ok(mut state) = get_agg_cache().lock() {
        state.entries.remove(rule_set);
    }
}

/// Invalidate all cache entries.
#[allow(dead_code)]
pub fn invalidate_all() {
    if let Ok(mut state) = get_cache().lock() {
        state.entries.clear();
    }
    if let Ok(mut state) = get_agg_cache().lock() {
        state.entries.clear();
    }
}

// ─── Aggregate-specific cache operations ─────────────────────────────────────

/// Look up the compiled aggregate SQL for a rule set.
pub fn lookup_agg(rule_set: &str) -> Option<Vec<String>> {
    if !crate::RULE_PLAN_CACHE.get() {
        return None;
    }
    let Ok(mut state) = get_agg_cache().lock() else {
        return None;
    };
    match state.entries.get_mut(rule_set) {
        Some(entry) if !entry.sqls.is_empty() => {
            entry.hits += 1;
            Some(entry.sqls.clone())
        }
        _ => {
            state.entries.entry(rule_set.to_owned()).or_default().misses += 1;
            None
        }
    }
}

/// Store compiled aggregate SQL for a rule set.
pub fn store_agg(rule_set: &str, sqls: &[String]) {
    if !crate::RULE_PLAN_CACHE.get() {
        return;
    }
    let Ok(mut state) = get_agg_cache().lock() else {
        return;
    };
    let entry = state.entries.entry(rule_set.to_owned()).or_default();
    entry.sqls = sqls.to_vec();
}

// ─── Statistics ───────────────────────────────────────────────────────────────

/// Cache statistics record returned by `rule_plan_cache_stats()`.
pub struct CacheStats {
    pub rule_set: String,
    pub hits: i64,
    pub misses: i64,
    pub entries: i32,
}

/// Collect cache statistics across both the regular and aggregate caches.
pub fn stats() -> Vec<CacheStats> {
    let mut result = Vec::new();

    // Merge regular + aggregate caches, summing hits/misses for shared keys.
    let mut combined: HashMap<String, (i64, i64)> = HashMap::new();

    if let Ok(state) = get_cache().lock() {
        for (k, v) in &state.entries {
            let e = combined.entry(k.clone()).or_default();
            e.0 += v.hits;
            e.1 += v.misses;
        }
    }
    if let Ok(state) = get_agg_cache().lock() {
        for (k, v) in &state.entries {
            let e = combined.entry(k.clone()).or_default();
            e.0 += v.hits;
            e.1 += v.misses;
        }
    }

    let total_entries = combined.len() as i32;
    for (rule_set, (hits, misses)) in combined {
        result.push(CacheStats {
            rule_set,
            hits,
            misses,
            entries: total_entries,
        });
    }
    result
}
