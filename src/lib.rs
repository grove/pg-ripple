//! pg_ripple — High-performance RDF triple store for PostgreSQL 18.
//! v0.87.0: Uncertain Knowledge Engine
//!
//! # Architecture
//!
//! Every IRI, blank node, and literal is encoded to `i64` via XXH3-128 hash
//! (see `src/dictionary/`) before being stored in vertical-partition (VP)
//! tables in the `_pg_ripple` schema (see `src/storage/`).  SPARQL queries
//! are parsed with `spargebra`, compiled to SQL, and executed via SPI
//! (see `src/sparql/`).
//!
//! In v0.6.0 (HTAP Architecture), VP tables are split into delta + main
//! partitions for non-blocking concurrent reads and writes.

// v0.37.0: Deny hard panics in library code; test modules exempt via #[allow].
#![cfg_attr(not(any(test, feature = "pg_test")), deny(clippy::unwrap_used))]
#![cfg_attr(not(any(test, feature = "pg_test")), deny(clippy::expect_used))]
// v0.46.0: Warn on missing doc comments for public items (rustdoc lint gate).
#![warn(missing_docs)]

use pgrx::prelude::*;

mod bulk_load;
mod cdc;
mod cdc_bridge_api;
mod data_ops;
mod datalog;
mod datalog_api;
mod dict_api;
mod dictionary;
mod error;
mod export;
mod export_api;
mod federation_registry;
mod framing;
mod fts;
mod graphrag_admin;
mod gucs;
mod kge;
mod llm;
mod maintenance_api;
mod r2rml;
mod replication;
mod schema;
mod security_api;
mod shacl;
mod shmem;
mod sparql;
mod sparql_api;
mod stats_admin;
mod storage;
pub(crate) mod telemetry;
mod tenant;
mod views;
mod views_api;
mod worker;
// v0.58.0 modules
mod citus;
mod prov;
mod temporal;
// v0.62.0 modules
mod flight;
// v0.63.0 modules
mod construct_rules;
mod construct_rules_api;
// v0.64.0 modules
mod feature_status;
// v0.66.0 modules
mod stats;
// v0.73.0 modules
mod json_mapping;
mod subscriptions;
// v0.77.0 modules
mod bidi;
// v0.87.0 modules — Uncertain Knowledge Engine
mod shacl_scoring;
mod uncertain_knowledge_api;

// Re-export all GUC statics at the crate root so that `crate::SOME_GUC` paths
// in existing code continue to work after the split.
pub(crate) use gucs::*;

pgrx::pg_module_magic!();

// ─── pg_trickle runtime detection (v0.6.0) ───────────────────────────────────

/// The pg_trickle version that pg_ripple was tested against (A-4, v0.25.0).
const PG_TRICKLE_TESTED_VERSION: &str = "0.3.0";

// ─── RDF Patch N-Triples term parser (v0.25.0) ───────────────────────────────

/// Parse an N-Triples triple statement string into (s, p, o) term strings.
///
/// Returns `None` when the input cannot be parsed as a valid N-Triples statement.
/// Supports IRIs (`<…>`), blank nodes (`_:…`), plain literals (`"…"`), and
/// datatyped/lang-tagged literals.
pub(crate) fn parse_nt_triple(line: &str) -> Option<(String, String, String)> {
    let line = line.trim().trim_end_matches('.').trim();
    let mut terms: Vec<String> = Vec::with_capacity(3);
    let mut chars = line.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => {
                chars.next();
            }
            '<' => {
                chars.next();
                let mut buf = String::from("<");
                let mut closed = false;
                for c in chars.by_ref() {
                    // C13-09 (v0.85.0): reject IRIs longer than 4 KiB (4096 chars of content
                    // not counting the surrounding `<>`).  At this size the content beyond `<` is
                    // 1 (initial `<`) + up to 4096 content chars = 4097; reject once we exceed it.
                    if buf.len() > 4097 {
                        pgrx::warning!("parse_nt_triple: IRI exceeds 4 KiB limit; line rejected");
                        return None;
                    }
                    buf.push(c);
                    if c == '>' {
                        closed = true;
                        break;
                    }
                }
                // C13-09: require closing `>` for IRI tokens; reject malformed input.
                if !closed {
                    pgrx::warning!("parse_nt_triple: IRI token missing closing `>`; line rejected");
                    return None;
                }
                terms.push(buf);
            }
            '"' => {
                chars.next();
                let mut buf = String::from("\"");
                let mut escaped = false;
                for c in chars.by_ref() {
                    buf.push(c);
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == '\\' {
                        escaped = true;
                        continue;
                    }
                    if c == '"' {
                        break;
                    }
                }
                // Consume optional ^^<datatype> or @lang suffix.
                while let Some(&p) = chars.peek() {
                    if p == '^' || p == '@' {
                        buf.push(p);
                        chars.next();
                    } else if p == '<' {
                        chars.next();
                        buf.push('<');
                        for c in chars.by_ref() {
                            buf.push(c);
                            if c == '>' {
                                break;
                            }
                        }
                        break;
                    } else if p.is_alphanumeric() || p == '-' || p == '_' {
                        buf.push(p);
                        chars.next();
                    } else {
                        break;
                    }
                }
                terms.push(buf);
            }
            '_' => {
                let mut buf = String::new();
                for c in chars.by_ref() {
                    if c == ' ' || c == '\t' {
                        break;
                    }
                    buf.push(c);
                }
                terms.push(buf);
            }
            _ => {
                chars.next();
            }
        }
        if terms.len() == 3 {
            break;
        }
    }
    if terms.len() == 3 {
        Some((terms.remove(0), terms.remove(0), terms.remove(0)))
    } else {
        None
    }
}

