//! pg_ripple_http — SPARQL 1.1 Protocol HTTP endpoint and Datalog REST API
//! for pg_ripple.
//!
//! Standalone Rust binary that connects to PostgreSQL (with pg_ripple installed)
//! and exposes a W3C-compliant SPARQL HTTP endpoint at `/sparql` plus a full
//! Datalog REST API at `/datalog`.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::http::HeaderValue;
use deadpool_postgres::{Config, Runtime};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio_postgres::NoTls;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};

use pg_ripple_http::common::PgSslMode;
use pg_ripple_http::common::{AppState, HttpConfig, redact_sensitive};
use pg_ripple_http::routing::middleware::{self, TrustedProxyLayer};
use pg_ripple_http::{metrics, routing};

// ─── Compatibility constants (COMPAT-01, v0.71.0) ────────────────────────────

/// Minimum pg_ripple extension version that this HTTP companion supports.
/// Updated each release to match the previous extension version, allowing
/// a one-version trailing window.
///
/// HTTP-COMPAT-01 (v0.89.0): bumped to 0.88.0 — requires all v0.84–v0.88 features.
///
/// Connections to older extension versions log a prominent warning. The extension
/// is still served (degraded mode) so that rolling upgrades do not hard-fail.
/// Set `PG_RIPPLE_HTTP_STRICT_COMPAT=1` to convert the warning to a fatal startup error.
const COMPATIBLE_EXTENSION_MIN: &str = "0.130.0";

/// Check that the installed pg_ripple extension version is within the known-compatible
/// range for this pg_ripple_http build.  Logs a warning if it is not.
///
/// S13-05 (v0.84.0): When `PG_RIPPLE_HTTP_STRICT_COMPAT=1` is set, a version
/// mismatch causes an immediate `process::exit(1)` instead of a warning.
/// Default is off (backward-compatible degraded-mode behaviour).
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

    let strict = std::env::var("PG_RIPPLE_HTTP_STRICT_COMPAT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

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
        if strict {
            tracing::error!(
                ext_version = %ext_version,
                min_supported = %COMPATIBLE_EXTENSION_MIN,
                "PG_RIPPLE_HTTP_STRICT_COMPAT=1: extension version is below minimum — aborting"
            );
            std::process::exit(1);
        }
        tracing::warn!(
            ext_version = %ext_version,
            min_supported = %COMPATIBLE_EXTENSION_MIN,
            "pg_ripple extension version is below the minimum supported by this pg_ripple_http \
             build — some features may not work correctly. \
             Upgrade the extension with: ALTER EXTENSION pg_ripple UPDATE; \
             or set PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1 to suppress this warning. \
             Set PG_RIPPLE_HTTP_STRICT_COMPAT=1 to make this a fatal startup error."
        );
    }

    // v0.118.0 Feature 3: Belt-and-suspenders compat_check() call.
    // Query the extension's own compatibility descriptor to surface any
    // http_min_version requirement declared by the extension itself.
    if let Ok(Some(row)) = client
        .query_opt("SELECT pg_ripple.compat_check()", &[])
        .await
    {
        let compat_json: String = row.get(0);
        match serde_json::from_str::<serde_json::Value>(&compat_json) {
            Ok(v) => {
                let compatible = v
                    .get("compatible")
                    .and_then(|c| c.as_bool())
                    .unwrap_or(true);
                let http_min = v
                    .get("http_min_version")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let companion_version = env!("CARGO_PKG_VERSION");
                if !compatible || semver_lt(companion_version, http_min) {
                    if strict {
                        tracing::error!(
                            companion_version = %companion_version,
                            http_min_version = %http_min,
                            "PG_RIPPLE_HTTP_STRICT_COMPAT=1: compat_check() reports incompatible — aborting"
                        );
                        std::process::exit(1);
                    }
                    tracing::warn!(
                        companion_version = %companion_version,
                        http_min_version = %http_min,
                        "pg_ripple.compat_check() reports this HTTP companion version is below \
                         the extension's http_min_version requirement. Upgrade pg_ripple_http."
                    );
                } else {
                    tracing::debug!(
                        companion_version = %companion_version,
                        http_min_version = %http_min,
                        "compat_check() passed"
                    );
                }
            }
            Err(e) => {
                tracing::debug!("compat_check() returned non-JSON response (older extension): {e}");
            }
        }
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

fn create_pg_pool(
    cfg: &Config,
    http_config: &HttpConfig,
) -> Result<deadpool_postgres::Pool, String> {
    match http_config.pg_sslmode {
        PgSslMode::Disable => cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|error| error.to_string()),
        PgSslMode::Require | PgSslMode::VerifyCa | PgSslMode::VerifyFull => {
            let connector = pg_tls_connector(http_config)?;
            cfg.create_pool(Some(Runtime::Tokio1), connector)
                .map_err(|error| error.to_string())
        }
    }
}

