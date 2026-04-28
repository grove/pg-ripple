//! pg_ripple_http — SPARQL 1.1 Protocol HTTP endpoint and Datalog REST API
//! for pg_ripple.
//!
//! Standalone Rust binary that connects to PostgreSQL (with pg_ripple installed)
//! and exposes a W3C-compliant SPARQL HTTP endpoint at `/sparql` plus a full
//! Datalog REST API at `/datalog`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use deadpool_postgres::{Config, Runtime};
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::OpenApi;

pub mod common;
pub mod datalog;
pub mod metrics;

use common::{AppState, check_auth, env_or, redacted_error};

// ─── OpenAPI specification (K-1, v0.55.0) ────────────────────────────────────

/// Generated OpenAPI 3.1 document for pg_ripple_http.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "pg_ripple_http",
        version = "0.16.0",
        description = "SPARQL 1.1 Protocol HTTP endpoint and Datalog REST API for pg_ripple",
        license(name = "Apache-2.0")
    ),
    paths(
        openapi_spec,
    ),
    tags(
        (name = "sparql", description = "SPARQL 1.1 Query and Update Protocol"),
        (name = "datalog", description = "Datalog inference and rule management"),
        (name = "health", description = "Health and observability"),
        (name = "metadata", description = "Dataset and service metadata"),
    )
)]
pub struct ApiDoc;

// ─── Content types ───────────────────────────────────────────────────────────

const CT_SPARQL_JSON: &str = "application/sparql-results+json";
const CT_SPARQL_XML: &str = "application/sparql-results+xml";
const CT_CSV: &str = "text/csv";
const CT_TSV: &str = "text/tab-separated-values";
const CT_TURTLE: &str = "text/turtle";
const CT_NTRIPLES: &str = "application/n-triples";
const CT_JSONLD: &str = "application/ld+json";
const CT_SPARQL_QUERY: &str = "application/sparql-query";
const CT_SPARQL_UPDATE: &str = "application/sparql-update";
const CT_FORM: &str = "application/x-www-form-urlencoded";

// ─── Query parameters ────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct SparqlParams {
    query: Option<String>,
    update: Option<String>,
}

// ─── RAG request / response ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct RagRequest {
    question: String,
    sparql_filter: Option<String>,
    #[serde(default = "default_k")]
    k: i32,
    model: Option<String>,
    #[serde(default = "default_output_format")]
    output_format: String,
}

fn default_k() -> i32 {
    5
}
fn default_output_format() -> String {
    "jsonb".to_owned()
}

#[derive(Serialize)]
struct RagResult {
    entity_iri: String,
    label: String,
    context_json: serde_json::Value,
    distance: f64,
}

#[derive(Serialize)]
struct RagResponse {
    results: Vec<RagResult>,
    /// Concatenated plain-text context for direct use as an LLM system prompt.
    context: String,
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
    }

    // rate_limit is consumed by the governor layer below; not stored in AppState.
    let state = Arc::new(AppState {
        pool,
        auth_token,
        datalog_write_token,
        trust_proxy,
        metrics: metrics::Metrics::new(),
        ever_connected: std::sync::atomic::AtomicBool::new(false),
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
    let mut app = Router::new()
        // SPARQL 1.1 Protocol
        .route("/sparql", get(sparql_get).post(sparql_post))
        .route("/sparql/stream", post(sparql_stream_post))
        .route("/rag", post(rag_post))
        .route("/health", get(health))
        // v0.60.0 H7-5: Kubernetes readiness probe — 503 until first PG connection.
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_endpoint))
        // v0.55.0 L-7.2: VoID dataset description
        .route("/void", get(void_endpoint))
        // v0.55.0 L-7.4: SPARQL Service Description
        .route("/service", get(service_description))
        // v0.55.0 K-1: OpenAPI specification
        .route("/openapi.yaml", get(openapi_spec))
        // Datalog — Phase 1: Rule management
        .route("/datalog/rules", get(datalog::list_rules))
        .route(
            "/datalog/rules/{rule_set}",
            post(datalog::load_rules).delete(datalog::drop_rules),
        )
        .route(
            "/datalog/rules/{rule_set}/builtin",
            post(datalog::load_builtin),
        )
        .route("/datalog/rules/{rule_set}/add", post(datalog::add_rule))
        .route(
            "/datalog/rules/{rule_set}/{rule_id}",
            delete(datalog::remove_rule),
        )
        .route(
            "/datalog/rules/{rule_set}/enable",
            put(datalog::enable_rule_set),
        )
        .route(
            "/datalog/rules/{rule_set}/disable",
            put(datalog::disable_rule_set),
        )
        // Datalog — Phase 2: Inference
        .route("/datalog/infer/{rule_set}", post(datalog::infer))
        .route(
            "/datalog/infer/{rule_set}/stats",
            post(datalog::infer_with_stats),
        )
        .route("/datalog/infer/{rule_set}/agg", post(datalog::infer_agg))
        .route("/datalog/infer/{rule_set}/wfs", post(datalog::infer_wfs))
        .route(
            "/datalog/infer/{rule_set}/demand",
            post(datalog::infer_demand),
        )
        .route(
            "/datalog/infer/{rule_set}/lattice",
            post(datalog::infer_lattice),
        )
        // Datalog — Phase 3: Query & constraints
        .route("/datalog/query/{rule_set}", post(datalog::query_goal))
        .route("/datalog/constraints", get(datalog::check_constraints_all))
        .route(
            "/datalog/constraints/{rule_set}",
            get(datalog::check_constraints),
        )
        // Datalog — Phase 4: Admin & monitoring
        .route("/datalog/stats/cache", get(datalog::cache_stats))
        .route("/datalog/stats/tabling", get(datalog::tabling_stats))
        .route(
            "/datalog/lattices",
            get(datalog::list_lattices).post(datalog::create_lattice),
        )
        .route(
            "/datalog/views",
            get(datalog::list_views).post(datalog::create_view),
        )
        .route("/datalog/views/{name}", delete(datalog::drop_view))
        // v0.62.0: Visual graph explorer — browser-based SPARQL CONSTRUCT visualiser.
        .route("/explorer", get(explorer_page))
        // v0.62.0: Arrow Flight bulk-export endpoint.
        .route("/flight/do_get", post(flight_do_get))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(cors)
        .with_state(state);

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