/// Returns `true` when the pg_trickle extension is installed in the current database.
///
/// All pg_trickle-dependent features gate on this check — core pg_ripple
/// functionality works without pg_trickle.
///
/// Also emits a one-time WARNING if the installed pg_trickle version is newer
/// than `PG_TRICKLE_TESTED_VERSION` (A-4, v0.25.0).
pub(crate) fn has_pg_trickle() -> bool {
    // Check existence first.
    let exists = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle')",
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if exists {
        // Version-lock probe (A-4): warn if installed version is newer than tested.
        if let Some(installed) = pgrx::Spi::get_one::<String>(
            "SELECT extversion FROM pg_extension WHERE extname = 'pg_trickle'",
        )
        .unwrap_or(None)
            && installed.as_str() > PG_TRICKLE_TESTED_VERSION
        {
            pgrx::warning!(
                "pg_ripple: pg_trickle version {} is newer than tested version {}; \
                 incremental views may behave unexpectedly",
                installed,
                PG_TRICKLE_TESTED_VERSION
            );
        }
    }

    exists
}

/// Returns `true` when the pg_trickle live-statistics stream tables have been
/// created (i.e. `enable_live_statistics()` was previously called successfully).
pub(crate) fn has_live_statistics() -> bool {
    pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = '_pg_ripple'
              AND c.relname = 'predicate_stats'
        )",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

// ─── ExecutorEnd hook (v0.6.0, updated v0.74.0 FLUSH-DEFER-01) ───────────────

/// Register a PostgreSQL `ExecutorEnd_hook` that:
/// 1. Pokes the merge worker's latch when accumulated delta rows exceed threshold.
/// 2. Flushes the mutation journal at statement end (FLUSH-DEFER-01) so that
///    single-triple inserts in a loop fire CWB rules once per statement, not
///    once per triple (quadratic cost elimination).
///
/// Must only be called from `_PG_init` inside the postmaster context
/// (i.e. when loaded via `shared_preload_libraries`).
fn register_executor_end_hook() {
    // SAFETY: ExecutorEnd_hook is a PostgreSQL global hook pointer; we install
    // the standard hook-chaining pattern in postmaster context during _PG_init.
    // The static mut is accessed only from `_PG_init` (single-threaded at startup).
    unsafe {
        static mut PREV_EXECUTOR_END: pg_sys::ExecutorEnd_hook_type = None;

        PREV_EXECUTOR_END = pg_sys::ExecutorEnd_hook;
        pg_sys::ExecutorEnd_hook = Some(pg_ripple_executor_end);

        #[pg_guard]
        unsafe extern "C-unwind" fn pg_ripple_executor_end(query_desc: *mut pg_sys::QueryDesc) {
            // Call the previous hook first.
            unsafe {
                if let Some(prev) = PREV_EXECUTOR_END {
                    prev(query_desc);
                } else {
                    pg_sys::standard_ExecutorEnd(query_desc);
                }
            }

            // FLUSH-DEFER-01: flush the mutation journal at statement end.
            // This converts per-triple CWB firing into per-statement firing.
            // Safe to call here because the executor end runs in normal SPI context.
            crate::storage::mutation_journal::flush();

            // If shmem is ready, check whether delta growth exceeds the threshold.
            if !crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let threshold = crate::LATCH_TRIGGER_THRESHOLD.get() as i64;
            let delta_rows = crate::shmem::TOTAL_DELTA_ROWS
                .get()
                .load(std::sync::atomic::Ordering::Relaxed);
            if delta_rows >= threshold {
                crate::shmem::poke_merge_worker();
            }
        }
    }
}

