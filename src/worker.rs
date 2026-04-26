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

/// Register background merge worker(s) with the postmaster.
///
/// When `pg_ripple.merge_workers > 1`, multiple workers are spawned, each
/// assigned a round-robin subset of predicates by worker index.  `pg_advisory_lock`
/// ensures no two workers race on the same VP table (v0.42.0).
///
/// Called once from `_PG_init` during shared_preload_libraries phase.
pub fn register_merge_workers() {
    let n_workers = crate::MERGE_WORKERS.get().clamp(1, 16) as u32;
    for worker_idx in 0..n_workers {
        // Encode the worker index into the bgworker argument datum so the
        // entry-point function knows which predicate subset to own.
        BackgroundWorkerBuilder::new(&format!("pg_ripple merge worker {worker_idx}"))
            .set_function("pg_ripple_merge_worker_main")
            .set_library("pg_ripple")
            .enable_shmem_access(None)
            .set_argument((worker_idx as i32).into_datum())
            .enable_spi_access()
            .set_start_time(BgWorkerStartTime::RecoveryFinished)
            .set_restart_time(Some(Duration::from_secs(10)))
            .load();
    }
}

/// Legacy single-worker shim kept for backward compatibility.
#[allow(dead_code)]
pub fn register_merge_worker() {
    register_merge_workers();
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
pub extern "C-unwind" fn pg_ripple_merge_worker_main(arg: pg_sys::Datum) {
    // Attach signal handlers: wake on SIGHUP, stop on SIGTERM.
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);

    // Decode worker index from bgworker argument (v0.42.0 parallel merge pool).
    // Worker 0 also handles the merge worker PID for the latch mechanism.
    // SAFETY: arg is the i32 datum we passed in set_argument(); it is valid as long
    // as this is a pgrx-managed background worker entry point.
    let worker_idx: u32 = unsafe { i32::from_datum(arg, false).unwrap_or(0).max(0) as u32 };

    // Record our PID in shared memory so backends can poke our latch.
    // Only worker 0 writes to the shared PID (other workers also process merges
    // but backends always wake worker 0 as the primary latch target).
    if worker_idx == 0 {
        let my_pid = unsafe { pg_sys::MyProcPid };
        crate::shmem::MERGE_WORKER_PID
            .get()
            .store(my_pid, Ordering::Release);
    }

    // Connect to SPI in the target database.
    let db_name = get_worker_database();
    BackgroundWorker::connect_worker_to_spi(Some(&db_name), None);

    pgrx::log!("pg_ripple merge worker {worker_idx} started (database: {db_name})");

    // Main loop: wait for latch or timeout, then run a merge cycle.
    let interval_secs = get_merge_interval();
    let mut consecutive_errors: u32 = 0;
    while BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs))) {
        if BackgroundWorker::sighup_received() {
            // SIGHUP: reload configuration.  The GUC system handles this.
            pgrx::log!(
                "pg_ripple merge worker {worker_idx}: SIGHUP received — configuration reloaded"
            );
        }

        let n_workers = crate::MERGE_WORKERS.get().clamp(1, 16) as u32;

        // Run merge cycle followed by async validation batch.
        let run_result = std::panic::catch_unwind(|| {
            BackgroundWorker::transaction(|| {
                run_merge_cycle_for_worker(worker_idx, n_workers);
            });
            // Only worker 0 runs validation and embedding queue drain.
            if worker_idx == 0 {
                BackgroundWorker::transaction(|| {
                    run_validation_cycle();
                });
            }
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

            // v0.51.0: use wait_latch for correct SIGTERM response during backoff (S1-3).
            // If SIGTERM is received, wait_latch returns false and the outer while loop exits.
            // If SIGHUP triggers an immediate return, consecutive_errors > 5 prevents spin-looping.
            if !BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs))) {
                break;
            }
            continue;
        }

        // Merge succeeded — reset error counter.
        consecutive_errors = 0;
    }

    // Worker is terminating.  Only worker 0 clears the shared PID.
    if worker_idx == 0 {
        crate::shmem::MERGE_WORKER_PID
            .get()
            .store(0, Ordering::Release);
    }

    pgrx::log!("pg_ripple merge worker {worker_idx} stopped");
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

