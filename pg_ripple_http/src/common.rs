//! Shared application state and helper functions used by both SPARQL and
//! Datalog handlers.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use constant_time_eq::constant_time_eq;
use dashmap::DashMap;
use deadpool_postgres::Pool;
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use uuid::Uuid;

use crate::metrics::Metrics;

// ─── HTTP-ERR-01 (v0.80.0): structured JSON error response ───────────────────

/// Standard JSON error body for all 4xx/5xx HTTP responses from pg_ripple_http.
///
/// Serialises as `{"error": "<code>", "message": "<human-readable text>"}`.
/// All HTTP error responses must use this type (not plain-text bodies) so that
/// API clients can reliably parse error details without checking Content-Type.
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: &'static str,
    pub message: String,
}

/// Build a standard JSON error response for a client error (4xx).
///
/// Sets `Content-Type: application/json`.
pub fn json_error(code: &'static str, message: impl Into<String>, status: StatusCode) -> Response {
    let body = serde_json::to_string(&ErrorResponse {
        error: code,
        message: message.into(),
    })
    .unwrap_or_else(|_| format!(r#"{{"error":"{code}","message":"serialisation error"}}"#));
    // SAFETY: status and header values are compile-time constants; builder never fails.
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("infallible: hardcoded valid HTTP headers")
}

// ─── Application state ───────────────────────────────────────────────────────

/// Shared state injected into every axum handler via `State<Arc<AppState>>`.
pub struct AppState {
    pub pool: Pool,
    pub auth_token: Option<String>,
    /// Optional separate write token for Datalog mutating endpoints
    /// (`POST /datalog/rules/*`, `PUT`, `DELETE`). When `None`, the main
    /// `auth_token` governs all requests.
    pub datalog_write_token: Option<String>,
    /// Comma-separated list of upstream IP/CIDR values that are trusted to set
    /// `X-Forwarded-For`. `None` means X-Forwarded-For is not trusted.
    pub trust_proxy: Option<String>,
    pub metrics: Metrics,
    /// v0.60.0 H7-5: Set to `true` after the first successful PostgreSQL
    /// connection.  Used by the `/ready` Kubernetes readiness probe — the
    /// pod is only added to the load-balancer once this is true.
    pub ever_connected: AtomicBool,
    /// v0.66.0 FLIGHT-01: HMAC-SHA256 secret for Arrow Flight ticket validation.
    /// Read from the `ARROW_FLIGHT_SECRET` environment variable at startup.
    /// `None` means unsigned tickets are accepted (insecure; dev only).
    pub arrow_flight_secret: Option<String>,
    /// v0.67.0 FLIGHT-SEC-01: when `true`, unsigned Arrow Flight tickets are
    /// accepted (local development only). Controlled by the env var
    /// `ARROW_UNSIGNED_TICKETS_ALLOWED=true`. Default `false`.
    pub arrow_unsigned_tickets_allowed: bool,
    /// v0.72.0 FLIGHT-NONCE-01: seen-nonce LRU cache for Arrow Flight replay protection.
    /// Maps nonce string → (accepted_at Instant, expiry_secs u64).
    /// Entries are lazily evicted when the expiry window has elapsed.
    /// Capped at `arrow_nonce_cache_max` entries.
    pub arrow_nonce_cache: DashMap<String, (Instant, u64)>,
    /// Maximum number of nonce entries in the replay-protection cache.
    /// Configurable via `ARROW_NONCE_CACHE_MAX` env var (default: 10000).
    pub arrow_nonce_cache_max: usize,
}

// ─── Configuration ────────────────────────────────────────────────────────────

/// Read an environment variable or fall back to a default.
pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

// ─── Error redaction ──────────────────────────────────────────────────────────

/// Build a redacted error response that hides internal database details from
/// API clients. Logs the full error + trace ID at ERROR level.
pub fn redacted_error(category: &str, detail: &str, status: StatusCode) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    tracing::error!(trace_id = %trace_id, detail = %detail, "query error");
    let body = serde_json::json!({
        "error": category,
        "trace_id": trace_id
    });
    // SAFETY: status and header values are compile-time constants; builder never fails.
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("infallible: hardcoded valid HTTP headers")
}

// ─── Authentication ───────────────────────────────────────────────────────────

/// Check the `Authorization` header against the read token. Returns `Err`
/// with a `401 Unauthorized` response if authentication fails.
#[allow(clippy::result_large_err)]
pub fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    check_token(state.auth_token.as_deref(), headers)
}

/// Check the `Authorization` header against the Datalog write token (if
/// configured) or fall back to the main auth token.
#[allow(clippy::result_large_err)]
pub fn check_auth_write(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let token = state
        .datalog_write_token
        .as_deref()
        .or(state.auth_token.as_deref());
    check_token(token, headers)
}

#[allow(clippy::result_large_err)]
fn check_token(expected: Option<&str>, headers: &HeaderMap) -> Result<(), Response> {
    if let Some(expected) = expected {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // Support "Bearer <token>" and "Basic <token>".
        let token = provided
            .strip_prefix("Bearer ")
            .or_else(|| provided.strip_prefix("Basic "))
            .unwrap_or(provided);
        // Constant-time comparison prevents timing side-channels (v0.22.0 S-4).
        if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
            return Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response());
        }
    }
    Ok(())
}
