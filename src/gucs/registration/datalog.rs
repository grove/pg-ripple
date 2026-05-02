//! GUC registration for Datalog inference and reasoning (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
#[allow(unused_imports)]
use pgrx::prelude::*;

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
/// Register all GUCs for this domain.
pub fn register() {
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

    // ── v0.87.0 Uncertain Knowledge Engine GUCs ───────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.prob_datalog_cyclic",
        c"Allow probabilistic evaluation on cyclic Datalog rule sets. Approximate evaluation; \
      requires explicit opt-in. Default off. (v0.87.0 CONF-CYCLIC-01)",
        c"",
        &crate::gucs::datalog::PROB_DATALOG_CYCLIC,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.prob_datalog_max_iterations",
        c"Maximum semi-naive inference rounds when prob_datalog_cyclic = on. \
      After this limit the evaluator emits a WARNING and returns the partial result. \
      Default 100, range 1–10000. (v0.87.0 CONF-CYCLIC-01)",
        c"",
        &crate::gucs::datalog::PROB_DATALOG_MAX_ITERATIONS,
        1,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.prob_datalog_convergence_delta",
        c"Early-exit threshold for cyclic probabilistic Datalog: iteration stops when the \
      maximum confidence delta is below this value. Default 0.001. (v0.87.0 CONF-CYCLIC-01)",
        c"",
        &crate::gucs::datalog::PROB_DATALOG_CONVERGENCE_DELTA,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.prob_datalog_cyclic_strict",
        c"When on, promote max-iterations-exceeded from WARNING to ERROR (PT0307). \
      Default off. (v0.87.0 CONF-CYCLIC-01)",
        c"",
        &crate::gucs::datalog::PROB_DATALOG_CYCLIC_STRICT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.default_fuzzy_threshold",
        c"Default similarity threshold for pg:fuzzy_match() and pg:confPath() when no \
      explicit threshold is provided. Default 0.7. (v0.87.0 FUZZY-SPARQL-01)",
        c"",
        &crate::gucs::datalog::DEFAULT_FUZZY_THRESHOLD,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.prov_confidence",
        c"Enable automatic confidence propagation from PROV-O pg:sourceTrust predicates. \
      Default off. (v0.87.0 PROV-CONF-01)",
        c"",
        &crate::gucs::datalog::PROV_CONFIDENCE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.export_confidence",
        c"Enable confidence annotations in RDF export functions (RDF* and JSON-LD 1.1). \
      Default off. (v0.87.0 CONF-EXPORT-01)",
        c"",
        &crate::gucs::datalog::EXPORT_CONFIDENCE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.cwb_confidence_propagation",
        c"CONSTRUCT writeback confidence propagation mode: 'explicit' (default, implicit 1.0) \
      or 'inherit' (propagate source confidence weighted by rule weight). \
      (v0.87.0 CONF-CWB-01)",
        c"",
        &crate::gucs::datalog::CWB_CONFIDENCE_PROPAGATION,
        GucContext::Userset,
        GucFlags::default(),
    );
}
