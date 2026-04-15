//! Shared memory for pg_ripple v0.6.0 (HTAP Architecture).
//!
//! # Shared objects
//!
//! | Name | Type | Purpose |
//! |------|------|---------|
//! | `MERGE_WORKER_PID` | `PgAtomic<AtomicI32>` | PID of the merge background worker |
//! | `LAYOUT_VERSION` | `PgAtomic<AtomicU32>` | Slot-versioning magic for safe upgrades |
//! | `TOTAL_DELTA_ROWS` | `PgAtomic<AtomicI64>` | Running count of unmerged delta rows |
//! | `DELTA_BLOOM` | `PgLwLock<[u64; 16]>` | 1024-bit bloom filter: which predicates have delta rows |
//! | `ENCODE_CACHE_S0..S3` | `PgLwLock<EncodeCacheShard>` | Sharded shared-memory encode cache |
//! | `CACHE_USED_SLOTS` | `PgAtomic<AtomicI64>` | Count of occupied encode-cache slots |
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
//! ## Encode cache (shared-memory dictionary)
//!
//! Four 1024-slot shards keyed on `hash128 >> 62` (top 2 bits).  Each slot
//! stores `([hash_lo: u64, hash_hi: u64], id: i64)`.  An empty slot has
//! `[0, 0]` (XXH3-128 of any real term is astronomically unlikely to be zero).
//!
//! Lookups use a **shared** LW lock (many readers); inserts use **exclusive**.
//! The hit rate is the primary performance metric; eviction is write-through
//! (new entries overwrite old entries in the same slot — no explicit eviction).
//!
//! These objects are only available when the extension is loaded via
//! `shared_preload_libraries`.  When loaded via `CREATE EXTENSION` (without
//! shared_preload_libraries), all shmem operations are no-ops — `SHMEM_READY`
//! ensures callers never attempt to access an uninitialised object.

use pgrx::prelude::*;
use pgrx::{PgAtomic, PgLwLock, pg_shmem_init};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, Ordering};

// ─── Encode cache types ───────────────────────────────────────────────────────

/// One slot in the shared-memory encode cache.
/// Layout: `([hash_lo, hash_hi], id)`.  Empty when hash == [0, 0].
pub type EncodeSlot = ([u64; 2], i64);

/// One shard: 1024 direct-mapped slots.
pub type EncodeCacheShard = [EncodeSlot; ENCODE_SHARD_SLOTS];

/// Number of slots in each encode-cache shard.
pub const ENCODE_SHARD_SLOTS: usize = 1024;

/// Number of encode-cache shards (must be a power of two ≤ 4).
pub const ENCODE_SHARDS: usize = 4;

/// Total encode-cache capacity across all shards.
pub const ENCODE_CACHE_CAPACITY: usize = ENCODE_SHARD_SLOTS * ENCODE_SHARDS;

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

// ─── Bloom filter (per-predicate delta presence) ─────────────────────────────

/// 1024-bit Bloom filter: which predicates have rows in their delta tables.
/// Indexed by two multiplicative hashes of the predicate ID.
pub static DELTA_BLOOM: PgLwLock<[u64; 16]> =
    unsafe { PgLwLock::new(c"pg_ripple_delta_bloom") };

// ─── Shared-memory encode cache (4 shards × 1024 slots) ─────────────────────

pub static ENCODE_CACHE_S0: PgLwLock<EncodeCacheShard> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_s0") };
pub static ENCODE_CACHE_S1: PgLwLock<EncodeCacheShard> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_s1") };
pub static ENCODE_CACHE_S2: PgLwLock<EncodeCacheShard> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_s2") };
pub static ENCODE_CACHE_S3: PgLwLock<EncodeCacheShard> =
    unsafe { PgLwLock::new(c"pg_ripple_ec_s3") };

/// Running count of occupied encode-cache slots (for budget utilization).
pub static CACHE_USED_SLOTS: PgAtomic<AtomicI64> =
    unsafe { PgAtomic::new(c"pg_ripple_cache_used") };

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

    // v0.6.0: Bloom filter + encode cache shards.
    pg_shmem_init!(DELTA_BLOOM);
    // Arrays of 1024 elements exceed Rust's Default-for-array limit (≤32) in
    // the current compiler, so we provide explicit zero initialisers.
    pg_shmem_init!(ENCODE_CACHE_S0 = [([0u64, 0u64], 0i64); ENCODE_SHARD_SLOTS]);
    pg_shmem_init!(ENCODE_CACHE_S1 = [([0u64, 0u64], 0i64); ENCODE_SHARD_SLOTS]);
    pg_shmem_init!(ENCODE_CACHE_S2 = [([0u64, 0u64], 0i64); ENCODE_SHARD_SLOTS]);
    pg_shmem_init!(ENCODE_CACHE_S3 = [([0u64, 0u64], 0i64); ENCODE_SHARD_SLOTS]);
    pg_shmem_init!(CACHE_USED_SLOTS = AtomicI64::new(0));

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
/// No-op when shmem is not initialised.
pub fn set_predicate_delta_bit(pred_id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let (p1, p2) = bloom_bits(pred_id);
    let mut guard = DELTA_BLOOM.exclusive();
    let words: &mut [u64; 16] = &mut *guard;
    words[p1 >> 6] |= 1u64 << (p1 & 63);
    words[p2 >> 6] |= 1u64 << (p2 & 63);
}

