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

// ─── v0.61.0 observability GUCs — OTLP traceparent ──────────────────────────

/// GUC: W3C traceparent header value forwarded from the HTTP layer (v0.61.0 I7-1).
///
/// Set via `SET LOCAL pg_ripple.tracing_traceparent = '...'` by `pg_ripple_http`
/// before executing each SPARQL or Datalog query.  The extension attaches this
/// trace ID to its OpenTelemetry spans, enabling end-to-end distributed traces
/// from the load balancer through the HTTP service into the query engine.
pub static TRACING_TRACEPARENT: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.78.0 observability GUCs — bidi audit retention ──────────────────────

/// GUC: retention period in days for `_pg_ripple.event_audit` rows (v0.78.0).
///
/// A background worker sweep prunes rows older than this many days once per hour.
/// Setting to 0 disables automatic pruning (manual archival required).
pub static AUDIT_RETENTION_DAYS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(90);
