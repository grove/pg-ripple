//! Shared application state and helper functions used by both SPARQL and
//! Datalog handlers.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use constant_time_eq::constant_time_eq;
use dashmap::DashMap;
use deadpool_postgres::Pool;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio_postgres::{CancelToken, NoTls};

/// TLS mode used when sending an out-of-band PostgreSQL query cancellation.
/// It must match the connection's SSL negotiation mode; `NoTls` is not valid
/// for a pool configured with PostgreSQL TLS.
#[derive(Clone)]
pub enum PgCancelTls {
    NoTls,
    Rustls(tokio_postgres_rustls::MakeRustlsConnect),
}

impl PgCancelTls {
    pub async fn cancel(&self, token: &CancelToken) -> Result<(), tokio_postgres::Error> {
        match self {
            Self::NoTls => token.cancel_query(NoTls).await,
            Self::Rustls(connector) => token.cancel_query(connector.clone()).await,
        }
    }
}
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
    /// Optional token for administrative endpoints. When unset, the read
    /// token remains the backwards-compatible administrator credential.
    pub admin_token: Option<String>,
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
    /// TLS connector used for cancellation packets on checked-out streams.
    pub cancel_tls: PgCancelTls,
}

// ─── Configuration ────────────────────────────────────────────────────────────

/// Startup-validated HTTP companion configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpConfig {
    pub bind: SocketAddr,
    pub mode: HttpMode,
    pub pg_url: String,
    pub pg_password: Option<String>,
    pub pool_size: usize,
    pub auth_token: Option<String>,
    pub write_token: Option<String>,
    pub admin_token: Option<String>,
    pub metrics_token: Option<String>,
    pub allow_unauthenticated: bool,
    pub pg_sslmode: PgSslMode,
    pub pg_ca_file: Option<String>,
    pub pg_client_cert_file: Option<String>,
    pub pg_client_key_file: Option<String>,
    pub rate_limit: u32,
    pub cors_origins: String,
    pub max_body_bytes: usize,
    pub trust_proxy: Option<String>,
    pub replica_dsn: Option<String>,
    pub auth_realm: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMode {
    Development,
    Production,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PgSslMode {
    Disable,
    Require,
    VerifyCa,
    VerifyFull,
}

impl HttpConfig {
    /// Load and validate all startup configuration before allocating resources.
    pub fn from_env() -> Result<Self, String> {
        let mode = match env_or("PG_RIPPLE_HTTP_MODE", "production").as_str() {
            "development" => HttpMode::Development,
            "production" => HttpMode::Production,
            value => {
                return Err(format!(
                    "PG_RIPPLE_HTTP_MODE must be development or production, got '{value}'"
                ));
            }
        };
        let allow_unauthenticated = parse_bool("PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED", false)?;
        if mode == HttpMode::Production && allow_unauthenticated {
            return Err(
                "PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1 is not allowed in production mode"
                    .to_owned(),
            );
        }

        let port = parse_value("PG_RIPPLE_HTTP_PORT", "7878")?;
        let bind = match std::env::var("PG_RIPPLE_HTTP_BIND") {
            Ok(value) => value.parse().map_err(|error| {
                format!("PG_RIPPLE_HTTP_BIND must be a socket address: {error}")
            })?,
            Err(_) => SocketAddr::from(([127, 0, 0, 1], port)),
        };
        let pg_url = env_or("PG_RIPPLE_HTTP_PG_URL", "postgresql://localhost/postgres");
        let pg_password = secret("PG_RIPPLE_HTTP_PG_PASSWORD")?;
        validate_database_url(mode, &pg_url, pg_password.as_deref())?;
        let replica_dsn = optional_env("PG_RIPPLE_HTTP_REPLICA_DSN");
        if let Some(replica_dsn) = &replica_dsn {
            validate_database_url(mode, replica_dsn, pg_password.as_deref())?;
        }
        let auth_token = secret("PG_RIPPLE_HTTP_AUTH_TOKEN")?;
        if !allow_unauthenticated && !bind.ip().is_loopback() && auth_token.is_none() {
            return Err("PG_RIPPLE_HTTP_AUTH_TOKEN is required for a non-loopback bind".to_owned());
        }

        let pg_sslmode = match env_or("PG_RIPPLE_HTTP_PG_SSLMODE", "disable").as_str() {
            "disable" => PgSslMode::Disable,
            "require" => PgSslMode::Require,
            "verify-ca" => PgSslMode::VerifyCa,
            "verify-full" => PgSslMode::VerifyFull,
            value => {
                return Err(format!(
                    "PG_RIPPLE_HTTP_PG_SSLMODE must be disable, require, verify-ca, or verify-full, got '{value}'"
                ));
            }
        };
        let pg_ca_file = optional_env("PG_RIPPLE_HTTP_PG_CA_FILE");
        let pg_client_cert_file = optional_env("PG_RIPPLE_HTTP_PG_CLIENT_CERT_FILE");
        let pg_client_key_file = optional_env("PG_RIPPLE_HTTP_PG_CLIENT_KEY_FILE");
        if matches!(pg_sslmode, PgSslMode::VerifyCa | PgSslMode::VerifyFull) && pg_ca_file.is_none()
        {
            return Err(
                "PG_RIPPLE_HTTP_PG_CA_FILE is required for PostgreSQL certificate verification"
                    .to_owned(),
            );
        }
        if pg_client_cert_file.is_some() != pg_client_key_file.is_some() {
            return Err("PG_RIPPLE_HTTP_PG_CLIENT_CERT_FILE and PG_RIPPLE_HTTP_PG_CLIENT_KEY_FILE must be set together".to_owned());
        }

        Ok(Self {
            bind,
            mode,
            pg_url,
            pg_password,
            pool_size: parse_value("PG_RIPPLE_HTTP_POOL_SIZE", "16")?,
            auth_token,
            write_token: secret_with_alias(
                "PG_RIPPLE_HTTP_WRITE_TOKEN",
                "PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN",
            )?,
            admin_token: secret("PG_RIPPLE_HTTP_ADMIN_TOKEN")?,
            metrics_token: secret("PG_RIPPLE_HTTP_METRICS_TOKEN")?,
            allow_unauthenticated,
            pg_sslmode,
            pg_ca_file,
            pg_client_cert_file,
            pg_client_key_file,
            rate_limit: parse_value("PG_RIPPLE_HTTP_RATE_LIMIT", "100")?,
            cors_origins: env_or("PG_RIPPLE_HTTP_CORS_ORIGINS", ""),
            max_body_bytes: parse_value("PG_RIPPLE_HTTP_MAX_BODY_BYTES", "10485760")?,
            trust_proxy: optional_env("PG_RIPPLE_HTTP_TRUST_PROXY"),
            replica_dsn,
            auth_realm: env_or("PG_RIPPLE_HTTP_AUTH_REALM", "pg_ripple"),
        })
    }

    /// Validate the effective database URL, including a command-line override.
    pub fn validate_database_url(&self, pg_url: &str) -> Result<(), String> {
        validate_database_url(self.mode, pg_url, self.pg_password.as_deref())
    }
}

fn validate_database_url(
    mode: HttpMode,
    pg_url: &str,
    pg_password: Option<&str>,
) -> Result<(), String> {
    if mode == HttpMode::Production
        && !dsn_has_password(pg_url)
        && pg_password.is_none()
        && dsn_host(pg_url).is_some_and(|host| !is_loopback_host(&host))
    {
        return Err(
            "PG_RIPPLE_HTTP_PG_URL must include a password or PG_RIPPLE_HTTP_PG_PASSWORD[_FILE] for a remote production database"
                .to_owned(),
        );
    }
    Ok(())
}

fn dsn_host(dsn: &str) -> Option<String> {
    let authority = dsn.split_once("://")?.1.split('/').next()?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, value)| value);
    if host_port.starts_with('[') {
        host_port
            .split_once(']')
            .map(|(host, _)| host.trim_start_matches('[').to_owned())
    } else {
        Some(host_port.split(':').next()?.to_owned())
    }
}