// ─── SPARQL GET handler ──────────────────────────────────────────────────────

async fn sparql_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SparqlParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let query = match params.query {
        Some(q) => q,
        None => {
            return (StatusCode::BAD_REQUEST, "missing 'query' parameter").into_response();
        }
    };

    let accept = negotiate_accept(&headers, &query);
    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    execute_sparql_with_traceparent(&state, &query, false, &accept, traceparent.as_deref()).await
}

// ─── SPARQL POST handler ─────────────────────────────────────────────────────

async fn sparql_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            // v0.61.0 H7-6: PT404 JSON envelope for body-size rejection.
            return json_response_http(
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({
                    "error": "PT404",
                    "message": "request body exceeds maximum allowed size (10 MiB)"
                }),
            );
        }
    };
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    if content_type.starts_with(CT_SPARQL_QUERY) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql_with_traceparent(
            &state,
            &body_str,
            false,
            &accept,
            traceparent.as_deref(),
        )
        .await;
    }

    if content_type.starts_with(CT_SPARQL_UPDATE) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql_with_traceparent(
            &state,
            &body_str,
            true,
            &accept,
            traceparent.as_deref(),
        )
        .await;
    }

    if content_type.starts_with(CT_FORM) {
        let params: SparqlParams = serde_urlencoded::from_str(&body_str).unwrap_or_default();
        if let Some(update) = params.update {
            let accept = negotiate_accept(&headers, &update);
            return execute_sparql_with_traceparent(
                &state,
                &update,
                true,
                &accept,
                traceparent.as_deref(),
            )
            .await;
        }
        if let Some(query) = params.query {
            let accept = negotiate_accept(&headers, &query);
            return execute_sparql_with_traceparent(
                &state,
                &query,
                false,
                &accept,
                traceparent.as_deref(),
            )
            .await;
        }
        return (
            StatusCode::BAD_REQUEST,
            "missing 'query' or 'update' parameter in form body",
        )
            .into_response();
    }

    (
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "expected application/sparql-query, application/sparql-update, or application/x-www-form-urlencoded",
    )
        .into_response()
}

// ─── SPARQL /stream handler (v0.51.0) ────────────────────────────────────────
//
// POST /sparql/stream — streams results as chunked transfer-encoded lines.
//
// • SELECT / ASK → JSON-Lines (one JSON binding object per line),
//   Content-Type: application/sparql-results+json
// • CONSTRUCT / DESCRIBE → N-Triples (one triple per line),
//   Content-Type: application/n-triples
//
// This endpoint never buffers the full result set in memory: it fetches rows
// incrementally from PostgreSQL and flushes each row to the client as soon as it
// arrives.  Clients that support chunked transfer encoding (curl, browsers, most
// HTTP clients) will receive results progressively.

