//! Shared memory for pg_ripple v0.6.0 (HTAP Architecture).
//!
//! # Shared objects
//!
//! | Name | Type | Purpose |
//! |------|------|---------|
//! | `MERGE_WORKER_PID` | `PgAtomic<AtomicI32>` | PID of the merge background worker |
//! | `LAYOUT_VERSION` | `PgAtomic<AtomicU32>` | Slot-versioning magic for safe upgrades |
//! | `TOTAL_DELTA_ROWS` | `PgAtomic<AtomicI64>` | Running count of unmerged delta rows |
//!
//! These objects are only available when the extension is loaded via
//! `shared_preload_libraries`.  When loaded via `CREATE EXTENSION` (without
//! shared_preload_libraries), all shmem operations are no-ops — `SHMEM_READY`
//! ensures callers never attempt to access an uninitialised `PgAtomic`.

use pgrx::prelude::*;
use pgrx::{PgAtomic, pg_shmem_init};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, Ordering};

// ─── Layout version guard ─────────────────────────────────────────────────────

/// Magic constant for shared-memory slot versioning: `"pgri"` as u32.
const SHMEM_MAGIC: u32 = 0x70677269;

/// Shared layout version.  Initialised to `SHMEM_MAGIC` on first startup.
pub static LAYOUT_VERSION: PgAtomic<AtomicU32> =
    unsafe { PgAtomic::new(c"pg_ripple_layout_version") };

// ─── Merge worker coordination ────────────────────────────────────────────────

/// PID of the running merge background worker (0 when not running).
pub static MERGE_WORKER_PID: PgAtomic<AtomicI32> =
    unsafe { PgAtomic::new(c"pg_ripple_merge_pid") };

// ─── Delta row tracker (bloom-filter substitute) ──────────────────────────────

/// Total number of unmerged rows across all VP delta tables.
pub static TOTAL_DELTA_ROWS: PgAtomic<AtomicI64> =
    unsafe { PgAtomic::new(c"pg_ripple_delta_rows") };

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

