//! Direct PostgreSQL-to-HTTP streaming for the SPARQL protocol.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use axum::response::Response;
use deadpool_postgres::Object;
use futures_util::{StreamExt, stream};
use tokio_postgres::{CancelToken, RowStream};
use uuid::Uuid;

use crate::common::{AppState, redacted_error};
use crate::metrics::StreamCancelReason;
use crate::streaming::{
    ChunkCoalescer, QueryForm, ResultBindingRow, ResultMetadata, StreamFormat, StreamingEncoder,
};

const DEFAULT_CHUNK_BYTES: usize = 65_536;
const MAX_CHUNK_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_ROW_BYTES: usize = 1024 * 1024;
const MAX_ROW_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Source {
    Select,
    Ask,
    Graph,
}

struct ActiveStream {
    app: Arc<AppState>,
    client: Option<Object>,
    rows: Pin<Box<RowStream>>,
    cancel_token: CancelToken,
    encoder: StreamingEncoder,
    coalescer: ChunkCoalescer,
    ready: VecDeque<Vec<u8>>,
    source: Source,
    deadline: Instant,
    idle_deadline: Instant,
    idle_timeout: Duration,
    max_row_bytes: usize,
    started_at: Instant,
    first_byte_recorded: bool,
    encoder_started: bool,
    finished: bool,
    metrics_started: bool,
    output_finished: bool,
}

impl ActiveStream {
    async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if let Some(chunk) = self.ready.pop_front() {
            return Some(self.return_chunk(chunk));
        }

        if self.output_finished {
            self.finish_cleanly().await;
            return None;
        }

        if !self.encoder_started {
            self.encoder_started = true;
            match self.encoder.start() {
                Ok(bytes) => self.push(bytes),
                Err(error) => {
                    self.fail_encoder(error.to_string()).await;
                    return None;
                }
            }
            if let Some(chunk) = self.ready.pop_front() {
                return Some(self.return_chunk(chunk));
            }
        }