/// RAII guard that holds the Citus merge fence advisory lock.
///
/// The guard emits `pg_ripple.merge_end` NOTIFY and releases the session
/// advisory lock in its `Drop` impl, ensuring cleanup happens even if the
/// merge cycle panics.  This satisfies the L-5.4 spec requirement:
/// "wrap the merge fencing in a Rust struct implementing `Drop` so that
/// `pg_notify('pg_ripple.merge_end', ...)` is emitted even on panic or error".
struct MergeFenceGuard {
    worker_idx: u32,
}

impl Drop for MergeFenceGuard {
    fn drop(&mut self) {
        const FENCE_KEY: i64 = 0x5052_5000_i64;
        let payload = format!(
            "{{\"worker\":{},\"pid\":{}}}",
            self.worker_idx,
            std::process::id()
        );
        // Best-effort: emit merge_end so listeners can resume CDC apply.
        let _ = Spi::run_with_args(
            &format!("SELECT pg_notify('pg_ripple.merge_end', '{payload}')"),
            &[],
        );
        // Release the session-level advisory lock unconditionally.
        let _ = Spi::run_with_args(
            "SELECT pg_advisory_unlock($1)",
            &[pgrx::datum::DatumWithOid::from(FENCE_KEY)],
        );
    }
}


    run_merge_cycle_for_worker(0, 1);
}

