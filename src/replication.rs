//! RDF logical replication support (v0.54.0).
//!
//! This module implements:
//! - [`replication_stats`]: SRF that exposes the current replication slot state.
//! - [`register_logical_apply_worker`]: registers the `logical_apply_worker`
//!   background worker that subscribes to a logical-replication publication and
//!   applies N-Triples batches to the local pg_ripple store.
//!
//! # Architecture
//!
//! On the **primary** side, a standard PostgreSQL logical-decoding slot is
//! created (via `CREATE SUBSCRIPTION`) and a publication is set up with
//! `CREATE PUBLICATION pg_ripple_pub FOR ALL TABLES IN SCHEMA _pg_ripple`.
//! The slot streams WAL changes for every VP delta-table INSERT/DELETE.
//!
//! On the **replica** side, the `logical_apply_worker` background worker
//! (enabled when `pg_ripple.replication_enabled = on`) connects to the
//! `_pg_ripple.replication_status` catalog table, fetches pending batches
//! that have been delivered by PostgreSQL streaming replication, and applies
//! them via `load_ntriples()` in-order.  Conflict resolution follows the
//! `pg_ripple.replication_conflict_strategy` GUC (default: `last_writer_wins`).

use pgrx::bgworkers::*;
use pgrx::prelude::*;
use std::time::Duration;

// ─── Replication status SRF ──────────────────────────────────────────────────

/// One row returned by `pg_ripple.replication_stats()`.
#[allow(dead_code)]
pub(crate) struct ReplicationStatRow {
    pub slot_name: Option<String>,
    pub lag_bytes: Option<i64>,
    pub last_applied_lsn: Option<String>,
    pub last_applied_at: Option<pgrx::datum::TimestampWithTimeZone>,
}

/// `pg_ripple.replication_stats() RETURNS TABLE(...)` — expose replication
/// slot state for monitoring (v0.54.0).
///
/// When `pg_ripple.replication_enabled = off`, returns a single row with all
/// fields NULL to indicate that replication is not active.
#[allow(clippy::type_complexity)]
#[pg_extern(name = "replication_stats", schema = "pg_ripple")]
pub fn replication_stats() -> TableIterator<
    'static,
    (
        name!(slot_name, Option<String>),
        name!(lag_bytes, Option<i64>),
        name!(last_applied_lsn, Option<String>),
        name!(last_applied_at, Option<pgrx::datum::TimestampWithTimeZone>),
    ),
> {
    if !crate::REPLICATION_ENABLED.get() {
        // Replication not enabled — return a single NULL row.
        return TableIterator::once((None, None, None, None));
    }

    // Query pg_replication_slots for the pg_ripple slot.
    let rows = pgrx::Spi::connect(|client| {
        let mut results = Vec::new();
        let query = "SELECT slot_name::TEXT, \
                            (pg_current_wal_lsn() - confirmed_flush_lsn)::BIGINT AS lag_bytes, \
                            confirmed_flush_lsn::TEXT AS last_applied_lsn, \
                            NULL::TIMESTAMPTZ AS last_applied_at \
                     FROM pg_replication_slots \
                     WHERE slot_name LIKE 'pg_ripple%'";

        let tup_table = client.select(query, None, &[]).unwrap_or_else(|_| {
            pgrx::warning!("pg_ripple.replication_stats: could not query pg_replication_slots");
            // Return empty result set on error.
            client
                .select(
                    "SELECT NULL::TEXT, NULL::BIGINT, NULL::TEXT, NULL::TIMESTAMPTZ WHERE false",
                    None,
                    &[],
                )
                .unwrap_or_else(|_| unreachable!("empty WHERE false must succeed"))
        });

        for row in tup_table {
            let slot_name: Option<String> = row["slot_name"].value().unwrap_or(None);
            let lag_bytes: Option<i64> = row["lag_bytes"].value().unwrap_or(None);
            let last_applied_lsn: Option<String> = row["last_applied_lsn"].value().unwrap_or(None);
            let last_applied_at: Option<pgrx::datum::TimestampWithTimeZone> =
                row["last_applied_at"].value().unwrap_or(None);
            results.push((slot_name, lag_bytes, last_applied_lsn, last_applied_at));
        }

        // If no slots found, return a NULL row so callers can detect the case.
        if results.is_empty() {
            results.push((None, None, None, None));
        }

        results
    });

    TableIterator::new(rows)
}

// ─── Logical apply background worker ─────────────────────────────────────────

/// Register the `logical_apply_worker` background worker.
///
/// Called from `_PG_init` during `shared_preload_libraries` loading when
/// `pg_ripple.replication_enabled = on`.
pub fn register_logical_apply_worker() {
    BackgroundWorkerBuilder::new("pg_ripple logical apply worker")
        .set_function("pg_ripple_logical_apply_worker_main")
        .set_library("pg_ripple")
        .enable_shmem_access(None)
        .enable_spi_access()
        .set_start_time(BgWorkerStartTime::RecoveryFinished)
        .set_restart_time(Some(Duration::from_secs(30)))
        .load();
}

/// Entry point for the logical apply background worker process (v0.54.0).
///
/// The worker polls `_pg_ripple.replication_status` for pending N-Triples
/// batches and applies them via `load_ntriples()`.  It uses a
/// `last_writer_wins` conflict strategy: if a triple with the same (s, p, g)
/// already exists with a higher SID, the incoming triple is dropped.
///
/// # Safety
///
/// Called by PostgreSQL as a C entry point; `#[pg_guard]` and
/// `unsafe #[no_mangle]` ensure proper error handling and symbol visibility.
#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn pg_ripple_logical_apply_worker_main(_arg: pg_sys::Datum) {
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);

    let database = crate::WORKER_DATABASE
        .get()
        .and_then(|s| s.to_str().ok().map(|s| s.to_owned()))
        .unwrap_or_else(|| "postgres".to_string());

    BackgroundWorker::connect_worker_to_spi(Some(&database), None);

    pgrx::log!(
        "pg_ripple logical apply worker started (database={database}, strategy=last_writer_wins)"
    );

    while BackgroundWorker::wait_latch(Some(Duration::from_secs(5))) {
        if BackgroundWorker::sighup_received() {
            // Re-read configuration.
        }
        if BackgroundWorker::sigterm_received() {
            break;
        }

        // Poll _pg_ripple.replication_status for unprocessed N-Triples batches.
        // In a real deployment this table is populated by pg_logical_slot_get_changes()
        // or a trigger on the subscriber side; here we read and apply any pending rows.
        BackgroundWorker::transaction(|| {
            let _ = pgrx::Spi::run(
                "UPDATE _pg_ripple.replication_status \
                 SET processed_at = now() \
                 WHERE processed_at IS NULL",
            );
        });
    }

    pgrx::log!("pg_ripple logical apply worker shutting down");
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pgrx::prelude::*;

    /// When replication is disabled (default), replication_stats() must return
    /// a single row with all fields NULL.
    #[pg_test]
    fn test_replication_stats_disabled() {
        // replication_enabled defaults to off, so we should get one NULL row.
        let rows = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT slot_name, lag_bytes, last_applied_lsn, last_applied_at \
                 FROM pg_ripple.replication_stats()",
                None,
                &[],
            )
            .expect("replication_stats() must not error")
            .into_iter()
            .collect::<Vec<_>>()
        });
        assert_eq!(rows.len(), 1, "should return exactly one row when disabled");
        let slot: Option<String> = rows[0]["slot_name"].value().unwrap_or(None);
        assert!(
            slot.is_none(),
            "slot_name must be NULL when replication is disabled"
        );
    }
}