/// Clear the bloom-filter bits for `pred_id` after a successful merge.
///
/// Clearing is conservative: we only clear bits that are exclusively owned
/// by this predicate (i.e., neither bit is shared with a different predicate
/// that still has delta rows).  Since we always clear both bits atomically
/// under the exclusive lock, at worst we introduce a false negative for a
/// different predicate mapped to the same bit — the query path handles that
/// safely by scanning delta (it never skips when uncertain).
///
/// No-op when shmem is not initialised.
pub fn clear_predicate_delta_bit(pred_id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let (p1, p2) = bloom_bits(pred_id);
    let mut guard = DELTA_BLOOM.exclusive();
    let words: &mut [u64; 16] = &mut *guard;
    words[p1 >> 6] &= !(1u64 << (p1 & 63));
    words[p2 >> 6] &= !(1u64 << (p2 & 63));
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
pub fn predicate_may_have_delta(pred_id: i64) -> bool {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return true;
    }
    let (p1, p2) = bloom_bits(pred_id);
    let guard = DELTA_BLOOM.share();
    let words: &[u64; 16] = &*guard;
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

/// Select the shard for `hash128` based on the top 2 bits.
fn shard_for(hash128: u128) -> usize {
    ((hash128 >> 126) as usize) & 3
}

/// Compute the slot index within a shard.
fn slot_for(hash128: u128) -> usize {
    ((hash128 as u64) as usize) & (ENCODE_SHARD_SLOTS - 1)
}

/// Split a u128 hash into (lo: u64, hi: u64) for slot storage.
fn split_hash(hash128: u128) -> [u64; 2] {
    [hash128 as u64, (hash128 >> 64) as u64]
}

/// Macro to dispatch to the correct shard static.
macro_rules! with_shard_shared {
    ($shard:expr, $body:expr) => {
        match $shard {
            0 => { let guard = ENCODE_CACHE_S0.share(); $body(&*guard) }
            1 => { let guard = ENCODE_CACHE_S1.share(); $body(&*guard) }
            2 => { let guard = ENCODE_CACHE_S2.share(); $body(&*guard) }
            _ => { let guard = ENCODE_CACHE_S3.share(); $body(&*guard) }
        }
    };
}

macro_rules! with_shard_exclusive {
    ($shard:expr, $body:expr) => {
        match $shard {
            0 => { let mut guard = ENCODE_CACHE_S0.exclusive(); $body(&mut *guard) }
            1 => { let mut guard = ENCODE_CACHE_S1.exclusive(); $body(&mut *guard) }
            2 => { let mut guard = ENCODE_CACHE_S2.exclusive(); $body(&mut *guard) }
            _ => { let mut guard = ENCODE_CACHE_S3.exclusive(); $body(&mut *guard) }
        }
    };
}

/// Look up a hash128 in the shared-memory encode cache.
///
/// Returns `Some(id)` on a hit, `None` on a miss or when shmem is not ready.
pub fn encode_cache_lookup(hash128: u128) -> Option<i64> {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return None;
    }
    let shard = shard_for(hash128);
    let slot = slot_for(hash128);
    let expected = split_hash(hash128);
    with_shard_shared!(shard, |cache: &EncodeCacheShard| {
        let (stored_hash, stored_id) = cache[slot];
        if stored_hash == expected && stored_id != 0 {
            Some(stored_id)
        } else {
            None
        }
    })
}

/// Insert a (hash128, id) pair into the shared-memory encode cache.
///
/// Uses direct-mapped eviction: the new entry unconditionally overwrites the
/// existing occupant of the slot (no LRU bookkeeping needed).
///
/// No-op when shmem is not initialised.
pub fn encode_cache_insert(hash128: u128, id: i64) {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return;
    }
    let shard = shard_for(hash128);
    let slot = slot_for(hash128);
    let hash_parts = split_hash(hash128);
    with_shard_exclusive!(shard, |cache: &mut EncodeCacheShard| {
        let was_empty = cache[slot].0 == [0u64; 2];
        cache[slot] = (hash_parts, id);
        if was_empty {
            CACHE_USED_SLOTS.get().fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// Return the current encode-cache utilization as a percentage (0–100).
///
/// Returns 0 when shmem is not initialised.
pub fn cache_utilization_pct() -> u8 {
    if !SHMEM_READY.load(Ordering::Acquire) {
        return 0;
    }
    let used = CACHE_USED_SLOTS.get().load(Ordering::Relaxed);
    let total = ENCODE_CACHE_CAPACITY as i64;
    ((used * 100) / total.max(1)).min(100) as u8
}
