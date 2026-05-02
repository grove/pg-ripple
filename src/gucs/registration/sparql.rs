//! GUC registration for SPARQL query engine (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

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
/// Register all GUCs for this domain.
pub fn register() {
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
}
