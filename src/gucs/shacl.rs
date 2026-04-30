//! GUC parameters for the SHACL validation subsystem.

/// GUC: SHACL validation mode — 'off', 'sync', or 'async'.
/// 'sync' rejects violating triples inline; 'async' queues them for the
/// background validation worker; 'off' disables all SHACL enforcement.
pub static SHACL_MODE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.79.0 SHACL-SPARQL GUCs ───────────────────────────────────────────────

/// GUC: maximum fixpoint iterations for sh:SPARQLRule evaluation per validation
/// cycle.  Prevents infinite loops when rules fire each other.  An error is
/// raised when the cap is reached.  (v0.79.0, SHACL-SPARQL-01)
pub static SHACL_RULE_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: when `on`, sh:SPARQLRule rules whose target graph matches an existing
/// CONSTRUCT writeback pipeline are registered as CWB rules.  Default: `off`.
/// (v0.79.0, SHACL-SPARQL-01d)
pub static SHACL_RULE_CWB: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);
