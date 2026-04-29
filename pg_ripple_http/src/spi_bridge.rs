//! SPARQL execution helpers — low-level PostgreSQL wire calls.
//!
//! Contains `execute_sparql_with_traceparent` and the per-form dispatch
//! functions `execute_select`, `execute_ask`, `execute_construct`, and
//! `execute_describe`. All caller-visible formatting stays in `routing`.

use std::time::Instant;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::common::{AppState, redacted_error};
use crate::routing::{format_ask_result, format_graph_results, format_select_results};

// ─── SPARQL execution ────────────────────────────────────────────────────────

/// Validate a W3C traceparent header value.
///
/// A valid traceparent has the form: `00-{32hex}-{16hex}-{2hex}`
/// e.g. `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
fn is_valid_traceparent(tp: &str) -> bool {
    // Total length: 2 + 1 + 32 + 1 + 16 + 1 + 2 = 55 characters
    tp.len() == 55 && tp.starts_with("00-") && tp.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub(crate) async fn execute_sparql_with_traceparent(
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
