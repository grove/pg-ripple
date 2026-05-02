//! GUC parameters for the Datalog reasoning engine (inference, aggregation,
//! semi-naive evaluation, DRed, parallel strata, WFS, lattice, tabling).

// ─── v0.10.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: Datalog inference execution mode.
pub static INFERENCE_MODE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: Datalog constraint enforcement mode.
pub static ENFORCE_CONSTRAINTS: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: graph scope for unscoped body atoms. Default is 'all' (match any graph).
/// Set to 'default' to restrict unscoped atoms to g = 0 only.
pub static RULE_GRAPH_SCOPE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.29.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: master switch for magic sets goal-directed inference (v0.29.0).
pub static MAGIC_SETS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: when `true` (default), sort Datalog rule body atoms by ascending
/// estimated VP-table cardinality before SQL compilation (v0.29.0).
pub static DATALOG_COST_REORDER: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum VP-table row count for negated body atoms to use anti-join (v0.29.0).
pub static DATALOG_ANTIJOIN_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: minimum semi-naive delta temp-table row count before creating a B-tree index (v0.29.0).
pub static DELTA_INDEX_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(500);

// ─── v0.30.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: master switch for the Datalog rule plan cache (v0.30.0).
pub static RULE_PLAN_CACHE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of rule sets whose compiled SQL is kept in the plan cache (v0.30.0).
pub static RULE_PLAN_CACHE_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

// ─── v0.31.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: master switch for `owl:sameAs` entity canonicalization (v0.31.0).
pub static SAMEAS_REASONING: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: master switch for demand transformation (v0.31.0).
pub static DEMAND_TRANSFORM: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.32.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: safety cap on alternating fixpoint rounds for well-founded semantics (v0.32.0).
pub static WFS_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: master switch for the Datalog / SPARQL tabling cache (v0.32.0).
pub static TABLING: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: TTL in seconds for tabling cache entries (v0.32.0).
pub static TABLING_TTL: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(300);

// ─── v0.34.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: maximum depth for bounded-depth Datalog fixpoint termination (v0.34.0).
pub static DATALOG_MAX_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: master switch for the Delete-Rederive (DRed) algorithm (v0.34.0).
pub static DRED_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of deleted base triples per DRed transaction (v0.34.0).
pub static DRED_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ─── v0.35.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: maximum number of parallel background workers for Datalog stratum evaluation (v0.35.0).
pub static DATALOG_PARALLEL_WORKERS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: minimum estimated total row count for a stratum before parallel group
/// analysis is applied (v0.35.0).
pub static DATALOG_PARALLEL_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.36.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: maximum fixpoint iterations for lattice-based Datalog inference (v0.36.0).
pub static LATTICE_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ─── v0.40.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: maximum derived facts produced by a single `infer()` call (v0.40.0).
pub static DATALOG_MAX_DERIVED: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

// ─── v0.42.0 Datalog GUCs ─────────────────────────────────────────────────────

/// GUC: maximum `owl:sameAs` equivalence-class size before emitting PT550 WARNING (v0.42.0).
pub static SAMEAS_MAX_CLUSTER_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100_000);

// ─── v0.57.0 Datalog / OWL profile GUCs ──────────────────────────────────────

/// GUC: active OWL reasoning profile: `'RL'` (default), `'EL'`, `'QL'`, or `'off'` (v0.57.0).
pub static OWL_PROFILE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: enable experimental probabilistic Datalog with rule confidence weights (v0.57.0).
pub static PROBABILISTIC_DATALOG: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.62.0 Datalog GUCs ────────────────────────────────────────────────────

/// GUC: when on, wrap Datalog stratum-iteration INSERT…SELECT in
/// `run_command_on_all_nodes` for parallel worker execution (v0.62.0 CITUS-27).
/// Requires `citus_sharding_enabled = on`. Default off.
pub static DATALOG_CITUS_DISPATCH: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.83.0 Datalog cost-model GUCs (DL-COST-GUC-01) ────────────────────────

/// GUC: synthetic cardinality divisor applied when a Datalog rule atom has the
/// subject position bound to a constant (v0.83.0 DL-COST-GUC-01).
///
/// A larger value makes single-bound atoms appear cheaper, sorting them earlier
/// in the join order.  Useful on datasets where the subject fanout is very low.
/// Replaces the hardcoded `100` divisor in `src/datalog/compiler.rs`.
pub static DATALOG_COST_BOUND_S_DIVISOR: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: synthetic cardinality divisor applied when a Datalog rule atom has both
/// the subject and object positions bound to constants (v0.83.0 DL-COST-GUC-01).
///
/// A larger value makes dual-bound atoms appear cheaper relative to other atoms.
/// Replaces the hardcoded `10` divisor in `src/datalog/compiler.rs`.
pub static DATALOG_COST_BOUND_SO_DIVISOR: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10);
