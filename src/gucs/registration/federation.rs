//! GUC registration for SPARQL federation (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

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
/// Register all GUCs for this domain.
pub fn register() {
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
}