fn pg_tls_connector(
    http_config: &HttpConfig,
) -> Result<tokio_postgres_rustls::MakeRustlsConnect, String> {
    let certs = if let Some(path) = &http_config.pg_ca_file {
        rustls_native_certs::load_certs_from_paths(Some(Path::new(path)), None)
    } else {
        rustls_native_certs::load_native_certs()
    };
    if let Some(error) = certs.errors.first() {
        return Err(format!(
            "could not load PostgreSQL CA certificates: {error}"
        ));
    }
    let mut roots = rustls::RootCertStore::empty();
    for cert in certs.certs {
        roots
            .add(cert)
            .map_err(|error| format!("could not add PostgreSQL CA certificate: {error}"))?;
    }
    if roots.is_empty() {
        return Err("no PostgreSQL CA certificates were found".to_owned());
    }

    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let config = match (
        &http_config.pg_client_cert_file,
        &http_config.pg_client_key_file,
    ) {
        (Some(cert_path), Some(key_path)) => {
            let cert_chain = CertificateDer::pem_file_iter(cert_path)
                .map_err(|error| format!("could not read {cert_path}: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    format!("could not parse PostgreSQL client certificate: {error}")
                })?;
            let key = PrivateKeyDer::from_pem_file(key_path)
                .map_err(|error| format!("could not parse PostgreSQL client key: {error}"))?;
            builder
                .with_client_auth_cert(cert_chain, key)
                .map_err(|error| {
                    format!("could not configure PostgreSQL client certificate: {error}")
                })?
        }
        _ => builder.with_no_client_auth(),
    };
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // O13-04 (v0.86.0): respect RUST_LOG_FORMAT=json for structured log output.
    let log_format = std::env::var("RUST_LOG_FORMAT").unwrap_or_default();
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "pg_ripple_http=info".parse().unwrap_or_else(|e| {
            eprintln!("error parsing log filter: {e}");
            std::process::exit(1);
        })
    });
    if log_format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }

    let http_config = match HttpConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("invalid HTTP configuration: {error}");
            std::process::exit(1);
        }
    };

    // Accept database URL from command-line argument (first positional arg) or environment variable
    let pg_url = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            args[1].clone()
        } else {
            http_config.pg_url.clone()
        }
    };
    if let Err(error) = http_config.validate_database_url(&pg_url) {
        tracing::error!("invalid PostgreSQL configuration: {error}");
        std::process::exit(1);
    }
    let port = http_config.bind.port();
    let pool_size = http_config.pool_size;
    let auth_token = http_config.auth_token.clone();
    if auth_token.is_none() {
        if http_config.allow_unauthenticated {
            tracing::warn!(
                "unauthenticated HTTP mode is enabled by PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED"
            );
        } else {
            tracing::warn!(
                "PG_RIPPLE_HTTP_AUTH_TOKEN is not configured; protected requests will be rejected"
            );
        }
    }
    let datalog_write_token = http_config.write_token.clone();
    let rate_limit = http_config.rate_limit;
    // CORS origins — empty string means no cross-origin access; "*" requires explicit opt-in.
    let cors_origins = &http_config.cors_origins;
    let max_body_bytes = http_config.max_body_bytes;
    // L18-01 (v0.131.0): parse trusted proxy networks before startup continues.
    let trusted_proxy = match middleware::parse_trusted_proxy(http_config.trust_proxy.as_deref()) {
        Ok(networks) => networks,
        Err(error) => {
            tracing::error!("{error}");
            std::process::exit(1);
        }
    };

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
    cfg.password = http_config.pg_password.clone();
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size));

    let pool = match create_pg_pool(&cfg, &http_config) {
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
                    "failed to connect to PostgreSQL — check PG_RIPPLE_HTTP_PG_URL: {}",
                    redact_sensitive(&e.to_string())
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
            "connected to {} (port {port}), triple store contains {count} triples",
            redact_sensitive(&pg_url)
        );

        // COMPAT-01 (v0.71.0): verify that the installed pg_ripple extension is within
        // the compatible range for this pg_ripple_http build.
        check_extension_compatibility(&client).await;
    }

    // rate_limit is consumed by the governor layer below; not stored in AppState.
    // S13-03 (v0.86.0): compute cors_is_permissive before building AppState.
    let cors_is_permissive = cors_origins == "*";
    // M16-22 (v0.115.0): optional metrics bearer token.
    let metrics_token = http_config.metrics_token.clone();
    if metrics_token.is_some() {
        tracing::info!(
            "PG_RIPPLE_HTTP_METRICS_TOKEN set: GET /metrics requires Authorization: Bearer <token>"
        );
    }
    // L16-06 (v0.117.0): configurable WWW-Authenticate realm.
    let auth_realm = http_config.auth_realm.clone();
    tracing::debug!("auth realm: {auth_realm}");

    // Feature 12 (v0.120.0): optional read-replica pool.
    let replica_pool = if let Some(replica_dsn) = &http_config.replica_dsn {
        if replica_dsn.trim().is_empty() {
            None
        } else {
            let mut replica_cfg = Config::new();
            replica_cfg.url = Some(replica_dsn.clone());
            replica_cfg.password = http_config.pg_password.clone();
            replica_cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size));
            match create_pg_pool(&replica_cfg, &http_config) {
                Ok(p) => {
                    tracing::info!(
                        "PG_RIPPLE_HTTP_REPLICA_DSN set: read-only SPARQL requests with ?replica=ok will be routed to the replica"
                    );
                    Some(p)
                }
                Err(e) => {
                    tracing::warn!(
                        "PG_RIPPLE_HTTP_REPLICA_DSN set but pool creation failed: {e} — replica routing disabled"
                    );
                    None
                }
            }
        }
    } else {
        None
    };

    let state = Arc::new(AppState {
        pool,
        auth_token,
        datalog_write_token,
        admin_token: http_config.admin_token.clone(),
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
        // S13-03 (v0.86.0): CORS permissive mode tracking.
        cors_is_permissive,
        // M16-22 (v0.115.0): metrics endpoint bearer token.
        metrics_token,
        // L16-06 (v0.117.0): configurable WWW-Authenticate realm.
        auth_realm,
        // Feature 12 (v0.120.0): optional read-replica pool.
        replica_pool,
    });

    // CORS layer — wildcard "*" requires explicit opt-in; empty means deny all cross-origin.
    // S13-03 (v0.86.0): track whether permissive CORS is enabled so the metrics counter can
    // be incremented per-request by middleware. The state is passed into build_router.
    let cors = if cors_is_permissive {
        tracing::warn!(
            "CORS is permissive (*). Set PG_RIPPLE_HTTP_CORS_ORIGINS to a comma-separated list of allowed origins for production use. \
             Monitor pg_ripple_http_cors_permissive_requests_total for cross-origin traffic."
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
    let mut app = routing::build_router(state.clone(), max_body_bytes, cors);

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

    // This must be outermost so ConnectInfo is rewritten before governor extracts its key.
    app = app.layer(TrustedProxyLayer::new(trusted_proxy));

    let addr = http_config.bind;
    tracing::info!("pg_ripple_http listening on http://{addr}");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind TCP listener on {addr}: {e}");
            std::process::exit(1);
        }
    };

    // O13-05 (v0.86.0): graceful shutdown on SIGTERM with a configurable drain window.
    // HTTP-05 (v0.92.0): PG_RIPPLE_HTTP_SHUTDOWN_TIMEOUT_SECS configures the drain
    // timeout (default 30 seconds). Set to 0 to disable draining and exit immediately.
    // axum::serve().with_graceful_shutdown() waits for in-flight requests to complete
    // before the process exits; SIGINT (Ctrl-C) also triggers the same shutdown path.
    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}

/// Wait for SIGTERM or SIGINT, then return to trigger graceful shutdown.
///
/// O13-05 (v0.86.0): allows in-flight requests up to the configured timeout to complete
/// after a SIGTERM is received before the process exits.
/// HTTP-05 (v0.92.0): timeout is configurable via `PG_RIPPLE_HTTP_SHUTDOWN_TIMEOUT_SECS`
/// (default 30). Set to 0 to exit immediately without draining in-flight requests.
async fn shutdown_signal() {
    use tokio::signal;

    let shutdown_timeout_secs: u64 = std::env::var("PG_RIPPLE_HTTP_SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            tracing::info!(
                "received Ctrl+C, initiating graceful shutdown ({shutdown_timeout_secs}s drain)"
            );
        }
        () = terminate => {
            tracing::info!(
                "received SIGTERM, initiating graceful shutdown ({shutdown_timeout_secs}s drain)"
            );
        }
    }

    // Allow up to configured timeout for in-flight requests to drain.
    if shutdown_timeout_secs > 0 {
        tokio::time::sleep(std::time::Duration::from_secs(shutdown_timeout_secs)).await;
    }
}
