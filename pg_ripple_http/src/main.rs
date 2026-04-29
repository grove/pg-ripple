//! pg_ripple_http — SPARQL 1.1 Protocol HTTP endpoint and Datalog REST API
//! for pg_ripple.
//!
//! Standalone Rust binary that connects to PostgreSQL (with pg_ripple installed)
//! and exposes a W3C-compliant SPARQL HTTP endpoint at `/sparql` plus a full
//! Datalog REST API at `/datalog`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::HeaderValue;
use deadpool_postgres::{Config, Runtime};
use tokio_postgres::NoTls;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub mod arrow_encode;
pub mod common;
pub mod datalog;
pub mod metrics;
pub mod routing;
pub mod spi_bridge;
pub mod stream;

use common::{AppState, env_or};

// ─── Compatibility constants (COMPAT-01, v0.71.0) ────────────────────────────

/// The minimum pg_ripple extension version this HTTP companion supports.
///
/// Connections to older extension versions log a prominent warning.  The extension
/// is still served (degraded mode) so that rolling upgrades do not hard-fail.
const COMPATIBLE_EXTENSION_MIN: &str = "0.73.0";

/// Check that the installed pg_ripple extension version is within the known-compatible
/// range for this pg_ripple_http build.  Logs a warning if it is not; does NOT exit.
async fn check_extension_compatibility(client: &deadpool_postgres::Object) {
    if std::env::var("PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        tracing::debug!(
            "PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1: skipping extension compatibility check"
        );
        return;
    }

    let ext_version = match client
        .query_opt(
            "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple'",
            &[],
        )
        .await
    {
        Ok(Some(row)) => row.get::<_, String>(0),
        Ok(None) => {
            tracing::warn!(
                "pg_ripple extension not found in pg_extension catalog — \
                 compatibility check skipped"
            );
            return;
        }
        Err(e) => {
            tracing::warn!("could not query pg_ripple extension version: {e}");
            return;
        }
    };

    tracing::info!(
        ext_version = %ext_version,
        min_supported = %COMPATIBLE_EXTENSION_MIN,
        "pg_ripple extension compatibility check"
    );

    if semver_lt(&ext_version, COMPATIBLE_EXTENSION_MIN) {
        tracing::warn!(
            ext_version = %ext_version,
            min_supported = %COMPATIBLE_EXTENSION_MIN,
            "pg_ripple extension version is below the minimum supported by this pg_ripple_http \
             build — some features may not work correctly. \
             Upgrade the extension with: ALTER EXTENSION pg_ripple UPDATE; \
             or set PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1 to suppress this warning."
        );
    }
}

/// Returns `true` when `version` < `min` using simple major.minor.patch comparison.
/// Falls back to `false` (no warning) if either string cannot be parsed.
fn semver_lt(version: &str, min: &str) -> bool {
    parse_semver(version)
        .zip(parse_semver(min))
        .map(|(v, m)| v < m)
        .unwrap_or(false)
}

