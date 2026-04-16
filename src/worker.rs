//! Background merge worker for pg_ripple v0.6.0 (HTAP Architecture).
//!
//! The worker periodically merges VP delta tables into the read-optimised main
//! partition.  It is registered in `_PG_init` via
//! [`register_merge_worker`] and started automatically by the postmaster.
//!
//! # Lifecycle
//!
//! 1. Registered via [`BackgroundWorkerBuilder`] with `load_at_startup`.
//! 2. The postmaster starts `pg_ripple_merge_worker_main` in a subprocess.
//! 3. The worker connects to SPI with `pg_ripple.worker_database` as the target.
//! 4. It writes its PID into `MERGE_WORKER_PID` shared memory.
//! 5. Loop:
//!    - Wait up to `pg_ripple.merge_interval_secs` on its latch.
//!    - On wake: run a transaction that calls [`crate::storage::merge::merge_all`].
//!    - After merge: rebuild subject_patterns and object_patterns.
//!    - Promote any rare predicates that crossed the threshold.
//! 6. On SIGTERM / postmaster death: exit cleanly.

use pgrx::bgworkers::*;
use pgrx::prelude::*;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Register the background merge worker with the postmaster.
///
/// Called once from `_PG_init` when PostgreSQL loads the extension library
/// at startup (requires `shared_preload_libraries = 'pg_ripple'`).
pub fn register_merge_worker() {
    BackgroundWorkerBuilder::new("pg_ripple merge worker")
        .set_function("pg_ripple_merge_worker_main")
        .set_library("pg_ripple")
        .enable_shmem_access(None)
        .enable_spi_access()
        .set_start_time(BgWorkerStartTime::RecoveryFinished)
        .set_restart_time(Some(Duration::from_secs(10)))
        .load();
}

/// Entry point for the background merge worker process.
///
/// # Safety
///
/// This function is called by PostgreSQL as a C entry point via the background
/// worker mechanism.  The `#[pg_guard]` and `unsafe #[no_mangle]` attributes ensure
/// proper PostgreSQL error handling and symbol visibility.
#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn pg_ripple_merge_worker_main(_arg: pg_sys::Datum) {
    // Attach signal handlers: wake on SIGHUP, stop on SIGTERM.
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);

    // Record our PID in shared memory so backends can poke our latch.
    let my_pid = unsafe { pg_sys::MyProcPid };
    crate::shmem::MERGE_WORKER_PID
        .get()
        .store(my_pid, Ordering::Release);

    // Connect to SPI in the target database.
    let db_name = get_worker_database();
    BackgroundWorker::connect_worker_to_spi(Some(&db_name), None);

    pgrx::log!("pg_ripple merge worker started (database: {db_name})");

    // Main loop: wait for latch or timeout, then run a merge cycle.
    let interval_secs = get_merge_interval();
    let mut consecutive_errors: u32 = 0;
    while BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs))) {
        if BackgroundWorker::sighup_received() {
            // SIGHUP: reload configuration.  The GUC system handles this.
            pgrx::log!("pg_ripple merge worker: SIGHUP received — configuration reloaded");
        }

        // Run merge cycle followed by async validation batch.
        let run_result = std::panic::catch_unwind(|| {
            BackgroundWorker::transaction(|| {
                run_merge_cycle();
            });
            BackgroundWorker::transaction(|| {
                run_validation_cycle();
            });
        });

        if let Err(e) = run_result {
            consecutive_errors += 1;

            // SAFETY: FlushErrorState resets PostgreSQL's ERRORDATA stack after
            // a caught panic, preventing ERRORDATA_STACK_SIZE overflow on
            // subsequent iterations.
            unsafe {
                pg_sys::FlushErrorState();
            }

            pgrx::log!(
                "pg_ripple merge worker: merge cycle panicked ({consecutive_errors}): {e:?}"
            );

            if consecutive_errors >= 5 {
                pgrx::log!(
                    "pg_ripple merge worker: {consecutive_errors} consecutive errors, \
                     backing off to full interval"
                );
            }

            // Sleep explicitly before retrying.  We cannot rely on wait_latch
            // because pending SIGHUP signals (sent by poke_merge_worker during
            // bulk loads) cause it to return immediately, creating a rapid
            // panic loop.
            std::thread::sleep(Duration::from_secs(interval_secs));
            continue;
        }

        // Merge succeeded — reset error counter.
        consecutive_errors = 0;
    }

    // Worker is terminating.  Clear our PID from shared memory.
    crate::shmem::MERGE_WORKER_PID
        .get()
        .store(0, Ordering::Release);

    pgrx::log!("pg_ripple merge worker stopped");
}

/// Run one async validation batch inside an open SPI transaction.
///
/// Only runs when `pg_ripple.shacl_mode = 'async'`.  Processes up to 1000
/// queued triples per cycle.
fn run_validation_cycle() {
    let shacl_mode = crate::SHACL_MODE.get();
    let mode_str = shacl_mode
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("off");
    if mode_str != "async" {
        return;
    }

    let processed = crate::shacl::process_validation_batch(1000);
    if processed > 0 {
        pgrx::log!("pg_ripple merge worker: processed {processed} async validation item(s)");
    }
}

/// Run one merge cycle inside an open SPI transaction.
fn run_merge_cycle() {
    // Check whether any deltas need merging.
    if crate::shmem::delta_is_empty() {
        // Nothing to merge.
        return;
    }

    let threshold = get_merge_threshold();

    // Find predicates whose delta table has >= threshold rows.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("merge worker: predicates scan error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    let mut merged_any = false;

    for p_id in pred_ids {
        let delta_rows: i64 = Spi::get_one_with_args::<i64>(
            &format!("SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_delta"),
            &[],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        if delta_rows >= threshold {
            crate::storage::merge::merge_predicate(p_id);
            merged_any = true;
        }
    }

    if merged_any {
        // Rebuild pattern tables after merge.
        crate::storage::merge::rebuild_subject_patterns();
        crate::storage::merge::rebuild_object_patterns();

        // Promote any rare predicates that crossed the threshold.
        crate::storage::promote_rare_predicates();

        // Reset shmem delta counter.
        crate::shmem::reset_delta_count();

        pgrx::log!("pg_ripple merge worker: merge cycle complete");
    }

    // Evict expired federation cache entries on each polling cycle (v0.19.0).
    crate::sparql::federation::evict_expired_cache();
}

// ─── GUC helpers ─────────────────────────────────────────────────────────────

fn get_worker_database() -> String {
    crate::WORKER_DATABASE
        .get()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "postgres".to_string())
}

fn get_merge_interval() -> u64 {
    crate::MERGE_INTERVAL_SECS.get().max(1) as u64
}

fn get_merge_threshold() -> i64 {
    crate::MERGE_THRESHOLD.get() as i64
}
