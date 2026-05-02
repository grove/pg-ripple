//! GUC parameters for the SPARQL query engine (query planning, plan cache,
//! property paths, WCOJ, TopN push-down, and DoS limits).

/// GUC: maximum number of cached SPARQL→SQL plan translations per backend.
pub static PLAN_CACHE_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(256);

/// GUC: maximum recursion depth for SPARQL property path queries (`+`, `*`).
pub static MAX_PATH_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: DESCRIBE algorithm — 'cbd', 'scbd', or 'simple'.
pub static DESCRIBE_STRATEGY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: SC13-04 (v0.86.0) — alias for `describe_strategy` using W3C-aligned
/// value names: 'cbd' (Concise Bounded Description), 'scbd' (Symmetric CBD),
/// or 'symmetric' (alias for 'scbd').
///
/// When set, this GUC takes precedence over `describe_strategy`.
/// Supported values: `cbd`, `scbd`, `symmetric`.
pub static DESCRIBE_FORM: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.13.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: enable BGP join reordering based on pg_stats selectivity estimates.
pub static BGP_REORDER: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum number of VP table joins before trying parallel query workers.
pub static PARALLEL_QUERY_MIN_JOINS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3);

// ─── v0.21.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: when `on` (default), raise ERRCODE_FEATURE_NOT_SUPPORTED for unsupported
/// SPARQL built-in functions.
pub static SPARQL_STRICT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.24.0 SPARQL GUCs ─────────────────────────────────────────────────────
// NOTE (v0.56.0 S2-5): PROPERTY_PATH_MAX_DEPTH removed; use MAX_PATH_DEPTH.

// ─── v0.36.0 SPARQL / WCOJ GUCs ──────────────────────────────────────────────

/// GUC: master switch for Worst-Case Optimal Join (WCOJ) optimisation (v0.36.0).
pub static WCOJ_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum number of VP table joins before WCOJ analysis is applied (v0.36.0).
pub static WCOJ_MIN_TABLES: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3);

/// GUC: minimum VP table cardinality before LFTI executor is used; below this
/// threshold the query falls back to the SQL hash-join path (v0.79.0).
pub static WCOJ_MIN_CARDINALITY: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

// ─── v0.40.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: maximum rows returned by a SPARQL SELECT or CONSTRUCT query (v0.40.0).
pub static SPARQL_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: action when `sparql_max_rows` is exceeded (v0.40.0).
pub static SPARQL_OVERFLOW_ACTION: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.46.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: enable TopN push-down for `ORDER BY … LIMIT N` queries (v0.46.0).
pub static TOPN_PUSHDOWN: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: SID range reserved per parallel Datalog worker per batch (v0.46.0).
pub static DATALOG_SEQUENCE_BATCH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.51.0 SPARQL DoS limit GUCs ───────────────────────────────────────────

/// GUC: maximum allowed algebra tree depth for SPARQL queries (v0.51.0).
pub static SPARQL_MAX_ALGEBRA_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(256);

/// GUC: maximum number of triple patterns allowed in a single SPARQL query (v0.51.0).
pub static SPARQL_MAX_TRIPLE_PATTERNS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4096);

// ─── v0.82.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: maximum number of cached SPARQL→SQL translations (v0.82.0 CACHE-CAP-01).
/// Replaces the hardcoded 256 constant in `src/sparql/plan_cache.rs`.
pub static PLAN_CACHE_CAPACITY: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1024);

/// GUC: maximum number of predicates used in a wildcard property-path expansion
/// (v0.82.0 PROPPATH-UNBOUNDED-01). When the schema has more predicates than this
/// limit, `build_all_nodes_sql()` uses only the top-N predicates by triple count.
pub static ALL_NODES_PREDICATE_LIMIT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(500);

// ─── v0.81.0 SPARQL GUCs ─────────────────────────────────────────────────────

/// GUC: when `on`, an unknown built-in function name in a FILTER expression raises
/// ERROR (PT422) rather than evaluating to UNDEF. Default: `off`.
/// (v0.81.0 FILTER-STRICT-01)
pub static STRICT_SPARQL_FILTERS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);
