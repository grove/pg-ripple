//! Shared memory for pg_ripple v0.6.0+ (HTAP Architecture).
//!
//! # Shared objects
//!
//! | Name | Type | Purpose |
//! |------|------|---------|
//! | `MERGE_WORKER_PID` | `PgAtomic<AtomicI32>` | PID of the merge background worker |
//! | `LAYOUT_VERSION` | `PgAtomic<AtomicU32>` | Slot-versioning magic for safe upgrades |
//! | `TOTAL_DELTA_ROWS` | `PgAtomic<AtomicI64>` | Running count of unmerged delta rows |
//! | `DELTA_BLOOM` | `PgLwLock<[u64; 16]>` | 1024-bit bloom filter: which predicates have delta rows |
//! | `ENCODE_CACHE_S0..S3` | `PgLwLock<EncodeCacheShard>` | 4-way set-associative shared-memory cache (v0.22.0+) |
//! | `CACHE_STATS` | `PgAtomic<CacheStats>` | Hit/miss/eviction counters |
//!
//! ## Bloom filter (delta existence)
//!
//! `DELTA_BLOOM` is a 1024-bit Bloom filter (16 × u64) that tracks which
//! predicates have rows in their delta tables.  Setting a bit is lossy (false
//! positives are acceptable — the query path just scans delta unnecessarily);
//! false negatives would silently drop results so we only clear bits during
//! an explicit merge cycle.
//!
//! Two independent multiplicative hash functions map a predicate ID to two bit
//! positions; a bit is set when either or both positions are set.  On merge the
//! two bits for that predicate are cleared so subsequent reads can skip delta.
//!
//! ## Encode cache (shared-memory dictionary, v0.22.0+)
//!
//! **4-way set-associative design**: 1024 sets × 4 ways per set.  Each way is
//! `(hash128_scoped: u128, id: i64, age: u8)`.  Empty slots have id == 0.
//!
//! Hashing: set index = `(hash128_scoped >> 126) & 0x3FF` (top 10 bits after
//! database-scoping).  LRU within each set: on hit, age is reset to 0; on insert,
//! the way with the highest age is evicted and replaced.  Age increments on each
//! miss within that set.
//!
//! Lookups use a **shared** LW lock (many readers); inserts use **exclusive**.
//! Hit rate is tracked in `CACHE_STATS` for monitoring.
//!
//! These objects are only available when the extension is loaded via
//! `shared_preload_libraries`.  When loaded via `CREATE EXTENSION` (without
//! shared_preload_libraries), all shmem operations are no-ops — `SHMEM_READY`
//! ensures callers never attempt to access an uninitialised object.

use pgrx::prelude::*;
use pgrx::{PgAtomic, PgLwLock, pg_shmem_init};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, AtomicU64, Ordering};

// ─── Encode cache types (v0.22.0: 4-way set-associative) ───────────────────────

/// One way in a set: stored as u128 (hash) with id in a separate array.
/// Packing: `hash | (age << 126)` uses top 2 bits for age (0..3).
/// id == 0 means empty.
/// One set: 4 ways per set.
pub type EncodeSet = [u128; 4];

/// Shard: all sets for this shard index.
pub type EncodeCacheShard = [EncodeSet; ENCODE_CACHE_SETS];

/// Parallel array: stores i64 IDs corresponding to ways.
pub type EncodeCacheIds = [[i64; 4]; ENCODE_CACHE_SETS];

/// Number of sets per shard (1024).
pub const ENCODE_CACHE_SETS: usize = 1024;

/// Total encode-cache capacity across all shards.
pub const ENCODE_CACHE_CAPACITY: usize = ENCODE_CACHE_SETS * 4;

// ─── Layout version guard ─────────────────────────────────────────────────────

/// Magic constant for shared-memory slot versioning: `"pgri"` as u32.
const SHMEM_MAGIC: u32 = 0x70677269;

/// Shared layout version.  Initialised to `SHMEM_MAGIC` on first startup.
pub static LAYOUT_VERSION: PgAtomic<AtomicU32> =
    unsafe { PgAtomic::new(c"pg_ripple_layout_version") };

// ─── Merge worker coordination ────────────────────────────────────────────────

