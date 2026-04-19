#![allow(dead_code)]

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
            // OTLP export: in a full implementation this would buffer spans and
            // flush via gRPC to OTEL_EXPORTER_OTLP_ENDPOINT.  For now, fall
            // through to stdout to avoid adding heavy dependencies.
            emit_stdout(name, elapsed_us);
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
#[inline]
pub fn is_enabled() -> bool {
    crate::TRACING_ENABLED.get()
}