/// Called once when the extension shared library is loaded.
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    // Register all GUC parameters (MOD-01: extracted to src/gucs/registration.rs).
    crate::gucs::registration::register_all_gucs();

    // ── Shared memory initialisation (v0.6.0) ────────────────────────────────
    // Only registers shmem hooks (pg_shmem_init!) when running in
    // shared_preload_libraries context.  When loaded via CREATE EXTENSION the
    // hooks have already fired; skip to avoid the "PgAtomic was not
    // initialized" panic.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        shmem::init();
        worker::register_merge_workers();
        // Register the RDF logical apply worker when replication is enabled (v0.54.0).
        if crate::REPLICATION_ENABLED.get() {
            replication::register_logical_apply_worker();
        }
        // CDC-SLOT-01 (v0.81.0): register the CDC slot cleanup background worker.
        cdc::register_cdc_slot_cleanup_worker();
        // Register ExecutorEnd hook to poke the merge worker latch when the
        // accumulated unmerged delta row count crosses the trigger threshold.
        register_executor_end_hook();
    } else {
        // PRELOAD-WARN-01 (v0.81.0): warn when loaded without shared_preload_libraries.
        // HTAP merge worker, CONSTRUCT writeback, and the dictionary shmem cache are
        // all disabled in this mode.
        pgrx::warning!(
            "pg_ripple: loaded without shared_preload_libraries; \
             HTAP merge worker, CONSTRUCT writeback, and dictionary cache are disabled. \
             Add pg_ripple to shared_preload_libraries in postgresql.conf."
        );
    }

    // ── Transaction callbacks (v0.22.0) ───────────────────────────────────────
    // Register transaction callback to clear the dictionary cache on abort.
    // This ensures rolled-back dictionary entries (from INSERT INTO dictionary
    // during a failed transaction) do not persist in the backend-local cache,
    // preventing phantom references (v0.22.0 critical fix C-2).
    register_xact_callback();

    // ── Relcache callback (v0.51.0) ───────────────────────────────────────────
    // Register a relcache invalidation callback so that the predicate-OID
    // thread-local cache is flushed whenever a VP table is rebuilt by
    // VACUUM FULL (which assigns a new OID to the replacement heap).
    crate::storage::catalog::register_relcache_callback();

    // Schema and base tables are created by the `schema_setup` extension_sql!
    // block, which runs inside the CREATE EXTENSION transaction where SPI and
    // DDL are available.  Nothing to do here.
    //
    // PROMO-01 crash recovery is exposed as pg_ripple.recover_interrupted_promotions()
    // so users can call it after an unclean shutdown.  It is intentionally NOT called
    // from _PG_init because SPI_connect inside _PG_init can corrupt the active
    // snapshot context and break subsequent SQL in the same session.
}

// ─── Transaction callbacks (v0.22.0) ──────────────────────────────────────────

/// Register transaction and subtransaction callbacks.
///
/// - `RegisterXactCallback`: clears dictionary cache on abort; clears the
///   mutation journal on abort so rolled-back writes do not fire CWB.
/// - `RegisterSubXactCallback` (XACT-01, v0.72.0): clears the mutation journal
///   when a SAVEPOINT is rolled back, preventing phantom CWB firings for
///   triples that were never durably written.
fn register_xact_callback() {
    unsafe {
        // SAFETY: RegisterXactCallback and RegisterSubXactCallback are standard
        // PostgreSQL callback registration APIs called from _PG_init while the
        // postmaster holds the process lock.  Both callbacks use only Rust code
        // with no SPI calls, making them safe to invoke from callback context.
        pg_sys::RegisterXactCallback(Some(xact_callback_c), std::ptr::null_mut());
        pg_sys::RegisterSubXactCallback(Some(sub_xact_callback_c), std::ptr::null_mut());
    }
}