/// PID of the running merge background worker (0 when not running).
pub static MERGE_WORKER_PID: PgAtomic<AtomicI32> = unsafe { PgAtomic::new(c"pg_ripple_merge_pid") };

// ─── Delta row tracker (bloom-filter substitute) ──────────────────────────────

/// Total number of unmerged rows across all VP delta tables.
pub static TOTAL_DELTA_ROWS: PgAtomic<AtomicI64> =
    unsafe { PgAtomic::new(c"pg_ripple_delta_rows") };

// ─── Bloom filter (per-bit reference counting, v0.22.0+) ──────────────────────

/// 1024-bit Bloom filter: which predicates may have rows in their delta tables.
/// Indexed by two multiplicative hashes of the predicate ID.
///
/// v0.22.0+: Replaced with per-bit reference counting to prevent hash collisions
/// from causing false-negative delta skips.  Each bit position gets an 8-bit
/// saturating counter; a bit is only cleared when its counter reaches 0.
pub static DELTA_BLOOM: PgLwLock<[u64; 16]> = unsafe { PgLwLock::new(c"pg_ripple_delta_bloom") };

/// Per-bit reference counters for the delta bloom filter (v0.22.0+).
/// 1024 u8 values, one for each bit position in DELTA_BLOOM.
/// Counter saturates at 255; a bit is only cleared when the counter reaches 0.
pub static DELTA_BLOOM_COUNTERS: PgLwLock<[u8; 1024]> =
    unsafe { PgLwLock::new(c"pg_ripple_delta_bloom_counters") };

// ─── Shared-memory encode cache (1 shard × 1024 sets × 4 ways = 4096 capacity) ─

pub static ENCODE_CACHE_S0: PgLwLock<EncodeCacheShard> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_s0") };

pub static ENCODE_CACHE_IDS: PgLwLock<EncodeCacheIds> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_ids") };

/// Cache statistics: hits counter.
pub static CACHE_HITS: PgAtomic<AtomicU64> = unsafe { PgAtomic::new(c"pg_ripple_cache_hits") };

/// Cache statistics: misses counter.
pub static CACHE_MISSES: PgAtomic<AtomicU64> = unsafe { PgAtomic::new(c"pg_ripple_cache_misses") };

/// Cache statistics: evictions counter.
pub static CACHE_EVICTIONS: PgAtomic<AtomicU64> =
    unsafe { PgAtomic::new(c"pg_ripple_cache_evictions") };

// ─── Initialisation guard ────────────────────────────────────────────────────

/// Set to `true` after `init()` is called (i.e., when loaded via
/// `shared_preload_libraries`).  When false, all shmem operations are no-ops.
pub static SHMEM_READY: AtomicBool = AtomicBool::new(false);

// ─── Public API ────────────────────────────────────────────────────────────────

/// Initialise all shared memory objects.
///
/// Must be called from `_PG_init` **only** when running in postmaster context
/// (i.e. `shared_preload_libraries` is set).  Calling this from a regular
/// backend context (`CREATE EXTENSION`) is not supported.
pub fn init() {
    // SAFETY: called from _PG_init in postmaster context only.
    pg_shmem_init!(LAYOUT_VERSION = AtomicU32::new(SHMEM_MAGIC));
    pg_shmem_init!(MERGE_WORKER_PID = AtomicI32::new(0));
    pg_shmem_init!(TOTAL_DELTA_ROWS = AtomicI64::new(0));

    // v0.6.0: Bloom filter.
    pg_shmem_init!(DELTA_BLOOM);

    // v0.22.0: Per-bit reference counting for bloom filter.
    pg_shmem_init!(DELTA_BLOOM_COUNTERS = [0u8; 1024]);

    // v0.22.0: 4-way set-associative encode cache.
    // Initialize: all ways in all sets are empty (hash=0, id=0).
    pg_shmem_init!(ENCODE_CACHE_S0 = [[0u128; 4]; ENCODE_CACHE_SETS]);
    pg_shmem_init!(ENCODE_CACHE_IDS = [[0i64; 4]; ENCODE_CACHE_SETS]);
    pg_shmem_init!(CACHE_HITS = AtomicU64::new(0));
    pg_shmem_init!(CACHE_MISSES = AtomicU64::new(0));
    pg_shmem_init!(CACHE_EVICTIONS = AtomicU64::new(0));

    // Register a FINAL shmem_startup_hook that sets SHMEM_READY = true only
    // AFTER all three PgAtomic startup hooks above have fired and the inner
    // pointers are valid.  This eliminates the window where SHMEM_READY is
    // true but PgAtomic::get() would still panic.
    //
    // The hook chain (newest-first):
    //   shmem_ready_hook → delta_rows_hook → pid_hook → layout_hook → prev
    // Execution order (oldest-first via `prev` call at front of each hook):
    //   layout_hook → pid_hook → delta_rows_hook → SHMEM_READY = true
    unsafe {
        static mut PREV_FINAL_STARTUP: Option<unsafe extern "C-unwind" fn()> = None;
        PREV_FINAL_STARTUP = pg_sys::shmem_startup_hook;
        pg_sys::shmem_startup_hook = Some(shmem_ready_hook);

        #[pg_guard]
        unsafe extern "C-unwind" fn shmem_ready_hook() {
            unsafe {
                if let Some(prev) = PREV_FINAL_STARTUP {
                    prev(); // initialises LAYOUT_VERSION, MERGE_WORKER_PID, TOTAL_DELTA_ROWS
                }
            }
            // All PgAtomics are now initialised; safe to allow access.
            SHMEM_READY.store(true, Ordering::Release);
        }
    }
}