/// Run one merge cycle for the given worker in a parallel pool (v0.42.0).
///
/// `worker_idx`: zero-based index of this worker.
/// `n_workers`: total number of workers in the pool.
///
/// Each worker owns predicates where `(pred_id % n_workers) == worker_idx`.
/// Before merging each predicate, the worker acquires `pg_advisory_lock(pred_id)`
/// to prevent races when `n_workers > 1`.
fn run_merge_cycle_for_worker(worker_idx: u32, n_workers: u32) {
    // Check whether any deltas need merging.
    if crate::shmem::delta_is_empty() {
        // Nothing to merge.
        return;
    }

    // v0.58.0: Citus merge fence — when Citus sharding is enabled and a fence
    // timeout is configured, try to acquire an advisory lock before merging.
    // This prevents split-brain during shard rebalancing: pg-trickle holds the
    // same advisory lock (key = 0x5052_5000 = "PRP\0") during apply.
    //
    // We use a MergeFenceGuard RAII guard so that the lock is *always* released
    // and merge_end is *always* emitted — even if the cycle panics or exits
    // early with nothing to merge (fixes session lock leak).
    let fence_timeout_ms = crate::gucs::storage::MERGE_FENCE_TIMEOUT_MS.get();
    let _fence_guard = if fence_timeout_ms > 0 && crate::citus::is_citus_loaded() {
        const FENCE_KEY: i64 = 0x5052_5000_i64; // "PRP\0"
        let locked: bool = Spi::get_one_with_args::<bool>(
            "SELECT pg_try_advisory_lock($1)",
            &[pgrx::datum::DatumWithOid::from(FENCE_KEY)],
        )
        .unwrap_or(None)
        .unwrap_or(false);
        if !locked {
            pgrx::log!(
                "pg_ripple merge worker {worker_idx}: fence lock held by rebalancer, skipping cycle"
            );
            return;
        }
        // Emit merge_start NOTIFY for observability.
        let payload = format!("{{\"worker\":{worker_idx},\"pid\":{}}}", std::process::id());
        let _ = Spi::run_with_args(
            &format!("SELECT pg_notify('pg_ripple.merge_start', '{payload}')"),
            &[],
        );
        // Guard will emit merge_end and release the lock in Drop.
        Some(MergeFenceGuard { worker_idx })
    } else {
        None
    };

    let threshold = get_merge_threshold();

    // Find predicates assigned to this worker (round-robin by pred_id % n_workers).
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("merge worker: predicates scan error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .filter(|&id| {
            // Round-robin partition: this worker handles predicates where
            // (id % n_workers) == worker_idx.  When n_workers == 1 all predicates
            // are assigned to worker 0.
            if n_workers <= 1 {
                true
            } else {
                // Use abs to handle negative IDs correctly.
                let bucket = (id.unsigned_abs() % (n_workers as u64)) as u32;
                bucket == worker_idx
            }
        })
        .collect()
    });

    // Also check for work-stealing: if any predicate above threshold is not
    // claimed by another worker (advisory lock available), process it too.
    let all_pred_ids: Vec<i64> = if n_workers > 1 {
        Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("merge worker: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .filter(|&id| {
                let bucket = (id.unsigned_abs() % (n_workers as u64)) as u32;
                bucket != worker_idx // only "foreign" predicates
            })
            .collect()
        })
    } else {
        Vec::new()
    };

    let mut merged_any = false;

    // Process this worker's assigned predicates.
    for p_id in pred_ids {
        let delta_rows: i64 = Spi::get_one_with_args::<i64>(
            &format!("SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_delta"),
            &[],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        if delta_rows >= threshold {
            // Acquire advisory lock to prevent races with other workers.
            if n_workers > 1 {
                let locked: bool = Spi::get_one_with_args::<bool>(
                    "SELECT pg_try_advisory_lock($1)",
                    &[pgrx::datum::DatumWithOid::from(p_id)],
                )
                .unwrap_or(None)
                .unwrap_or(false);
                if !locked {
                    continue; // Another worker is processing this predicate.
                }
            }
            crate::storage::merge::merge_predicate(p_id);
            // L-3.3 (v0.56.0): When inference_mode = 'incremental_rdfs', trigger
            // targeted RDFS closure rules for subClassOf / subPropertyOf predicates.
            if crate::INFERENCE_MODE
                .get()
                .as_ref()
                .and_then(|c| c.to_str().ok())
                .unwrap_or("")
                == "incremental_rdfs"
            {
                crate::datalog::run_incremental_rdfs_for_predicate(p_id);
            }
            if n_workers > 1 {
                let _ = Spi::run_with_args(
                    "SELECT pg_advisory_unlock($1)",
                    &[pgrx::datum::DatumWithOid::from(p_id)],
                );
            }
            merged_any = true;
        }
    }

    // Work-stealing: check foreign predicates above threshold with no owner.
    if n_workers > 1 {
        for p_id in all_pred_ids {
            let delta_rows: i64 = Spi::get_one_with_args::<i64>(
                &format!("SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_delta"),
                &[],
            )
            .unwrap_or(None)
            .unwrap_or(0);

            if delta_rows >= threshold {
                // Try to steal — only proceed if we can acquire the lock.
                let locked: bool = Spi::get_one_with_args::<bool>(
                    "SELECT pg_try_advisory_lock($1)",
                    &[pgrx::datum::DatumWithOid::from(p_id)],
                )
                .unwrap_or(None)
                .unwrap_or(false);
                if locked {
                    crate::storage::merge::merge_predicate(p_id);
                    let _ = Spi::run_with_args(
                        "SELECT pg_advisory_unlock($1)",
                        &[pgrx::datum::DatumWithOid::from(p_id)],
                    );
                    merged_any = true;
                }
            }
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

        // merge_end NOTIFY and fence lock release are handled automatically
        // by MergeFenceGuard::drop() when _fence_guard goes out of scope.
    }

    // Only worker 0 runs housekeeping tasks to avoid duplicate work.
    if worker_idx == 0 {
        // Evict expired federation cache entries on each polling cycle (v0.19.0).
        crate::sparql::federation::evict_expired_cache();

        // v0.28.0: drain embedding queue if auto_embed is on.
        drain_embedding_queue();
    }

    // A-3: clear backend-local LRU cache at end of merge transaction to prevent
    // stale IDs from being used if dictionary rows are rewritten by a future migration.
    crate::dictionary::clear_caches();
}

// ─── v0.28.0: Embedding queue drain ──────────────────────────────────────────

/// Drain the embedding queue: dequeue up to `pg_ripple.embedding_batch_size`
/// entities and generate embeddings for them via the configured API.
///
/// Only runs when `pg_ripple.auto_embed = true` AND an embedding API URL is
/// configured.  Silently skips when either condition is not met.
fn drain_embedding_queue() {
    if !crate::AUTO_EMBED.get() {
        return;
    }

    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    if api_url.is_empty() {
        return; // API not configured — silently skip.
    }

    let batch_size = crate::EMBEDDING_BATCH_SIZE.get().clamp(1, 10_000);

    // Dequeue entity IDs from the queue.
    let queued: Vec<i64> = pgrx::Spi::connect(|c| {
        c.select(
            &format!(
                "DELETE FROM _pg_ripple.embedding_queue \
                 WHERE entity_id IN ( \
                     SELECT entity_id FROM _pg_ripple.embedding_queue \
                     ORDER BY enqueued_at \
                     LIMIT {batch_size} \
                 ) \
                 RETURNING entity_id"
            ),
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("drain_embedding_queue: SPI error: {e}"))
        .map(|row| row.get::<i64>(1).ok().flatten().unwrap_or(0))
        .filter(|&id| id != 0)
        .collect()
    });

    if queued.is_empty() {
        return;
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_tag = {
        let m = crate::EMBEDDING_MODEL.get();
        m.as_ref()
            .and_then(|s| s.to_str().ok())
            .filter(|s| !s.is_empty())
            .unwrap_or("text-embedding-3-small")
            .to_owned()
    };

    let dims = crate::EMBEDDING_DIMENSIONS.get();
    let mut embedded = 0u32;

    for entity_id in &queued {
        // Resolve IRI from dictionary.
        let iri = match crate::dictionary::decode(*entity_id) {
            Some(v) => v,
            None => continue,
        };

        // Use graph context if enabled.
        let text_to_embed = if crate::USE_GRAPH_CONTEXT.get() {
            crate::sparql::embedding::contextualize_entity(&iri, 1, 20)
        } else {
            // Use local name as fallback.
            iri.rfind(['#', '/'])
                .map(|pos| iri[pos + 1..].to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| iri.clone())
        };

        let embedding = match crate::sparql::embedding::call_embedding_api_pub(
            &text_to_embed,
            &model_tag,
            api_url,
            api_key,
        ) {
            Ok(v) => v,
            Err(e) => {
                pgrx::log!("pg_ripple embed worker: API error for entity {entity_id}: {e}");
                continue;
            }
        };

        if embedding.len() != dims as usize {
            pgrx::log!(
                "pg_ripple embed worker: dimension mismatch for entity {entity_id}: \
                 expected {dims}, got {}",
                embedding.len()
            );
            continue;
        }

        let array_lit = format!(
            "ARRAY[{}]::float8[]",
            embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let sql = format!(
            "INSERT INTO _pg_ripple.embeddings (entity_id, model, embedding, updated_at) \
             VALUES ({entity_id}, $1, ({array_lit})::vector, now()) \
             ON CONFLICT (entity_id, model) \
             DO UPDATE SET embedding = EXCLUDED.embedding, updated_at = now()"
        );

        if pgrx::Spi::run_with_args(&sql, &[pgrx::datum::DatumWithOid::from(model_tag.as_str())])
            .is_ok()
        {
            embedded += 1;
        }
    }

    if embedded > 0 {
        pgrx::log!(
            "pg_ripple embed worker: embedded {embedded}/{} entities",
            queued.len()
        );
    }
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
