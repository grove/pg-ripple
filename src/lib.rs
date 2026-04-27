//! pg_ripple — High-performance RDF triple store for PostgreSQL 18.
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

use pgrx::guc::{GucContext, GucFlags};
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
                for c in chars.by_ref() {
                    buf.push(c);
                    if c == '>' {
                        break;
                    }
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

// ─── ExecutorEnd hook (v0.6.0) ────────────────────────────────────────────────

/// Register a PostgreSQL `ExecutorEnd_hook` that pokes the merge worker's latch
/// whenever the accumulated unmerged delta row count crosses
/// `pg_ripple.latch_trigger_threshold`.
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
    // ── v0.37.0: Register string-enum GUCs with check_hook validators ────────
    // These validators reject invalid enum values immediately (at SET time)
    // rather than allowing them to propagate silently to the execution path.

    /// Validate `inference_mode`: `off`, `on_demand`, or `materialized`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_inference_mode(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "off" | "on_demand" | "materialized" | "incremental_rdfs")
    }

    /// Validate `enforce_constraints`: `off`, `warn`, or `error`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_enforce_constraints(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "off" | "warn" | "error")
    }

    /// Validate `rule_graph_scope`: `default` or `all`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_rule_graph_scope(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "default" | "all")
    }

    /// Validate `shacl_mode`: `off`, `sync`, or `async`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_shacl_mode(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "off" | "sync" | "async")
    }

    /// Validate `describe_strategy`: `cbd`, `scbd`, or `simple`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_describe_strategy(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "cbd" | "scbd" | "simple")
    }

    // ── v0.47.0 check_hook validators ─────────────────────────────────────────

    /// Validate `federation_on_error`: `warning`, `error`, or `empty`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_federation_on_error(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "warning" | "error" | "empty")
    }

    /// Validate `federation_on_partial`: `empty` or `use`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_federation_on_partial(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "empty" | "use")
    }

    /// Validate `sparql_overflow_action`: `warn` or `error`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_sparql_overflow_action(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "warn" | "error")
    }

    /// Validate `tracing_exporter`: `stdout` or `otlp`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_tracing_exporter(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "stdout" | "otlp")
    }

    /// Validate `embedding_index_type`: `hnsw` or `ivfflat`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_embedding_index_type(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "hnsw" | "ivfflat")
    }

    /// Validate `embedding_precision`: `single`, `half`, or `binary`.
    #[pg_guard]
    unsafe extern "C-unwind" fn check_embedding_precision(
        newval: *mut *mut std::ffi::c_char,
        _extra: *mut *mut std::ffi::c_void,
        _source: pgrx::pg_sys::GucSource::Type,
    ) -> bool {
        if newval.is_null() {
            return true;
        }
        // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe {
            if (*newval).is_null() {
                return true;
            }
            std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
        };
        matches!(s, "single" | "half" | "binary")
    }

    /// v0.55.0 H-2: Assign hook for `pg_ripple.llm_api_key_env`.
    ///
    /// Emits a WARNING if the value looks like a raw API key (i.e., contains
    /// non-identifier characters such as hyphens, dots, slashes, or lowercase
    /// letters mixed with digits) rather than an environment-variable name.
    /// Environment variable names are conventionally ALL_CAPS with underscores.
    #[pg_guard]
    unsafe extern "C-unwind" fn assign_llm_api_key_env(
        newval: *const std::ffi::c_char,
        _extra: *mut std::ffi::c_void,
    ) {
        if newval.is_null() {
            return;
        }
        // SAFETY: newval is a GUC assign-hook argument; pointer is valid for
        // the duration of this call and the string has at least a NUL terminator.
        let s = unsafe { std::ffi::CStr::from_ptr(newval).to_str().unwrap_or("") };
        if s.is_empty() {
            return;
        }
        // Heuristic: env var names only contain A-Z, 0-9, and underscores.
        // If the value contains lowercase letters, hyphens, slashes, or looks
        // like a base64/JWT token (long string with mixed chars), warn the user.
        let looks_like_raw_key = s.len() > 20
            || s.contains(['-', '.', '/', '=', '+'])
            || s.chars().any(|c| c.is_ascii_lowercase());
        if looks_like_raw_key {
            pgrx::warning!(
                "pg_ripple.llm_api_key_env looks like a raw API key, not an \
                 environment variable name. Set it to the NAME of an env var \
                 (e.g. MY_LLM_KEY) rather than the key value itself. \
                 Storing API keys in GUCs is insecure: they appear in \
                 pg_settings and server logs."
            );
        }
    }

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.default_graph",
        c"IRI of the default named graph (empty = built-in default graph)",
        c"",
        &DEFAULT_GRAPH,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vp_promotion_threshold",
        c"Minimum triple count before a predicate gets its own VP table (default: 1000, range: 10–10,000,000)",
        c"",
        &VPP_THRESHOLD,
        10,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.named_graph_optimized",
        c"Add a (g, s, o) index to each VP table to speed up named-graph queries",
        c"",
        &NAMED_GRAPH_OPTIMIZED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.plan_cache_size",
        c"Maximum number of cached SPARQL-to-SQL plan translations per backend (0 = disabled)",
        c"",
        &PLAN_CACHE_SIZE,
        0,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.max_path_depth",
        c"Maximum recursion depth for SPARQL property path queries (+ and *); 0 = unlimited",
        c"",
        &MAX_PATH_DEPTH,
        0,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.37.0: validated describe_strategy
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.describe_strategy",
            c"DESCRIBE algorithm: 'cbd' (Concise Bounded Description), 'scbd' (Symmetric CBD), or 'simple'",
            c"",
            &DESCRIBE_STRATEGY,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_describe_strategy),
            None,
            None,
        );
    }

    // ── v0.6.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_threshold",
        c"Minimum rows in a delta table before triggering a background merge (default: 10000)",
        c"",
        &MERGE_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_interval_secs",
        c"Maximum seconds between merge worker polling cycles (default: 60)",
        c"",
        &MERGE_INTERVAL_SECS,
        1,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_retention_seconds",
        c"Seconds to keep the previous main table after a merge before dropping it (default: 60)",
        c"",
        &MERGE_RETENTION_SECONDS,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.latch_trigger_threshold",
        c"Rows written in one batch before poking the merge worker latch (default: 10000)",
        c"",
        &LATCH_TRIGGER_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.worker_database",
        c"Database the background merge worker connects to (default: postgres)",
        c"",
        &WORKER_DATABASE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_watchdog_timeout",
        c"Seconds of merge worker inactivity before a WARNING is logged (default: 300)",
        c"",
        &MERGE_WATCHDOG_TIMEOUT,
        10,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.7.0 GUCs ──────────────────────────────────────────────────────────

    // v0.37.0: validated shacl_mode
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.shacl_mode",
            c"SHACL validation mode: 'off' (default), 'sync' (reject violations inline), 'async' (queue for background worker)",
            c"",
            &SHACL_MODE,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_shacl_mode),
            None,
            None,
        );
    }

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.dedup_on_merge",
        c"When true, the HTAP generation merge deduplicates (s,o,g) rows keeping the lowest SID (default: false)",
        c"",
        &DEDUP_ON_MERGE,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.10.0 GUCs ─────────────────────────────────────────────────────────

    // v0.37.0: Use define_string_guc_with_hooks to validate enum values at SET time.
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.inference_mode",
            c"Datalog inference mode: 'off' (default), 'on_demand', 'materialized', 'incremental_rdfs' (v0.56.0)",
            c"",
            &INFERENCE_MODE,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_inference_mode),
            None,
            None,
        );

        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.enforce_constraints",
            c"Constraint rule enforcement: 'off' (default), 'warn', 'error'",
            c"",
            &ENFORCE_CONSTRAINTS,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_enforce_constraints),
            None,
            None,
        );

        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.rule_graph_scope",
            c"Graph scope for unscoped Datalog atoms: 'default' (g=0 only) or 'all' (any graph)",
            c"",
            &RULE_GRAPH_SCOPE,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_rule_graph_scope),
            None,
            None,
        );
    }

    // ── v0.13.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.bgp_reorder",
        c"Reorder BGP triple patterns by estimated selectivity before SQL generation (default: on)",
        c"",
        &BGP_REORDER,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.parallel_query_min_joins",
        c"Minimum number of VP-table joins before enabling parallel query workers (default: 3)",
        c"",
        &PARALLEL_QUERY_MIN_JOINS,
        1,
        100,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.14.0 GUCs ─────────────────────────────────────────────────────────

    // v0.37.0: rls_bypass is elevated to PGC_POSTMASTER so it cannot be
    // flipped per-session (a user could otherwise bypass RLS with SET LOCAL).
    // This requires the registration to happen only during shared_preload_libraries
    // loading (where Postmaster-context GUCs are accepted).
    // When loaded outside that context (e.g. direct CREATE EXTENSION), fall back
    // to Suset context so the GUC is still registered.
    {
        let ctx = if unsafe { pgrx::pg_sys::process_shared_preload_libraries_in_progress } {
            GucContext::Postmaster
        } else {
            GucContext::Suset
        };
        pgrx::GucRegistry::define_bool_guc(
            c"pg_ripple.rls_bypass",
            c"Superuser override: when on, graph-level RLS policies are bypassed; \
              cannot be changed per-session (v0.37.0: PGC_POSTMASTER scope)",
            c"",
            &RLS_BYPASS,
            ctx,
            GucFlags::default(),
        );
    }

    // ── v0.16.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_timeout",
        c"Per-SERVICE-call wall-clock timeout in seconds (default: 30)",
        c"",
        &FEDERATION_TIMEOUT,
        1,
        3600,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_max_results",
        c"Maximum rows accepted from a single remote SERVICE call (default: 10000)",
        c"",
        &FEDERATION_MAX_RESULTS,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.47.0: validated federation_on_error
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.federation_on_error",
            c"Behaviour on SERVICE call failure: 'warning' (default), 'error', or 'empty'",
            c"",
            &FEDERATION_ON_ERROR,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_federation_on_error),
            None,
            None,
        );
    }

    // ── v0.19.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_pool_size",
        c"Idle connections per remote endpoint kept in the thread-local HTTP pool (default: 4, range: 1-32)",
        c"",
        &FEDERATION_POOL_SIZE,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_cache_ttl",
        c"TTL in seconds for cached SERVICE results; 0 disables caching (default: 0, range: 0-86400)",
        c"",
        &FEDERATION_CACHE_TTL,
        0,
        86400,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.47.0: validated federation_on_partial
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.federation_on_partial",
            c"Behaviour on mid-stream SERVICE failure: 'empty' (default, discard) or 'use' (keep partial rows)",
            c"",
            &FEDERATION_ON_PARTIAL,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_federation_on_partial),
            None,
            None,
        );
    }

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_adaptive_timeout",
        c"When on, derive per-endpoint timeout from P95 latency in federation_health (default: off)",
        c"",
        &FEDERATION_ADAPTIVE_TIMEOUT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_partial_recovery_max_bytes",
        c"Maximum response body size in bytes for partial federation result recovery; responses larger than this return empty with a WARNING (default: 65536, min: 1024, max: 104857600)",
        c"",
        &FEDERATION_PARTIAL_RECOVERY_MAX_BYTES,
        1024,
        104_857_600,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.21.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.sparql_strict",
        c"When on (default), unsupported SPARQL FILTER functions raise ERRCODE_FEATURE_NOT_SUPPORTED; \
          when off, they are silently dropped for backward compatibility",
        c"",
        &SPARQL_STRICT,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.24.0 GUCs ─────────────────────────────────────────────────────────
    // NOTE (v0.56.0 S2-5): pg_ripple.property_path_max_depth GUC was removed.
    // Use pg_ripple.max_path_depth instead (raises PT501 if the old name is set).

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.auto_analyze",
        c"When on (default), run ANALYZE on VP main tables after each merge cycle to keep planner statistics current",
        c"",
        &AUTO_ANALYZE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.export_batch_size",
        c"Number of triples fetched per cursor batch during streaming export (default: 10000, min: 100, max: 1000000)",
        c"",
        &EXPORT_BATCH_SIZE,
        100,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.27.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_model",
        c"Embedding model name tag (e.g. 'text-embedding-3-small'); stored in the model column of _pg_ripple.embeddings",
        c"",
        &EMBEDDING_MODEL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.embedding_dimensions",
        c"Vector dimension count; must match the actual model output (default: 1536, range: 1-16000)",
        c"",
        &EMBEDDING_DIMENSIONS,
        1,
        16_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_api_url",
        c"Base URL for an OpenAI-compatible embedding API (e.g. https://api.openai.com/v1)",
        c"",
        &EMBEDDING_API_URL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_api_key",
        c"API key for the embedding endpoint (superuser-only; masked in pg_settings)",
        c"",
        &EMBEDDING_API_KEY,
        GucContext::Suset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pgvector_enabled",
        c"When off, disable all pgvector-dependent code paths without uninstalling the extension (default: on)",
        c"",
        &PGVECTOR_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.47.0: validated embedding_index_type and embedding_precision
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.embedding_index_type",
            c"Index type on _pg_ripple.embeddings: 'hnsw' (default) or 'ivfflat'; changing requires REINDEX",
            c"",
            &EMBEDDING_INDEX_TYPE,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_embedding_index_type),
            None,
            None,
        );

        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.embedding_precision",
            c"Embedding storage precision: 'single' (default, vector(N)), 'half' (halfvec(N), -50% storage), 'binary' (bit(N), -96% storage); requires pgvector >= 0.7.0",
            c"",
            &EMBEDDING_PRECISION,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_embedding_precision),
            None,
            None,
        );
    }

    // ── v0.28.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.auto_embed",
        c"When on, a trigger on _pg_ripple.dictionary enqueues new entity IDs for automatic embedding (default: off)",
        c"",
        &AUTO_EMBED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.embedding_batch_size",
        c"Number of entities dequeued and embedded per background worker batch (default: 100, range: 1–10000)",
        c"",
        &EMBEDDING_BATCH_SIZE,
        1,
        10_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.use_graph_context",
        c"When on, embed_entities() serializes each entity's RDF neighborhood for richer vectors (default: off)",
        c"",
        &USE_GRAPH_CONTEXT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vector_federation_timeout_ms",
        c"HTTP timeout in milliseconds for external vector service endpoint calls (default: 5000, range: 100–300000)",
        c"",
        &VECTOR_FEDERATION_TIMEOUT_MS,
        100,
        300_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.29.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.magic_sets",
        c"When on (default), infer_goal() uses magic sets for goal-directed inference; \
          off falls back to full materialization + filter (v0.29.0)",
        c"",
        &MAGIC_SETS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.datalog_cost_reorder",
        c"When on (default), sort Datalog rule body atoms by ascending estimated \
          VP-table cardinality before SQL compilation (v0.29.0)",
        c"",
        &DATALOG_COST_REORDER,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_antijoin_threshold",
        c"Minimum VP-table rows for NOT body atoms to compile to LEFT JOIN IS NULL \
          anti-join form instead of NOT EXISTS (default: 1000, 0=always NOT EXISTS; v0.29.0)",
        c"",
        &DATALOG_ANTIJOIN_THRESHOLD,
        0,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.delta_index_threshold",
        c"Minimum semi-naive delta-table rows before creating a B-tree index on (s,o) \
          join columns (default: 500, 0=disabled; v0.29.0)",
        c"",
        &DELTA_INDEX_THRESHOLD,
        0,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.30.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.rule_plan_cache",
        c"When on (default), cache compiled SQL for each rule set to speed up \
          repeated infer() / infer_agg() calls; invalidated by drop_rules() and \
          load_rules() (v0.30.0)",
        c"",
        &RULE_PLAN_CACHE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.rule_plan_cache_size",
        c"Maximum number of rule sets kept in the plan cache (default: 64, \
          min: 1, max: 4096); oldest entries are evicted on overflow (v0.30.0)",
        c"",
        &RULE_PLAN_CACHE_SIZE,
        1,
        4096,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.31.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.sameas_reasoning",
        c"When on (default), Datalog inference applies an owl:sameAs \
          canonicalization pre-pass so that rules and SPARQL queries referencing \
          non-canonical entities are transparently rewritten to the canonical form \
          (v0.31.0)",
        c"",
        &SAMEAS_REASONING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.demand_transform",
        c"When on (default), create_datalog_view() automatically applies demand \
          transformation when multiple goal patterns are specified; infer_demand() \
          always applies demand filtering regardless (v0.31.0)",
        c"",
        &DEMAND_TRANSFORM,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.32.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.wfs_max_iterations",
        c"Safety cap on alternating fixpoint rounds per WFS pass (default: 100, \
          min: 1, max: 10000); emits PT520 WARNING if a pass does not converge (v0.32.0)",
        c"",
        &WFS_MAX_ITERATIONS,
        1,
        10_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.tabling",
        c"When on (default), infer_wfs() and SPARQL results are cached in \
          _pg_ripple.tabling_cache and reused on matching subsequent calls; \
          invalidated by drop_rules(), load_rules(), and triple modifications (v0.32.0)",
        c"",
        &TABLING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.tabling_ttl",
        c"TTL in seconds for tabling cache entries (default: 300; set 0 to disable \
          TTL-based expiry) (v0.32.0)",
        c"",
        &TABLING_TTL,
        0,
        86_400,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.34.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_max_depth",
        c"Maximum depth for bounded-depth Datalog fixpoint termination; 0 = unlimited (default: 0, min: 0, max: 100000) (v0.34.0)",
        c"",
        &DATALOG_MAX_DEPTH,
        0,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.dred_enabled",
        c"When on (default), deleting a base triple uses DRed incremental retraction \
          to surgically remove only affected derived facts; off falls back to full \
          re-materialization (v0.34.0)",
        c"",
        &DRED_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.dred_batch_size",
        c"Maximum number of deleted base triples processed in a single DRed \
          transaction (default: 1000, min: 1, max: 1000000) (v0.34.0)",
        c"",
        &DRED_BATCH_SIZE,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.35.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_parallel_workers",
        c"Maximum parallel worker count for Datalog stratum evaluation; 1 = serial \
          (default: 4, min: 1, max: 32) (v0.35.0)",
        c"",
        &DATALOG_PARALLEL_WORKERS,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_parallel_threshold",
        c"Minimum estimated total-row count for a stratum before parallel group \
          analysis is applied (default: 10000, min: 0) (v0.35.0)",
        c"",
        &DATALOG_PARALLEL_THRESHOLD,
        0,
        100_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.36.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.wcoj_enabled",
        c"When on (default), cyclic SPARQL BGPs are detected and executed via \
          sort-merge join hints simulating Leapfrog Triejoin (v0.36.0)",
        c"",
        &WCOJ_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.wcoj_min_tables",
        c"Minimum VP table join count before WCOJ cyclic-pattern detection is applied \
          (default: 3, min: 2, max: 100) (v0.36.0)",
        c"",
        &WCOJ_MIN_TABLES,
        2,
        100,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.lattice_max_iterations",
        c"Maximum fixpoint iterations for lattice-based Datalog inference; \
          emits PT540 WARNING on non-convergence (default: 1000, min: 1, max: 1000000) (v0.36.0)",
        c"",
        &LATTICE_MAX_ITERATIONS,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.37.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.tombstone_gc_enabled",
        c"When on (default), automatically VACUUM VP tombstone tables after merge \
          when the tombstone/main ratio exceeds tombstone_gc_threshold (v0.37.0)",
        c"",
        &TOMBSTONE_GC_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.tombstone_gc_threshold",
        c"Tombstone-to-main-row ratio that triggers automatic VACUUM after merge \
          (default: '0.05' = 5%; accepts a decimal string, range: 0.0–1.0) (v0.37.0)",
        c"",
        &TOMBSTONE_GC_THRESHOLD_STR,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.38.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.predicate_cache_enabled",
        c"When on (default), cache VP table OID lookups per backend so repeated \
          SPARQL queries on the same predicates avoid SPI catalog round-trips \
          (v0.38.0)",
        c"",
        &PREDICATE_CACHE_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.40.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sparql_max_rows",
        c"Maximum rows returned by a SPARQL SELECT/CONSTRUCT query. \
          0 = unlimited (default). Overflow behaviour: sparql_overflow_action (v0.40.0)",
        c"",
        &SPARQL_MAX_ROWS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_max_derived",
        c"Maximum derived facts produced by a single infer() call. \
          0 = unlimited (default). Emits PT602 WARNING when exceeded (v0.40.0)",
        c"",
        &DATALOG_MAX_DERIVED,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.export_max_rows",
        c"Maximum rows returned by export functions (Turtle/N-Triples/JSON-LD). \
          0 = unlimited (default). Emits PT603 WARNING when exceeded (v0.40.0)",
        c"",
        &EXPORT_MAX_ROWS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.47.0: validated sparql_overflow_action
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.sparql_overflow_action",
            c"Action when sparql_max_rows is exceeded: 'warn' (default, truncate with PT601 WARNING) \
              or 'error' (raise ERROR) (v0.40.0)",
            c"",
            &SPARQL_OVERFLOW_ACTION,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_sparql_overflow_action),
            None,
            None,
        );
    }

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.tracing_enabled",
        c"When on, emit OpenTelemetry spans for SPARQL/merge/federation/Datalog operations. \
          Off by default (zero overhead when off) (v0.40.0)",
        c"",
        &TRACING_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.47.0: validated tracing_exporter
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.tracing_exporter",
            c"OpenTelemetry exporter backend: 'stdout' (default, writes to PG log at DEBUG5) \
              or 'otlp' (reads OTEL_EXPORTER_OTLP_ENDPOINT) (v0.40.0)",
            c"",
            &TRACING_EXPORTER,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_tracing_exporter),
            None,
            None,
        );
    }

    // ── v0.42.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sameas_max_cluster_size",
        c"Maximum owl:sameAs equivalence-class size before emitting PT550 WARNING and \
          switching to sampling approximation. 0 = disabled (v0.42.0)",
        c"",
        &SAMEAS_MAX_CLUSTER_SIZE,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_stats_ttl_secs",
        c"TTL in seconds for cached VoID statistics per federation endpoint. \
          0 = disabled (v0.42.0)",
        c"",
        &FEDERATION_STATS_TTL_SECS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_planner_enabled",
        c"Enable cost-based FedX-style federation source selection using VoID statistics. \
          On by default (v0.42.0)",
        c"",
        &FEDERATION_PLANNER_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_parallel_max",
        c"Maximum number of parallel SERVICE clause workers for independent atoms. \
          Default: 4 (v0.42.0)",
        c"",
        &FEDERATION_PARALLEL_MAX,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_parallel_timeout",
        c"Wall-clock timeout in seconds for parallel federation workers. \
          Default: 60 (v0.42.0)",
        c"",
        &FEDERATION_PARALLEL_TIMEOUT,
        1,
        3600,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_inline_max_rows",
        c"SERVICE responses exceeding this row count are spooled to a temp table \
          instead of VALUES clause inline. Emits PT620 INFO. Default: 10000 (v0.42.0)",
        c"",
        &FEDERATION_INLINE_MAX_ROWS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_allow_private",
        c"Allow federation endpoints with RFC-1918/loopback/link-local IP addresses. \
          Off by default (PT621 emitted when rejected). (v0.42.0)",
        c"",
        &FEDERATION_ALLOW_PRIVATE,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.46.0 GUCs ─────────────────────────────────────────────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.topn_pushdown",
        c"Push LIMIT N into the SQL plan for ORDER BY + LIMIT queries (default: on). \
          Disabled when DISTINCT is in scope. (v0.46.0)",
        c"",
        &TOPN_PUSHDOWN,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_sequence_batch",
        c"SID range reserved per parallel Datalog worker per batch (default: 10000, min: 100). \
          Each worker uses its pre-allocated slice without touching the global sequence. (v0.46.0)",
        c"",
        &DATALOG_SEQUENCE_BATCH,
        100,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.48.0 GUCs ─────────────────────────────────────────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_max_response_bytes",
        c"Maximum federation response body in bytes (default: 100 MiB = 104857600). \
          Responses larger than this are refused with PT543. Set -1 to disable. (v0.48.0)",
        c"",
        &FEDERATION_MAX_RESPONSE_BYTES,
        -1,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.55.0 GUCs — Federation SSRF Security ──────────────────────────────
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.federation_endpoint_policy",
        c"Network policy for SERVICE clause endpoints: 'default-deny' (block RFC-1918/loopback/link-local), \
          'allowlist' (only pg_ripple.federation_allowed_endpoints), 'open' (allow all). (v0.55.0)",
        c"",
        &FEDERATION_ENDPOINT_POLICY,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.federation_allowed_endpoints",
        c"Comma-separated list of allowed federation SERVICE endpoints. \
          Only consulted when federation_endpoint_policy = 'allowlist'. (v0.55.0)",
        c"",
        &FEDERATION_ALLOWED_ENDPOINTS,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.49.0 GUCs — AI & LLM Integration ──────────────────────────────────
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.llm_endpoint",
        c"LLM API base URL for NL→SPARQL generation (empty = disabled, 'mock' = built-in test mock). \
          Must be an OpenAI-compatible base URL. (v0.49.0)",
        c"",
        &LLM_ENDPOINT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.llm_model",
        c"LLM model identifier for NL→SPARQL generation (default: gpt-4o). (v0.49.0)",
        c"",
        &LLM_MODEL,
        GucContext::Userset,
        GucFlags::default(),
    );

    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
            c"pg_ripple.llm_api_key_env",
            c"Name of the environment variable holding the LLM API key \
              (default: PG_RIPPLE_LLM_API_KEY). Never stored inline. (v0.49.0)",
            c"",
            &LLM_API_KEY_ENV,
            GucContext::Userset,
            GucFlags::default(),
            None,
            Some(assign_llm_api_key_env),
            None,
        );
    }

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.llm_include_shapes",
        c"Include active SHACL shapes as LLM context when generating SPARQL \
          (default: on). (v0.49.0)",
        c"",
        &LLM_INCLUDE_SHAPES,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.51.0 GUCs — Security Hardening & Production Readiness ─────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sparql_max_algebra_depth",
        c"Maximum allowed SPARQL algebra tree depth; queries deeper than this are \
          rejected with PT440 (default: 256, 0=disabled). (v0.51.0)",
        c"",
        &SPARQL_MAX_ALGEBRA_DEPTH,
        0,
        65535,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sparql_max_triple_patterns",
        c"Maximum number of triple patterns in a single SPARQL query; queries \
          exceeding this are rejected with PT440 (default: 4096, 0=disabled). (v0.51.0)",
        c"",
        &SPARQL_MAX_TRIPLE_PATTERNS,
        0,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.tracing_otlp_endpoint",
        c"OTLP collector endpoint for OpenTelemetry span export when \
          tracing_exporter = 'otlp' (e.g. 'http://jaeger:4318/v1/traces'). \
          Falls back to OTEL_EXPORTER_OTLP_ENDPOINT env var if empty. (v0.51.0)",
        c"",
        &TRACING_OTLP_ENDPOINT,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.52.0 GUCs — pg-trickle Relay Integration ───────────────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.cdc_bridge_enabled",
        c"Enable the CDC → pg-trickle outbox bridge worker (default: off). \
          Requires pg-trickle to be installed. (v0.52.0)",
        c"",
        &CDC_BRIDGE_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_bridge_batch_size",
        c"Maximum CDC notifications batched before a bridge worker flush \
          (default: 100, min: 1, max: 10000). (v0.52.0)",
        c"",
        &CDC_BRIDGE_BATCH_SIZE,
        1,
        10_000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_bridge_flush_ms",
        c"Maximum milliseconds between bridge worker flush cycles \
          (default: 200, min: 10, max: 60000). (v0.52.0)",
        c"",
        &CDC_BRIDGE_FLUSH_MS,
        10,
        60_000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.cdc_bridge_outbox_table",
        c"Target outbox table for the CDC bridge worker (default: 'enriched_events'). \
          Must have (event_id TEXT, payload JSONB) columns. (v0.52.0)",
        c"",
        &CDC_BRIDGE_OUTBOX_TABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.trickle_integration",
        c"Enable pg-trickle integration features; set off to disable bridge even \
          when pg-trickle is installed (default: on). (v0.52.0)",
        c"",
        &TRICKLE_INTEGRATION,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.54.0 GUCs — High Availability & Logical Replication ───────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.replication_enabled",
        c"Enable the RDF logical replication consumer (logical_apply_worker). \
          When on, a background worker subscribes to the pg_ripple_pub publication \
          and applies N-Triples batches to the local store (default: off). (v0.54.0)",
        c"",
        &REPLICATION_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.replication_conflict_strategy",
        c"Conflict resolution strategy for the logical apply worker: \
          'last_writer_wins' (default) — keeps the row with the highest SID. (v0.54.0)",
        c"",
        &REPLICATION_CONFLICT_STRATEGY,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.55.0 GUCs — Security & Storage Quality ────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.tombstone_retention_seconds",
        c"Seconds to retain tombstones after a merge cycle. \
          0 (default) = truncate tombstones immediately after a full merge. (v0.55.0)",
        c"",
        &TOMBSTONE_RETENTION_SECONDS,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.normalize_iris",
        c"When on (default), normalize IRI strings to NFC before dictionary encoding. \
          Ensures that semantically equivalent IRIs with different Unicode normalization \
          map to the same dictionary entry. (v0.55.0)",
        c"",
        &NORMALIZE_IRIS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.copy_rdf_allowed_paths",
        c"Comma-separated list of allowed path prefixes for copy_rdf_from(). \
          When set, only paths matching a listed prefix are permitted. \
          When empty (default), ALL paths are denied (PT403 default-deny policy). (v0.55.0)",
        c"",
        &COPY_RDF_ALLOWED_PATHS,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.read_replica_dsn",
        c"DSN for a read replica to route SELECT/CONSTRUCT/ASK/DESCRIBE queries to. \
          Falls back to primary on connection failure (PT530 WARNING). (v0.55.0)",
        c"",
        &READ_REPLICA_DSN,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.56.0 GUCs — Audit log & federation circuit breaker ────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.audit_log_enabled",
        c"When on, record SPARQL UPDATE/DELETE/DROP/CLEAR operations in _pg_ripple.audit_log. \
          Default off. (v0.56.0)",
        c"",
        &crate::gucs::observability::AUDIT_LOG_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.tracing_traceparent",
        c"W3C traceparent header value forwarded from pg_ripple_http. \
          Set via SET LOCAL by the HTTP service before each SPARQL/Datalog query. \
          Enables end-to-end distributed tracing. (v0.61.0)",
        c"",
        &crate::gucs::observability::TRACING_TRACEPARENT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_circuit_breaker_threshold",
        c"Consecutive endpoint failures before the federation circuit breaker opens (default: 5). \
          0 = circuit breaker disabled. (v0.56.0)",
        c"",
        &crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_THRESHOLD,
        0,
        1000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_circuit_breaker_reset_seconds",
        c"Seconds until a tripped federation circuit breaker transitions to half-open (default: 60). \
          (v0.56.0)",
        c"",
        &crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_RESET_SECONDS,
        1,
        3600,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.57.0 GUCs — OWL profiles, KGE, multi-tenant, columnar, adaptive index ──

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.owl_profile",
        c"Active OWL reasoning profile: 'RL' (default), 'EL', 'QL', or 'off'. (v0.57.0)",
        c"",
        &crate::gucs::datalog::OWL_PROFILE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.probabilistic_datalog",
        c"Enable experimental probabilistic Datalog with @weight rule annotations. \
          Preview quality; no stability guarantee. Default off. (v0.57.0)",
        c"",
        &crate::gucs::datalog::PROBABILISTIC_DATALOG,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.kge_enabled",
        c"Enable the knowledge-graph embedding background worker. Default off. (v0.57.0)",
        c"",
        &crate::gucs::llm::KGE_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.kge_model",
        c"Knowledge-graph embedding model: 'transe' (default) or 'rotate'. (v0.57.0)",
        c"",
        &crate::gucs::llm::KGE_MODEL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.columnar_threshold",
        c"VP table triple count above which HTAP merge converts vp_main to columnar storage. \
          -1 = disabled (default). Requires pg_columnar. (v0.57.0)",
        c"",
        &crate::gucs::storage::COLUMNAR_THRESHOLD,
        -1,
        1_000_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.adaptive_indexing_enabled",
        c"Enable adaptive B-tree index creation based on per-predicate query access patterns. \
          Default off. (v0.57.0)",
        c"",
        &crate::gucs::storage::ADAPTIVE_INDEXING_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.58.0 GUCs — Citus sharding, merge fence, PROV-O ──────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_sharding_enabled",
        c"Enable Citus horizontal sharding of VP tables on predicate promotion. \
          Requires the Citus extension. Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::CITUS_SHARDING_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_trickle_compat",
        c"When on, VP delta tables use colocate_with='none' for pg-trickle CDC compatibility. \
          Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::CITUS_TRICKLE_COMPAT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_fence_timeout_ms",
        c"Milliseconds the merge worker waits for an advisory fence lock during Citus rebalancing. \
          0 = no fence. (v0.58.0)",
        c"",
        &crate::gucs::storage::MERGE_FENCE_TIMEOUT_MS,
        0,
        300_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.prov_enabled",
        c"Emit PROV-O provenance triples for all bulk ingest operations. Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::PROV_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // PGC_POSTMASTER GUCs can only be registered during shared_preload_libraries
    // loading.  `process_shared_preload_libraries_in_progress` is the correct
    // flag — `IsPostmasterEnvironment` is true in every server process and
    // cannot be used to distinguish this case.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.dictionary_cache_size",
            c"Shared-memory encode-cache capacity in entries (default: 4096; startup only)",
            c"",
            &DICTIONARY_CACHE_SIZE,
            0,
            1_000_000,
            GucContext::Postmaster,
            GucFlags::default(),
        );

        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.cache_budget",
            c"Shared-memory budget cap in MB; bulk loads throttle when >90% utilised (default: 64; startup only)",
            c"",
            &CACHE_BUDGET_MB,
            0,
            65536,
            GucContext::Postmaster,
            GucFlags::default(),
        );

        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.merge_workers",
            c"Number of parallel background merge worker processes (default: 1, max: 16; startup only). \
              Each worker handles a round-robin subset of VP table predicates (v0.42.0)",
            c"",
            &MERGE_WORKERS,
            1,
            16,
            GucContext::Postmaster,
            GucFlags::default(),
        );
    }

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
        // Register ExecutorEnd hook to poke the merge worker latch when the
        // accumulated unmerged delta row count crosses the trigger threshold.
        register_executor_end_hook();
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
}