/// Signal the merge worker to wake up and run a merge cycle immediately.
///
/// No-op if shmem is not initialised or the merge worker is not running.
pub fn poke_merge_worker() {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let pid = MERGE_WORKER_PID.get().load(Ordering::Relaxed);
    if pid == 0 {
        return;
    }
    unsafe {
        // SAFETY: pid is a process ID from shared memory; we send SIGHUP to
        // wake the merge worker from its WaitLatch call.  The worker installs
        // a SIGHUP handler that only sets an atomic flag — safe to deliver.
        let _ = libc::kill(pid as libc::pid_t, libc::SIGHUP);
    }
}

/// Record that `n` rows were inserted into delta tables this batch.
/// No-op when shmem is not initialised.
pub fn record_delta_inserts(n: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    TOTAL_DELTA_ROWS.get().fetch_add(n, Ordering::Relaxed);
}

/// Reset the delta row counter to zero after a successful merge.
pub fn reset_delta_count() {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    TOTAL_DELTA_ROWS.get().store(0, Ordering::Relaxed);
}

/// Returns true when there are no unmerged rows in any delta table.
/// Returns `false` (conservative: include delta) when shmem is not initialised.
pub fn delta_is_empty() -> bool {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return false;
    }
    TOTAL_DELTA_ROWS.get().load(Ordering::Relaxed) == 0
}

// ─── Bloom filter API ─────────────────────────────────────────────────────────

/// Compute two bit positions for `pred_id` in the 1024-bit bloom filter.
///
/// Uses two independent multiplicative hash functions so a single predicate
/// sets two bits, halving the false-positive rate compared to a single hash.
fn bloom_bits(pred_id: i64) -> (usize, usize) {
    let h = pred_id as u64;
    let pos1 = h.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 54; // 10 high bits → 0..1023
    let pos2 = h.wrapping_mul(0x6C62_272E_07BB_0142) >> 54;
    (pos1 as usize, pos2 as usize)
}

/// Mark that predicate `pred_id` has rows in its delta table.
///
/// Increments the reference counter for both bloom bit positions.
/// The bits themselves are set immediately (no ref-counting needed for set).
/// No-op when shmem is not initialised.
pub fn set_predicate_delta_bit(pred_id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let (p1, p2) = bloom_bits(pred_id);

    let mut guard_bits = DELTA_BLOOM.exclusive();
    let bits: &mut [u64; 16] = &mut guard_bits;
    bits[p1 >> 6] |= 1u64 << (p1 & 63);
    bits[p2 >> 6] |= 1u64 << (p2 & 63);

    let mut guard_counters = DELTA_BLOOM_COUNTERS.exclusive();
    let counters: &mut [u8; 1024] = &mut guard_counters;
    // Increment both counters, saturating at 255
    counters[p1] = counters[p1].saturating_add(1);
    counters[p2] = counters[p2].saturating_add(1);
}