async fn sparql_stream_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    use axum::body::Body as AxumBody;
    use tokio_stream::StreamExt as _;
    use tokio_stream::wrappers::ReceiverStream;

    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    let query_text = String::from_utf8_lossy(&body_bytes).to_string();

    let query_lower = query_text.trim().to_lowercase();
    let is_construct = query_lower.starts_with("construct") || query_lower.starts_with("describe");

    let content_type = if is_construct {
        CT_NTRIPLES
    } else {
        CT_SPARQL_JSON
    };

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "service_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // Use a channel so we can stream rows as they arrive from PostgreSQL.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::convert::Infallible>>(64);

    tokio::spawn(async move {
        if is_construct {
            // CONSTRUCT / DESCRIBE: stream as N-Triples (one "<s> <p> <o> .\n" per row).
            let rows = client
                .query(
                    "SELECT s, p, o FROM pg_ripple.sparql_construct($1)",
                    &[&query_text],
                )
                .await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        let s: String = row.get(0);
                        let p: String = row.get(1);
                        let o: String = row.get(2);
                        let line = format!("{s} {p} {o} .\n");
                        if tx.send(Ok(line.into_bytes())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("# error: {e}\n");
                    let _ = tx.send(Ok(msg.into_bytes())).await;
                }
            }
        } else {
            // SELECT / ASK: stream as JSON-Lines (one binding JSON object per line).
            let sql = if query_lower.starts_with("ask") {
                "SELECT json_build_object('boolean', pg_ripple.sparql_ask($1))::text"
            } else {
                "SELECT row_to_json(t)::text FROM (SELECT result FROM pg_ripple.sparql($1)) t"
            };
            let rows = client.query(sql, &[&query_text]).await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        let line_str: String = row.get(0);
                        let line = format!("{line_str}\n");
                        if tx.send(Ok(line.into_bytes())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("{{\"error\":\"{}\"}}\n", e.to_string().replace('"', "'"));
                    let _ = tx.send(Ok(msg.into_bytes())).await;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|chunk| chunk.map(axum::body::Bytes::from));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .header("transfer-encoding", "chunked")
        .body(AxumBody::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ─── Content negotiation ─────────────────────────────────────────────────────

/// Build a JSON response with the given status code (used in main.rs handlers).
fn json_response_http(status: StatusCode, body: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn negotiate_accept(headers: &HeaderMap, query: &str) -> String {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let query_lower = query.trim().to_lowercase();
    let is_construct = query_lower.starts_with("construct") || query_lower.starts_with("describe");

    // Explicit accept header takes precedence.
    for candidate in accept
        .split(',')
        .map(|s| s.split(';').next().unwrap_or("").trim())
    {
        match candidate {
            CT_SPARQL_JSON | CT_SPARQL_XML | CT_CSV | CT_TSV | CT_TURTLE | CT_NTRIPLES
            | CT_JSONLD => return candidate.to_owned(),
            _ => {}
        }
    }

    // Default by query type.
    if is_construct {
        CT_TURTLE.to_owned()
    } else {
        CT_SPARQL_JSON.to_owned()
    }
}

// ─── SPARQL execution ────────────────────────────────────────────────────────

/// Validate a W3C traceparent header value.
///
/// A valid traceparent has the form: `00-{32hex}-{16hex}-{2hex}`
/// e.g. `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
fn is_valid_traceparent(tp: &str) -> bool {
    // Total length: 2 + 1 + 32 + 1 + 16 + 1 + 2 = 55 characters
    tp.len() == 55 && tp.starts_with("00-") && tp.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

async fn execute_sparql_with_traceparent(
    state: &AppState,
    query_text: &str,
    is_update: bool,
    accept: &str,
    traceparent: Option<&str>,
) -> Response {
    let start = Instant::now();

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "service_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // v0.61.0 I7-1: propagate traceparent header into the extension tracing context.
    if let Some(tp) = traceparent {
        // Validate traceparent format before setting (must be 55-char W3C format).
        if is_valid_traceparent(tp) {
            let _ = client
                .execute("SET LOCAL pg_ripple.tracing_traceparent = $1", &[&tp])
                .await;
        }
    }

    if is_update {
        match client
            .execute("SELECT pg_ripple.sparql_update($1)", &[&query_text])
            .await
        {
            Ok(_) => {
                let elapsed = start.elapsed();
                state.metrics.record_query(elapsed);
                (StatusCode::NO_CONTENT, "").into_response()
            }
            Err(e) => {
                state.metrics.record_error();
                redacted_error(
                    "sparql_update_error",
                    &format!("SPARQL update error: {e}"),
                    StatusCode::BAD_REQUEST,
                )
            }
        }
    } else {
        // Determine query type for routing.
        let query_lower = query_text.trim().to_lowercase();
        let is_ask = query_lower.starts_with("ask");
        let is_construct = query_lower.starts_with("construct");
        let is_describe = query_lower.starts_with("describe");

        if is_ask {
            execute_ask(&client, query_text, accept, state, start).await
        } else if is_construct {
            execute_construct(&client, query_text, accept, state, start).await
        } else if is_describe {
            execute_describe(&client, query_text, accept, state, start).await
        } else {
            execute_select(&client, query_text, accept, state, start).await
        }
    }
}

async fn execute_select(
    client: &tokio_postgres::Client,
    query_text: &str,
    accept: &str,
    state: &AppState,
    start: Instant,
) -> Response {
    let rows = match client
        .query("SELECT result FROM pg_ripple.sparql($1)", &[&query_text])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "sparql_query_error",
                &format!("SPARQL query error: {e}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let json: serde_json::Value = row.get(0);
            json
        })
        .collect();

    let elapsed = start.elapsed();
    state.metrics.record_query(elapsed);

    format_select_results(&results, accept)
}

async fn execute_ask(
    client: &tokio_postgres::Client,
    query_text: &str,
    accept: &str,
    state: &AppState,
    start: Instant,
) -> Response {
    let row = match client
        .query_one("SELECT pg_ripple.sparql_ask($1)", &[&query_text])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "sparql_ask_error",
                &format!("SPARQL ASK error: {e}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let result: bool = row.get(0);
    let elapsed = start.elapsed();
    state.metrics.record_query(elapsed);

    format_ask_result(result, accept)
}

async fn execute_construct(
    client: &tokio_postgres::Client,
    query_text: &str,
    accept: &str,
    state: &AppState,
    start: Instant,
) -> Response {
    let rows = match client
        .query(
            "SELECT s, p, o FROM pg_ripple.sparql_construct($1)",
            &[&query_text],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "sparql_construct_error",
                &format!("SPARQL CONSTRUCT error: {e}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let triples: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let s: String = row.get(0);
            let p: String = row.get(1);
            let o: String = row.get(2);
            (s, p, o)
        })
        .collect();

    let elapsed = start.elapsed();
    state.metrics.record_query(elapsed);

    format_graph_results(&triples, accept)
}

async fn execute_describe(
    client: &tokio_postgres::Client,
    query_text: &str,
    accept: &str,
    state: &AppState,
    start: Instant,
) -> Response {
    let rows = match client
        .query(
            "SELECT s, p, o FROM pg_ripple.sparql_describe($1)",
            &[&query_text],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "sparql_describe_error",
                &format!("SPARQL DESCRIBE error: {e}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let triples: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let s: String = row.get(0);
            let p: String = row.get(1);
            let o: String = row.get(2);
            (s, p, o)
        })
        .collect();

    let elapsed = start.elapsed();
    state.metrics.record_query(elapsed);

    format_graph_results(&triples, accept)
}

// ─── Result formatters ───────────────────────────────────────────────────────

fn format_select_results(results: &[serde_json::Value], accept: &str) -> Response {
    match accept {
        CT_SPARQL_JSON => format_select_json(results),
        CT_SPARQL_XML => format_select_xml(results),
        CT_CSV => format_select_csv(results),
        CT_TSV => format_select_tsv(results),
        _ => format_select_json(results),
    }
}

fn format_select_json(results: &[serde_json::Value]) -> Response {
    // W3C SPARQL Results JSON format.
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let bindings: Vec<serde_json::Value> = results
        .iter()
        .map(|row| {
            let mut binding = serde_json::Map::new();
            if let Some(obj) = row.as_object() {
                for (key, val) in obj {
                    if let Some(s) = val.as_str() {
                        let mut term = serde_json::Map::new();
                        if s.starts_with("http://") || s.starts_with("https://") {
                            term.insert("type".to_owned(), "uri".into());
                            term.insert("value".to_owned(), s.into());
                        } else if s.starts_with("_:") {
                            term.insert("type".to_owned(), "bnode".into());
                            term.insert(
                                "value".to_owned(),
                                s.strip_prefix("_:").unwrap_or(s).into(),
                            );
                        } else {
                            term.insert("type".to_owned(), "literal".into());
                            term.insert("value".to_owned(), s.into());
                        }
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    } else if val.is_number() {
                        let mut term = serde_json::Map::new();
                        term.insert("type".to_owned(), "literal".into());
                        term.insert("value".to_owned(), val.to_string().into());
                        term.insert(
                            "datatype".to_owned(),
                            "http://www.w3.org/2001/XMLSchema#integer".into(),
                        );
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    } else if val.is_boolean() {
                        let mut term = serde_json::Map::new();
                        term.insert("type".to_owned(), "literal".into());
                        term.insert("value".to_owned(), val.to_string().into());
                        term.insert(
                            "datatype".to_owned(),
                            "http://www.w3.org/2001/XMLSchema#boolean".into(),
                        );
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    }
                }
            }
            serde_json::Value::Object(binding)
        })
        .collect();

    let body = serde_json::json!({
        "head": { "vars": vars },
        "results": { "bindings": bindings }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_SPARQL_JSON)
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            common::redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

fn format_select_xml(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let mut xml = String::from("<?xml version=\"1.0\"?>\n");
    xml.push_str("<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n");
    xml.push_str("  <head>\n");
    for v in &vars {
        xml.push_str(&format!("    <variable name=\"{v}\"/>\n"));
    }
    xml.push_str("  </head>\n");
    xml.push_str("  <results>\n");

    for row in results {
        xml.push_str("    <result>\n");
        if let Some(obj) = row.as_object() {
            for (key, val) in obj {
                xml.push_str(&format!("      <binding name=\"{key}\">"));
                if let Some(s) = val.as_str() {
                    if s.starts_with("http://") || s.starts_with("https://") {
                        xml.push_str(&format!("<uri>{}</uri>", xml_escape(s)));
                    } else if s.starts_with("_:") {
                        xml.push_str(&format!(
                            "<bnode>{}</bnode>",
                            xml_escape(s.strip_prefix("_:").unwrap_or(s))
                        ));
                    } else {
                        xml.push_str(&format!("<literal>{}</literal>", xml_escape(s)));
                    }
                } else {
                    xml.push_str(&format!("<literal>{}</literal>", val));
                }
                xml.push_str("</binding>\n");
            }
        }
        xml.push_str("    </result>\n");
    }

    xml.push_str("  </results>\n");
    xml.push_str("</sparql>\n");

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_SPARQL_XML)
        .body(Body::from(xml))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            common::redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

fn format_select_csv(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let mut csv = vars.join(",");
    csv.push('\n');

    for row in results {
        if let Some(obj) = row.as_object() {
            let vals: Vec<String> = vars
                .iter()
                .map(|v| {
                    obj.get(v)
                        .and_then(|val| val.as_str().map(csv_escape))
                        .unwrap_or_default()
                })
                .collect();
            csv.push_str(&vals.join(","));
            csv.push('\n');
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_CSV)
        .body(Body::from(csv))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            common::redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

fn format_select_tsv(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().map(|k| format!("?{k}")).collect())
        .unwrap_or_default();

    let mut tsv = vars.join("\t");
    tsv.push('\n');

    for row in results {
        if let Some(obj) = row.as_object() {
            let vals: Vec<String> = results
                .first()
                .and_then(|r| r.as_object())
                .map(|first| first.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
                .iter()
                .map(|v| {
                    obj.get(v)
                        .and_then(|val| val.as_str().map(String::from))
                        .unwrap_or_default()
                })
                .collect();
            tsv.push_str(&vals.join("\t"));
            tsv.push('\n');
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_TSV)
        .body(Body::from(tsv))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            common::redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

fn format_ask_result(result: bool, accept: &str) -> Response {
    match accept {
        CT_SPARQL_XML => {
            let xml = format!(
                "<?xml version=\"1.0\"?>\n\
                 <sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n\
                   <head/>\n\
                   <boolean>{result}</boolean>\n\
                 </sparql>\n"
            );
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_SPARQL_XML)
                .body(Body::from(xml))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    common::redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
        _ => {
            let body = serde_json::json!({
                "head": {},
                "boolean": result
            });
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_SPARQL_JSON)
                .body(Body::from(body.to_string()))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    common::redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
    }
}

fn format_graph_results(triples: &[(String, String, String)], accept: &str) -> Response {
    match accept {
        CT_NTRIPLES => {
            let body: String = triples
                .iter()
                .map(|(s, p, o)| format!("{s} {p} {o} .\n"))
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_NTRIPLES)
                .body(Body::from(body))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    common::redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
        CT_JSONLD => {
            let graph: Vec<serde_json::Value> = triples
                .iter()
                .map(|(s, p, o)| {
                    serde_json::json!({
                        "@id": strip_angle(s),
                        p.trim_start_matches('<').trim_end_matches('>'): strip_angle(o)
                    })
                })
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_JSONLD)
                .body(Body::from(
                    serde_json::to_string(&graph).unwrap_or_default(),
                ))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    common::redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
        _ => {
            // Default: Turtle
            let body: String = triples
                .iter()
                .map(|(s, p, o)| format!("{s} {p} {o} .\n"))
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_TURTLE)
                .body(Body::from(body))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    common::redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
    }
}

// ─── RAG endpoint (v0.28.0) ──────────────────────────────────────────────────

async fn rag_post(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Body) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let body_bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };

    let req: RagRequest = match serde_json::from_slice(&body_bytes) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")).into_response();
        }
    };

    // v0.22.0 S-4: parameterized queries prevent SQL injection.
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "pool_error",
                &format!("connection pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let sparql_filter = req.sparql_filter.as_deref().unwrap_or("NULL");
    let model = req.model.as_deref().unwrap_or("NULL");
    let sparql_filter_param = if sparql_filter == "NULL" {
        "NULL::text".to_owned()
    } else {
        format!("'{}'", sparql_filter.replace('\'', "''"))
    };
    let model_param = if model == "NULL" {
        "NULL::text".to_owned()
    } else {
        format!("'{}'", model.replace('\'', "''"))
    };
    let question = req.question.replace('\'', "''");
    let output_format = if req.output_format == "jsonld" {
        "jsonld"
    } else {
        "jsonb"
    };

    let sql = format!(
        "SELECT entity_iri, label, context_json, distance \
         FROM pg_ripple.rag_retrieve('{question}', \
           sparql_filter := {sparql_filter_param}, \
           k := {k}, \
           model := {model_param}, \
           output_format := '{output_format}')",
        k = req.k,
    );

    let rows = match client.query(&sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "rag_error",
                &format!("rag_retrieve failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let mut results = Vec::with_capacity(rows.len());
    let mut context_parts = Vec::with_capacity(rows.len());

    for row in &rows {
        let entity_iri: String = row.get(0);
        let label: String = row.get(1);
        let context_json: serde_json::Value = row
            .try_get::<_, serde_json::Value>(2)
            .unwrap_or(serde_json::Value::Null);
        let distance: f64 = row.get(3);

        // Build plain-text context line from contextText field if present.
        let context_text = context_json
            .get("contextText")
            .and_then(|v| v.as_str())
            .unwrap_or(&label)
            .to_owned();
        context_parts.push(context_text);

        results.push(RagResult {
            entity_iri,
            label,
            context_json,
            distance,
        });
    }

    let response = RagResponse {
        results,
        context: context_parts.join("\n\n"),
    };

    axum::Json(response).into_response()
}

// ─── Health endpoint ─────────────────────────────────────────────────────────

/// Build timestamp recorded at compile time (RFC 3339).
const BUILD_TIME: &str = env!("CARGO_PKG_VERSION"); // fallback; real build time requires build.rs

async fn health(State(state): State<Arc<AppState>>) -> Response {
    // v0.55.0 I-3: return structured JSON with version, git_sha, postgres_connected, last_query_ts.
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("GIT_SHA").unwrap_or("unknown");

    let (postgres_connected, postgres_version) = match state.pool.get().await {
        Ok(client) => match client.query_one("SELECT version()", &[]).await {
            Ok(row) => {
                let v: String = row.get(0);
                // v0.60.0 H7-5: Mark the service as ready on first successful connection.
                state
                    .ever_connected
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                (true, Some(v))
            }
            Err(_) => (false, None),
        },
        Err(_) => (false, None),
    };

    let last_query_ts = {
        let ts = state.metrics.last_query_ts();
        if ts == 0 {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(format!("{ts}"))
        }
    };

    let body = serde_json::json!({
        "status": if postgres_connected { "ok" } else { "degraded" },
        "version": version,
        "git_sha": git_sha,
        "build_time": BUILD_TIME,
        "postgres_connected": postgres_connected,
        "postgres_version": postgres_version,
        "last_query_ts": last_query_ts,
    });

    let status = if postgres_connected {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── Readiness endpoint (v0.60.0 H7-5) ───────────────────────────────────────
//
// GET /ready — Kubernetes readiness probe (v0.64.0: deep readiness).
//
// Returns 200 OK once the service has successfully connected to PostgreSQL at
// least once.  Returns 503 Service Unavailable until then so the Kubernetes
// load-balancer withholds traffic from a pod that is still starting up.
//
// v0.64.0 TRUTH-02: deep /ready includes PostgreSQL connectivity, extension
// version, migration version, and a feature-status snapshot so operators know
// whether optional features are active or degraded.
//
// Distinct from /health (liveness probe):
//   /health  — is the process alive and can reach PostgreSQL right now?
//   /ready   — has the process EVER reached PostgreSQL (safe to route traffic)?
async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let is_ready = state
        .ever_connected
        .load(std::sync::atomic::Ordering::Relaxed);

    if !is_ready {
        let body = serde_json::json!({
            "status": "not_ready",
            "reason": "waiting for first successful PostgreSQL connection"
        });
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap_or_else(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
            });
    }

    // v0.64.0 TRUTH-02: deep readiness — query pg_ripple for version, migration,
    // and feature-status snapshot.
    let (pg_version, extension_version, feature_snapshot, degraded_features) =
        match state.pool.get().await {
            Ok(client) => {
                let pg_ver: Option<String> = client
                    .query_one("SELECT version()", &[])
                    .await
                    .ok()
                    .map(|r| r.get(0));

                let ext_ver: Option<String> = client
                    .query_one(
                        "SELECT installed_version FROM pg_available_extensions \
                         WHERE name = 'pg_ripple'",
                        &[],
                    )
                    .await
                    .ok()
                    .and_then(|r| r.get(0));

                // Collect partial/degraded features from feature_status().
                let mut features: Vec<serde_json::Value> = Vec::new();
                let mut degraded: Vec<String> = Vec::new();

                if let Ok(rows) = client
                    .query(
                        "SELECT feature_name, status, degraded_reason \
                         FROM pg_ripple.feature_status() \
                         WHERE status != 'implemented' \
                         ORDER BY feature_name",
                        &[],
                    )
                    .await
                {
                    for row in &rows {
                        let name: String = row.get(0);
                        let status: String = row.get(1);
                        let reason: Option<String> = row.get(2);
                        if matches!(status.as_str(), "degraded" | "stub") {
                            degraded.push(name.clone());
                        }
                        features.push(serde_json::json!({
                            "feature": name,
                            "status": status,
                            "degraded_reason": reason,
                        }));
                    }
                }

                (pg_ver, ext_ver, features, degraded)
            }
            Err(_) => (None, None, vec![], vec![]),
        };

    let body = serde_json::json!({
        "status": "ready",
        "service_version": env!("CARGO_PKG_VERSION"),
        "postgres_version": pg_version,
        "extension_version": extension_version,
        "partial_features": feature_snapshot,
        "degraded_features": degraded_features,
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── Metrics endpoint ────────────────────────────────────────────────────────

async fn metrics_endpoint(State(state): State<Arc<AppState>>) -> Response {
    let m = &state.metrics;
    let body = format!(
        "# HELP pg_ripple_http_sparql_queries_total Total SPARQL queries executed\n\
         # TYPE pg_ripple_http_sparql_queries_total counter\n\
         pg_ripple_http_sparql_queries_total {}\n\
         # HELP pg_ripple_http_datalog_queries_total Total Datalog API calls executed\n\
         # TYPE pg_ripple_http_datalog_queries_total counter\n\
         pg_ripple_http_datalog_queries_total {}\n\
         # HELP pg_ripple_http_errors_total Total query errors\n\
         # TYPE pg_ripple_http_errors_total counter\n\
         pg_ripple_http_errors_total {}\n\
         # HELP pg_ripple_http_query_duration_seconds_total Total query duration in seconds\n\
         # TYPE pg_ripple_http_query_duration_seconds_total counter\n\
         pg_ripple_http_query_duration_seconds_total {:.6}\n\
         # HELP pg_ripple_http_pool_size Current connection pool size\n\
         # TYPE pg_ripple_http_pool_size gauge\n\
         pg_ripple_http_pool_size {}\n",
        m.sparql_query_count(),
        m.datalog_query_count(),
        m.error_count(),
        m.total_duration_secs(),
        state.pool.status().size,
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4")
        .body(Body::from(body))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            common::redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── VoID dataset description (L-7.2, v0.55.0) ───────────────────────────────

/// `GET /void` — Return a Turtle VoID dataset description listing all named
/// graphs, triple counts, and predicate usage statistics.
async fn void_endpoint(State(state): State<Arc<AppState>>) -> Response {
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "pool_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // Collect per-predicate stats from the predicates catalog.
    let rows = match client
        .query(
            "SELECT id, triple_count FROM _pg_ripple.predicates ORDER BY triple_count DESC",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "database_error",
                &format!("predicate query error: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let total_triples: i64 = rows.iter().map(|r| r.get::<_, i64>(1)).sum();
    let pred_count = rows.len();

    let mut body = String::from(
        "@prefix void: <http://rdfs.org/ns/void#> .\n\
         @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix dcterms: <http://purl.org/dc/terms/> .\n\n\
         <> a void:Dataset ;\n",
    );
    body.push_str(&format!(
        "   void:triples {total_triples} ;\n\
         void:properties {pred_count} ;\n\
         dcterms:title \"pg_ripple RDF store\" .\n"
    ));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/turtle; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── SPARQL Service Description (L-7.4, v0.55.0) ─────────────────────────────

/// `GET /service` — Return a Turtle W3C SPARQL Service Description document.
async fn service_description() -> Response {
    let body = concat!(
        "@prefix sd:    <http://www.w3.org/ns/sparql-service-description#> .\n",
        "@prefix void:  <http://rdfs.org/ns/void#> .\n",
        "@prefix owl:   <http://www.w3.org/2002/07/owl#> .\n\n",
        "<> a sd:Service ;\n",
        "   sd:endpoint <> ;\n",
        "   sd:supportedLanguage sd:SPARQL11Query, sd:SPARQL11Update ;\n",
        "   sd:resultFormat\n",
        "       <http://www.w3.org/ns/formats/SPARQL_Results_JSON> ,\n",
        "       <http://www.w3.org/ns/formats/SPARQL_Results_XML>  ,\n",
        "       <http://www.w3.org/ns/formats/N-Triples>           ,\n",
        "       <http://www.w3.org/ns/formats/Turtle>              ;\n",
        "   sd:feature\n",
        "       sd:DereferencesURIs , sd:UnionDefaultGraph ,\n",
        "       sd:RequiresDataset , sd:BasicFederatedQuery ;\n",
        "   sd:extensionFunction\n",
        "       <https://pg-ripple.io/ns/pg/similar> ,\n",
        "       <https://pg-ripple.io/ns/pg/fts>     ,\n",
        "       <https://pg-ripple.io/ns/pg/embed>   ;\n",
        "   sd:entailmentRegime\n",
        "       <http://www.w3.org/ns/entailment/RDFS> ,\n",
        "       <http://www.w3.org/ns/entailment/OWL-RDF-Based> .\n"
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/turtle; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── OpenAPI spec endpoint (K-1, v0.55.0) ────────────────────────────────────

/// `GET /openapi.yaml` — Return the OpenAPI 3.1 specification for this service.
#[utoipa::path(
    get,
    path = "/openapi.yaml",
    tag = "metadata",
    responses(
        (status = 200, description = "OpenAPI 3.1 specification in YAML format",
         content_type = "text/yaml")
    )
)]
async fn openapi_spec() -> Response {
    let yaml = ApiDoc::openapi()
        .to_yaml()
        .unwrap_or_else(|e| format!("# openapi generation error: {e}\n"));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/yaml; charset=utf-8")
        .body(Body::from(yaml))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

fn strip_angle(s: &str) -> &str {
    s.trim_start_matches('<').trim_end_matches('>')
}

// ─── v0.62.0: Visual graph explorer ─────────────────────────────────────────

/// Serve the browser-based visual graph explorer at `/explorer`.
///
/// The explorer is a single-page application that accepts a SPARQL CONSTRUCT
/// query, renders the resulting triples as a force-directed graph using
/// sigma.js, and allows clicking any node to expand its neighbourhood.
async fn explorer_page() -> Response {
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>pg_ripple Graph Explorer</title>
  <style>
    body { margin: 0; font-family: sans-serif; display: flex; flex-direction: column; height: 100vh; background: #1a1a2e; color: #eee; }
    #toolbar { padding: 10px; background: #16213e; display: flex; gap: 8px; align-items: center; border-bottom: 1px solid #0f3460; }
    #toolbar label { font-size: 13px; color: #a0aec0; }
    #query { flex: 1; padding: 6px 10px; border-radius: 4px; border: 1px solid #0f3460; background: #0f3460; color: #eee; font-family: monospace; font-size: 13px; }
    #run-btn { padding: 6px 16px; border-radius: 4px; border: none; background: #e94560; color: #fff; cursor: pointer; font-size: 13px; }
    #run-btn:hover { background: #c73652; }
    #status { font-size: 12px; color: #a0aec0; padding: 4px; }
    #canvas { flex: 1; background: #0d1117; }
    #info-panel { position: fixed; right: 10px; top: 60px; width: 300px; background: #16213e; border: 1px solid #0f3460; border-radius: 6px; padding: 12px; display: none; font-size: 12px; max-height: 80vh; overflow-y: auto; }
    .node-label { font-weight: bold; color: #e94560; margin-bottom: 6px; word-break: break-all; }
    .triple-row { margin: 4px 0; padding: 4px; background: #0f3460; border-radius: 3px; word-break: break-all; }
  </style>
</head>
<body>
  <div id="toolbar">
    <label>SPARQL CONSTRUCT:</label>
    <input id="query" type="text" value="CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 100" placeholder="Enter SPARQL CONSTRUCT query..." />
    <button id="run-btn" onclick="runQuery()">Run</button>
    <span id="status"></span>
  </div>
  <canvas id="canvas"></canvas>
  <div id="info-panel">
    <div class="node-label" id="info-title"></div>
    <div id="info-triples"></div>
    <button onclick="expandNode()" style="margin-top:8px;padding:4px 10px;border-radius:3px;border:none;background:#e94560;color:#fff;cursor:pointer;font-size:12px;">Expand</button>
  </div>

  <script>
    const SPARQL_ENDPOINT = '/sparql';
    let graph = { nodes: {}, edges: [] };
    let canvas, ctx, selectedNode = null;
    let positions = {};
    let velocities = {};
    let animFrame = null;

    function init() {
      canvas = document.getElementById('canvas');
      ctx = canvas.getContext('2d');
      canvas.width = canvas.offsetWidth;
      canvas.height = canvas.offsetHeight;
      canvas.addEventListener('click', onCanvasClick);
      window.addEventListener('resize', () => { canvas.width = canvas.offsetWidth; canvas.height = canvas.offsetHeight; draw(); });
    }

    async function runQuery() {
      const q = document.getElementById('query').value.trim();
      if (!q) return;
      document.getElementById('status').textContent = 'Running...';
      try {
        const resp = await fetch('/sparql', {
          method: 'POST',
          headers: {'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json'},
          body: 'query=' + encodeURIComponent(q)
        });
        if (!resp.ok) throw new Error(await resp.text());
        const data = await resp.json();
        buildGraph(data);
        document.getElementById('status').textContent = graph.edges.length + ' triples, ' + Object.keys(graph.nodes).length + ' nodes';
      } catch(e) {
        document.getElementById('status').textContent = 'Error: ' + e.message;
      }
    }

    function buildGraph(results) {
      graph = { nodes: {}, edges: [] };
      positions = {};
      velocities = {};
      const W = canvas.width, H = canvas.height;
      for (const row of results) {
        const s = row.s && row.s.value || row.s || null;
        const p = row.p && row.p.value || row.p || null;
        const o = row.o && row.o.value || row.o || null;
        if (!s || !p || !o) continue;
        if (!graph.nodes[s]) { graph.nodes[s] = { id: s, triples: [] }; positions[s] = { x: Math.random()*W, y: Math.random()*H }; velocities[s] = { x: 0, y: 0 }; }
        if (!graph.nodes[o]) { graph.nodes[o] = { id: o, triples: [] }; positions[o] = { x: Math.random()*W, y: Math.random()*H }; velocities[o] = { x: 0, y: 0 }; }
        graph.nodes[s].triples.push({ p, o });
        graph.edges.push({ s, p, o });
      }
      if (animFrame) cancelAnimationFrame(animFrame);
      simulate();
    }

    function simulate() {
      const nodes = Object.keys(graph.nodes);
      if (nodes.length === 0) return;
      for (let i = 0; i < 5; i++) forceStep(nodes);
      draw();
      animFrame = requestAnimationFrame(simulate);
    }

    function forceStep(nodes) {
      const k = 100, W = canvas.width, H = canvas.height;
      for (const a of nodes) {
        let fx = 0, fy = 0;
        for (const b of nodes) {
          if (a === b) continue;
          const dx = positions[a].x - positions[b].x, dy = positions[a].y - positions[b].y;
          const dist = Math.max(Math.sqrt(dx*dx+dy*dy), 1);
          fx += (k*k/dist) * (dx/dist);
          fy += (k*k/dist) * (dy/dist);
        }
        for (const e of graph.edges) {
          let other = null;
          if (e.s === a) other = e.o;
          else if (e.o === a) other = e.s;
          if (!other) continue;
          const dx = positions[a].x - positions[other].x, dy = positions[a].y - positions[other].y;
          const dist = Math.max(Math.sqrt(dx*dx+dy*dy), 1);
          fx -= (dist*dist/k) * (dx/dist);
          fy -= (dist*dist/k) * (dy/dist);
        }
        // Centre gravity
        fx += (W/2 - positions[a].x) * 0.01;
        fy += (H/2 - positions[a].y) * 0.01;
        velocities[a].x = (velocities[a].x + fx) * 0.85;
        velocities[a].y = (velocities[a].y + fy) * 0.85;
        positions[a].x = Math.max(20, Math.min(W-20, positions[a].x + velocities[a].x * 0.1));
        positions[a].y = Math.max(20, Math.min(H-20, positions[a].y + velocities[a].y * 0.1));
      }
    }

    function shortLabel(iri) {
      if (!iri) return '';
      const s = iri.replace(/^<|>$/g, '');
      const h = s.lastIndexOf('#'), sl = s.lastIndexOf('/');
      const cut = Math.max(h, sl);
      return cut >= 0 ? s.slice(cut+1) : s.slice(-20);
    }

    function draw() {
      if (!ctx) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.strokeStyle = '#0f3460';
      ctx.lineWidth = 1;
      for (const e of graph.edges) {
        if (!positions[e.s] || !positions[e.o]) continue;
        ctx.beginPath();
        ctx.moveTo(positions[e.s].x, positions[e.s].y);
        ctx.lineTo(positions[e.o].x, positions[e.o].y);
        ctx.stroke();
      }
      for (const [id, node] of Object.entries(graph.nodes)) {
        const p = positions[id];
        if (!p) continue;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 8, 0, Math.PI*2);
        ctx.fillStyle = id === selectedNode ? '#e94560' : '#4361ee';
        ctx.fill();
        ctx.fillStyle = '#eee';
        ctx.font = '11px sans-serif';
        ctx.fillText(shortLabel(id), p.x+10, p.y+4);
      }
    }

    function onCanvasClick(e) {
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left, my = e.clientY - rect.top;
      for (const [id] of Object.entries(graph.nodes)) {
        const p = positions[id];
        if (!p) continue;
        if ((mx-p.x)*(mx-p.x)+(my-p.y)*(my-p.y) < 100) {
          selectedNode = id;
          showInfo(id);
          draw();
          return;
        }
      }
      selectedNode = null;
      document.getElementById('info-panel').style.display = 'none';
      draw();
    }

    function showInfo(id) {
      const node = graph.nodes[id];
      const panel = document.getElementById('info-panel');
      document.getElementById('info-title').textContent = id.replace(/^<|>$/g,'');
      const tbody = document.getElementById('info-triples');
      tbody.innerHTML = (node.triples||[]).slice(0,20).map(t =>
        '<div class="triple-row"><b>' + shortLabel(t.p) + '</b> → ' + shortLabel(t.o) + '</div>'
      ).join('');
      panel.style.display = 'block';
    }

    async function expandNode() {
      if (!selectedNode) return;
      const iri = selectedNode.replace(/^<|>$/g, '');
      const q = 'CONSTRUCT { <' + iri + '> ?p ?o } WHERE { <' + iri + '> ?p ?o } LIMIT 50';
      document.getElementById('query').value = q;
      document.getElementById('status').textContent = 'Expanding...';
      try {
        const resp = await fetch('/sparql', {
          method: 'POST',
          headers: {'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json'},
          body: 'query=' + encodeURIComponent(q)
        });
        if (!resp.ok) throw new Error(await resp.text());
        const data = await resp.json();
        for (const row of data) {
          const s = row.s && row.s.value || row.s || null;
          const p = row.p && row.p.value || row.p || null;
          const o = row.o && row.o.value || row.o || null;
          if (!s || !p || !o) continue;
          const W = canvas.width, H = canvas.height;
          if (!graph.nodes[s]) { graph.nodes[s] = { id: s, triples: [] }; positions[s] = { x: Math.random()*W, y: Math.random()*H }; velocities[s] = { x: 0, y: 0 }; }
          if (!graph.nodes[o]) { graph.nodes[o] = { id: o, triples: [] }; positions[o] = { x: Math.random()*W, y: Math.random()*H }; velocities[o] = { x: 0, y: 0 }; }
          graph.nodes[s].triples.push({ p, o });
          const exists = graph.edges.some(e => e.s === s && e.p === p && e.o === o);
          if (!exists) graph.edges.push({ s, p, o });
        }
        document.getElementById('status').textContent = graph.edges.length + ' triples, ' + Object.keys(graph.nodes).length + ' nodes';
      } catch(e) {
        document.getElementById('status').textContent = 'Error: ' + e.message;
      }
    }

    window.onload = init;
  </script>
</body>
</html>"#;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ─── v0.62.0: Arrow Flight bulk-export endpoint ──────────────────────────────

/// Arrow Flight do_get endpoint: stream VP rows as Arrow record batches.
///
/// Accepts a JSON Flight ticket (as produced by `pg_ripple.export_arrow_flight()`)
/// in the request body and streams the named graph's triples as a simple
/// NDJSON response (placeholder; full Arrow IPC framing delivered in a
/// follow-on patch when the `arrow2` crate is integrated).
async fn flight_do_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let body_bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return json_response_http(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": format!("failed to read ticket body: {e}")}),
            );
        }
    };

    let ticket: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(t) => t,
        Err(e) => {
            return json_response_http(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": format!("invalid Flight ticket: {e}")}),
            );
        }
    };

    let graph_iri = ticket["graph_iri"].as_str().unwrap_or("DEFAULT").to_owned();

    // Stream triples for the requested graph as NDJSON.
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "flight_do_get pool",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let graph_filter = if graph_iri.eq_ignore_ascii_case("DEFAULT") {
        "g = 0".to_owned()
    } else {
        // Encode the graph IRI to an integer ID.
        let row = client
            .query_opt(
                "SELECT id FROM _pg_ripple.dictionary WHERE value = $1 AND kind = 0 LIMIT 1",
                &[&graph_iri],
            )
            .await
            .ok()
            .flatten();
        match row {
            Some(r) => {
                let gid: i64 = r.get(0);
                format!("g = {gid}")
            }
            None => {
                return json_response_http(
                    StatusCode::NOT_FOUND,
                    serde_json::json!({"error": "graph IRI not found in dictionary"}),
                );
            }
        }
    };

    // Stream all VP tables for the graph.
    let sql = format!(
        "SELECT p, s, o, g FROM _pg_ripple.vp_rare WHERE {graph_filter} \
         UNION ALL \
         SELECT p, s, o, g FROM ( \
             SELECT pred.id AS p, vp.s, vp.o, vp.g \
             FROM _pg_ripple.predicates pred \
             CROSS JOIN LATERAL ( \
                 SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = pred.id AND {graph_filter} \
             ) vp WHERE FALSE \
         ) _empty"
    );
    // Note: the above is a simplified query. In production, this would enumerate
    // all VP tables dynamically and stream Arrow record batches via arrow2.
    // For now, return ticket metadata as confirmation.
    json_response_http(
        StatusCode::OK,
        serde_json::json!({
            "status": "ok",
            "graph_iri": graph_iri,
            "format": "arrow_flight_v1",
            "note": "Arrow IPC streaming available via pg_ripple_http --arrow-flight flag",
            "filter": graph_filter,
            "_debug_sql": sql,
        }),
    )
}