        loop {
            let now = Instant::now();
            let (remaining, reason) = if self.deadline <= self.idle_deadline {
                (
                    self.deadline.saturating_duration_since(now),
                    StreamCancelReason::Deadline,
                )
            } else {
                (
                    self.idle_deadline.saturating_duration_since(now),
                    StreamCancelReason::IdleTimeout,
                )
            };
            if remaining.is_zero() {
                self.cancel(
                    reason,
                    if matches!(reason, StreamCancelReason::IdleTimeout) {
                        "stream idle timeout exceeded"
                    } else {
                        "stream deadline exceeded"
                    },
                )
                .await;
                return None;
            }

            let row = match tokio::time::timeout(remaining, self.rows.next()).await {
                Ok(Some(Ok(row))) => row,
                Ok(Some(Err(error))) => {
                    self.fail_db(error.to_string()).await;
                    return None;
                }
                Ok(None) => {
                    match self.encoder.finish() {
                        Ok(bytes) => self.push(bytes),
                        Err(error) => {
                            self.fail_encoder(error.to_string()).await;
                            return None;
                        }
                    }
                    self.ready.extend(self.coalescer.finish());
                    self.output_finished = true;
                    if self.ready.is_empty() {
                        self.finish_cleanly().await;
                        return None;
                    }
                    return self.ready.pop_front().map(|chunk| self.return_chunk(chunk));
                }
                Err(_) => {
                    self.cancel(
                        reason,
                        if matches!(reason, StreamCancelReason::IdleTimeout) {
                            "stream idle timeout exceeded"
                        } else {
                            "stream deadline exceeded"
                        },
                    )
                    .await;
                    return None;
                }
            };

            self.idle_deadline = Instant::now() + self.idle_timeout;

            let encoded = match self.encode_row(&row) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.fail_encoder(error.to_string()).await;
                    return None;
                }
            };
            if encoded.len() > self.max_row_bytes {
                self.fail_encoder("stream row exceeds maximum row bytes".to_owned())
                    .await;
                return None;
            }
            self.push(encoded);
            self.app.metrics.record_stream_row();
            if let Some(chunk) = self.ready.pop_front() {
                return Some(self.return_chunk(chunk));
            }
        }
    }

    fn encode_row(
        &mut self,
        row: &tokio_postgres::Row,
    ) -> Result<Vec<u8>, crate::streaming::StreamError> {
        match self.source {
            Source::Select => {
                let value: serde_json::Value = row
                    .try_get(0)
                    .map_err(|error| crate::streaming::StreamError(error.to_string()))?;
                self.encoder
                    .encode_row(&ResultBindingRow::from_json(&value)?)
            }
            Source::Ask => {
                let value: bool = row
                    .try_get(0)
                    .map_err(|error| crate::streaming::StreamError(error.to_string()))?;
                self.encoder.encode_boolean(value)
            }
            Source::Graph => {
                let value: String = row
                    .try_get(0)
                    .map_err(|error| crate::streaming::StreamError(error.to_string()))?;
                if value.contains("<<") {
                    return Err(crate::streaming::StreamError(
                        "RDF-star quoted triples require an RDF-star media type".to_owned(),
                    ));
                }
                Ok(value.into_bytes())
            }
        }
    }

    fn push(&mut self, bytes: Vec<u8>) {
        self.ready.extend(self.coalescer.push(&bytes));
    }

    fn return_chunk(&mut self, chunk: Vec<u8>) -> Vec<u8> {
        if !self.first_byte_recorded {
            self.first_byte_recorded = true;
            self.app
                .metrics
                .record_stream_first_byte(self.started_at.elapsed());
        }
        self.app.metrics.record_stream_bytes(chunk.len());
        chunk
    }

    async fn finish_cleanly(&mut self) {
        if self.finished {
            return;
        }
        let Some(client) = self.client.take() else {
            self.finished = true;
            return;
        };
        if let Err(error) = client.batch_execute("COMMIT").await {
            self.app.metrics.record_stream_error();
            self.app.metrics.record_stream_db_error();
            tracing::error!(error = %error, "stream commit failed; discarding connection");
            discard(client);
            self.app.metrics.record_stream_connection_discard();
        }
        self.finished = true;
        self.app
            .metrics
            .record_stream_finished(self.started_at.elapsed());
    }

    async fn fail_db(&mut self, detail: String) {
        self.app.metrics.record_stream_error();
        self.app.metrics.record_stream_db_error();
        tracing::error!(detail = %detail, "SPARQL stream failed");
        self.rollback(false).await;
    }

    async fn fail_encoder(&mut self, detail: String) {
        self.app.metrics.record_stream_error();
        self.app.metrics.record_stream_encoder_error();
        tracing::error!(detail = %detail, "SPARQL stream encoding failed");
        self.cancel(StreamCancelReason::EncoderError, &detail).await;
    }

    async fn cancel(&mut self, reason: StreamCancelReason, detail: &str) {
        let failed = self
            .app
            .cancel_tls
            .cancel(&self.cancel_token)
            .await
            .is_err();
        self.app.metrics.record_stream_cancellation(reason, failed);
        tracing::warn!(
            ?reason,
            detail,
            cancel_failed = failed,
            "SPARQL stream cancelled"
        );
        self.rollback(true).await;
    }

    async fn rollback(&mut self, discard_connection: bool) {
        if self.finished {
            return;
        }
        let Some(client) = self.client.take() else {
            self.finished = true;
            self.app
                .metrics
                .record_stream_finished(self.started_at.elapsed());
            return;
        };
        let rollback_failed = client.batch_execute("ROLLBACK").await.is_err();
        if discard_connection || rollback_failed {
            discard(client);
            self.app.metrics.record_stream_connection_discard();
        }
        self.finished = true;
        self.app
            .metrics
            .record_stream_finished(self.started_at.elapsed());
    }
}

impl Drop for ActiveStream {
    fn drop(&mut self) {
        if self.finished || !self.metrics_started {
            return;
        }

        if let Some(client) = self.client.take() {
            discard(client);
            self.app.metrics.record_stream_connection_discard();
        }
        let token = self.cancel_token.clone();
        let app = self.app.clone();
        let duration = self.started_at.elapsed();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let failed = app.cancel_tls.cancel(&token).await.is_err();
                app.metrics
                    .record_stream_cancellation(StreamCancelReason::Disconnect, failed);
                app.metrics.record_stream_finished(duration);
            });
        } else {
            self.app
                .metrics
                .record_stream_cancellation(StreamCancelReason::Disconnect, true);
            self.app.metrics.record_stream_finished(duration);
        }
        self.finished = true;
    }
}

