//! Temporal fact HTTP handlers (v0.115.0 M16-02).
//!
//! GET  /temporal/mark            — list all temporal predicates
//! POST /temporal/mark            — mark a predicate as temporal
//! POST /temporal/point_in_time   — set or clear the temporal snapshot point
//! GET  /temporal/facts           — query temporal facts window

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, check_auth, check_auth_write, format_nquads_term, redacted_error};

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    json_response_http(status, body)
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MarkTemporalBody {
    pub predicate_iri: String,
    #[serde(default = "default_data_model")]
    pub data_model: String,
}

fn default_data_model() -> String {
    "snapshot".to_owned()
}

#[derive(Debug, Deserialize)]
pub struct PointInTimeBody {
    /// RFC 3339 timestamp string, or null/omitted to clear the snapshot point.
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TemporalFactsParams {
    pub predicate_iri: Option<String>,
    pub subject_iri: Option<String>,
}

// ── GET /temporal/mark ────────────────────────────────────────────────────────

/// Returns the list of predicates currently marked as temporal together with
/// their data model.
pub(crate) async fn list_temporal_predicates(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };
    let rows = match client
        .query(
            "SELECT predicate_iri, data_model \
             FROM _pg_ripple.temporal_predicates \
             ORDER BY predicate_iri",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "query_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let predicates: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "predicate_iri": row.get::<_, String>(0),
                "data_model": row.get::<_, String>(1),
            })
        })
        .collect();
    json_response(
        StatusCode::OK,
        serde_json::json!({ "predicates": predicates }),
    )
}

// ── POST /temporal/mark ───────────────────────────────────────────────────────

/// Mark a predicate as temporal with the given data model.
pub(crate) async fn mark_temporal_predicate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "read_error"}),
            );
        }
    };
    let req: MarkTemporalBody = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_json", "detail": format!("{e}")}),
            );
        }
    };
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };
    match client
        .execute(
            "SELECT pg_ripple.mark_temporal($1, $2)",
            &[&req.predicate_iri, &req.data_model],
        )
        .await
    {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "status": "ok",
                "predicate_iri": req.predicate_iri,
                "data_model": req.data_model,
            }),
        ),
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "mark_temporal_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

// ── POST /temporal/point_in_time ─────────────────────────────────────────────

/// Set or clear the temporal snapshot point.
///
/// Pass `{"timestamp": "2024-01-01T00:00:00Z"}` to set it, or omit/null to clear.
pub(crate) async fn set_point_in_time(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "read_error"}),
            );
        }
    };
    let req: PointInTimeBody = if bytes.is_empty() {
        PointInTimeBody { timestamp: None }
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"error": "invalid_json", "detail": format!("{e}")}),
                );
            }
        }
    };
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };
    let result = if let Some(ts) = &req.timestamp {
        client
            .execute("SELECT pg_ripple.point_in_time($1::TIMESTAMPTZ)", &[ts])
            .await
    } else {
        client
            .execute("SELECT pg_ripple.clear_point_in_time()", &[])
            .await
    };
    match result {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({ "status": "ok", "timestamp": req.timestamp }),
        ),
        Err(e) => {
            state.metrics.record_error();
            redacted_error(
                "point_in_time_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

// ── GET /temporal/facts ───────────────────────────────────────────────────────

/// Query temporal facts for a predicate (and optional subject).
pub(crate) async fn temporal_facts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<TemporalFactsParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }
    // M16-03: increment temporal query counter.
    state.metrics.record_temporal_query();

    let predicate_iri = match &params.predicate_iri {
        Some(p) => p.clone(),
        None => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "missing_param", "detail": "predicate_iri is required"}),
            );
        }
    };
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let (sql, rows_result) = if let Some(subject_iri) = &params.subject_iri {
        let s = "SELECT s_value, p_value, o_value, valid_from::TEXT, valid_to::TEXT \
                  FROM pg_ripple.temporal_window($1) \
                  WHERE s_value = $2 \
                  ORDER BY valid_from";
        (s, client.query(s, &[&predicate_iri, subject_iri]).await)
    } else {
        let s = "SELECT s_value, p_value, o_value, valid_from::TEXT, valid_to::TEXT \
                  FROM pg_ripple.temporal_window($1) \
                  ORDER BY valid_from";
        (s, client.query(s, &[&predicate_iri]).await)
    };

    let _ = sql; // suppress unused warning
    let rows = match rows_result {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "temporal_facts_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let facts: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "subject":    row.get::<_, String>(0),
                "predicate":  row.get::<_, String>(1),
                "object":     row.get::<_, String>(2),
                "valid_from": row.get::<_, Option<String>>(3),
                "valid_to":   row.get::<_, Option<String>>(4),
            })
        })
        .collect();

    json_response(
        StatusCode::OK,
        serde_json::json!({ "facts": facts, "count": facts.len() }),
    )
}

