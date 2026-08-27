//! JSON mapping relational writeback HTTP handlers (v0.128.0 JSON-WRITEBACK-01).
//!
//! `POST /json-mapping/{name}/writeback`
//!    body: `{"subject_iri": "…"}`
//!    calls `pg_ripple.writeback_json_row(name, subject_iri)` synchronously.
//!    returns `{"rows_affected": N}`; requires write-auth.
//!
//! `GET /json-mapping/{name}/writeback/status`
//!    returns queue depth, error count, and `last_error` JSON; requires read-auth.
//!
//! `GET /json-mapping/{name}/writeback/config`
//!    returns validated target configuration and queue health; requires read-auth.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, check_auth, check_auth_write, redacted_error};

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    json_response_http(status, body)
}

#[derive(Deserialize)]
pub(crate) struct WritebackRequest {
    subject_iri: String,
}

/// `POST /json-mapping/{name}/writeback`
///
/// Synchronously calls `pg_ripple.writeback_json_row(name, subject_iri)`.
/// Returns `{"rows_affected": N}`.  Requires write-auth.
pub(crate) async fn json_mapping_writeback_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<WritebackRequest>,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let row = client
        .query_one(
            "SELECT pg_ripple.writeback_json_row($1, $2)",
            &[&name, &body.subject_iri],
        )
        .await;

    match row {
        Ok(r) => {
            let rows_affected: i64 = r.try_get(0).unwrap_or(0);
            json_response(
                StatusCode::OK,
                serde_json::json!({ "rows_affected": rows_affected }),
            )
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("PT0550") {
                json_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    serde_json::json!({ "error": "writeback_target_not_configured", "detail": msg }),
                )
            } else if msg.contains("PT0551") {
                json_response(
                    StatusCode::CONFLICT,
                    serde_json::json!({ "error": "writeback_conflict", "detail": msg }),
                )
            } else {
                redacted_error("writeback_error", &msg, StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

/// `GET /json-mapping/{name}/writeback/status`
///
/// Returns queue depth, error count, and last_error for the named mapping.
/// Requires read-auth.
pub(crate) async fn json_mapping_writeback_status_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let row = client
        .query_opt(
            "SELECT mapping_name, pending, errors, last_error, \
                    last_processed_at::text \
             FROM pg_ripple.json_writeback_status() \
             WHERE mapping_name = $1",
            &[&name],
        )
        .await;

    match row {
        Ok(Some(r)) => {
            let mapping_name: String = r.try_get(0).unwrap_or_default();
            let pending: i64 = r.try_get(1).unwrap_or(0);
            let errors: i64 = r.try_get(2).unwrap_or(0);
            let last_error: Option<String> = r.try_get(3).unwrap_or(None);
            let last_processed_at: Option<String> = r.try_get(4).unwrap_or(None);
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "mapping_name": mapping_name,
                    "pending": pending,
                    "errors": errors,
                    "last_error": last_error,
                    "last_processed_at": last_processed_at
                }),
            )
        }
        Ok(None) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "mapping_name": name,
                "pending": 0i64,
                "errors": 0i64,
                "last_error": null,
                "last_processed_at": null
            }),
        ),
        Err(e) => redacted_error(
            "status_query_error",
            &e.to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}

/// `GET /json-mapping/{name}/writeback/config`
pub(crate) async fn json_mapping_writeback_config_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };
    match client
        .query_opt(
            "SELECT row_to_json(config)::jsonb \
             FROM pg_ripple.writeback_inspect($1) AS config",
            &[&name],
        )
        .await
    {
        Ok(Some(row)) => match row.try_get::<_, serde_json::Value>(0) {
            Ok(config) => json_response(StatusCode::OK, config),
            Err(e) => redacted_error(
                "config_query_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        },
        Ok(None) => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({"error": "json_mapping_not_found"}),
        ),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("json mapping") && msg.contains("not found") {
                json_response(
                    StatusCode::NOT_FOUND,
                    serde_json::json!({"error": "json_mapping_not_found"}),
                )
            } else {
                redacted_error(
                    "config_query_error",
                    &msg,
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            }
        }
    }
}
