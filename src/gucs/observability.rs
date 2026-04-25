//! GUC parameters for the observability subsystem (OpenTelemetry tracing,
//! export limits, and result-set overflow).

// ─── v0.40.0 observability GUCs ──────────────────────────────────────────────

/// GUC: maximum rows returned by export functions (Turtle/N-Triples/JSON-LD) (v0.40.0).
pub static EXPORT_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: master switch for OpenTelemetry tracing (v0.40.0).
pub static TRACING_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: OpenTelemetry exporter backend (v0.40.0).
pub static TRACING_EXPORTER: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.51.0 observability GUCs ──────────────────────────────────────────────

/// GUC: OTLP collector endpoint for OpenTelemetry span export (v0.51.0).
pub static TRACING_OTLP_ENDPOINT: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.56.0 observability GUCs — SPARQL audit log ──────────────────────────

/// GUC: enable SPARQL write-operation audit logging into `_pg_ripple.audit_log` (v0.56.0).
pub static AUDIT_LOG_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);