fn dsn_has_password(dsn: &str) -> bool {
    dsn.split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .and_then(|authority| authority.rsplit_once('@').map(|(userinfo, _)| userinfo))
        .and_then(|userinfo| userinfo.split_once(':').map(|(_, password)| password))
        .is_some_and(|password| !password.is_empty())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn parse_value<T: std::str::FromStr>(name: &str, default: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    env_or(name, default)
        .parse()
        .map_err(|error| format!("{name} has an invalid value: {error}"))
}

fn parse_bool(name: &str, default: bool) -> Result<bool, String> {
    match std::env::var(name) {
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => Ok(true),
        Ok(value) if value == "0" || value.eq_ignore_ascii_case("false") => Ok(false),
        Ok(value) => Err(format!(
            "{name} must be 0, 1, true, or false, got '{value}'"
        )),
        Err(_) => Ok(default),
    }
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn secret(name: &str) -> Result<Option<String>, String> {
    secret_with_alias(name, "")
}

fn secret_with_alias(name: &str, alias: &str) -> Result<Option<String>, String> {
    let file_name = format!("{name}_FILE");
    if let Ok(path) = std::env::var(&file_name) {
        let value = std::fs::read_to_string(&path)
            .map_err(|error| format!("{file_name} could not read '{path}': {error}"))?
            .trim()
            .to_owned();
        if value.is_empty() {
            return Err(format!("{file_name} points to an empty secret"));
        }
        return Ok(Some(value));
    }
    if !alias.is_empty() {
        let alias_file = format!("{alias}_FILE");
        if let Ok(path) = std::env::var(&alias_file) {
            let value = std::fs::read_to_string(&path)
                .map_err(|error| format!("{alias_file} could not read '{path}': {error}"))?
                .trim()
                .to_owned();
            if value.is_empty() {
                return Err(format!("{alias_file} points to an empty secret"));
            }
            return Ok(Some(value));
        }
        return Ok(std::env::var(name)
            .ok()
            .or_else(|| {
                (!alias.is_empty())
                    .then(|| std::env::var(alias).ok())
                    .flatten()
            })
            .filter(|value| !value.is_empty()));
    }

    Ok(std::env::var(name)
        .ok()
        .or_else(|| {
            (!alias.is_empty())
                .then(|| std::env::var(alias).ok())
                .flatten()
        })
        .filter(|value| !value.is_empty()))
}

/// Read an environment variable or fall back to a default.
pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

// ─── Error redaction ──────────────────────────────────────────────────────────

/// Build a redacted error response that hides internal database details from
/// API clients. Logs the full error + trace ID at ERROR level.
pub fn redacted_error(category: &str, detail: &str, status: StatusCode) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    let safe_detail = redact_sensitive(detail);
    tracing::error!(trace_id = %trace_id, detail = %safe_detail, "query error");
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

/// Remove credentials and bearer values from diagnostic text before logging.
/// This intentionally handles the common DSN and HTTP error formats without a
/// regex dependency; callers still return only the trace ID to clients.
pub fn redact_sensitive(input: &str) -> String {
    let mut output = input.to_owned();

    for marker in ["postgresql://", "postgres://"] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(marker) {
            let start = search_from + relative + marker.len();
            let end = output[start..].find('@').map(|offset| start + offset);
            if let Some(end) = end {
                let authority = &output[start..end];
                if authority.contains(':') {
                    output.replace_range(start..end, "***");
                    search_from = start + 3;
                } else {
                    search_from = end + 1;
                }
            } else {
                break;
            }
        }
    }

    for marker in [
        "password=",
        "passwd=",
        "token=",
        "api_key=",
        "access_token=",
    ] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(marker) {
            let start = search_from + relative + marker.len();
            let end = output[start..]
                .find(['&', ' ', '\n', '\r', ';', ','])
                .map(|offset| start + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "***");
            search_from = start + 3;
        }
    }

    for marker in ["Authorization: Bearer ", "authorization: Bearer "] {
        if let Some(start) = output.find(marker) {
            let value_start = start + marker.len();
            let end = output[value_start..]
                .find(|c: char| c.is_whitespace())
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..end, "***");
        }
    }

    output
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

