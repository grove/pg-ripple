//! RAG endpoint handler -- extracted from routing.rs (MOD-01, v0.72.0).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::common::{AppState, check_auth, redacted_error};
// Re-use types declared in parent routing module.
use super::{RagRequest, RagResponse, RagResult, SparqlParams};

use super::sparql_handlers::negotiate_accept;

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
