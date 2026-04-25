// v0.56.0 dead-code audit (A-6):
// All items in this module are live but reached only when tracing is enabled
// at runtime.  Because the module is pub(crate) and items are called
// conditionally, rustc reports them as dead.
// Finding: start_span, is_enabled, SpanGuard — reachable via pub(crate) module;
//           emit_span, emit_stdout — called by SpanGuard::drop; keep all.
//           Replaced file-wide #![allow(dead_code)] with per-item annotations.

//! OpenTelemetry tracing facade (v0.40.0).
//!
//! Provides a thin, zero-overhead tracing layer for pg_ripple operations.
//! When `pg_ripple.tracing_enabled = off` (default), all functions are no-ops
//! with no measurable overhead.
//!
//! # Supported spans
//!
//! - `sparql.parse` — SPARQL query parsing
//! - `sparql.translate` — SPARQL algebra → SQL translation
//! - `sparql.execute` — SQL execution via SPI
//! - `merge.cycle` — HTAP merge cycle (per predicate)
//! - `federation.call` — remote SERVICE call (per endpoint)
//! - `datalog.stratum` — Datalog inference (per stratum)
//!
//! # Configuration
//!
//! ```sql
//! SET pg_ripple.tracing_enabled = on;
//! SET pg_ripple.tracing_exporter = 'stdout';  -- or 'otlp'
//! ```

use std::time::Instant;

/// A guard that records a span when dropped.
///
/// Created by `start_span()`.  When dropped, the elapsed time is emitted
/// to the configured exporter.
#[cfg_attr(not(test), allow(dead_code))]
pub struct SpanGuard {
    name: &'static str,
    start: Instant,
    enabled: bool,
}

impl SpanGuard {
    fn new(name: &'static str, enabled: bool) -> Self {
        Self {
            name,
            start: Instant::now(),
            enabled,
        }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        let elapsed_us = self.start.elapsed().as_micros();
        emit_span(self.name, elapsed_us);
    }
}

/// Start a named tracing span.  Returns a guard that records the span on drop.
///
/// When `pg_ripple.tracing_enabled` is `false` (default), this is a no-op
/// — the guard is created but the `Drop` impl exits immediately without I/O.
#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub fn start_span(name: &'static str) -> SpanGuard {
    let enabled = crate::TRACING_ENABLED.get();
    SpanGuard::new(name, enabled)
}

/// Emit a completed span to the configured exporter.
fn emit_span(name: &str, elapsed_us: u128) {
    let exporter = crate::TRACING_EXPORTER
        .get()
        .as_ref()
        .and_then(|s| s.to_str().ok().map(|s| s.to_owned()))
        .unwrap_or_else(|| "stdout".to_owned());

    match exporter.as_str() {
        "otlp" => {
            // v0.51.0: emit span via the OTLP HTTP/JSON exporter.
            // The endpoint is read from pg_ripple.tracing_otlp_endpoint GUC
            // (fallback: OTEL_EXPORTER_OTLP_ENDPOINT env var).
            let endpoint = crate::TRACING_OTLP_ENDPOINT
                .get()
                .as_ref()
                .and_then(|c| c.to_str().ok().map(|s| s.to_owned()))
                .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
                .unwrap_or_default();

            if endpoint.is_empty() {
                // No endpoint configured — fall back to stdout so spans are not silently dropped.
                emit_stdout(name, elapsed_us);
            } else {
                // Emit span as a minimal OTLP-compatible JSON log line.
                // A production implementation would batch spans and POST them to
                // `{endpoint}/v1/traces`; for v0.51.0 we emit to the PostgreSQL
                // log at DEBUG5 level with the endpoint recorded so operators can
                // verify configuration without full OTLP client dependencies.
                let msg = format!(
                    r#"{{"span":"{}","elapsed_us":{},"otlp_endpoint":"{}"}}"#,
                    name, elapsed_us, endpoint
                );
                pgrx::debug5!("pg_ripple otlp trace: {}", msg);
            }
        }
        _ => {
            // Default: stdout (PostgreSQL log).
            emit_stdout(name, elapsed_us);
        }
    }
}

/// Write a span record to the PostgreSQL log as a JSON line.
fn emit_stdout(name: &str, elapsed_us: u128) {
    // Avoid pulling in pgrx error-handling macros for a logging path.
    // Use the DEBUG5 level so the output is invisible by default and only
    // appears when log_min_messages = debug5.
    let msg = format!(r#"{{"span":"{}","elapsed_us":{}}}"#, name, elapsed_us);
    // SAFETY: `pgrx::debug5!` is safe to call from within an SPI context.
    pgrx::debug5!("pg_ripple trace: {}", msg);
}

/// Return whether tracing is currently enabled.
#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub fn is_enabled() -> bool {
    crate::TRACING_ENABLED.get()
}
