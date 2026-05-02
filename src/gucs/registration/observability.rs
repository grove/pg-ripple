//! GUC registration for observability and AI/LLM (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
#[allow(unused_imports)]
use pgrx::prelude::*;

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
/// Register all GUCs for this domain.
pub fn register() {
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
}
