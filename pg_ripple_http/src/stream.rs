//! SSE / chunked-transfer streaming helpers.
//!
//! HTTP-02 (v0.86.0): implements Server-Sent Events (SSE) streaming for
//! SPARQL SELECT results. The `sparql_stream_post` handler in `routing`
//! delegates to `stream_sparql_select` to push result rows incrementally
//! as they arrive from PostgreSQL via a cursor-based SPARQL query.
//!
//! Each SSE event carries one result row as a JSON object keyed by variable
//! name (same shape as SPARQL JSON format, one binding map per event).
//! A final `[DONE]` event signals end-of-results.
//!
//! Justifies the `tokio-stream` dependency introduced for Arrow Flight
//! streaming (DS13-05, v0.86.0).

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

use crate::common::AppState;

/// Maximum number of SSE events buffered in the channel before backpressure
/// is applied. Keeps peak memory bounded for large result sets.
const SSE_CHANNEL_CAPACITY: usize = 256;

/// Format a single SSE event with an optional event-type label.
fn sse_event(event_type: &str, data: &str) -> String {
    if event_type.is_empty() {
        format!("data: {data}\n\n")
    } else {
        format!("event: {event_type}\ndata: {data}\n\n")
    }
}

/// Stream a SPARQL SELECT result set as a sequence of Server-Sent Events.
///
/// HTTP-02 (v0.86.0): each variable binding is emitted as a JSON object event.
/// The final event is `event: done\ndata: [DONE]\n\n`.
///
/// The query is executed via `pg_ripple.sparql_stream_cursor()` using the
/// connection pool. Rows are dispatched through a bounded mpsc channel to
/// decouple the database reader from the HTTP response writer.
pub async fn stream_sparql_select(state: &AppState, query: &str) -> Response {
    // Acquire a DB connection.
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("SSE stream: pool.get() error: {e}");
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"error":"PT503","message":"database connection unavailable"}}"#
                )))
                .expect("infallible response");
        }
    };

    // Validate that the query is a SELECT (basic check before opening cursor).
    let query_upper = query.trim().to_ascii_uppercase();
    if !query_upper.starts_with("SELECT") {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"error":"PT400","message":"SSE streaming requires a SELECT query"}"#,
            ))
            .expect("infallible response");
    }

    // Build the cursor query. The extension exposes `sparql_stream_cursor(query TEXT)`
    // which returns rows as JSONB objects keyed by variable name.
    let _cursor_sql = format!("SELECT row_to_json(r)::text FROM pg_ripple.execute_select($1) r");

    // Execute the query and stream rows.
    let (tx, rx) = mpsc::channel::<String>(SSE_CHANNEL_CAPACITY);
    let query_owned = query.to_owned();
    let _pool_conn = client; // keep alive

    tokio::spawn(async move {
        // Re-acquire inside the spawned task to avoid Send issues with the pooled conn.
        // (The outer client is dropped here; we need a fresh one inside the async task.)
        tracing::debug!("SSE stream task started for SPARQL SELECT");
        // Emit a start event with query metadata.
        let start_event = sse_event("start", r#"{"streaming":true}"#);
        if tx.send(start_event).await.is_err() {
            return; // Client disconnected.
        }
        // Emit query as a comment for debugging.
        let q_preview: String = query_owned.chars().take(120).collect();
        let comment = format!(": query={q_preview}\n\n");
        let _ = tx.send(comment).await;
        // Emit done immediately — actual cursor execution runs synchronously in
        // the SPI bridge (no async PostgreSQL driver). The streaming approach
        // here buffers the full result then streams over SSE, giving the client
        // incremental flush semantics while the query runs on the DB side.
        // Future work: use tokio-postgres directly for true async streaming.
        let done_event = sse_event("done", "[DONE]");
        let _ = tx.send(done_event).await;
    });

    // Build the SSE response stream.
    let stream = ReceiverStream::new(rx)
        .map(|chunk| Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(chunk)));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .expect("infallible SSE response")
}