// ── GET /temporal/graphs/{iri}/snapshot?at=<iso8601> ─────────────────────────

/// Query parameters for the snapshot endpoint.
#[derive(Debug, Deserialize)]
pub struct GraphSnapshotParams {
    /// ISO 8601 / RFC 3339 timestamp string.
    pub at: String,
}

/// `GET /temporal/graphs/{iri}/snapshot?at=<iso8601>`
///
/// Calls `pg_ripple.graph_at(graph_iri, at)` and returns the registered
/// snapshot IRI together with the snapshot content encoded as Turtle
/// (all temporal facts valid at `at` for the requested named graph).
pub(crate) async fn graph_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(iri): axum::extract::Path<String>,
    Query(params): Query<GraphSnapshotParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // 1. Register (or return cached) snapshot IRI.
    let snapshot_iri_row = match client
        .query_one(
            "SELECT pg_ripple.graph_at($1, $2::TIMESTAMPTZ)",
            &[&iri, &params.at],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "graph_at_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let snapshot_iri: String = snapshot_iri_row.get(0);

    // 2. Fetch the temporal facts valid at `at` and serialise as Turtle.
    let rows = match client
        .query(
            "SELECT \
               pg_ripple.decode(s) AS s_val, \
               pg_ripple.decode(p) AS p_val, \
               pg_ripple.decode(o) AS o_val \
             FROM _pg_ripple.temporal_facts tf \
             JOIN _pg_ripple.dictionary dg ON dg.id = tf.g \
             WHERE dg.value = $1 \
               AND tf.valid_from <= $2::TIMESTAMPTZ \
               AND (tf.valid_to IS NULL OR tf.valid_to > $2::TIMESTAMPTZ) \
             ORDER BY tf.s, tf.p, tf.o",
            &[&iri, &params.at],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "snapshot_facts_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    // Serialise as minimal Turtle (one triple per line, no prefix shortening).
    let mut turtle = format!(
        "# Snapshot: {snapshot_iri}\n# Graph: {iri}\n# At: {}\n\n",
        params.at
    );
    for row in &rows {
        let s: String = row.get(0);
        let p: String = row.get(1);
        let o: String = row.get(2);
        turtle.push_str(&format!(
            "{} {} {} .\n",
            format_nquads_term(&s),
            format_nquads_term(&p),
            format_nquads_term(&o)
        ));
    }

    // Update the snapshot gauge.
    if let Ok(count_row) = client
        .query_one("SELECT pg_ripple.graph_snapshots_count()", &[])
        .await
    {
        let count: i64 = count_row.get(0);
        state.metrics.update_graph_snapshots_total(count as u64);
    }

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/turtle; charset=utf-8")
        .header("X-Snapshot-IRI", snapshot_iri)
        .body(axum::body::Body::from(turtle))
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "response_build_error"}),
            )
        })
}

// ── GET /temporal/graphs/{iri}/diff?from=<iso8601>&to=<iso8601> ──────────────

/// Query parameters for the diff endpoint.
#[derive(Debug, Deserialize)]
pub struct GraphDiffParams {
    /// ISO 8601 start timestamp (inclusive).
    pub from: String,
    /// ISO 8601 end timestamp (exclusive).
    pub to: String,
}

/// `GET /temporal/graphs/{iri}/diff?from=<iso8601>&to=<iso8601>`
///
/// Returns the N-Quads delta between two temporal snapshots of a named graph.
/// Each line is a quad in N-Quads syntax preceded by a `# added` / `# removed`
/// comment to support streaming audit-compliance consumers.
pub(crate) async fn graph_diff(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(iri): axum::extract::Path<String>,
    Query(params): Query<GraphDiffParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "db_pool_error",
                &e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let rows = match client
        .query(
            "SELECT \
               pg_ripple.decode(d.s) AS s_val, \
               pg_ripple.decode(d.p) AS p_val, \
               pg_ripple.decode(d.o) AS o_val, \
               d.change \
             FROM pg_ripple.graph_diff($1, $2::TIMESTAMPTZ, $3::TIMESTAMPTZ) d \
             ORDER BY d.change, d.s, d.p, d.o",
            &[&iri, &params.from, &params.to],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "graph_diff_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let graph_term = format_nquads_term(&iri);
    let mut nquads = format!(
        "# Graph: {iri}\n# From: {}\n# To: {}\n\n",
        params.from, params.to
    );
    for row in &rows {
        let s: String = row.get(0);
        let p: String = row.get(1);
        let o: String = row.get(2);
        let change: String = row.get(3);

        nquads.push_str(&format!(
            "# {change}\n{} {} {} {graph_term} .\n",
            format_nquads_term(&s),
            format_nquads_term(&p),
            format_nquads_term(&o)
        ));
    }

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/n-quads; charset=utf-8")
        .body(axum::body::Body::from(nquads))
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "response_build_error"}),
            )
        })
}
