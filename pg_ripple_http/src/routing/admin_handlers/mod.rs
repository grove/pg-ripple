//! Admin/observability/explorer handlers -- extracted from routing.rs (MOD-01, v0.72.0).
//! v0.122.0 H17-02: explorer and diagnostic sections extracted to sub-modules.

pub(crate) mod diagnostic;
pub(crate) mod explorer;

pub(crate) use diagnostic::diagnostic_snapshot;
pub(crate) use explorer::explorer_page;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use constant_time_eq::constant_time_eq;

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, redacted_error};

// Re-import routing types for ApiDoc
use super::ApiDoc;
use utoipa::OpenApi as _;

/// Build timestamp recorded at compile time (RFC 3339).
/// BUILD-TIME-FIELD-01 (v0.83.0): populated by build.rs with an RFC-3339
/// timestamp, or falls back to the Cargo package version string prefixed with
/// "build-version=" when SOURCE_DATE_EPOCH is unset.
const BUILD_TIME: &str = match option_env!("BUILD_TIMESTAMP") {
    Some(ts) => ts,
    None => concat!("build-version=", env!("CARGO_PKG_VERSION")),
};

pub(crate) async fn health(State(state): State<Arc<AppState>>) -> Response {
    // v0.55.0 I-3: return structured JSON with version, git_sha, postgres_connected, last_query_ts.
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("GIT_SHA").unwrap_or("unknown");

    let (postgres_connected, postgres_version) = match state.pool.get().await {
        Ok(client) => match client.query_one("SELECT version()", &[]).await {
            Ok(row) => {
                let v: String = row.get(0);
                // v0.60.0 H7-5: Mark the service as ready on first successful connection.
                state
                    .ever_connected
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                (true, Some(v))
            }
            Err(_) => (false, None),
        },
        Err(_) => (false, None),
    };

    let last_query_ts = {
        let ts = state.metrics.last_query_ts();
        if ts == 0 {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(format!("{ts}"))
        }
    };

    let body = serde_json::json!({
        "status": if postgres_connected { "ok" } else { "degraded" },
        "version": version,
        "git_sha": git_sha,
        "build_time": BUILD_TIME,
        "postgres_connected": postgres_connected,
        "postgres_version": postgres_version,
        "last_query_ts": last_query_ts,
    });

    let status = if postgres_connected {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── Readiness endpoint (v0.60.0 H7-5) ───────────────────────────────────────
//
// GET /ready — Kubernetes readiness probe (v0.64.0: deep readiness).
//
// Returns 200 OK once the service has successfully connected to PostgreSQL at
// least once.  Returns 503 Service Unavailable until then so the Kubernetes
// load-balancer withholds traffic from a pod that is still starting up.
//
// v0.64.0 TRUTH-02: deep /ready includes PostgreSQL connectivity, extension
// version, migration version, and a feature-status snapshot so operators know
// whether optional features are active or degraded.
//
// Distinct from /health (liveness probe):
//   /health  — is the process alive and can reach PostgreSQL right now?
//   /ready   — has the process EVER reached PostgreSQL (safe to route traffic)?
pub(crate) async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let is_ready = state
        .ever_connected
        .load(std::sync::atomic::Ordering::Relaxed);

    if !is_ready {
        let body = serde_json::json!({
            "status": "not_ready",
            "reason": "waiting for first successful PostgreSQL connection"
        });
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap_or_else(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
            });
    }

    // v0.64.0 TRUTH-02: deep readiness — query pg_ripple for version, migration,
    // and feature-status snapshot.
    let (pg_version, extension_version, feature_snapshot, degraded_features) =
        match state.pool.get().await {
            Ok(client) => {
                let pg_ver: Option<String> = client
                    .query_one("SELECT version()", &[])
                    .await
                    .ok()
                    .map(|r| r.get(0));

                let ext_ver: Option<String> = client
                    .query_one(
                        "SELECT installed_version FROM pg_available_extensions \
                         WHERE name = 'pg_ripple'",
                        &[],
                    )
                    .await
                    .ok()
                    .and_then(|r| r.get(0));

                // Collect partial/degraded features from feature_status().
                let mut features: Vec<serde_json::Value> = Vec::new();
                let mut degraded: Vec<String> = Vec::new();

                if let Ok(rows) = client
                    .query(
                        "SELECT feature_name, status, degraded_reason \
                         FROM pg_ripple.feature_status() \
                         WHERE status != 'implemented' \
                         ORDER BY feature_name",
                        &[],
                    )
                    .await
                {
                    for row in &rows {
                        let name: String = row.get(0);
                        let status: String = row.get(1);
                        let reason: Option<String> = row.get(2);
                        if matches!(status.as_str(), "degraded" | "stub") {
                            degraded.push(name.clone());
                        }
                        features.push(serde_json::json!({
                            "feature": name,
                            "status": status,
                            "degraded_reason": reason,
                        }));
                    }
                }

                (pg_ver, ext_ver, features, degraded)
            }
            Err(_) => (None, None, vec![], vec![]),
        };

    let body = serde_json::json!({
        "status": "ready",
        "service_version": env!("CARGO_PKG_VERSION"),
        "postgres_version": pg_version,
        "extension_version": extension_version,
        "partial_features": feature_snapshot,
        "degraded_features": degraded_features,
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// GET /health/ready — Deep readiness probe (O13-01, v0.84.0).
//
// Verifies that the pg_ripple extension is installed and responding within
// a strict 2-second deadline.  Distinct from /health (liveness) and /ready
// (first-connection readiness):
//   /health        — process alive + PG reachable
//   /ready         — process has EVER connected + deep feature snapshot
//   /health/ready  — extension installed + responding, hard 2-second timeout
//
// Returns 200 {"status":"ok"} or 503 {"status":"unavailable","reason":"..."}.
pub(crate) async fn health_ready(State(state): State<Arc<AppState>>) -> Response {
    let deadline = tokio::time::timeout(std::time::Duration::from_secs(2), state.pool.get()).await;

    let client = match deadline {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            let body = serde_json::json!({
                "status": "unavailable",
                "reason": format!("database connection failed: {e}")
            });
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
                });
        }
        Err(_) => {
            let body = serde_json::json!({
                "status": "unavailable",
                "reason": "database connection timed out after 2 seconds"
            });
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
                });
        }
    };

    let row = match client
        .query_opt(
            "SELECT 1 FROM pg_extension WHERE extname = 'pg_ripple'",
            &[],
        )
        .await
    {
        Ok(Some(_)) => None,
        Ok(None) => Some("pg_ripple extension is not installed in this database"),
        Err(_) => Some("pg_extension query failed"),
    };

    if let Some(reason) = row {
        let body = serde_json::json!({ "status": "unavailable", "reason": reason });
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap_or_else(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
            });
    }

    let body = serde_json::json!({ "status": "ok" });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── Metrics endpoint ────────────────────────────────────────────────────────

