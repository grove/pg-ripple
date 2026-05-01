//! RAG endpoint handler -- extracted from routing.rs (MOD-01, v0.72.0).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::common::{AppState, check_auth, redacted_error};
// Re-use types declared in parent routing module.
use super::{RagRequest, RagResponse, RagResult};

pub(crate) async fn rag_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
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

    // v0.22.0 S-4 (fixed v0.81.0 RAG-SQL-INJECT-02): parameterized queries prevent SQL injection.
    // The previous implementation used format!() + replace('\'', "''") which is not
    // parameterized query execution and fails under standard_conforming_strings=off.
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

    let sparql_filter_val: Option<String> = req.sparql_filter.clone();
    let model_val: Option<String> = req.model.clone();
    let output_format = if req.output_format == "jsonld" {
        "jsonld"
    } else {
        "jsonb"
    };

    let rows = match client
        .query(
            "SELECT entity_iri, label, context_json, distance \
         FROM pg_ripple.rag_retrieve($1, sparql_filter := $2::text, \
           k := $3, model := $4::text, output_format := $5)",
            &[
                &req.question,
                &sparql_filter_val,
                &req.k,
                &model_val,
                &output_format,
            ],
        )
        .await
    {
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
