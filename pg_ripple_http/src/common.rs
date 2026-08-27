//! Shared application state and helper functions used by both SPARQL and
//! Datalog handlers.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
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
    /// S13-03 (v0.86.0): whether the CORS wildcard-origin policy (*) is active.
    /// When `true`, every request increments `cors_permissive_requests_total`.
    pub cors_is_permissive: bool,
    /// M16-22 (v0.115.0): optional bearer token that protects `GET /metrics`.
    /// When `Some`, the metrics endpoint requires `Authorization: Bearer <token>`.
    /// Uses constant-time comparison to prevent timing side-channels.
    pub metrics_token: Option<String>,
    /// L16-06 (v0.117.0): `Bearer realm=` value used in `WWW-Authenticate` response header.
    /// Read from `PG_RIPPLE_HTTP_AUTH_REALM` at startup; defaults to `"pg_ripple"`.
    pub auth_realm: String,
    /// Feature 12 (v0.120.0): Optional read-replica connection pool.
    /// When `Some`, read-only SPARQL SELECT/CONSTRUCT/ASK requests with
    /// `?replica=ok` are routed to this pool instead of the primary.
    /// Configured via `PG_RIPPLE_HTTP_REPLICA_DSN` environment variable.
    pub replica_pool: Option<Pool>,
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
// A16-CQ: result_large_err expected — error type is inherently large due to pgrx Response payload.
#[allow(clippy::result_large_err)]
pub fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    check_token(state.auth_token.as_deref(), headers, &state.auth_realm)
}

/// Check the `Authorization` header against the Datalog write token (if
/// configured) or fall back to the main auth token.
// A16-CQ: result_large_err expected — error type is inherently large due to pgrx Response payload.
#[allow(clippy::result_large_err)]
pub fn check_auth_write(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let token = state
        .datalog_write_token
        .as_deref()
        .or(state.auth_token.as_deref());
    check_token(token, headers, &state.auth_realm)
}

// A16-CQ: result_large_err expected — error type is inherently large due to pgrx Response payload.
#[allow(clippy::result_large_err)]
fn check_token(expected: Option<&str>, headers: &HeaderMap, realm: &str) -> Result<(), Response> {
    check_token_with_policy(expected, headers, realm, unauthenticated_allowed())
}

fn unauthenticated_allowed() -> bool {
    std::env::var("PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn check_token_with_policy(
    expected: Option<&str>,
    headers: &HeaderMap,
    realm: &str,
    allow_unauthenticated: bool,
) -> Result<(), Response> {
    let Some(expected) = expected else {
        if allow_unauthenticated {
            return Ok(());
        }
        return unauthorized_response(realm);
    };

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
        return unauthorized_response(realm);
    }
    Ok(())
}

fn unauthorized_response(realm: &str) -> Result<(), Response> {
    // HTTP-401-WWW-AUTH-01 (v0.83.0): RFC 7235 §4.1 requires WWW-Authenticate
    // on every 401. AUTH-RESP-FMT-01 (v0.83.0): structured JSON response.
    let body = serde_json::json!({"error": "PT401", "message": "unauthorized"}).to_string();
    let www_auth = format!("Bearer realm=\"{realm}\"");
    // SAFETY: status code and header values are compile-time constants; builder never fails.
    Err(Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("www-authenticate", www_auth)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("infallible: hardcoded valid HTTP headers"))
}

/// Format a decoded RDF term as an N-Quads term.
pub fn format_nquads_term(term: &str) -> String {
    if term.starts_with("_:") {
        return term.to_owned();
    }
    if term.starts_with('<') && term.ends_with('>') {
        return format!("<{}>", escape_iri(&term[1..term.len() - 1]));
    }
    if term.starts_with('"') {
        if let Some(end) = literal_end(term) {
            let value = unescape_literal(&term[1..end]);
            let suffix = &term[end + 1..];
            if suffix.is_empty() || suffix.starts_with('@') || suffix.starts_with("^^<") {
                let suffix = suffix
                    .strip_prefix("^^<")
                    .and_then(|dt| dt.strip_suffix('>'))
                    .map(|dt| format!("^^<{}>", escape_iri(dt)))
                    .unwrap_or_else(|| suffix.to_owned());
                return format!("\"{}\"{suffix}", escape_literal(&value));
            }
        }
    }
    if is_iri(term) {
        return format!("<{}>", escape_iri(term));
    }
    format!("\"{}\"", escape_literal(term))
}

fn is_iri(term: &str) -> bool {
    term.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.chars().enumerate().all(|(i, c)| {
                c.is_ascii_alphabetic() && i == 0
                    || i > 0 && (c.is_ascii_alphanumeric() || "+-.".contains(c))
            })
    })
}

fn literal_end(term: &str) -> Option<usize> {
    let bytes = term.as_bytes();
    (1..bytes.len()).find(|&i| bytes[i] == b'"' && !is_escaped(bytes, i))
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut slashes = 0;
    let mut i = index;
    while i > 0 && bytes[i - 1] == b'\\' {
        slashes += 1;
        i -= 1;
    }
    slashes % 2 == 1
}

fn unescape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{0008}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{000c}'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => out.push_str(&decode_hex_escape(&mut chars, 4)),
            Some('U') => out.push_str(&decode_hex_escape(&mut chars, 8)),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn decode_hex_escape(chars: &mut impl Iterator<Item = char>, width: usize) -> String {
    let digits: String = chars.take(width).collect();
    u32::from_str_radix(&digits, 16)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{fffd}')
        .to_string()
}

fn escape_literal(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\t' => "\\t".chars().collect(),
            '\u{0008}' => "\\b".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\u{000c}' => "\\f".chars().collect(),
            c if c.is_control() => format!("\\u{:04X}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

fn escape_iri(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                format!("\\u{:04X}", c as u32).chars().collect()
            }
            c if c.is_control() => format!("\\u{:04X}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn no_token_fails_closed_but_explicit_dev_mode_allows() {
        let headers = HeaderMap::new();
        assert_eq!(
            check_token_with_policy(None, &headers, "pg_ripple", false)
                .unwrap_err()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(check_token_with_policy(None, &headers, "pg_ripple", true).is_ok());
    }

    #[test]
    fn nquads_formatter_handles_terms_and_escaping() {
        assert_eq!(format_nquads_term("http://example/s"), "<http://example/s>");
        assert_eq!(format_nquads_term("http://example/a>b"), "<http://example/a\\u003Eb>");
        assert_eq!(format_nquads_term("_:b1"), "_:b1");
        assert_eq!(format_nquads_term("a\"\\\nb"), "\"a\\\"\\\\\\nb\"");
        assert_eq!(format_nquads_term("\"a\\n b\"@en"), "\"a\\n b\"@en");
        assert_eq!(
            format_nquads_term("\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }
}
