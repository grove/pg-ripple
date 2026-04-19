//! Datalog REST API handlers for pg_ripple_http.
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
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::common::{AppState, check_auth, check_auth_write, redacted_error};

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Read up to 10 MiB from a request body.
async fn read_body(body: Body) -> Result<String, Response> {
    let bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response());
        }
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
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

// ─── Phase 2 — Inference ──────────────────────────────────────────────────────

/// `POST /datalog/infer/{rule_set}`
///
/// Calls `pg_ripple.infer($1)`. Returns `{"derived": N}`.
pub async fn infer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.infer($1)", &[&rule_set])
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

    let derived: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"derived": derived}))
}

/// `POST /datalog/infer/{rule_set}/stats`
///
/// Calls `pg_ripple.infer_with_stats($1)`. Returns full stats JSONB.
pub async fn infer_with_stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.infer_with_stats($1)", &[&rule_set])
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

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `POST /datalog/infer/{rule_set}/agg`
///
/// Calls `pg_ripple.infer_agg($1)`. Returns `{"derived": N}`.
pub async fn infer_agg(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.infer_agg($1)", &[&rule_set])
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

    let derived: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"derived": derived}))
}

/// `POST /datalog/infer/{rule_set}/wfs`
///
/// Calls `pg_ripple.infer_wfs($1)`. Returns `{"derived": N}`.
pub async fn infer_wfs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
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

    let row = match client
        .query_one("SELECT pg_ripple.infer_wfs($1)", &[&rule_set])
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

    let derived: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"derived": derived}))
}

/// `POST /datalog/infer/{rule_set}/demand`
///
/// Body: `{"demands": […]}` JSON. Calls `pg_ripple.infer_demand($1, $2::jsonb)`.
/// Returns full JSONB stats from the extension.
pub async fn infer_demand(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    let body_str = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    // Validate that the body is valid JSON.
    let demands_json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_request", "detail": format!("invalid JSON body: {e}")}),
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
        .query_one(
            "SELECT pg_ripple.infer_demand($1, $2::jsonb)",
            &[&rule_set, &demands_json],
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

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `POST /datalog/infer/{rule_set}/lattice`
///
/// Body: `{"lattice": "min"}` JSON. Calls `pg_ripple.infer_lattice($1, $2)`.
/// Returns `{"derived": N}`.
#[derive(Deserialize)]
pub struct LatticeBody {
    pub lattice: String,
}

pub async fn infer_lattice(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    let body_str = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let req: LatticeBody = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_request", "detail": format!("expected {{\"lattice\": \"…\"}}: {e}")}),
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
        .query_one(
            "SELECT pg_ripple.infer_lattice($1, $2)",
            &[&rule_set, &req.lattice],
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

    let derived: i64 = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"derived": derived}))
}

// ─── Phase 3 — Query & constraints ────────────────────────────────────────────

/// `POST /datalog/query/{rule_set}`
///
/// Body: Datalog goal text (`text/x-datalog` or `text/plain`).
/// Calls `pg_ripple.infer_goal($1, $2)`.
/// Returns `{"derived": N, "iterations": N, "matching": […]}`.
pub async fn query_goal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    let goal_text = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if goal_text.trim().is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_request", "detail": "empty goal body"}),
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
            "SELECT pg_ripple.infer_goal($1, $2)",
            &[&rule_set, &goal_text],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            let msg = e.to_string();
            let (cat, status) = classify_pg_goal_error(&msg);
            return redacted_error(cat, &msg, status);
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `GET /datalog/constraints`
///
/// Calls `pg_ripple.check_constraints(NULL)`. Returns violation array.
pub async fn check_constraints_all(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    check_constraints_inner(&state, None).await
}

/// `GET /datalog/constraints/{rule_set}`
///
/// Calls `pg_ripple.check_constraints($1)`. Returns violation array.
pub async fn check_constraints(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    check_constraints_inner(&state, Some(&rule_set)).await
}

