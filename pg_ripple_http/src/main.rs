//! pg_ripple_http — SPARQL 1.1 Protocol HTTP endpoint for pg_ripple.
//!
//! Standalone Rust binary that connects to PostgreSQL (with pg_ripple installed)
//! and exposes a W3C-compliant SPARQL HTTP endpoint.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use deadpool_postgres::{Config, Pool, Runtime};
use serde::Deserialize;
use tokio_postgres::NoTls;
use tower_http::cors::{AllowOrigin, CorsLayer};

mod metrics;

// ─── Application state ──────────────────────────────────────────────────────

struct AppState {
    pool: Pool,
    auth_token: Option<String>,
    #[allow(dead_code)]
    rate_limit: u32,
    metrics: metrics::Metrics,
}

// ─── Configuration ───────────────────────────────────────────────────────────

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

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

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pg_ripple_http=info".parse().unwrap()),
        )
        .init();

    let pg_url = env_or("PG_TRIPLE_HTTP_PG_URL", "postgresql://localhost/postgres");
    let port: u16 = env_or("PG_TRIPLE_HTTP_PORT", "7878")
        .parse()
        .expect("PG_TRIPLE_HTTP_PORT must be a valid port number");
    let pool_size: usize = env_or("PG_TRIPLE_HTTP_POOL_SIZE", "16")
        .parse()
        .expect("PG_TRIPLE_HTTP_POOL_SIZE must be a positive integer");
    let auth_token = std::env::var("PG_TRIPLE_HTTP_AUTH_TOKEN").ok();
    let rate_limit: u32 = env_or("PG_TRIPLE_HTTP_RATE_LIMIT", "0")
        .parse()
        .expect("PG_TRIPLE_HTTP_RATE_LIMIT must be a non-negative integer");
    let cors_origins = env_or("PG_TRIPLE_HTTP_CORS_ORIGINS", "*");

    // Build connection pool.
    let mut cfg = Config::new();
    cfg.url = Some(pg_url.clone());
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size));

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("failed to create PostgreSQL connection pool");

    // Verify connectivity.
    {
        let client = pool
            .get()
            .await
            .expect("failed to connect to PostgreSQL — check PG_TRIPLE_HTTP_PG_URL");
        let row = client
            .query_one("SELECT pg_ripple.triple_count()", &[])
            .await
            .expect("pg_ripple extension not available — is it installed?");
        let count: i64 = row.get(0);
        tracing::info!("connected to PostgreSQL, triple store contains {count} triples");
    }

    let state = Arc::new(AppState {
        pool,
        auth_token,
        rate_limit,
        metrics: metrics::Metrics::new(),
    });

    // CORS layer.
    let cors = if cors_origins == "*" {
        CorsLayer::permissive()
    } else {
        let origins: Vec<HeaderValue> = cors_origins
            .split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect();
        CorsLayer::new().allow_origin(AllowOrigin::list(origins))
    };

    let app = Router::new()
        .route("/sparql", get(sparql_get).post(sparql_post))
        .route("/health", get(health))
        .route("/metrics", get(metrics_endpoint))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("pg_ripple_http listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");
    axum::serve(listener, app).await.expect("server error");
}

// ─── Authentication ──────────────────────────────────────────────────────────

#[allow(clippy::result_large_err)]
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    if let Some(expected) = &state.auth_token {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // Support "Bearer <token>" and "Basic <token>".
        let token = provided
            .strip_prefix("Bearer ")
            .or_else(|| provided.strip_prefix("Basic "))
            .unwrap_or(provided);
        if token != expected.as_str() {
            return Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response());
        }
    }
    Ok(())
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
    execute_sparql(&state, &query, false, &accept).await
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
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    if content_type.starts_with(CT_SPARQL_QUERY) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql(&state, &body_str, false, &accept).await;
    }

    if content_type.starts_with(CT_SPARQL_UPDATE) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql(&state, &body_str, true, &accept).await;
    }

    if content_type.starts_with(CT_FORM) {
        let params: SparqlParams = serde_urlencoded::from_str(&body_str).unwrap_or_default();
        if let Some(update) = params.update {
            let accept = negotiate_accept(&headers, &update);
            return execute_sparql(&state, &update, true, &accept).await;
        }
        if let Some(query) = params.query {
            let accept = negotiate_accept(&headers, &query);
            return execute_sparql(&state, &query, false, &accept).await;
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

// ─── Content negotiation ─────────────────────────────────────────────────────

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

async fn execute_sparql(
    state: &AppState,
    query_text: &str,
    is_update: bool,
    accept: &str,
) -> Response {
    let start = Instant::now();

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("database connection error: {e}"),
            )
                .into_response();
        }
    };

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
                (StatusCode::BAD_REQUEST, format!("SPARQL update error: {e}")).into_response()
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
            return (StatusCode::BAD_REQUEST, format!("SPARQL query error: {e}")).into_response();
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
            return (StatusCode::BAD_REQUEST, format!("SPARQL ASK error: {e}")).into_response();
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
            return (
                StatusCode::BAD_REQUEST,
                format!("SPARQL CONSTRUCT error: {e}"),
            )
                .into_response();
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
            return (
                StatusCode::BAD_REQUEST,
                format!("SPARQL DESCRIBE error: {e}"),
            )
                .into_response();
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
        .unwrap()
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
        .unwrap()
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
        .unwrap()
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
        .unwrap()
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
                .unwrap()
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
                .unwrap()
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
                .unwrap()
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
                .unwrap()
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
                .unwrap()
        }
    }
}

// ─── Health endpoint ─────────────────────────────────────────────────────────

async fn health(State(state): State<Arc<AppState>>) -> Response {
    match state.pool.get().await {
        Ok(client) => match client.query_one("SELECT 1", &[]).await {
            Ok(_) => (StatusCode::OK, "ok").into_response(),
            Err(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("database check failed: {e}"),
            )
                .into_response(),
        },
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, format!("pool error: {e}")).into_response(),
    }
}

// ─── Metrics endpoint ────────────────────────────────────────────────────────

async fn metrics_endpoint(State(state): State<Arc<AppState>>) -> Response {
    let m = &state.metrics;
    let body = format!(
        "# HELP pg_ripple_queries_total Total SPARQL queries executed\n\
         # TYPE pg_ripple_queries_total counter\n\
         pg_ripple_queries_total {}\n\
         # HELP pg_ripple_errors_total Total SPARQL query errors\n\
         # TYPE pg_ripple_errors_total counter\n\
         pg_ripple_errors_total {}\n\
         # HELP pg_ripple_query_duration_seconds_sum Total query duration in seconds\n\
         # TYPE pg_ripple_query_duration_seconds_sum counter\n\
         pg_ripple_query_duration_seconds_sum {:.6}\n\
         # HELP pg_ripple_pool_size Current connection pool size\n\
         # TYPE pg_ripple_pool_size gauge\n\
         pg_ripple_pool_size {}\n",
        m.query_count(),
        m.error_count(),
        m.total_duration_secs(),
        state.pool.status().size,
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4")
        .body(Body::from(body))
        .unwrap()
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
