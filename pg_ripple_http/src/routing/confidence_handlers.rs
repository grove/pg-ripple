//! Uncertain Knowledge Engine HTTP handlers (v0.87.0 CONF-HTTP-01).
//!
//! POST /confidence/load          — load triples with a uniform confidence score
//! GET  /confidence/shacl-score   — return SHACL quality score for a graph
//! GET  /confidence/shacl-report  — return scored SHACL violation report
//! POST /confidence/vacuum        — purge orphaned confidence rows

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, check_auth, check_auth_write, redacted_error};

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    json_response_http(status, body)
}

async fn read_body(body: Body) -> Result<String, Response> {
    let bytes = match axum::body::to_bytes(body, 64 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return Err(json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "read_error", "detail": format!("{e}")}),
            ));
        }
    };
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_utf8", "detail": "request body is not valid UTF-8"}),
        )
    })
}

/// Query parameters for /confidence/load
#[derive(Debug, Deserialize)]
pub struct LoadConfidenceParams {
    /// Confidence value in [0.0, 1.0] to attach to all loaded triples.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Triple/quad serialisation format: ntriples (default), nquads, turtle.
    #[serde(default = "default_format")]
    pub format: String,
    /// Named graph URI to load triples into.
    pub graph_uri: Option<String>,
}

fn default_confidence() -> f64 {
    1.0
}
fn default_format() -> String {
    "ntriples".to_owned()
}

/// Query parameters for SHACL score/report endpoints.
#[derive(Debug, Deserialize)]
pub struct ShaclParams {
    /// IRI of the named graph to evaluate.
    pub graph: String,
}

/// POST /confidence/load
///
/// Body: serialised triples (format determined by `?format=`).
/// Returns `{"triples_loaded": N, "confidence": X, "elapsed_ms": Y}`.
pub(crate) async fn load_with_confidence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<LoadConfidenceParams>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let data = match read_body(body).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    if !(0.0..=1.0).contains(&params.confidence) {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "invalid_confidence",
                "detail": "confidence must be in [0.0, 1.0]"
            }),
        );
    }
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

    let sql = "SELECT pg_ripple.load_triples_with_confidence($1, $2, $3, $4)";
    let rows = client
        .query(
            sql,
            &[
                &data,
                &params.confidence,
                &params.format.as_str(),
                &params.graph_uri.as_deref(),
            ],
        )
        .await;

    match rows {
        Ok(rows) => {
            let n: i64 = rows.first().and_then(|r| r.try_get(0).ok()).unwrap_or(0);
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "triples_loaded": n,
                    "confidence": params.confidence,
                    "elapsed_ms": start.elapsed().as_millis()
                }),
            )
        }
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "load_error",
                &format!("load_triples_with_confidence failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

/// GET /confidence/shacl-score?graph=<IRI>
///
/// Returns `{"graph": "...", "score": 0.95, "elapsed_ms": N}`.
pub(crate) async fn shacl_score(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ShaclParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
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

    match client
        .query_one(
            "SELECT pg_ripple.shacl_score($1)",
            &[&params.graph.as_str()],
        )
        .await
    {
        Ok(row) => {
            let score: f64 = row.try_get(0).unwrap_or(1.0);
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "graph": params.graph,
                    "score": score,
                    "elapsed_ms": start.elapsed().as_millis()
                }),
            )
        }
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "shacl_score_error",
                &format!("shacl_score failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

/// GET /confidence/shacl-report?graph=<IRI>
///
/// Returns the scored SHACL violation report as a JSON array.
pub(crate) async fn shacl_report_scored(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ShaclParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
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

    match client
        .query(
            "SELECT focus_node, shape_iri, result_severity, result_severity_score, message \
             FROM pg_ripple.shacl_report_scored($1)",
            &[&params.graph.as_str()],
        )
        .await
    {
        Ok(rows) => {
            let violations: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "focusNode": row.try_get::<_, &str>(0).unwrap_or(""),
                        "shapeIRI": row.try_get::<_, &str>(1).unwrap_or(""),
                        "severity": row.try_get::<_, &str>(2).unwrap_or(""),
                        "score": row.try_get::<_, f64>(3).unwrap_or(1.0),
                        "message": row.try_get::<_, &str>(4).unwrap_or("")
                    })
                })
                .collect();
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "graph": params.graph,
                    "violations": violations,
                    "elapsed_ms": start.elapsed().as_millis()
                }),
            )
        }
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "shacl_report_error",
                &format!("shacl_report_scored failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

/// POST /confidence/vacuum
///
/// Returns `{"deleted": N, "elapsed_ms": Y}`.
pub(crate) async fn vacuum_confidence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
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

    match client
        .query_one("SELECT pg_ripple.vacuum_confidence()", &[])
        .await
    {
        Ok(row) => {
            let deleted: i64 = row.try_get(0).unwrap_or(0);
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "deleted": deleted,
                    "elapsed_ms": start.elapsed().as_millis()
                }),
            )
        }
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "vacuum_confidence_error",
                &format!("vacuum_confidence failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}