async fn check_constraints_inner(state: &AppState, rule_set: Option<&str>) -> Response {
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
        .query_one("SELECT pg_ripple.check_constraints($1)", &[&rule_set])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("check_constraints failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

// ─── Phase 4 — Admin & monitoring ─────────────────────────────────────────────

/// `GET /datalog/stats/cache`
///
/// Calls `pg_ripple.rule_plan_cache_stats()`.
pub async fn cache_stats(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    let row = match client
        .query_one("SELECT pg_ripple.rule_plan_cache_stats()", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("rule_plan_cache_stats failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `GET /datalog/stats/tabling`
///
/// Calls `pg_ripple.tabling_stats()`.
pub async fn tabling_stats(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    let row = match client
        .query_one("SELECT pg_ripple.tabling_stats()", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("tabling_stats failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `GET /datalog/lattices`
///
/// Calls `pg_ripple.list_lattices()`.
pub async fn list_lattices(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    let row = match client
        .query_one("SELECT pg_ripple.list_lattices()", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("list_lattices failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `POST /datalog/lattices`
///
/// Body: `{"name": "…", "join_fn": "…", "bottom": "…"}` JSON.
/// Calls `pg_ripple.create_lattice($1, $2, $3)`.
#[derive(Deserialize)]
pub struct CreateLatticeBody {
    pub name: String,
    pub join_fn: String,
    pub bottom: String,
}

pub async fn create_lattice(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let body_str = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let req: CreateLatticeBody = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_request", "detail": format!("expected {{\"name\", \"join_fn\", \"bottom\"}}: {e}")}),
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

    if let Err(e) = client
        .execute(
            "SELECT pg_ripple.create_lattice($1, $2, $3)",
            &[&req.name, &req.join_fn, &req.bottom],
        )
        .await
    {
        state.metrics.record_error();
        return redacted_error(
            "datalog_error",
            &format!("create_lattice failed: {e}"),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::CREATED,
        serde_json::json!({"created": req.name}),
    )
}

/// `GET /datalog/views`
///
/// Calls `pg_ripple.list_datalog_views()`.
pub async fn list_views(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    let row = match client
        .query_one("SELECT pg_ripple.list_datalog_views()", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "datalog_error",
                &format!("list_datalog_views failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let result: serde_json::Value = row.get(0);
    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, result)
}

/// `POST /datalog/views`
///
/// Body: JSON object with view definition fields.
/// Calls `pg_ripple.create_datalog_view(name, rules, goal, rule_set, schedule, decode)`.
#[derive(Deserialize)]
pub struct CreateViewBody {
    pub name: String,
    pub rules: Option<String>,
    pub goal: String,
    pub rule_set: Option<String>,
    pub schedule: Option<String>,
    pub decode: Option<bool>,
}

pub async fn create_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let body_str = match read_body(body).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let req: CreateViewBody = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_request", "detail": format!("invalid view definition: {e}")}),
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

    if let Err(e) = client
        .execute(
            "SELECT pg_ripple.create_datalog_view($1, $2, $3, $4, $5, $6)",
            &[
                &req.name,
                &req.rules,
                &req.goal,
                &req.rule_set,
                &req.schedule,
                &req.decode,
            ],
        )
        .await
    {
        state.metrics.record_error();
        return redacted_error(
            "datalog_error",
            &format!("create_datalog_view failed: {e}"),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    state.metrics.record_datalog_query(start.elapsed());
    json_response(
        StatusCode::CREATED,
        serde_json::json!({"created": req.name}),
    )
}

/// `DELETE /datalog/views/{name}`
///
/// Calls `pg_ripple.drop_datalog_view($1)`.
pub async fn drop_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
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

    if let Err(e) = client
        .execute("SELECT pg_ripple.drop_datalog_view($1)", &[&name])
        .await
    {
        state.metrics.record_error();
        let msg = e.to_string();
        let (cat, status) = classify_pg_error(&msg);
        return redacted_error(cat, &msg, status);
    }

    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::json!({"dropped": name}))
}

// ─── Error classification ─────────────────────────────────────────────────────

/// Map a PostgreSQL error message to an HTTP error category and status code.
fn classify_pg_error(msg: &str) -> (&'static str, StatusCode) {
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
fn classify_pg_goal_error(msg: &str) -> (&'static str, StatusCode) {
    let lower = msg.to_lowercase();
    if lower.contains("parse") || lower.contains("syntax") || lower.contains("invalid goal") {
        ("datalog_goal_error", StatusCode::BAD_REQUEST)
    } else if lower.contains("does not exist") || lower.contains("not found") {
        ("rule_set_not_found", StatusCode::NOT_FOUND)
    } else {
        ("datalog_error", StatusCode::INTERNAL_SERVER_ERROR)
    }
}
