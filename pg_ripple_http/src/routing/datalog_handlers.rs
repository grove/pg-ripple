//! Datalog REST API handlers — moved to `routing/datalog_handlers.rs` (v0.90.0 CQ-05).
//!
//! This module implements all 24 endpoints in the `/datalog` namespace,
//! organised into four phases:
//!
//! - **Phase 1**: Rule management (8 endpoints)
//! - **Phase 2**: Inference (6 endpoints)
//! - **Phase 3**: Query & constraints (3 endpoints)
//! - **Phase 4**: Admin & monitoring (7 endpoints)
//!
//! Every handler:
//!   1. Calls `check_auth` or `check_auth_write` depending on whether the
//!      request mutates state.
//!   2. Acquires a connection from the shared pool.
//!   3. Executes exactly one parameterised `pg_ripple.*` SQL function — no
//!      string concatenation of user input into SQL.
//!   4. Maps the result to a JSON response.
//!   5. Records metrics via `state.metrics`.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::common::{AppState, check_auth, check_auth_write, redacted_error};

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Read up to 10 MiB from a request body.
#[allow(clippy::result_large_err)]
pub(crate) async fn read_body(body: Body) -> Result<String, Response> {
    let bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            // v0.61.0 H7-6: wrap 413 in a JSON envelope with PT404 error code.
            return Err(json_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({
                    "error": "PT404",
                    "message": "request body exceeds maximum allowed size (10 MiB)"
                }),
            ));
        }
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    // SAFETY: status and header values are compile-time constants; builder never fails.
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("infallible: hardcoded valid HTTP headers")
}

// v0.96.0 M15-14: re-export handlers from split sub-modules
pub use super::datalog_admin::{
    cache_stats, create_lattice, create_view, drop_view, list_lattices, list_views, tabling_stats,
};
pub use super::datalog_inference::{
    check_constraints, check_constraints_all, infer, infer_agg, infer_demand, infer_lattice,
    infer_wfs, infer_with_stats, query_goal,
};

// ─── Error classification ─────────────────────────────────────────────────────

/// Map a PostgreSQL error message to an HTTP error category and status code.
pub(crate) fn classify_pg_error(msg: &str) -> (&'static str, StatusCode) {
    let lower = msg.to_lowercase();
    if lower.contains("parse") || lower.contains("syntax") || lower.contains("invalid rule") {
        ("datalog_parse_error", StatusCode::BAD_REQUEST)
    } else if lower.contains("does not exist") || lower.contains("not found") {
        ("rule_set_not_found", StatusCode::NOT_FOUND)
    } else {
        ("datalog_error", StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// Map a goal-query PostgreSQL error message to an HTTP error category.
pub(crate) fn classify_pg_goal_error(msg: &str) -> (&'static str, StatusCode) {
    let lower = msg.to_lowercase();
    if lower.contains("parse") || lower.contains("syntax") || lower.contains("invalid goal") {
        ("datalog_goal_error", StatusCode::BAD_REQUEST)
    } else if lower.contains("does not exist") || lower.contains("not found") {
        ("rule_set_not_found", StatusCode::NOT_FOUND)
    } else {
        ("datalog_error", StatusCode::INTERNAL_SERVER_ERROR)
    }
}

// ─── Phase 1 — Rule management ────────────────────────────────────────────────

/// `POST /datalog/rules/{rule_set}`
///
/// Body: Datalog rule text (`text/x-datalog` or `text/plain`).
/// Calls `pg_ripple.load_rules($1, $2)`.
/// Returns `{"rule_set": "…", "rules_loaded": N}`.
pub async fn load_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let rule_text = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if rule_text.trim().is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_request", "detail": "empty rule body"}),
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

    // load_rules(rule_text, rule_set) — note argument order matches SQL signature
    let row = match client
        .query_one(
            "SELECT pg_ripple.load_rules($1, $2)",
            &[&rule_text, &rule_set],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let rules_loaded: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::OK,
        serde_json::json!({"rule_set": rule_set, "rules_loaded": rules_loaded}),
    )
}

/// `POST /datalog/rules/{rule_set}/builtin`
///
/// Calls `pg_ripple.load_rules_builtin($1)`.
/// Returns `{"rule_set": "…", "rules_loaded": N}`.
pub async fn load_builtin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.load_rules_builtin($1)", &[&rule_set])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let rules_loaded: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::OK,
        serde_json::json!({"rule_set": rule_set, "rules_loaded": rules_loaded}),
    )
}

/// `GET /datalog/rules`
///
/// Calls `pg_ripple.list_rules()`. Returns JSONB array.
pub async fn list_rules(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    let row = match client.query_one("SELECT pg_ripple.list_rules()", &[]).await {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("list_rules failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `DELETE /datalog/rules/{rule_set}`
///
/// Calls `pg_ripple.drop_rules($1)`. Returns `{"deleted": N}`.
pub async fn drop_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.drop_rules($1)", &[&rule_set])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let deleted: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"deleted": deleted}))
}

/// `POST /datalog/rules/{rule_set}/add`
///
/// Body: single Datalog rule text. Calls `pg_ripple.add_rule($1, $2)`.
/// Returns `{"rule_set": "…", "rule_id": N}`.
pub async fn add_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let rule_text = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if rule_text.trim().is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_request", "detail": "empty rule body"}),
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

    let row = match client
        .query_one(
            "SELECT pg_ripple.add_rule($1, $2)",
            &[&rule_set, &rule_text],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let rule_id: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::OK,
        serde_json::json!({"rule_set": rule_set, "rule_id": rule_id}),
    )
}

/// `DELETE /datalog/rules/{rule_set}/{rule_id}`
///
/// Calls `pg_ripple.remove_rule($1::bigint)` (triggers DRed).
/// Returns `{"removed": N}`.
pub async fn remove_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((_rule_set, rule_id_str)): Path<(String, String)>,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }

    let rule_id: i64 = match rule_id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "error": "invalid_request",
                    "detail": "rule_id must be a non-negative integer"
                }),
            );
        }
    };

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

    let row = match client
        .query_one("SELECT pg_ripple.remove_rule($1)", &[&rule_id])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let removed: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"removed": removed}))
}

/// `PUT /datalog/rules/{rule_set}/enable`
///
/// Calls `pg_ripple.enable_rule_set($1)`. Returns `{"rule_set": "…", "enabled": true}`.
pub async fn enable_rule_set(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    rule_set_toggle(&state, &rule_set, true).await
}

/// `PUT /datalog/rules/{rule_set}/disable`
///
/// Calls `pg_ripple.disable_rule_set($1)`. Returns `{"rule_set": "…", "enabled": false}`.
pub async fn disable_rule_set(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    rule_set_toggle(&state, &rule_set, false).await
}

async fn rule_set_toggle(state: &AppState, rule_set: &str, enable: bool) -> Response {
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

    let sql = if enable {
        "SELECT pg_ripple.enable_rule_set($1)"
    } else {
        "SELECT pg_ripple.disable_rule_set($1)"
    };

    if let Err(e) = client.execute(sql, &[&rule_set]).await {
        state.metrics.record_error();
        let msg = e.to_string();
        let (cat, status) = classify_pg_error(&msg);
        return redacted_error(cat, &msg, status);
    }

    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::OK,
        serde_json::json!({"rule_set": rule_set, "enabled": enable}),
    )
}
