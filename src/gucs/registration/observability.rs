//! GUC registration for observability and AI/LLM (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

// A16-CQ: unused_imports here is intentional for test/cfg-gated code paths.
#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
// A16-CQ: unused_imports here is intentional for test/cfg-gated code paths.
#[allow(unused_imports)]
use pgrx::prelude::*;

#[pg_guard]
unsafe extern "C-unwind" fn check_llm_api_key_env(
    newval: *mut *mut std::ffi::c_char,
    _extra: *mut *mut std::ffi::c_void,
    _source: pgrx::pg_sys::GucSource::Type,
) -> bool {
    if newval.is_null() || LLM_API_KEY_ENV_ALLOW_RAW.get() {
        return true;
    }
    // SAFETY: PostgreSQL supplies a valid pointer to the candidate GUC value
    // when the outer pointer is non-null.
    let value = unsafe { *newval };
    if value.is_null() {
        return true;
    }
    // SAFETY: newval is a GUC check-hook argument; pointer is valid for the
    // duration of this call and the string has at least a NUL terminator.
    let s = unsafe { std::ffi::CStr::from_ptr(value).to_str().unwrap_or("") };
    let valid = s
        .as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_uppercase() || *c == b'_')
        && s.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_');
    // PostgreSQL turns a false check-hook result into a configuration error;
    // returning false also works for SET/ALTER SYSTEM and does not leak the
    // rejected value into the server log.
    valid
}

/// Register all pg_ripple GUC parameters.
///
/// Register all GUCs for this domain.
pub fn register() {
    // ── v0.49.0 GUCs — AI & LLM Integration ──────────────────────────────────
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.llm_endpoint",
        c"LLM API base URL — no-op in the extension itself; the HTTP companion \
      `pg_ripple_http` uses this GUC via the /rules/{id}/explain endpoint to \
      handle LLM enrichment for rule explanations. \
      Must be an OpenAI-compatible base URL (empty = disabled, 'mock' = built-in test mock). \
      (v0.49.0, H16-04 v0.112.0)",
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
            c"Name of the environment variable holding the LLM API key; must match ^[A-Z_][A-Z0-9_]*$ \
          (default: PG_RIPPLE_LLM_API_KEY). Raw values require pg_ripple.llm_api_key_env_allow_raw = on. (v0.131.0)",
            c"PG_RIPPLE_LLM_API_KEY",
            &LLM_API_KEY_ENV,
            GucContext::Userset,
            GucFlags::default(),
            Some(check_llm_api_key_env),
            None,
            None,
        );
    }

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.llm_api_key_env_allow_raw",
        c"Allow raw API-key values in pg_ripple.llm_api_key_env during pre-0.131 migration (default: off).",
        c"",
        &LLM_API_KEY_ENV_ALLOW_RAW,
        GucContext::Suset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.llm_include_shapes",
        c"Include active SHACL shapes as LLM context when generating SPARQL \
      (default: on). (v0.49.0)",
        c"",
        &LLM_INCLUDE_SHAPES,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.91.0 GUCs — SHACL score log retention (OBS-02) ────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.shacl_score_log_retention_days",
        c"Retention period in days for _pg_ripple.shacl_score_log rows (v0.91.0 OBS-02). \
          Background maintenance deletes rows older than this limit. 0 disables pruning.",
        c"",
        &crate::gucs::observability::SHACL_SCORE_LOG_RETENTION_DAYS,
        0,
        3650,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.101.0 GUCs — NL Explanation Cache ─────────────────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.explanation_cache_ttl",
        c"TTL in seconds for _pg_ripple.explanation_cache entries (v0.101.0). \
          Explanations older than this are regenerated. 0 disables caching (default: 3600).",
        c"",
        &crate::gucs::llm::EXPLANATION_CACHE_TTL_SECS,
        0,
        86400 * 30, // max 30 days
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.105.0 GUCs — Rule Authoring ───────────────────────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.suggest_rules_max_candidates",
        c"Maximum number of candidate rules returned by suggest_rules() (v0.105.0). \
          Default: 20. Range: 1–200.",
        c"",
        &crate::gucs::llm::SUGGEST_RULES_MAX_CANDIDATES,
        1,
        200,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.119.0 GUCs — Schema-Aware NL→SPARQL (Feature 10) ─────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.nl_sparql_include_bundles",
        c"When on (default), sparql_from_nl() includes active vocabulary-bundle metadata \
          (SKOS preferred labels, DCTERMS titles, Schema.org type names, FOAF names) in \
          the LLM system prompt grounding for improved domain-specific SPARQL generation. \
          Feature 10, v0.119.0.",
        c"",
        &crate::gucs::llm::NL_SPARQL_INCLUDE_BUNDLES,
        GucContext::Userset,
        GucFlags::default(),
    );
}