fn discard(client: Object) {
    drop(Object::take(client));
}

async fn rollback_before_response(client: Object) {
    if client.batch_execute("ROLLBACK").await.is_err() {
        discard(client);
    }
}

/// Stream a SPARQL result directly from a PostgreSQL `RowStream`.
pub async fn stream_sparql(
    app: &Arc<AppState>,
    query: &str,
    accept: &str,
    traceparent: Option<&str>,
    use_replica: bool,
    timeout_override_ms: Option<u64>,
) -> Response {
    stream_sparql_inner(
        app,
        query,
        accept,
        traceparent,
        use_replica,
        timeout_override_ms,
        None,
        None,
    )
    .await
}

/// Stream a typed-binding query through the same portal-backed pipeline.
pub async fn stream_sparql_with_bindings(
    app: &Arc<AppState>,
    query: &str,
    bindings: &serde_json::Value,
    accept: &str,
    traceparent: Option<&str>,
    use_replica: bool,
    timeout_override_ms: Option<u64>,
    prefix_mode: Option<&str>,
) -> Response {
    stream_sparql_inner(
        app,
        query,
        accept,
        traceparent,
        use_replica,
        timeout_override_ms,
        Some(bindings),
        prefix_mode,
    )
    .await
}

async fn stream_sparql_inner(
    app: &Arc<AppState>,
    query: &str,
    accept: &str,
    traceparent: Option<&str>,
    use_replica: bool,
    timeout_override_ms: Option<u64>,
    bindings: Option<&serde_json::Value>,
    prefix_mode: Option<&str>,
) -> Response {
    let (source, form) = match query_form(query) {
        Some(value) => value,
        None => {
            return crate::common::json_error(
                "PT400",
                "unsupported SPARQL query form",
                StatusCode::BAD_REQUEST,
            );
        }
    };
    let Some(format) = stream_format(form, accept) else {
        return crate::common::json_error(
            "PT406",
            "requested format is not streamable; use SPARQL Results JSON, CSV, TSV, or N-Triples",
            StatusCode::NOT_ACCEPTABLE,
        );
    };

    let client = if use_replica {
        if let Some(replica_pool) = &app.replica_pool {
            match replica_pool.get().await {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(error = %error, "stream replica unavailable; falling back to primary");
                    match app.pool.get().await {
                        Ok(client) => client,
                        Err(error) => {
                            return redacted_error(
                                "service_unavailable",
                                &error.to_string(),
                                StatusCode::SERVICE_UNAVAILABLE,
                            );
                        }
                    }
                }
            }
        } else {
            match app.pool.get().await {
                Ok(client) => client,
                Err(error) => {
                    return redacted_error(
                        "service_unavailable",
                        &error.to_string(),
                        StatusCode::SERVICE_UNAVAILABLE,
                    );
                }
            }
        }
    } else {
        match app.pool.get().await {
            Ok(client) => client,
            Err(error) => {
                return redacted_error(
                    "service_unavailable",
                    &error.to_string(),
                    StatusCode::SERVICE_UNAVAILABLE,
                );
            }
        }
    };

    let timeout_max_ms = env_u64("PG_RIPPLE_HTTP_QUERY_TIMEOUT_MAX_MS", 900_000, 3_600_000);
    let configured_timeout_ms = env_u64("PG_RIPPLE_HTTP_QUERY_TIMEOUT_MS", 300_000, timeout_max_ms);
    let timeout_ms = timeout_override_ms
        .filter(|value| *value > 0)
        .map(|value| configured_timeout_ms.min(value))
        .unwrap_or(configured_timeout_ms);
    let idle_ms = env_u64(
        "PG_RIPPLE_HTTP_STREAM_IDLE_TIMEOUT_MS",
        60_000,
        timeout_max_ms,
    );
    let chunk_bytes = env_usize(
        "PG_RIPPLE_HTTP_STREAM_CHUNK_BYTES",
        DEFAULT_CHUNK_BYTES,
        MAX_CHUNK_BYTES,
    );
    let max_row_bytes = env_usize(
        "PG_RIPPLE_HTTP_STREAM_MAX_ROW_BYTES",
        DEFAULT_MAX_ROW_BYTES,
        MAX_ROW_BYTES,
    );
    let transaction = if bindings.is_some() {
        "BEGIN"
    } else {
        "BEGIN READ ONLY"
    };
    if let Err(error) = client
        .batch_execute(&format!(
            "{transaction}; SET LOCAL statement_timeout = '{timeout_ms}ms'; SET LOCAL idle_in_transaction_session_timeout = '{idle_ms}ms';"
        ))
        .await
    {
        discard(client);
        return redacted_error(
            "stream_transaction",
            &error.to_string(),
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }
    let prefix_mode = prefix_mode.unwrap_or("strict");
    if let Err(error) = client
        .execute(
            "SELECT set_config('pg_ripple.sparql_prefix_mode', $1, true)",
            &[&prefix_mode],
        )
        .await
    {
        rollback_before_response(client).await;
        return redacted_error(
            "stream_configuration",
            &error.to_string(),
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }
    if let Some(traceparent) = traceparent
        && let Err(error) = client
            .execute(
                "SET LOCAL pg_ripple.tracing_traceparent = $1",
                &[&traceparent],
            )
            .await
    {
        tracing::debug!(error = %error, "could not set stream traceparent");
    }

    let metadata = match client
        .query_one("SELECT _pg_ripple.sparql_stream_metadata($1)", &[&query])
        .await
        .and_then(|row| row.try_get::<_, serde_json::Value>(0))
    {
        Ok(value) => match metadata_from_json(&value) {
            Ok(metadata) => metadata,
            Err(error) => {
                rollback_before_response(client).await;
                return redacted_error("stream_metadata", &error, StatusCode::BAD_REQUEST);
            }
        },
        Err(error) => {
            rollback_before_response(client).await;
            return redacted_error(
                "sparql_query_error",
                &error.to_string(),
                StatusCode::BAD_REQUEST,
            );
        }
    };
    let cancel_token = client.cancel_token();
    let sql = match (source, bindings.is_some()) {
        (Source::Select, false) => "SELECT result FROM _pg_ripple.sparql_stream_bindings($1)",
        (Source::Select, true) => {
            "SELECT result FROM _pg_ripple.sparql_stream_bindings_with_bindings($1, $2)"
        }
        (Source::Ask, false) => "SELECT pg_ripple.sparql_ask($1)",
        (Source::Ask, true) => "SELECT (result->>'result')::boolean FROM pg_ripple.sparql($1, $2)",
        (Source::Graph, false) => "SELECT triple FROM _pg_ripple.sparql_stream_triples($1)",
        (Source::Graph, true) => {
            "SELECT triple FROM _pg_ripple.sparql_stream_triples_with_bindings($1, $2)"
        }
    };
    let rows_result = match bindings {
        Some(bindings) => {
            let args = [
                &query as &(dyn tokio_postgres::types::ToSql + Sync),
                bindings,
            ];
            client.query_raw(sql, args).await
        }
        None => client.query_raw(sql, std::iter::once(query)).await,
    };
    let rows = match rows_result {
        Ok(rows) => Box::pin(rows),
        Err(error) => {
            rollback_before_response(client).await;
            return redacted_error(
                "sparql_query_error",
                &error.to_string(),
                StatusCode::BAD_REQUEST,
            );
        }
    };
    let encoder = match StreamingEncoder::new(format, metadata) {
        Ok(encoder) => encoder,
        Err(error) => {
            rollback_before_response(client).await;
            return redacted_error(
                "stream_format",
                &error.to_string(),
                StatusCode::NOT_ACCEPTABLE,
            );
        }
    };
    let query_id = Uuid::new_v4().to_string();
    let started_at = Instant::now();
    app.metrics.record_stream_started();
    let active = ActiveStream {
        app: app.clone(),
        client: Some(client),
        rows,
        cancel_token,
        encoder,
        coalescer: ChunkCoalescer::new(chunk_bytes).unwrap_or_else(|_| unreachable!()),
        ready: VecDeque::new(),
        source,
        deadline: started_at + Duration::from_millis(timeout_ms),
        idle_deadline: started_at + Duration::from_millis(idle_ms),
        idle_timeout: Duration::from_millis(idle_ms),
        max_row_bytes,
        started_at,
        first_byte_recorded: false,
        encoder_started: false,
        finished: false,
        metrics_started: true,
        output_finished: false,
    };
    let body_stream = stream::unfold(active, |mut state| async move {
        state
            .next_chunk()
            .await
            .map(|chunk| (Ok::<Bytes, Infallible>(Bytes::from(chunk)), state))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", format.content_type())
        .header("x-pg-ripple-streaming", "true")
        .header("x-pg-ripple-query-id", query_id)
        .body(Body::from_stream(body_stream))
        .unwrap_or_else(|error| {
            redacted_error(
                "stream_response",
                &error.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

pub fn is_streamable(query: &str, accept: &str) -> bool {
    query_form(query).is_some_and(|(_, form)| stream_format(form, accept).is_some())
}

pub(crate) fn is_graph_query(query: &str) -> bool {
    query_form(query).is_some_and(|(source, _)| matches!(source, Source::Graph))
}

pub(crate) fn is_ask_query(query: &str) -> bool {
    query_form(query).is_some_and(|(source, _)| matches!(source, Source::Ask))
}

pub(crate) fn is_select_query(query: &str) -> bool {
    query_form(query).is_some_and(|(source, _)| matches!(source, Source::Select))
}

pub(crate) fn is_construct_query(query: &str) -> bool {
    query_form(query).is_some_and(|(_, form)| matches!(form, QueryForm::Construct))
}

pub(crate) fn is_describe_query(query: &str) -> bool {
    query_form(query).is_some_and(|(_, form)| matches!(form, QueryForm::Describe))
}

fn query_form(query: &str) -> Option<(Source, QueryForm)> {
    let keyword = query_after_prologue(query)
        .split_ascii_whitespace()
        .next()?
        .to_ascii_lowercase();
    match keyword.as_str() {
        "select" => Some((Source::Select, QueryForm::Select)),
        "ask" => Some((Source::Ask, QueryForm::Ask)),
        "construct" => Some((Source::Graph, QueryForm::Construct)),
        "describe" => Some((Source::Graph, QueryForm::Describe)),
        _ => None,
    }
}

fn query_after_prologue(mut query: &str) -> &str {
    loop {
        query = query.trim_start();
        if let Some(comment) = query.strip_prefix('#') {
            query = comment
                .find('\n')
                .map(|offset| &comment[offset + 1..])
                .unwrap_or("");
            continue;
        }
        let keyword = query
            .split_ascii_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if (keyword == "prefix" || keyword == "base")
            && let Some(end) = query.find('>')
        {
            query = &query[end + 1..];
            continue;
        }
        return query;
    }
}

fn stream_format(form: QueryForm, accept: &str) -> Option<StreamFormat> {
    match (form, accept.split(';').next().unwrap_or(accept).trim()) {
        (QueryForm::Select | QueryForm::Ask, "application/sparql-results+json") => {
            Some(StreamFormat::SparqlJson)
        }
        (QueryForm::Select | QueryForm::Ask, "text/csv") => Some(StreamFormat::Csv),
        (QueryForm::Select | QueryForm::Ask, "text/tab-separated-values") => {
            Some(StreamFormat::Tsv)
        }
        (QueryForm::Construct | QueryForm::Describe, "application/n-triples") => {
            Some(StreamFormat::NTriples)
        }
        _ => None,
    }
}

fn metadata_from_json(value: &serde_json::Value) -> Result<ResultMetadata, String> {
    let form = match value.get("form").and_then(serde_json::Value::as_str) {
        Some("select") => QueryForm::Select,
        Some("ask") => QueryForm::Ask,
        Some("construct") => QueryForm::Construct,
        Some("describe") => QueryForm::Describe,
        _ => return Err("stream metadata has an invalid query form".to_owned()),
    };
    let variables = value
        .get("variables")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "stream metadata has no variables".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "stream metadata variable is not a string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    ResultMetadata::new(form, variables).map_err(|error| error.to_string())
}

fn env_u64(name: &str, default: u64, maximum: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.clamp(1, maximum))
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, maximum))
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamability_skips_comments_and_prologue() {
        let query = "# comment\nPREFIX ex: <https://example.test/>\nSELECT * WHERE { ?s ?p ?o }";
        assert!(is_streamable(query, "application/sparql-results+json"));
        assert!(is_streamable(
            "PREFIX ex: <https://example.test/> CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }",
            "application/n-triples"
        ));
        assert!(!is_streamable(query, "application/sparql-results+xml"));
    }
}
