//! GUC registration -- extracted from lib.rs (MOD-01, v0.72.0).
//!
//! `register_all_gucs()` is called once from `_PG_init`.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

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

/// Register all pg_ripple GUC parameters.
///
/// Called exactly once from `_PG_init` during extension load.
pub fn register_all_gucs() {
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
    c"Minimum triple count before a predicate gets its own VP table (default: 1000, range: 100–10,000,000)",
    c"",
    &VPP_THRESHOLD,
    100,
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

    // ── v0.79.0 SHACL-SPARQL GUCs ────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.shacl_rule_max_iterations",
        c"Maximum fixpoint iterations for sh:SPARQLRule evaluation per validation cycle; \
      raises an error when the cap is reached (default: 100, min: 1, max: 10000) (v0.79.0)",
        c"",
        &crate::gucs::shacl::SHACL_RULE_MAX_ITERATIONS,
        1,
        10_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.shacl_rule_cwb",
        c"When on, sh:SPARQLRule rules whose target graph matches a CONSTRUCT writeback \
      pipeline are registered as CWB rules (default: off) (v0.79.0)",
        c"",
        &crate::gucs::shacl::SHACL_RULE_CWB,
        GucContext::Userset,
        GucFlags::default(),
    );

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
            c"Graph scope for unscoped Datalog atoms: 'all' (any graph, default) or 'default' (g=0 only)",
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
        c"pg_ripple.wcoj_min_cardinality",
        c"Minimum VP table edge count before the Leapfrog Triejoin executor is used; \
      below this threshold the query falls back to the SQL hash-join path. \
      0 = always use LFTI when the pattern is cyclic (default: 0, min: 0, max: 1000000000) (v0.79.0)",
        c"",
        &crate::gucs::sparql::WCOJ_MIN_CARDINALITY,
        0,
        1_000_000_000,
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

    // ── v0.62.0 GUCs — Arrow Flight, Citus scalability ──────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.dictionary_tier_threshold",
        c"Dictionary tier threshold for Citus cold-entry offload (v0.62.0). \
      Terms with access_count < N are eligible for cold tier. \
      -1 = disabled (default); only active when citus_sharding_enabled = on.",
        c"",
        &crate::gucs::storage::DICTIONARY_TIER_THRESHOLD,
        -1,
        1_000_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.citus_prune_carry_max",
        c"Maximum carry-forward set size for multi-hop shard pruning (v0.62.0 CITUS-29). \
      Above this threshold, falls back to full fan-out. Default 1000.",
        c"",
        &crate::gucs::storage::CITUS_PRUNE_CARRY_MAX,
        0,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.66.0 Arrow Flight GUCs (FLIGHT-01).
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.arrow_flight_secret",
        c"HMAC-SHA256 secret for signing Arrow Flight tickets (v0.66.0 FLIGHT-01). \
      Empty = unsigned tickets (rejected by default in pg_ripple_http).",
        c"",
        &crate::gucs::storage::ARROW_FLIGHT_SECRET,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.arrow_flight_expiry_secs",
        c"Arrow Flight ticket validity in seconds (v0.66.0 FLIGHT-01). Default: 3600.",
        c"",
        &crate::gucs::storage::ARROW_FLIGHT_EXPIRY_SECS,
        60,
        86400,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.67.0 Arrow Flight GUCs (FLIGHT-SEC-01, FLIGHT-SEC-02).
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.arrow_unsigned_tickets_allowed",
        c"When on, unsigned Arrow Flight tickets (sig=\"unsigned\") are accepted for \
      local development. Default off — production must use a signed secret. \
      (v0.67.0 FLIGHT-SEC-01)",
        c"",
        &crate::gucs::storage::ARROW_UNSIGNED_TICKETS_ALLOWED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.arrow_batch_size",
        c"Number of rows per Arrow record batch when streaming export (v0.67.0 FLIGHT-SEC-02). \
      Default: 1000.",
        c"",
        &crate::gucs::storage::ARROW_BATCH_SIZE,
        1,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.68.0 Citus/scalability GUCs ───────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.approx_distinct",
        c"When on, route SPARQL COUNT(DISTINCT …) through Citus HLL when available. \
      Falls back to exact COUNT(DISTINCT …) when hll extension is absent. \
      Default off. (v0.68.0 CITUS-HLL-01)",
        c"",
        &crate::gucs::storage::APPROX_DISTINCT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_service_pruning",
        c"When on, the SPARQL federation translator rewrites SERVICE subqueries targeting \
      Citus workers to include shard-constraint annotations to prune irrelevant shards. \
      Default off. (v0.68.0 CITUS-SVC-01)",
        c"",
        &crate::gucs::storage::CITUS_SERVICE_PRUNING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vp_promotion_batch_size",
        c"Batch size for nonblocking VP promotion background copy phase. \
      Number of rows copied from vp_rare to shadow tables per iteration. \
      Default: 10000. (v0.68.0 PROMO-01)",
        c"",
        &crate::gucs::storage::VP_PROMOTION_BATCH_SIZE,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.datalog_citus_dispatch",
        c"When on, wrap Datalog stratum-iteration INSERT…SELECT in \
      run_command_on_all_nodes for parallel worker execution (v0.62.0 CITUS-27). \
      Requires citus_sharding_enabled = on. Default off.",
        c"",
        &crate::gucs::datalog::DATALOG_CITUS_DISPATCH,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.81.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.strict_dictionary",
        c"When on, decode() returns an error for missing dictionary IDs instead of \
      the _unknown_<id> placeholder string. Useful for strict data-quality contexts. \
      Default off. (v0.81.0 DICT-STRICT-01)",
        c"",
        &crate::gucs::storage::STRICT_DICTIONARY,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.strict_sparql_filters",
        c"When on, unknown built-in function names in FILTER expressions raise \
      ERROR (PT422) rather than evaluating to UNDEF. Default off. \
      (v0.81.0 FILTER-STRICT-01)",
        c"",
        &crate::gucs::sparql::STRICT_SPARQL_FILTERS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_slot_idle_timeout_seconds",
        c"Seconds of no LSN advance before the CDC slot cleanup worker drops an \
      orphaned replication slot. Default: 3600. (v0.81.0 CDC-SLOT-01)",
        c"",
        &crate::gucs::storage::CDC_SLOT_IDLE_TIMEOUT_SECONDS,
        60,
        86400,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.82.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.plan_cache_capacity",
        c"Maximum number of cached SPARQL-to-SQL plan translations (default: 1024, range: 64–65536). \
      Replaces the hardcoded constant in plan_cache.rs. (v0.82.0 CACHE-CAP-01)",
        c"",
        &crate::gucs::sparql::PLAN_CACHE_CAPACITY,
        64,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_lock_timeout_ms",
        c"Milliseconds to wait for the merge fence lock before skipping this cycle \
      (default: 5000, range: 100–60000). (v0.82.0 MERGE-LOCK-GUC-01)",
        c"",
        &crate::gucs::storage::MERGE_LOCK_TIMEOUT_MS,
        100,
        60000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_heartbeat_interval_seconds",
        c"Seconds between merge worker heartbeat log lines (default: 60, range: 10–3600). \
      (v0.82.0 MERGE-HBEAT-01)",
        c"",
        &crate::gucs::storage::MERGE_HEARTBEAT_INTERVAL_SECONDS,
        10,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.stats_scan_limit",
        c"Maximum number of VP tables scanned per graph_stats() call \
      (default: 1000, range: 1–100000). (v0.82.0 STATS-DOC-01)",
        c"",
        &crate::gucs::storage::STATS_SCAN_LIMIT,
        1,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.stats_refresh_interval_seconds",
        c"Seconds between background refreshes of predicate_stats_cache \
      (default: 300, range: 10–86400). (v0.82.0 STATS-CACHE-01)",
        c"",
        &crate::gucs::storage::STATS_REFRESH_INTERVAL_SECONDS,
        10,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vacuum_dict_batch_size",
        c"Number of predicates processed per batch in vacuum_dictionary() \
      (default: 200, range: 10–10000). (v0.82.0 VACUUM-DICT-BATCH-01)",
        c"",
        &crate::gucs::storage::VACUUM_DICT_BATCH_SIZE,
        10,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.all_nodes_predicate_limit",
        c"Maximum number of predicates in a wildcard property-path UNION ALL expansion \
      (default: 500, range: 10–50000). Excess predicates sorted by triple count and truncated. \
      (v0.82.0 PROPPATH-UNBOUNDED-01)",
        c"",
        &crate::gucs::sparql::ALL_NODES_PREDICATE_LIMIT,
        10,
        50000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // GUC-BOUNDS-01 (v0.82.0): merge_batch_size — controls merge worker batch size.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_batch_size",
        c"Maximum rows processed per merge worker INSERT…SELECT batch (default: 1000000, \
          range: 100–100,000,000). (v0.82.0 GUC-BOUNDS-01)",
        c"",
        &crate::gucs::storage::MERGE_BATCH_SIZE,
        100,
        100_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.83.0 GUCs ──────────────────────────────────────────────────────────

    // DL-COST-GUC-01 (v0.83.0): Datalog cost-model divisors for rule body ordering.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_cost_bound_s_divisor",
        c"Synthetic cardinality divisor for Datalog rule atoms with subject bound to a constant \
      (default: 100, range: 1–10000). Larger values push single-bound atoms earlier in the join \
      order. Replaces hardcoded divisor 100 in compiler.rs. (v0.83.0 DL-COST-GUC-01)",
        c"",
        &crate::gucs::datalog::DATALOG_COST_BOUND_S_DIVISOR,
        1,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_cost_bound_so_divisor",
        c"Synthetic cardinality divisor for Datalog rule atoms with both subject and object bound \
      to constants (default: 10, range: 1–1000). Larger values push dual-bound atoms earlier. \
      Replaces hardcoded divisor 10 in compiler.rs. (v0.83.0 DL-COST-GUC-01)",
        c"",
        &crate::gucs::datalog::DATALOG_COST_BOUND_SO_DIVISOR,
        1,
        1000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // MERGE-BACKOFF-01 (v0.83.0): merge worker exponential backoff cap.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_max_backoff_secs",
        c"Maximum backoff duration in seconds for the merge worker exponential backoff \
      after consecutive errors (default: same as merge_interval_secs=60, range: 10–3600). \
      First error doubles the wait; each subsequent error doubles again; capped here. \
      (v0.83.0 MERGE-BACKOFF-01)",
        c"",
        &crate::gucs::storage::MERGE_MAX_BACKOFF_SECS,
        10,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // PGC_POSTMASTER GUCs can only be registered during shared_preload_libraries
    // loading.  `process_shared_preload_libraries_in_progress` is the correct
    // flag — `IsPostmasterEnvironment` is true in every server process and
    // cannot be used to distinguish this case.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.dictionary_cache_size",
            c"Shared-memory encode-cache capacity in entries (default: 4096; startup only; range: 1024–1,073,741,824)",
            c"",
            &DICTIONARY_CACHE_SIZE,
            1024,
            1_073_741_824,
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

        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.audit_retention",
            c"Retention period in days for _pg_ripple.event_audit rows (v0.78.0). \
          0 disables automatic pruning.",
            c"",
            &crate::gucs::observability::AUDIT_RETENTION_DAYS,
            0,
            3650,
            GucContext::Suset,
            GucFlags::default(),
        );
    }
}