// ─── Transaction callbacks (v0.22.0) ──────────────────────────────────────────

/// Register a transaction callback to clear the dictionary cache on abort.
///
/// This prevents rolled-back dictionary entries from persisting in the
/// backend-local cache, which would create phantom references in subsequent
/// transactions (critical fix C-2).
fn register_xact_callback() {
    unsafe {
        // SAFETY: RegisterXactCallback is a standard PostgreSQL callback mechanism
        // for transaction events. We register a C-compatible callback that will be
        // called at various transaction events. The callback uses only Rust code
        // (clear_caches) which has no dependencies on PG's signal handling, so it
        // is safe to call from a callback context.
        pg_sys::RegisterXactCallback(Some(xact_callback_c), std::ptr::null_mut());
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
    } else if event == 0 || event == 1 {
        // Transaction committed successfully: dictionary rows are durable, so
        // the shmem entries are correct — just clear the tracking list.
        crate::dictionary::commit_cleanup();
    }
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_encode_decode_roundtrip() {
        let id = crate::dictionary::encode("https://example.org/subject", 0);
        let decoded = crate::dictionary::decode(id).expect("decode should succeed");
        assert_eq!(decoded, "https://example.org/subject");
    }

    #[pg_test]
    fn test_insert_and_count() {
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            0,
        );
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_typed_literal_roundtrip() {
        // xsd:integer is now inline-encoded (bit 63 = 1, no dictionary row).
        let id = crate::dictionary::encode_typed_literal(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        );
        assert!(
            crate::dictionary::inline::is_inline(id),
            "xsd:integer should be inline-encoded"
        );
        // decode() must still return the correct N-Triples literal string.
        let decoded = crate::dictionary::decode(id).expect("decode should succeed for inline");
        assert_eq!(
            decoded,
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[pg_test]
    fn test_lang_literal_roundtrip() {
        let id = crate::dictionary::encode_lang_literal("hello", "en");
        let full = crate::dictionary::decode_full(id).expect("decode_full should succeed");
        assert_eq!(full.value, "hello");
        assert_eq!(full.lang.as_deref(), Some("en"));
    }

    #[pg_test]
    fn test_ntriples_bulk_load() {
        let data =
            "<https://example.org/a> <https://example.org/knows> <https://example.org/b> .\n";
        let count = crate::bulk_load::load_ntriples(data, false);
        assert_eq!(count, 1);
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_turtle_bulk_load() {
        let data = "@prefix ex: <https://example.org/> .\nex:x ex:rel ex:y .\n";
        let count = crate::bulk_load::load_turtle(data, false);
        assert_eq!(count, 1);
    }

    #[pg_test]
    fn test_named_graph_drop() {
        let graph = "<https://example.org/mygraph>";
        let g_id = crate::storage::create_graph(graph);
        assert!(g_id > 0);
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            g_id,
        );
        let deleted = crate::storage::drop_graph(graph);
        assert!(deleted >= 1);
    }

    #[pg_test]
    fn test_export_ntriples_roundtrip() {
        let nt =
            "<https://example.org/ex> <https://example.org/pred> <https://example.org/obj> .\n";
        crate::bulk_load::load_ntriples(nt, false);
        let exported = crate::export::export_ntriples(None);
        assert!(exported.contains("<https://example.org/pred>"));
    }

    // ─── SPARQL engine tests (v0.3.0) ─────────────────────────────────────────

    /// A SELECT that returns no rows on an empty store must produce an empty set.
    #[pg_test]
    fn pg_test_sparql_select_empty() {
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 0, "expected no rows on empty store");
    }

    /// After loading one triple, SELECT ?s ?p ?o must return exactly one row.
    #[pg_test]
    fn pg_test_sparql_select_one_triple() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n",
            false,
        );
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 1, "expected exactly one row");
        // The row must contain a non-null ?s binding.
        let obj = rows[0].0.as_object().expect("row must be a JSON object");
        assert!(obj.contains_key("s"), "row must have ?s binding");
        assert!(obj.contains_key("p"), "row must have ?p binding");
        assert!(obj.contains_key("o"), "row must have ?o binding");
    }

    /// sparql_ask() on an empty store returns false.
    #[pg_test]
    fn pg_test_sparql_ask_empty() {
        let result = crate::sparql::sparql_ask("ASK { ?s ?p ?o }");
        assert!(!result, "ASK on empty store must be false");
    }

    /// sparql_ask() returns true after a matching triple is inserted.
    #[pg_test]
    fn pg_test_sparql_ask_match() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/x> <https://example.org/q> <https://example.org/y> .\n",
            false,
        );
        let result =
            crate::sparql::sparql_ask("ASK { <https://example.org/x> <https://example.org/q> ?o }");
        assert!(result, "ASK must be true after matching triple loaded");
    }

    /// sparql_explain() returns non-empty SQL for a simple SELECT.
    #[pg_test]
    fn pg_test_sparql_explain_returns_sql() {
        let plan = crate::sparql::sparql_explain(
            "SELECT ?s WHERE { ?s <https://example.org/p> ?o }",
            false,
        );
        assert!(
            plan.contains("Generated SQL"),
            "explain output must contain 'Generated SQL'"
        );
    }

    /// SPARQL LIMIT 1 must return at most one row.
    #[pg_test]
    fn pg_test_sparql_limit() {
        // Load two triples.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
            false,
        );
        let rows =
            crate::sparql::sparql("SELECT ?s ?o WHERE { ?s <https://example.org/p> ?o } LIMIT 1");
        assert!(rows.len() <= 1, "LIMIT 1 must return at most one row");
    }

    // ─── RDF-star / Statement Identifiers tests (v0.4.0) ──────────────────────

    /// N-Triples-star: loading an object-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_object_position() {
        let n = crate::bulk_load::load_ntriples(
            "<https://example.org/eve> <https://example.org/said> \
             << <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> .\n",
            false,
        );
        assert_eq!(n, 1, "object-position quoted triple must load as 1 triple");
    }

    /// N-Triples-star: loading a subject-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_subject_position() {
        let n = crate::bulk_load::load_ntriples(
            "<< <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> <https://example.org/certainty> \
             \"0.9\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n",
            false,
        );
        assert_eq!(n, 1, "subject-position quoted triple must load as 1 triple");
    }

    /// encode_quoted_triple / decode_quoted_triple_components round-trip.
    #[pg_test]
    fn pg_test_quoted_triple_encode_decode() {
        let s_id =
            crate::dictionary::encode("https://example.org/alice", crate::dictionary::KIND_IRI);
        let p_id =
            crate::dictionary::encode("https://example.org/knows", crate::dictionary::KIND_IRI);
        let o_id =
            crate::dictionary::encode("https://example.org/bob", crate::dictionary::KIND_IRI);
        let qt_id = crate::dictionary::encode_quoted_triple(s_id, p_id, o_id);
        assert!(qt_id != 0, "quoted triple must have a non-zero ID");
        let components = crate::dictionary::decode_quoted_triple_components(qt_id);
        assert!(components.is_some(), "decode must return Some");
        let (ds, dp, ob) = components.unwrap();
        assert_eq!(ds, s_id);
        assert_eq!(dp, p_id);
        assert_eq!(ob, o_id);
    }

    /// insert_triple returns a positive SID; get_statement can look it back up.
    #[pg_test]
    fn pg_test_statement_identifier_lifecycle() {
        let sid = crate::storage::insert_triple(
            "<https://example.org/subject1>",
            "<https://example.org/predicate1>",
            "<https://example.org/object1>",
            0,
        );
        assert!(sid > 0, "insert must return a positive SID");
    }

    /// SPARQL DISTINCT must deduplicate results.
    #[pg_test]
    fn pg_test_sparql_distinct() {
        // Two triples sharing the same predicate and object.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/same> <https://example.org/o> .\n\
             <https://example.org/s2> <https://example.org/same> <https://example.org/o> .\n",
            false,
        );
        // Select just ?o — should be deduplicated to 1 row.
        let rows =
            crate::sparql::sparql("SELECT DISTINCT ?o WHERE { ?s <https://example.org/same> ?o }");
        assert_eq!(rows.len(), 1, "DISTINCT ?o must collapse duplicates");
    }

    /// FILTER with a bound IRI constant must restrict results correctly.
    #[pg_test]
    fn pg_test_sparql_filter_bound() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
            false,
        );
        // Only one subject matches the binding of ?s to s1.
        let rows = crate::sparql::sparql(
            "SELECT ?o WHERE { <https://example.org/s1> <https://example.org/p> ?o }",
        );
        assert_eq!(rows.len(), 1, "bound subject must restrict to one row");
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["allow_system_table_mods = on"]
    }
}