pub(crate) async fn metrics_endpoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    // M16-22 (v0.115.0): optional bearer-token auth for the metrics endpoint.
    if let Some(expected) = &state.metrics_token {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = provided.strip_prefix("Bearer ").unwrap_or(provided);
        if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
            // SAFETY: status code and header values are compile-time constants.
            return axum::response::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("www-authenticate", "Bearer realm=\"pg_ripple\"")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"error":"PT401","message":"metrics token required"}"#,
                ))
                .expect("infallible: hardcoded valid HTTP headers");
        }
    }
    let m = &state.metrics;
    let body = format!(
        "# HELP pg_ripple_http_sparql_queries_total Total SPARQL queries executed\n\
         # TYPE pg_ripple_http_sparql_queries_total counter\n\
         pg_ripple_http_sparql_queries_total {}\n\
         # HELP pg_ripple_http_datalog_queries_total Total Datalog API calls executed\n\
         # TYPE pg_ripple_http_datalog_queries_total counter\n\
         pg_ripple_http_datalog_queries_total {}\n\
         # HELP pg_ripple_http_errors_total Total query errors\n\
         # TYPE pg_ripple_http_errors_total counter\n\
         pg_ripple_http_errors_total {}\n\
         # HELP pg_ripple_http_query_duration_seconds_total Total query duration in seconds\n\
         # TYPE pg_ripple_http_query_duration_seconds_total counter\n\
         pg_ripple_http_query_duration_seconds_total {:.6}\n\
         # HELP pg_ripple_http_pool_size Current connection pool size\n\
         # TYPE pg_ripple_http_pool_size gauge\n\
         pg_ripple_http_pool_size {}\n\
         # HELP pg_ripple_http_sparql_query_duration_seconds Total SPARQL query duration by type (METRICS-LABELS-01)\n\
         # TYPE pg_ripple_http_sparql_query_duration_seconds counter\n\
         pg_ripple_http_sparql_query_duration_seconds{{query_type=\"SELECT\"}} {:.6}\n\
         pg_ripple_http_sparql_query_duration_seconds{{query_type=\"ASK\"}} {:.6}\n\
         pg_ripple_http_sparql_query_duration_seconds{{query_type=\"CONSTRUCT\"}} {:.6}\n\
         pg_ripple_http_sparql_query_duration_seconds{{query_type=\"DESCRIBE\"}} {:.6}\n\
         pg_ripple_http_sparql_query_duration_seconds{{query_type=\"UPDATE\"}} {:.6}\n\
         # HELP pg_ripple_http_sparql_queries_by_type_total SPARQL queries by query type (METRICS-LABELS-01)\n\
         # TYPE pg_ripple_http_sparql_queries_by_type_total counter\n\
         pg_ripple_http_sparql_queries_by_type_total{{query_type=\"SELECT\"}} {}\n\
         pg_ripple_http_sparql_queries_by_type_total{{query_type=\"ASK\"}} {}\n\
         pg_ripple_http_sparql_queries_by_type_total{{query_type=\"CONSTRUCT\"}} {}\n\
         pg_ripple_http_sparql_queries_by_type_total{{query_type=\"DESCRIBE\"}} {}\n\
         pg_ripple_http_sparql_queries_by_type_total{{query_type=\"UPDATE\"}} {}\n\
         # HELP pg_ripple_http_sparql_queries_by_result_size_total SPARQL queries by result size bucket (METRICS-LABELS-01)\n\
         # TYPE pg_ripple_http_sparql_queries_by_result_size_total counter\n\
         pg_ripple_http_sparql_queries_by_result_size_total{{result_size_bucket=\"empty\"}} {}\n\
         pg_ripple_http_sparql_queries_by_result_size_total{{result_size_bucket=\"small\"}} {}\n\
         pg_ripple_http_sparql_queries_by_result_size_total{{result_size_bucket=\"medium\"}} {}\n\
         pg_ripple_http_sparql_queries_by_result_size_total{{result_size_bucket=\"large\"}} {}\n\
         # HELP pg_ripple_dictionary_hot_cache_hits_total Backend-local dictionary LRU cache hits (P13-08)\n\
         # TYPE pg_ripple_dictionary_hot_cache_hits_total counter\n\
         pg_ripple_dictionary_hot_cache_hits_total {}\n\
         # HELP pg_ripple_dictionary_hot_cache_misses_total Backend-local dictionary LRU cache misses (P13-08)\n\
         # TYPE pg_ripple_dictionary_hot_cache_misses_total counter\n\
         pg_ripple_dictionary_hot_cache_misses_total {}\n\
         # HELP pg_ripple_federation_endpoint_requests_total Total federation SERVICE endpoint requests (O13-02)\n\
         # TYPE pg_ripple_federation_endpoint_requests_total counter\n\
         pg_ripple_federation_endpoint_requests_total {}\n\
         # HELP pg_ripple_federation_endpoint_duration_seconds Total federation SERVICE latency in seconds (O13-02)\n\
         # TYPE pg_ripple_federation_endpoint_duration_seconds counter\n\
         pg_ripple_federation_endpoint_duration_seconds {:.6}\n\
         # HELP pg_ripple_dictionary_cache_hit_ratio Dictionary hot-cache hit ratio 0.0-1.0 (O13-02)\n\
         # TYPE pg_ripple_dictionary_cache_hit_ratio gauge\n\
         pg_ripple_dictionary_cache_hit_ratio {:.6}\n\
         # HELP pg_ripple_merge_worker_delta_rows_pending Merge worker delta rows pending flush (O13-02)\n\
         # TYPE pg_ripple_merge_worker_delta_rows_pending gauge\n\
         pg_ripple_merge_worker_delta_rows_pending {}\n\
         # HELP pg_ripple_http_cors_permissive_requests_total Requests served under CORS wildcard origin (S13-03)\n\
         # TYPE pg_ripple_http_cors_permissive_requests_total counter\n\
         pg_ripple_http_cors_permissive_requests_total {}\n\
         # HELP pg_ripple_pagerank_queue_depth Number of dirty edges queued for incremental PageRank refresh (OBS-01)\n\
         # TYPE pg_ripple_pagerank_queue_depth gauge\n\
         pg_ripple_pagerank_queue_depth{{topic=\"\"}} {}\n\
         # HELP pg_ripple_pagerank_queue_max_delta Largest accumulated score delta in the PageRank dirty-edges queue (OBS-01)\n\
         # TYPE pg_ripple_pagerank_queue_max_delta gauge\n\
         pg_ripple_pagerank_queue_max_delta{{topic=\"\"}} {:.6}\n\
         # HELP pg_ripple_pagerank_queue_oldest_enqueue_seconds Age in seconds of the oldest entry in the PageRank dirty-edges queue (OBS-01)\n\
         # TYPE pg_ripple_pagerank_queue_oldest_enqueue_seconds gauge\n\
         pg_ripple_pagerank_queue_oldest_enqueue_seconds{{topic=\"\"}} {}\n\
         # HELP pg_ripple_bidi_relay_dropped_total Total bidi relay dispatch calls dropped due to inflight overflow (H15-03)\n\
         # TYPE pg_ripple_bidi_relay_dropped_total counter\n\
         pg_ripple_bidi_relay_dropped_total {}\n\
         # HELP pg_ripple_json_writeback_drain_errors_total Total drain_json_writeback_queue() status-update failures (L18-02)\n\
         # TYPE pg_ripple_json_writeback_drain_errors_total counter\n\
         pg_ripple_json_writeback_drain_errors_total {}\n\
         # HELP pg_ripple_merge_cycle_duration_seconds Cumulative merge cycle wall-clock time in seconds (M15-19)\n\
         # TYPE pg_ripple_merge_cycle_duration_seconds counter\n\
         pg_ripple_merge_cycle_duration_seconds {}\n\
         # HELP pg_ripple_datalog_stratum_duration_seconds Cumulative Datalog stratum execution time in seconds (M15-19)\n\
         # TYPE pg_ripple_datalog_stratum_duration_seconds counter\n\
         pg_ripple_datalog_stratum_duration_seconds {}\n\
         # HELP pg_ripple_shacl_validation_queue_depth SHACL async validation queue depth (M15-19)\n\
         # TYPE pg_ripple_shacl_validation_queue_depth gauge\n\
         pg_ripple_shacl_validation_queue_depth {}\n\
         # HELP pg_ripple_cdc_replication_slot_lag_bytes CDC replication slot lag in bytes (M15-19)\n\
         # TYPE pg_ripple_cdc_replication_slot_lag_bytes gauge\n\
         pg_ripple_cdc_replication_slot_lag_bytes {}\n\
         # HELP pg_ripple_er_stage_duration_seconds Cumulative NS-RL entity-resolution stage latency in seconds (M16-03)\n\
         # TYPE pg_ripple_er_stage_duration_seconds counter\n\
         pg_ripple_er_stage_duration_seconds{{stage=\"blocking\"}} {:.6}\n\
         pg_ripple_er_stage_duration_seconds{{stage=\"embedding\"}} {:.6}\n\
         pg_ripple_er_stage_duration_seconds{{stage=\"shacl\"}} {:.6}\n\
         pg_ripple_er_stage_duration_seconds{{stage=\"canonicalization\"}} {:.6}\n\
         pg_ripple_er_stage_duration_seconds{{stage=\"provenance\"}} {:.6}\n\
         # HELP pg_ripple_sameas_assertions_total Total owl:sameAs assertions from entity-resolution (M16-03)\n\
         # TYPE pg_ripple_sameas_assertions_total counter\n\
         pg_ripple_sameas_assertions_total {}\n\
         # HELP pg_ripple_bayesian_propagation_duration_seconds Cumulative Bayesian confidence propagation latency (M16-03)\n\
         # TYPE pg_ripple_bayesian_propagation_duration_seconds counter\n\
         pg_ripple_bayesian_propagation_duration_seconds {:.6}\n\
         # HELP pg_ripple_temporal_facts_total Current number of temporal facts (M16-03)\n\
         # TYPE pg_ripple_temporal_facts_total gauge\n\
         pg_ripple_temporal_facts_total {}\n\
         # HELP pg_ripple_temporal_queries_total Total temporal fact queries (M16-03)\n\
         # TYPE pg_ripple_temporal_queries_total counter\n\
         pg_ripple_temporal_queries_total {}\n\
         # HELP pg_ripple_pprl_bloom_encodes_total Total PPRL Bloom-filter encodes (M16-03)\n\
         # TYPE pg_ripple_pprl_bloom_encodes_total counter\n\
         pg_ripple_pprl_bloom_encodes_total {}\n\
         # HELP pg_ripple_llm_cache_hits_total Total LLM explanation cache hits (M16-03)\n\
         # TYPE pg_ripple_llm_cache_hits_total counter\n\
         pg_ripple_llm_cache_hits_total {}\n\
         # HELP pg_ripple_llm_cache_misses_total Total LLM explanation cache misses (M16-03)\n\
         # TYPE pg_ripple_llm_cache_misses_total counter\n\
         pg_ripple_llm_cache_misses_total {}\n\
         # HELP pg_ripple_proof_tree_duration_seconds Cumulative proof-tree generation latency in seconds (M16-03)\n\
         # TYPE pg_ripple_proof_tree_duration_seconds counter\n\
         pg_ripple_proof_tree_duration_seconds {:.6}\n\
         # HELP pg_ripple_conflict_detections_total Total rule conflict detections (M16-03)\n\
         # TYPE pg_ripple_conflict_detections_total counter\n\
         pg_ripple_conflict_detections_total {}\n\
         # HELP pg_ripple_http_replica_pool_size Current read-replica connection pool size (OBS-M-01)\n\
         # TYPE pg_ripple_http_replica_pool_size gauge\n\
         pg_ripple_http_replica_pool_size{{pool=\"replica\"}} {}\n\
         # HELP pg_ripple_http_replica_pool_available Available idle connections in the read-replica pool (OBS-M-01)\n\
         # TYPE pg_ripple_http_replica_pool_available gauge\n\
         pg_ripple_http_replica_pool_available{{pool=\"replica\"}} {}\n\
         # HELP pg_ripple_rule_library_stream_duration_seconds Cumulative latency of rule-library stream responses in seconds (OBS-M-02)\n\
         # TYPE pg_ripple_rule_library_stream_duration_seconds counter\n\
         pg_ripple_rule_library_stream_duration_seconds {:.6}\n\
         # HELP pg_ripple_rule_library_subscribe_errors_total Total errors from rule-library subscribe calls (OBS-M-02)\n\
         # TYPE pg_ripple_rule_library_subscribe_errors_total counter\n\
         pg_ripple_rule_library_subscribe_errors_total {}\n\
         # HELP pg_ripple_graph_snapshots_total Current number of registered temporal graph snapshots (FEAT-02)\n\
         # TYPE pg_ripple_graph_snapshots_total gauge\n\
         pg_ripple_graph_snapshots_total {}\n",
        m.sparql_query_count(),
        m.datalog_query_count(),
        m.error_count(),
        m.total_duration_secs(),
        state.pool.status().size,
        m.select_duration_secs(),
        m.ask_duration_secs(),
        m.construct_duration_secs(),
        m.describe_duration_secs(),
        m.update_duration_secs(),
        m.select_count(),
        m.ask_count(),
        m.construct_count(),
        m.describe_count(),
        m.update_count(),
        m.result_empty_count(),
        m.result_small_count(),
        m.result_medium_count(),
        m.result_large_count(),
        m.dictionary_hot_cache_hits(),
        m.dictionary_hot_cache_misses(),
        m.federation_endpoint_requests(),
        m.federation_endpoint_duration_secs(),
        m.dictionary_cache_hit_ratio(),
        m.merge_worker_delta_rows_pending(),
        m.cors_permissive_requests_total(),
        m.pagerank_queue_depth(),
        m.pagerank_queue_max_delta(),
        m.pagerank_queue_oldest_enqueue_seconds(),
        m.bidi_relay_dropped_total(),
        m.json_writeback_drain_errors_total(),
        m.merge_cycle_duration_secs(),
        m.datalog_stratum_duration_secs(),
        m.shacl_validation_queue_depth(),
        m.cdc_replication_slot_lag_bytes(),
        // M16-03 (v0.115.0): new subsystem metrics.
        m.er_stage_duration_secs("blocking"),
        m.er_stage_duration_secs("embedding"),
        m.er_stage_duration_secs("shacl"),
        m.er_stage_duration_secs("canonicalization"),
        m.er_stage_duration_secs("provenance"),
        m.sameas_assertions_total(),
        m.bayesian_propagation_duration_secs(),
        m.temporal_facts_total(),
        m.temporal_queries_total(),
        m.pprl_bloom_encodes_total(),
        m.llm_cache_hits_total(),
        m.llm_cache_misses_total(),
        m.proof_tree_duration_secs(),
        m.conflict_detections_total(),
        // OBS-M-01 (v0.123.0): replica pool gauges — read directly from the live pool object.
        state
            .replica_pool
            .as_ref()
            .map(|p| p.status().size as u64)
            .unwrap_or(0),
        state
            .replica_pool
            .as_ref()
            .map(|p| p.status().available as u64)
            .unwrap_or(0),
        // OBS-M-02 (v0.123.0): rule-library federation metrics.
        m.rule_library_stream_duration_secs(),
        m.rule_library_subscribe_errors_total(),
        // FEAT-02 (v0.125.0): graph snapshot gauge.
        m.graph_snapshots_total(),
    );

    // Feature 6 (v0.119.0): Append per-endpoint federation circuit breaker
    // Prometheus gauge from _pg_ripple.federation_circuit_state table.
    // state: 0=closed, 1=open, 2=half_open.
    let circuit_gauge = if let Ok(client) = state.pool.get().await {
        match client
            .query(
                "SELECT endpoint_iri, state, failure_count \
                 FROM _pg_ripple.federation_circuit_state",
                &[],
            )
            .await
        {
            Ok(rows) if !rows.is_empty() => {
                let mut g = String::from(
                    "# HELP pg_ripple_federation_circuit_state Federation endpoint circuit breaker state (0=closed,1=open,2=half_open) (Feature 6, v0.119.0)\n\
                     # TYPE pg_ripple_federation_circuit_state gauge\n",
                );
                for row in &rows {
                    let endpoint: &str = row.get(0);
                    let state_str: &str = row.get(1);
                    let state_val: i32 = match state_str {
                        "closed" => 0,
                        "open" => 1,
                        "half_open" => 2,
                        _ => 0,
                    };
                    g.push_str(&format!(
                        "pg_ripple_federation_circuit_state{{endpoint=\"{}\"}} {}\n",
                        endpoint.replace('"', "\\\""),
                        state_val
                    ));
                }
                g
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let full_body = body + &circuit_gauge;

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4")
        .body(Body::from(full_body))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

// ─── Extension streaming metrics endpoint (OBS-02, v0.72.0) ─────────────────

/// `GET /metrics/extension` — Return the pg_ripple extension's streaming
/// metrics as JSON (calls `pg_ripple.streaming_metrics()` via SPI).
///
/// This exposes SPARQL cursor statistics, streaming query counts, and related
/// observability data that are maintained inside the PostgreSQL process and
/// not visible to the HTTP companion's own Prometheus counters.
pub(crate) async fn extension_metrics_endpoint(State(state): State<Arc<AppState>>) -> Response {
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "pool_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    let row = match client
        .query_one("SELECT pg_ripple.streaming_metrics()", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "extension_metrics",
                &format!("streaming_metrics() error: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let val: serde_json::Value = row
        .try_get::<_, serde_json::Value>(0)
        .unwrap_or_else(|_| serde_json::json!({}));

    json_response_http(StatusCode::OK, val)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── VoID dataset description (L-7.2, v0.55.0) ───────────────────────────────

/// `GET /void` — Return a Turtle VoID dataset description listing all named
/// graphs, triple counts, and predicate usage statistics.
pub(crate) async fn void_endpoint(State(state): State<Arc<AppState>>) -> Response {
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "pool_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // Collect per-predicate stats from the predicates catalog.
    let rows = match client
        .query(
            "SELECT id, triple_count FROM _pg_ripple.predicates ORDER BY triple_count DESC",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "database_error",
                &format!("predicate query error: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let total_triples: i64 = rows.iter().map(|r| r.get::<_, i64>(1)).sum();
    let pred_count = rows.len();

    let mut body = String::from(
        "@prefix void: <http://rdfs.org/ns/void#> .\n\
         @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix dcterms: <http://purl.org/dc/terms/> .\n\n\
         <> a void:Dataset ;\n",
    );
    body.push_str(&format!(
        "   void:triples {total_triples} ;\n\
         void:properties {pred_count} ;\n\
         dcterms:title \"pg_ripple RDF store\" .\n"
    ));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/turtle; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── SPARQL Service Description (L-7.4, v0.55.0) ─────────────────────────────

/// `GET /service` — Return a Turtle W3C SPARQL Service Description document.
pub(crate) async fn service_description() -> Response {
    let body = concat!(
        "@prefix sd:    <http://www.w3.org/ns/sparql-service-description#> .\n",
        "@prefix void:  <http://rdfs.org/ns/void#> .\n",
        "@prefix owl:   <http://www.w3.org/2002/07/owl#> .\n\n",
        "<> a sd:Service ;\n",
        "   sd:endpoint <> ;\n",
        "   sd:supportedLanguage sd:SPARQL11Query, sd:SPARQL11Update ;\n",
        "   sd:resultFormat\n",
        "       <http://www.w3.org/ns/formats/SPARQL_Results_JSON> ,\n",
        "       <http://www.w3.org/ns/formats/SPARQL_Results_XML>  ,\n",
        "       <http://www.w3.org/ns/formats/N-Triples>           ,\n",
        "       <http://www.w3.org/ns/formats/Turtle>              ;\n",
        "   sd:feature\n",
        "       sd:DereferencesURIs , sd:UnionDefaultGraph ,\n",
        "       sd:RequiresDataset , sd:BasicFederatedQuery ;\n",
        "   sd:extensionFunction\n",
        "       <https://pg-ripple.io/ns/pg/similar> ,\n",
        "       <https://pg-ripple.io/ns/pg/fts>     ,\n",
        "       <https://pg-ripple.io/ns/pg/embed>   ;\n",
        "   sd:entailmentRegime\n",
        "       <http://www.w3.org/ns/entailment/RDFS> ,\n",
        "       <http://www.w3.org/ns/entailment/OWL-RDF-Based> .\n"
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/turtle; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

// ─── OpenAPI spec endpoint (K-1, v0.55.0) ────────────────────────────────────

/// `GET /openapi.yaml` — Return the OpenAPI 3.1 specification for this service.
#[utoipa::path(
    get,
    path = "/openapi.yaml",
    tag = "metadata",
    responses(
        (status = 200, description = "OpenAPI 3.1 specification in YAML format",
         content_type = "text/yaml")
    )
)]
pub(crate) async fn openapi_spec() -> Response {
    let yaml = ApiDoc::openapi()
        .to_yaml()
        .unwrap_or_else(|e| format!("# openapi generation error: {e}\n"));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/yaml; charset=utf-8")
        .body(Body::from(yaml))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

pub(crate) fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

pub(crate) fn strip_angle(s: &str) -> &str {
    s.trim_start_matches('<').trim_end_matches('>')
}

// ─── v0.118.0: Benchmark history endpoint ────────────────────────────────────

/// Return recent benchmark run history from `_pg_ripple.bench_history`.
///
/// Requires authentication (check_auth_write).
/// Optional query param `limit` (default 20, max 1000).
///
/// Response: `{"runs": [{"run_id":..., "profile":"...", ...}]}`
pub(crate) async fn bench_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    use crate::common::check_auth_write;
    if let Err(r) = check_auth_write(&state, &headers) {
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
            "SELECT run_id, profile, \
                    to_char(started_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS started_at, \
                    duration_ms, triples_processed, queries_per_second \
             FROM _pg_ripple.bench_history \
             ORDER BY started_at DESC \
             LIMIT 100",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "bench_history_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let runs: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "run_id": row.get::<_, i64>(0),
                "profile": row.get::<_, String>(1),
                "started_at": row.get::<_, String>(2),
                "duration_ms": row.get::<_, Option<i64>>(3),
                "triples_processed": row.get::<_, Option<i64>>(4),
                "queries_per_second": row.get::<_, Option<f64>>(5),
            })
        })
        .collect();
    json_response_http(StatusCode::OK, serde_json::json!({ "runs": runs }))
}