/// Clear the bloom-filter bits for `pred_id` after a successful merge.
///
/// v0.22.0+: Decrements the reference counters for both bloom bit positions.
/// Only clears the bit when the counter reaches 0, preventing false negatives
/// from hash collisions where different predicates share a bit position.
///
/// No-op when shmem is not initialised.
pub fn clear_predicate_delta_bit(pred_id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let (p1, p2) = bloom_bits(pred_id);

    let mut guard_counters = DELTA_BLOOM_COUNTERS.exclusive();
    let counters: &mut [u8; 1024] = &mut guard_counters;

    // v0.37.0: Use saturating_sub so a counter saturated at 255 (from many
    // hash-colliding predicates) is decremented safely. A saturated counter
    // stays conservatively high — the bit is never cleared until the counter
    // reaches zero, preventing false negatives.
    counters[p1] = counters[p1].saturating_sub(1);
    counters[p2] = counters[p2].saturating_sub(1);

    let mut guard_bits = DELTA_BLOOM.exclusive();
    let bits: &mut [u64; 16] = &mut guard_bits;

    // Only clear the bits when their counters reach 0
    if counters[p1] == 0 {
        bits[p1 >> 6] &= !(1u64 << (p1 & 63));
    }
    if counters[p2] == 0 {
        bits[p2 >> 6] &= !(1u64 << (p2 & 63));
    }
}

/// Returns `false` if the predicate definitely has no delta rows (both bloom
/// bits are clear).  Returns `true` if it *may* have delta rows (one or both
/// bits are set).
///
/// A `false` return allows the query path to skip the delta scan for this
/// predicate.  A `true` return may be a false positive — the delta scan is
/// then performed and may find no rows.
///
/// Returns `true` (conservative: scan delta) when shmem is not initialised.
#[allow(dead_code)]
pub fn predicate_may_have_delta(pred_id: i64) -> bool {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return true;
    }
    let (p1, p2) = bloom_bits(pred_id);
    let guard = DELTA_BLOOM.share();
    let words: &[u64; 16] = &guard;
    let bit1_set = (words[p1 >> 6] >> (p1 & 63)) & 1 == 1;
    let bit2_set = (words[p2 >> 6] >> (p2 & 63)) & 1 == 1;
    bit1_set || bit2_set
}

/// Reset the entire bloom filter (e.g., after a full compact of all predicates).
///
/// No-op when shmem is not initialised.
pub fn reset_bloom_filter() {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let mut guard = DELTA_BLOOM.exclusive();
    *guard = [0u64; 16];
}

// ─── Shared-memory encode cache API ──────────────────────────────────────────

/// Mix the current database OID into a term hash so that entries from different
/// databases never share a cache slot.  Without this, loading data into database
/// A populates the cache with A's dictionary IDs, and a subsequent load into
/// database B would hit those stale IDs — inserting wrong foreign-key values
/// into B's VP tables.
///
/// Only used for the shared-memory cache key; the dictionary table still stores
/// the original `hash128`.
fn db_scoped_hash(hash128: u128) -> u128 {
    let db_oid = u32::from(unsafe { pg_sys::MyDatabaseId }) as u128;
    // Multiplicative mixing spreads the OID bits across the hash so that
    // set selection and way selection both change when the database changes.
    hash128 ^ db_oid.wrapping_mul(0x9E3779B97F4A7C15)
}

/// Compute the set index for the 4-way set-associative cache.
/// Uses the top 10 bits of the scoped hash to select one of 1024 sets.
fn set_index(hash128: u128) -> usize {
    ((hash128 >> 118) as usize) & (ENCODE_CACHE_SETS - 1)
}

/// Extract the age (2-bit field) from a packed cache entry.
#[inline]
fn extract_age(way_hash: u128) -> u8 {
    ((way_hash >> 126) as u8) & 3
}

/// Pack age into the top 2 bits of a hash.
#[inline]
fn pack_age(hash: u128, age: u8) -> u128 {
    (hash & 0x3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF) | ((age as u128) << 126)
}