/// Parse a "major.minor.patch" string into a comparable tuple.
fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.splitn(3, '.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next()?.split('-').next()?.parse::<u32>().ok()?;
    Some((major, minor, patch))
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "pg_ripple_http=info".parse().unwrap_or_else(|e| {
                    eprintln!("error parsing log filter: {e}");
                    std::process::exit(1);
                })
            }),
        )
        .init();

    // Accept database URL from command-line argument (first positional arg) or environment variable
    let pg_url = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            args[1].clone()
        } else {
            env_or("PG_RIPPLE_HTTP_PG_URL", "postgresql://localhost/postgres")
        }
    };
    let port: u16 = match env_or("PG_RIPPLE_HTTP_PORT", "7878").parse() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("PG_RIPPLE_HTTP_PORT must be a valid port number: {e}");
            std::process::exit(1);
        }
    };
    let pool_size: usize = match env_or("PG_RIPPLE_HTTP_POOL_SIZE", "16").parse() {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("PG_RIPPLE_HTTP_POOL_SIZE must be a positive integer: {e}");
            std::process::exit(1);
        }
    };
    let auth_token = std::env::var("PG_RIPPLE_HTTP_AUTH_TOKEN").ok();
    let datalog_write_token = std::env::var("PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN").ok();
    let rate_limit: u32 = match env_or("PG_RIPPLE_HTTP_RATE_LIMIT", "0").parse() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("PG_RIPPLE_HTTP_RATE_LIMIT must be a non-negative integer: {e}");
            std::process::exit(1);
        }
    };
    // CORS origins — empty string means no cross-origin access; "*" requires explicit opt-in.
    let cors_origins = env_or("PG_RIPPLE_HTTP_CORS_ORIGINS", "");
    // Body limit — default 10 MiB.
    let max_body_bytes: usize = match env_or("PG_RIPPLE_HTTP_MAX_BODY_BYTES", "10485760").parse() {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("PG_RIPPLE_HTTP_MAX_BODY_BYTES must be a positive integer: {e}");
            std::process::exit(1);
        }
    };
    // Trust proxy: comma-separated list of upstream IP/CIDR values trusted for X-Forwarded-For.
    let trust_proxy = std::env::var("PG_RIPPLE_HTTP_TRUST_PROXY").ok();

    // ── v0.46.0: CA-bundle for outbound TLS (PG_RIPPLE_HTTP_CA_BUNDLE) ───────
    // If set, load the PEM file at the given path as the trust anchor for all
    // outbound TLS connections (SERVICE federation, SPARQL endpoint queries).
    // Falls back to the system trust store on error; never silently ignores.
    if let Ok(ca_path) = std::env::var("PG_RIPPLE_HTTP_CA_BUNDLE") {
        match std::fs::read_to_string(&ca_path) {
            Ok(pem) if !pem.trim().is_empty() && pem.contains("BEGIN CERTIFICATE") => {
                tracing::info!("PG_RIPPLE_HTTP_CA_BUNDLE: loaded CA bundle from {ca_path}");
                // Store as a thread-local so outbound HTTP clients can access it.
                // Actual TLS configuration is applied when building reqwest clients
                // inside federation handlers.
                // SAFETY: called once during single-threaded startup before any
                // worker threads are spawned, so no concurrent reads of the env.
                unsafe { std::env::set_var("PG_RIPPLE_HTTP_CA_PEM", pem) };
            }
            Ok(_) => {
                tracing::error!(
                    "PG_RIPPLE_HTTP_CA_BUNDLE: file at '{ca_path}' is not a valid PEM bundle \
                     (no 'BEGIN CERTIFICATE' marker) — falling back to system trust store"
                );
            }
            Err(e) => {
                tracing::error!(
                    "PG_RIPPLE_HTTP_CA_BUNDLE: cannot read '{ca_path}': {e} \
                     — falling back to system trust store"
                );
            }
        }
    }

    // ── v0.51.0: TLS certificate-fingerprint pinning ─────────────────────────
    // PG_RIPPLE_HTTP_PIN_FINGERPRINTS: comma-separated SHA-256 hex fingerprints
    // of trusted TLS server certificates.  When set, any outbound TLS connection
    // (federation proxying, future /sparql/stream upstream calls) is rejected if
    // the peer certificate fingerprint is not in this list.  Stored in the env so
    // downstream client builders can pick it up without a separate config channel.
    if let Ok(fps) = std::env::var("PG_RIPPLE_HTTP_PIN_FINGERPRINTS") {
        let count = fps.split(',').filter(|s| !s.trim().is_empty()).count();
        if count == 0 {
            tracing::warn!(
                "PG_RIPPLE_HTTP_PIN_FINGERPRINTS is set but contains no valid fingerprints \
                 — pinning is disabled"
            );
        } else {
            tracing::info!(
                "PG_RIPPLE_HTTP_PIN_FINGERPRINTS: {count} pinned certificate fingerprint(s) loaded"
            );
        }
    }

    // Build connection pool.
    let mut cfg = Config::new();
    cfg.url = Some(pg_url.clone());
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size));

    let pool = match cfg.create_pool(Some(Runtime::Tokio1), NoTls) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("failed to create PostgreSQL connection pool: {e}");
            std::process::exit(1);
        }
    };

    // Verify connectivity.
    {
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    "failed to connect to PostgreSQL — check PG_RIPPLE_HTTP_PG_URL: {e}"
                );
                std::process::exit(1);
            }
        };
        let row = match client
            .query_one("SELECT pg_ripple.triple_count()", &[])
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("pg_ripple extension not available — is it installed? ({e})");
                std::process::exit(1);
            }
        };
        let count: i64 = row.get(0);
        tracing::info!(
            "connected to {pg_url} (port {port}), triple store contains {count} triples"
        );

        // COMPAT-01 (v0.71.0): verify that the installed pg_ripple extension is within
        // the compatible range for this pg_ripple_http build.
        check_extension_compatibility(&client).await;
    }

    // rate_limit is consumed by the governor layer below; not stored in AppState.
    let state = Arc::new(AppState {
        pool,
        auth_token,
        datalog_write_token,
        trust_proxy,
        metrics: metrics::Metrics::new(),
        ever_connected: std::sync::atomic::AtomicBool::new(false),
        arrow_flight_secret: std::env::var("ARROW_FLIGHT_SECRET").ok(),
        // FLIGHT-SEC-01: unsigned tickets allowed only in dev mode.
        arrow_unsigned_tickets_allowed: std::env::var("ARROW_UNSIGNED_TICKETS_ALLOWED")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false),
        // FLIGHT-NONCE-01 (v0.72.0): nonce replay protection cache.
        arrow_nonce_cache: dashmap::DashMap::new(),
        arrow_nonce_cache_max: std::env::var("ARROW_NONCE_CACHE_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10_000),
    });

    // CORS layer — wildcard "*" requires explicit opt-in; empty means deny all cross-origin.
    let cors = if cors_origins == "*" {
        tracing::warn!(
            "CORS is permissive (*). Set PG_RIPPLE_HTTP_CORS_ORIGINS to a comma-separated list of allowed origins for production use."
        );
        CorsLayer::permissive()
    } else if cors_origins.is_empty() {
        // No cross-origin access.
        CorsLayer::new()
    } else {
        let origins: Vec<HeaderValue> = cors_origins
            .split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect();
        CorsLayer::new().allow_origin(AllowOrigin::list(origins))
    };

    // Build the rate-limiting layer (governor) if a rate limit is configured.
    // governor operates per source IP; 0 means unlimited.
    let mut app = routing::build_router(state, max_body_bytes, cors);

    if rate_limit > 0 {
        let governor_conf = match GovernorConfigBuilder::default()
            .per_second(rate_limit as u64)
            .burst_size(rate_limit)
            .finish()
        {
            Some(c) => c,
            None => {
                tracing::error!("invalid governor rate-limit configuration");
                std::process::exit(1);
            }
        };
        app = app.layer(GovernorLayer::new(Arc::new(governor_conf)));
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("pg_ripple_http listening on http://{addr}");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind TCP listener on {addr}: {e}");
            std::process::exit(1);
        }
    };
    // Pass ConnectInfo for per-IP rate limiting.
    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}