/// C-compatible transaction callback wrapper.
///
/// PostgreSQL calls this callback with XactEvent and an opaque arg pointer.
/// We forward to the Rust clear_caches function only on XACT_EVENT_ABORT and
/// XACT_EVENT_PARALLEL_ABORT events.
#[allow(non_snake_case)]
unsafe extern "C-unwind" fn xact_callback_c(event: u32, _arg: *mut std::ffi::c_void) {
    // XactEvent enum values from PostgreSQL 18 src/include/access/xact.h:
    //   XACT_EVENT_COMMIT          = 0
    //   XACT_EVENT_PARALLEL_COMMIT = 1
    //   XACT_EVENT_ABORT           = 2
    //   XACT_EVENT_PARALLEL_ABORT  = 3
    //   XACT_EVENT_PREPARE         = 4
    //   XACT_EVENT_PRE_COMMIT      = 5
    if event == 2 || event == 3 {
        // Transaction is being rolled back: evict shmem entries inserted in
        // this transaction so stale hash→id mappings cannot pollute later txns.
        crate::dictionary::clear_caches();
        // Also clear any pending journal entries — they must not fire after rollback.
        crate::storage::mutation_journal::clear();
    } else if event == 0 || event == 1 {
        // Transaction committed successfully: dictionary rows are durable, so
        // the shmem entries are correct — just clear the tracking list.
        crate::dictionary::commit_cleanup();
    }
    // Note: we do NOT call flush() here for XACT_EVENT_PRE_COMMIT (event 5)
    // because SPI is not safely callable from within a PostgreSQL xact callback
    // at the PRE_COMMIT stage.
    //
    // XACT-SPI-DOC-01 (v0.76.0): This claim is supported by PostgreSQL source:
    // src/backend/access/transam/xact.c – CallXactCallbacks() is invoked from
    // CommitTransaction() AFTER CommandCounterIncrement() and BEFORE the commit
    // record is written to WAL.  At that point the CurrentMemoryContext is
    // TopTransactionContext, which is about to be freed; any SPI_connect() call
    // would allocate in that context and the resulting portal/memory would be
    // invalidated before SPI_finish().  Additionally, pg_catalog writes from
    // within a pre-commit callback can deadlock when the heap AM acquires locks
    // that are already held by the outer transaction.
    // Reference: https://github.com/postgres/postgres/blob/REL_18_STABLE/src/backend/access/transam/xact.c
    // (search for "XACT_EVENT_PRE_COMMIT" and "CallXactCallbacks").
    //
    // Flush is called directly at each write API boundary in dict_api.rs and
    // views_api.rs instead (FLUSH-01 revised).
}

/// C-compatible subtransaction callback (XACT-01, v0.72.0).
///
/// Clears the mutation journal when a SAVEPOINT is rolled back so that any
/// `record_write`/`record_delete` calls accumulated inside the subtransaction
/// do not survive to fire CWB rules after the subtransaction is aborted.
///
/// SubXactEvent constants (PostgreSQL 18):
///   SUBXACT_EVENT_START_SUB      = 0
///   SUBXACT_EVENT_COMMIT_SUB     = 1
///   SUBXACT_EVENT_ABORT_SUB      = 2
///   SUBXACT_EVENT_PRE_COMMIT_SUB = 3
#[allow(non_snake_case)]
unsafe extern "C-unwind" fn sub_xact_callback_c(
    event: pgrx::pg_sys::SubXactEvent::Type,
    _mySubid: pgrx::pg_sys::SubTransactionId,
    _parentSubid: pgrx::pg_sys::SubTransactionId,
    _arg: *mut std::ffi::c_void,
) {
    // SUBXACT_EVENT_ABORT_SUB = 2: subtransaction is being rolled back.
    // Clear the mutation journal so phantom CWB writes do not fire.
    if event == pgrx::pg_sys::SubXactEvent::SUBXACT_EVENT_ABORT_SUB {
        crate::storage::mutation_journal::clear();
        // DICT-SUBXACT-01 (v0.81.0): also invalidate the dictionary decode cache
        // so that stale id→string mappings from the aborted subtransaction
        // cannot be returned by subsequent decode() calls.
        crate::dictionary::invalidate_decode_cache();
    }
}

/// Called when the extension shared library is unloaded (e.g. DROP EXTENSION).
///
/// PGFINI-01 (v0.81.0): unregisters all PostgreSQL callbacks registered in
/// `_PG_init` so they do not fire after the shared library is unloaded.
/// Without this, stale callback pointers in PostgreSQL's hook lists cause
/// undefined behaviour or crashes when the extension is recreated in the same
/// backend session.
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_fini() {
    // Unregister the transaction-level and subtransaction-level callbacks.
    // SAFETY: UnregisterXactCallback and UnregisterSubXactCallback are standard
    // PostgreSQL C APIs that safely remove callback entries from their respective
    // linked lists.  They are no-ops if the callback is not registered.
    unsafe {
        pg_sys::UnregisterXactCallback(Some(xact_callback_c), std::ptr::null_mut());
        pg_sys::UnregisterSubXactCallback(Some(sub_xact_callback_c), std::ptr::null_mut());
    }
    // Unregister the ExecutorEnd hook (storage/mod.rs).
    crate::unregister_executor_end_hook();
}

/// Unregister the ExecutorEnd hook installed in `_PG_init`.
///
/// Restores `pg_sys::ExecutorEnd_hook` to NULL (the standard handler).
/// Called from `_PG_fini` before the shared library is unloaded.
fn unregister_executor_end_hook() {
    // SAFETY: ExecutorEnd_hook is a PostgreSQL global hook pointer.  Setting it
    // to None restores the standard handler.  Only called from _PG_fini which
    // runs in the backend process holding the extension lock.
    unsafe {
        pg_sys::ExecutorEnd_hook = None;
    }
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[allow(non_snake_case)]
mod lib_tests;

/// Required by pgrx test framework: defines setup/teardown hooks for pg_test.
#[cfg(any(test, feature = "pg_test"))]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["allow_system_table_mods = on"]
    }
}