/// Look up a hash128 in the shared-memory encode cache.
///
/// Returns `Some(id)` on a hit, `None` on a miss or when shmem is not ready.
/// On hit, resets the age field of the matching way to 0 (MRU).
/// On miss, increments age of all other ways in the set.
pub fn encode_cache_lookup(hash128: u128) -> Option<i64> {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return None;
    }
    let scoped = db_scoped_hash(hash128);
    let set_idx = set_index(scoped);

    // Clear top 2 bits from the search key (we'll match on the lower 126 bits)
    let search_hash = scoped & 0x3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF;

    let mut guard_hashes = ENCODE_CACHE_S0.exclusive();
    let mut guard_ids = ENCODE_CACHE_IDS.exclusive();

    let set_hashes = &mut guard_hashes[set_idx];
    let set_ids = &mut guard_ids[set_idx];

    // Search for a matching way
    for (way_idx, way_hash) in set_hashes.iter_mut().enumerate() {
        let stored_hash = *way_hash & 0x3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF;
        if stored_hash == search_hash && set_ids[way_idx] != 0 {
            // Hit: reset age to 0 (MRU)
            *way_hash = pack_age(search_hash, 0);
            CACHE_HITS.get().fetch_add(1, Ordering::Relaxed);
            return Some(set_ids[way_idx]);
        }
    }

    // Miss: increment age of all occupied ways
    for (way_idx, way_hash) in set_hashes.iter_mut().enumerate() {
        if set_ids[way_idx] != 0 {
            let age = extract_age(*way_hash);
            if age < 3 {
                *way_hash = pack_age(*way_hash & 0x3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF, age + 1);
            }
        }
    }
    CACHE_MISSES.get().fetch_add(1, Ordering::Relaxed);
    None
}

/// Insert a (hash128, id) pair into the shared-memory encode cache.
///
/// 4-way set-associative eviction: finds the way with the highest age
/// and overwrites it. If an empty slot exists, uses that instead.
///
/// No-op when shmem is not initialised.
pub fn encode_cache_insert(hash128: u128, id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let scoped = db_scoped_hash(hash128);
    let set_idx = set_index(scoped);

    // Clear top 2 bits (they're reserved for age packing)
    let clean_hash = scoped & 0x3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF;

    let mut guard_hashes = ENCODE_CACHE_S0.exclusive();
    let mut guard_ids = ENCODE_CACHE_IDS.exclusive();

    let set_hashes = &mut guard_hashes[set_idx];
    let set_ids = &mut guard_ids[set_idx];

    // Find an empty slot first
    for (way_idx, &id_val) in set_ids.iter().enumerate() {
        if id_val == 0 {
            set_hashes[way_idx] = clean_hash; // age=0 implicitly
            set_ids[way_idx] = id;
            return;
        }
    }

    // No empty slot; find the way with the highest age and evict it
    let mut victim_idx = 0;
    let mut max_age = extract_age(set_hashes[0]);
    for (way_idx, &hash_val) in set_hashes.iter().enumerate().skip(1) {
        let age = extract_age(hash_val);
        if age > max_age {
            max_age = age;
            victim_idx = way_idx;
        }
    }

    set_hashes[victim_idx] = clean_hash;
    set_ids[victim_idx] = id;

    CACHE_EVICTIONS.get().fetch_add(1, Ordering::Relaxed);
}

/// Return cache statistics as (hits, misses, evictions, utilisation_pct).
///
/// Returns (0, 0, 0, 0.0) when shmem is not initialised.
pub fn get_cache_stats() -> (u64, u64, u64, f64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return (0, 0, 0, 0.0);
    }

    let guard_ids = ENCODE_CACHE_IDS.share();

    let hits = CACHE_HITS.get().load(Ordering::Relaxed);
    let misses = CACHE_MISSES.get().load(Ordering::Relaxed);
    let evictions = CACHE_EVICTIONS.get().load(Ordering::Relaxed);

    // Calculate utilisation: count non-empty ways across all sets
    let mut occupied = 0i64;
    for set in guard_ids.iter() {
        for &id in set.iter() {
            if id != 0 {
                occupied += 1;
            }
        }
    }

    let utilisation = (occupied as f64) / (ENCODE_CACHE_CAPACITY as f64);

    (hits, misses, evictions, utilisation)
}
