//! SPARQL execution helpers — low-level PostgreSQL wire calls.
//!
//! Contains `execute_sparql_with_traceparent` and the per-form dispatch
//! functions `execute_select`, `execute_ask`, `execute_construct`, and
//! `execute_describe`. All caller-visible formatting stays in `routing`.

use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::common::{AppState, json_error, redacted_error};
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

/// A13-06 (v0.86.0): Detect whether a PostgreSQL error message is a SPARQL
/// parse error emitted by the pg_ripple extension.
///
/// The extension calls `pgrx::error!("SPARQL parse error: {e}")` for query
/// parse failures.  We match on that prefix so the HTTP companion can return
/// the standardised `PT400_SPARQL_PARSE` error code.
fn is_sparql_parse_error(e: &tokio_postgres::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("sparql parse error")
        || msg.contains("sparql_parse_error")
        || msg.contains("pt400_sparql_parse")
}

pub(crate) async fn execute_sparql_with_traceparent(
    state: &Arc<AppState>,
    query_text: &str,
    is_update: bool,
    accept: &str,
    traceparent: Option<&str>,
) -> Response {
    execute_sparql_with_traceparent_routed(
        state,
        query_text,
        is_update,
        accept,
        traceparent,
        false,
        None,
    )
    .await
}

/// Execute a read query through the v0.135 public binding overloads.
pub(crate) async fn execute_sparql_with_bindings(
    state: &Arc<AppState>,
    query_text: &str,
    bindings: &serde_json::Value,
    accept: &str,
    traceparent: Option<&str>,
    use_replica: bool,
    timeout_override_ms: Option<u64>,
    prefix_mode: Option<&str>,
) -> Response {
    let client = if use_replica {
        match state.replica_pool.as_ref() {
            Some(pool) => match pool.get().await {
                Ok(client) => Ok(client),
                Err(_) => state.pool.get().await,
            },
            None => state.pool.get().await,
        }
    } else {
        state.pool.get().await
    };
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            return redacted_error(
                "service_unavailable",
                &error.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };
    let query_kind = if crate::stream::is_ask_query(query_text) {
        "ASK"
    } else if crate::stream::is_construct_query(query_text) {
        "CONSTRUCT"
    } else if crate::stream::is_describe_query(query_text) {
        "DESCRIBE"
    } else if crate::stream::is_select_query(query_text) {
        "SELECT"
    } else {
        return json_error(
            "PT0579",
            "bindings require a SELECT, ASK, CONSTRUCT, or DESCRIBE query",
            StatusCode::BAD_REQUEST,
        );
    };

    let start = Instant::now();
    if client.batch_execute("BEGIN").await.is_err() {
        return redacted_error(
            "service_unavailable",
            "could not start query transaction",
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }
    let mode = prefix_mode.unwrap_or("strict");
    let _ = client
        .execute(
            "SELECT set_config('pg_ripple.sparql_prefix_mode', $1, true)",
            &[&mode],
        )
        .await;
    if let Some(traceparent) = traceparent.filter(|value| is_valid_traceparent(value)) {
        let _ = client
            .execute(
                "SELECT set_config('pg_ripple.tracing_traceparent', $1, true)",
                &[&traceparent],
            )
            .await;
    }
    if let Some(timeout_ms) = timeout_override_ms.filter(|value| *value > 0) {
        let _ = client
            .execute(
                "SELECT set_config('statement_timeout', $1, true)",
                &[&format!("{timeout_ms}ms")],
            )
            .await;
    }

    let query_rows = match query_kind {
        "SELECT" | "ASK" => {
            client
                .query(
                    "SELECT result FROM pg_ripple.sparql($1, $2)",
                    &[&query_text, bindings],
                )
                .await
        }
        "CONSTRUCT" => {
            client
                .query(
                    "SELECT result FROM pg_ripple.sparql_construct($1, $2)",
                    &[&query_text, bindings],
                )
                .await
        }
        _ => {
            client
                .query(
                    "SELECT result FROM pg_ripple.sparql_describe($1, $2, $3)",
                    &[&query_text, bindings, &"cbd"],
                )
                .await
        }
    };
    let rows = match query_rows {
        Ok(rows) => rows,
        Err(error) => {
            let _ = client.batch_execute("ROLLBACK").await;
            return redacted_error(
                "sparql_query_error",
                &error.to_string(),
                StatusCode::BAD_REQUEST,
            );
        }
    };
    let _ = client.batch_execute("COMMIT").await;
    let results = rows
        .iter()
        .map(|row| row.get(0))
        .collect::<Vec<serde_json::Value>>();
    match query_kind {
        "SELECT" => {
            state
                .metrics
                .record_query_typed(start.elapsed(), query_kind, results.len());
            format_select_results(&results, accept)
        }
        "ASK" => {
            let result = results
                .first()
                .and_then(|value| value.get("result"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == "true");
            state.metrics.record_query_typed(
                start.elapsed(),
                query_kind,
                if result { 1 } else { 0 },
            );
            format_ask_result(result, accept)
        }
        _ => {
            let triples = results
                .iter()
                .filter_map(|value| {
                    let object = value.as_object()?;
                    Some((
                        object.get("s")?.as_str()?.to_owned(),
                        object.get("p")?.as_str()?.to_owned(),
                        object.get("o")?.as_str()?.to_owned(),
                    ))
                })
                .collect::<Vec<_>>();
            state
                .metrics
                .record_query_typed(start.elapsed(), query_kind, triples.len());
            format_graph_results(&triples, accept)
        }
    }
}

/// Internal version with explicit replica-routing flag.
///
/// Feature 12 (v0.120.0): when `use_replica` is `true` AND `state.replica_pool`
/// is configured AND `is_update` is `false`, the query is sent to the replica
/// pool instead of the primary.  Falls back to the primary when the replica is
/// unavailable.
pub(crate) async fn execute_sparql_with_traceparent_routed(
    state: &Arc<AppState>,
    query_text: &str,
    is_update: bool,
    accept: &str,
    traceparent: Option<&str>,
    use_replica: bool,
    timeout_override_ms: Option<u64>,
) -> Response {
    let start = Instant::now();

    if !is_update && crate::stream::is_streamable(query_text, accept) {
        return crate::stream::stream_sparql(
            state,
            query_text,
            accept,
            traceparent,
            use_replica,
            timeout_override_ms,
        )
        .await;
    }

    // Feature 12 (v0.120.0): replica routing.
    // Only read-only queries can be sent to the replica; updates always go primary.
    let client = if use_replica && !is_update {
        if let Some(replica_pool) = &state.replica_pool {
            match replica_pool.get().await {
                Ok(c) => {
                    tracing::debug!("?replica=ok: routing read-only SPARQL query to replica");
                    c
                }
                Err(e) => {
                    tracing::warn!(
                        "?replica=ok: replica pool unavailable ({}), falling back to primary",
                        e
                    );
                    match state.pool.get().await {
                        Ok(c) => c,
                        Err(e) => {
                            state.metrics.record_error();
                            return redacted_error(
                                "service_unavailable",
                                &format!("pool error: {e}"),
                                StatusCode::SERVICE_UNAVAILABLE,
                            );
                        }
                    }
                }
            }
        } else {
            // No replica pool configured — use primary silently.
            match state.pool.get().await {
                Ok(c) => c,
                Err(e) => {
                    state.metrics.record_error();
                    return redacted_error(
                        "service_unavailable",
                        &format!("pool error: {e}"),
                        StatusCode::SERVICE_UNAVAILABLE,
                    );
                }
            }
        }
    } else {
        match state.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                state.metrics.record_error();
                return redacted_error(
                    "service_unavailable",
                    &format!("pool error: {e}"),
                    StatusCode::SERVICE_UNAVAILABLE,
                );
            }
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
                state.metrics.record_query_typed(elapsed, "UPDATE", 0);
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
        let is_ask = crate::stream::is_ask_query(query_text);
        let is_construct = crate::stream::is_construct_query(query_text);
        let is_describe = crate::stream::is_describe_query(query_text);

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
            if is_sparql_parse_error(&e) {
                return json_error(
                    "PT400_SPARQL_PARSE",
                    "SPARQL parse error — check query syntax",
                    StatusCode::BAD_REQUEST,
                );
            }
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
    state
        .metrics
        .record_query_typed(elapsed, "SELECT", results.len());

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
            if is_sparql_parse_error(&e) {
                return json_error(
                    "PT400_SPARQL_PARSE",
                    "SPARQL parse error — check query syntax",
                    StatusCode::BAD_REQUEST,
                );
            }
            return redacted_error(
                "sparql_ask_error",
                &format!("SPARQL ASK error: {e}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let result: bool = row.get(0);
    let elapsed = start.elapsed();
    state
        .metrics
        .record_query_typed(elapsed, "ASK", if result { 1 } else { 0 });

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
            if is_sparql_parse_error(&e) {
                return json_error(
                    "PT400_SPARQL_PARSE",
                    "SPARQL parse error — check query syntax",
                    StatusCode::BAD_REQUEST,
                );
            }
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
    state
        .metrics
        .record_query_typed(elapsed, "CONSTRUCT", triples.len());

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
            if is_sparql_parse_error(&e) {
                return json_error(
                    "PT400_SPARQL_PARSE",
                    "SPARQL parse error — check query syntax",
                    StatusCode::BAD_REQUEST,
                );
            }
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
    state
        .metrics
        .record_query_typed(elapsed, "DESCRIBE", triples.len());

    format_graph_results(&triples, accept)
}