/// Check the administrative bearer token. A configured admin token is kept
/// separate from the read and write credentials; without one, the main token
/// remains the compatible administrator credential.
#[allow(clippy::result_large_err)]
pub fn check_auth_admin(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let token = state.admin_token.as_deref().or(state.auth_token.as_deref());
    check_token(token, headers, &state.auth_realm)
}

/// Check the optional metrics bearer token. Metrics remain public when no
/// metrics token is configured so existing Prometheus deployments continue to
/// work, while a configured token is enforced centrally and by the handler.
#[allow(clippy::result_large_err)]
pub fn check_auth_metrics(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    match state.metrics_token.as_deref() {
        Some(token) => check_token(Some(token), headers, &state.auth_realm),
        None => Ok(()),
    }
}

// A16-CQ: result_large_err expected — error type is inherently large due to pgrx Response payload.
#[allow(clippy::result_large_err)]
fn check_token(expected: Option<&str>, headers: &HeaderMap, realm: &str) -> Result<(), Response> {
    check_token_with_policy(expected, headers, realm, unauthenticated_allowed())
}

fn unauthenticated_allowed() -> bool {
    std::env::var("PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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
    if term.starts_with('"')
        && let Some(end) = literal_end(term)
    {
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
        assert_eq!(
            format_nquads_term("http://example/a>b"),
            "<http://example/a\\u003Eb>"
        );
        assert_eq!(format_nquads_term("_:b1"), "_:b1");
        assert_eq!(format_nquads_term("a\"\\\nb"), "\"a\\\"\\\\\\nb\"");
        assert_eq!(format_nquads_term("\"a\\n b\"@en"), "\"a\\n b\"@en");
        assert_eq!(
            format_nquads_term("\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[test]
    fn sensitive_details_are_redacted() {
        let redacted = redact_sensitive(
            "postgresql://app:secret@example.test/db?password=secret&sslmode=require Authorization: Bearer token",
        );
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("Bearer token"));
        assert!(redacted.contains("postgresql://***@example.test"));
    }

    #[test]
    fn production_database_password_check_distinguishes_local_and_remote() {
        assert!(dsn_has_password("postgresql://app:secret@db.example/graph"));
        assert!(!dsn_has_password("postgresql://app@db.example/graph"));
        assert_eq!(
            dsn_host("postgresql://app@db.example:5432/graph").as_deref(),
            Some("db.example")
        );
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(!is_loopback_host("db.example"));
    }
}
