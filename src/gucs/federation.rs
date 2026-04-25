//! GUC parameters for the SPARQL federation subsystem (connection pooling,
//! result caching, source selection, parallel dispatch, and security).

// ─── v0.16.0 federation GUCs ─────────────────────────────────────────────────

/// GUC: per-SERVICE-call wall-clock timeout in seconds (default: 30).
pub static FEDERATION_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(30);

/// GUC: maximum number of rows accepted from a single remote SERVICE call (default: 10,000).
pub static FEDERATION_MAX_RESULTS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: behaviour when a SERVICE call fails.
pub static FEDERATION_ON_ERROR: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.19.0 federation GUCs ─────────────────────────────────────────────────

/// GUC: number of idle connections to keep per remote endpoint (default: 4).
pub static FEDERATION_POOL_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: TTL in seconds for cached SERVICE results (0 = disabled).
pub static FEDERATION_CACHE_TTL: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: behaviour when a SERVICE call delivers rows then fails.
pub static FEDERATION_ON_PARTIAL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: when `on`, derive the effective per-endpoint timeout from P95 latency.
pub static FEDERATION_ADAPTIVE_TIMEOUT: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum body size in bytes for partial federation result recovery (v0.25.0).
pub static FEDERATION_PARTIAL_RECOVERY_MAX_BYTES: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(65_536);

// ─── v0.42.0 federation GUCs ─────────────────────────────────────────────────

/// GUC: TTL in seconds for cached VoID statistics per federation endpoint (v0.42.0).
pub static FEDERATION_STATS_TTL_SECS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3600);

/// GUC: enable cost-based federation source selection (v0.42.0).
pub static FEDERATION_PLANNER_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of parallel SERVICE clause workers (v0.42.0).
pub static FEDERATION_PARALLEL_MAX: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: wall-clock timeout in seconds for parallel federation workers (v0.42.0).
pub static FEDERATION_PARALLEL_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: maximum inline rows for federation results (v0.42.0).
pub static FEDERATION_INLINE_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: allow federation endpoints with private/loopback IP addresses (v0.42.0).
pub static FEDERATION_ALLOW_PRIVATE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.48.0 federation GUCs ─────────────────────────────────────────────────

/// GUC: maximum federation response body in bytes (v0.48.0).
pub static FEDERATION_MAX_RESPONSE_BYTES: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(104_857_600);

// ─── v0.55.0 federation security GUCs ──────────────────────────────────────

/// GUC: federation endpoint network policy (v0.55.0).
/// Values: 'default-deny' (block private/loopback/link-local),
///         'allowlist' (only explicitly listed),
///         'open' (allow all — use with care).
pub static FEDERATION_ENDPOINT_POLICY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(Some(c"default-deny"));

/// GUC: comma-separated list of allowed federation endpoints (v0.55.0).
/// Only consulted when `pg_ripple.federation_endpoint_policy = 'allowlist'`.
pub static FEDERATION_ALLOWED_ENDPOINTS: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.56.0 federation GUCs — circuit breaker ──────────────────────────────

/// GUC: consecutive failures before opening the federation circuit breaker (v0.56.0).
pub static FEDERATION_CIRCUIT_BREAKER_THRESHOLD: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(5);

/// GUC: seconds until a tripped circuit half-opens for a retry (v0.56.0).
pub static FEDERATION_CIRCUIT_BREAKER_RESET_SECONDS: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(60);
