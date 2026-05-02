//! PageRank & Graph Analytics HTTP handlers (v0.88.0 PR-HTTP-01).
//!
//! POST /pagerank/run              — trigger full PageRank computation
//! GET  /pagerank/results          — top-N PageRank results (paginated)
//! GET  /pagerank/status           — last run metadata
//! POST /pagerank/vacuum-dirty     — drain the dirty-edges queue
//! GET  /pagerank/export           — streaming export
//! GET  /pagerank/explain/:node    — score explanation tree
//! GET  /pagerank/queue-stats      — IVM queue metrics
//! POST /centrality/run            — trigger centrality computation
//! GET  /centrality/results        — centrality scores by metric
//! POST /pagerank/find-duplicates  — centrality-guided entity deduplication

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, check_auth, check_auth_write, redacted_error};

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    json_response_http(status, body)
}

// ── Request / response types ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
pub struct PageRankRunRequest {
    pub edge_predicates: Option<Vec<String>>,
    #[serde(default = "default_damping")]
    pub damping: f64,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i32,
    #[serde(default = "default_convergence_delta")]
    pub convergence_delta: f64,
    pub graph_uri: Option<String>,
    #[serde(default = "default_direction")]
    pub direction: String,
    pub edge_weight_predicate: Option<String>,
    pub topic: Option<String>,
    #[serde(default)]
    pub decay_rate: f64,
    pub temporal_predicate: Option<String>,
    pub seed_iris: Option<Vec<String>>,
    #[serde(default = "default_bias")]
    pub bias: f64,
    pub predicate_filter: Option<Vec<String>>,
}

fn default_damping() -> f64 {
    0.85
}
fn default_max_iterations() -> i32 {
    100
}
fn default_convergence_delta() -> f64 {
    0.0001
}
fn default_direction() -> String {
    "forward".to_owned()
}
fn default_bias() -> f64 {
    0.15
}

#[derive(Debug, Deserialize)]
pub struct PageRankResultsParams {
    pub topic: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub exact_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PageRankExportParams {
    #[serde(default = "default_export_format")]
    pub format: String,
    pub top_k: Option<i64>,
    pub topic: Option<String>,
}
fn default_export_format() -> String {
    "csv".to_owned()
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct CentralityRunRequest {
    pub metric: String,
    pub edge_predicates: Option<Vec<String>>,
    pub graph_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CentralityResultsParams {
    pub metric: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FindDuplicatesRequest {
    #[serde(default = "default_centrality_metric")]
    pub metric: String,
    #[serde(default = "default_centrality_threshold")]
    pub centrality_threshold: f64,
    #[serde(default = "default_fuzzy_threshold")]
    pub fuzzy_threshold: f64,
}
fn default_centrality_metric() -> String {
    "betweenness".to_owned()
}
fn default_centrality_threshold() -> f64 {
    0.1
}
fn default_fuzzy_threshold() -> f64 {
    0.85
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /pagerank/run
///
/// Triggers a full PageRank computation. Returns a job summary with
/// node count, iteration count, and convergence status.
pub(crate) async fn pagerank_run(
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
    let req: PageRankRunRequest = if bytes.is_empty() {
        PageRankRunRequest::default()
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

    let topic = req.topic.clone().unwrap_or_default();
    let sql = format!(
        "SELECT COUNT(*) FROM pg_ripple.pagerank_run(\
          damping => {}, max_iterations => {}, convergence_delta => {}, \
          direction => '{}', decay_rate => {}, bias => {} \
        )",
        req.damping,
        req.max_iterations,
        req.convergence_delta,
        req.direction.replace('\'', "''"),
        req.decay_rate,
        req.bias,
    );

    let row_count: i64 = match client.query_one(&sql, &[]).await {
        Ok(row) => row.get::<_, i64>(0),
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "pagerank_run_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    json_response(
        StatusCode::OK,
        serde_json::json!({
            "status": "ok",
            "nodes_ranked": row_count,
            "topic": topic,
        }),
    )
}

/// GET /pagerank/results
///
/// Returns top-N PageRank results, optionally filtered by topic.
/// Supports `?exact_only=true` to restrict to non-stale scores.
pub(crate) async fn pagerank_results(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<PageRankResultsParams>,
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

    let topic = params.topic.clone().unwrap_or_default();
    let topic_esc = topic.replace('\'', "''");
    let limit = params.limit.unwrap_or(100).min(10000);
    let offset = params.offset.unwrap_or(0).max(0);
    let stale_filter = if params.exact_only.unwrap_or(false) {
        "AND ps.stale = false"
    } else {
        ""
    };

    let sql = format!(
        "SELECT d.value, ps.score, ps.score_lower, ps.score_upper, \
                ps.iterations, ps.converged, ps.stale, ps.computed_at::TEXT \
         FROM _pg_ripple.pagerank_scores ps \
         JOIN _pg_ripple.dictionary d ON d.id = ps.node \
         WHERE ps.topic = '{topic_esc}' {stale_filter} \
         ORDER BY ps.score DESC \
         LIMIT {limit} OFFSET {offset}"
    );

    let rows = match client.query(&sql, &[]).await {
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

    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let iri: String = row.get(0);
            let clean = iri.trim_matches(|c| c == '<' || c == '>');
            serde_json::json!({
                "node": clean,
                "score": row.get::<_, f64>(1),
                "score_lower": row.get::<_, f64>(2),
                "score_upper": row.get::<_, f64>(3),
                "iterations": row.get::<_, i32>(4),
                "converged": row.get::<_, bool>(5),
                "stale": row.get::<_, bool>(6),
                "computed_at": row.get::<_, String>(7),
            })
        })
        .collect();

    json_response(
        StatusCode::OK,
        serde_json::json!({"results": results, "count": results.len()}),
    )
}

/// GET /pagerank/status
///
/// Returns metadata for the last pagerank_run().
pub(crate) async fn pagerank_status(
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

    let sql = "SELECT \
        MAX(computed_at)::TEXT, \
        BOOL_AND(converged), \
        COUNT(*) FILTER (WHERE stale) \
      FROM _pg_ripple.pagerank_scores WHERE topic = ''";

    let row = match client.query_one(sql, &[]).await {
        Ok(r) => r,
        Err(_) => {
            return json_response(
                StatusCode::OK,
                serde_json::json!({"computed_at": null, "converged": null, "stale_count": 0}),
            );
        }
    };

    json_response(
        StatusCode::OK,
        serde_json::json!({
            "computed_at": row.get::<_, Option<String>>(0),
            "converged": row.get::<_, Option<bool>>(1),
            "stale_count": row.get::<_, i64>(2),
        }),
    )
}

/// POST /pagerank/vacuum-dirty
///
/// Drains processed entries from the dirty-edges queue.
pub(crate) async fn vacuum_dirty(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
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
    let sql = "WITH deleted AS ( \
        DELETE FROM _pg_ripple.pagerank_dirty_edges \
        WHERE enqueued_at < NOW() - INTERVAL '1 day' \
        RETURNING 1 \
      ) SELECT COUNT(*)::BIGINT FROM deleted";
    let deleted: i64 = match client.query_one(sql, &[]).await {
        Ok(r) => r.get(0),
        Err(_) => 0,
    };
    json_response(StatusCode::OK, serde_json::json!({"deleted": deleted}))
}

/// GET /pagerank/export
///
/// Stream PageRank scores in the requested format.
/// `?format=csv&top_k=1000&topic=` supported.
pub(crate) async fn pagerank_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<PageRankExportParams>,
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

    let topic = params.topic.clone().unwrap_or_default();
    let topic_esc = topic.replace('\'', "''");
    let top_k = params.top_k.map(|k| k.min(100_000)).unwrap_or(10_000);

    let sql = format!(
        "SELECT d.value, ps.score, ps.stale \
         FROM _pg_ripple.pagerank_scores ps \
         JOIN _pg_ripple.dictionary d ON d.id = ps.node \
         WHERE ps.topic = '{topic_esc}' \
         ORDER BY ps.score DESC \
         LIMIT {top_k}"
    );

    let rows = match client.query(&sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            return redacted_error(
                "export_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let (content_type, body) = match params.format.as_str() {
        "csv" => {
            let mut out = String::from("node_iri,score,stale\n");
            for row in &rows {
                let iri: String = row.get(0);
                let clean = iri.trim_matches(|c| c == '<' || c == '>');
                let score: f64 = row.get(1);
                let stale: bool = row.get(2);
                out.push_str(&format!("{clean},{score:.8},{stale}\n"));
            }
            ("text/csv", out)
        }
        "turtle" => {
            let mut out = String::from("@prefix pg: <http://pg-ripple.io/ns#> .\n\n");
            for row in &rows {
                let iri: String = row.get(0);
                let score: f64 = row.get(1);
                out.push_str(&format!("{iri} pg:hasPageRank \"{score:.8}\"^^<http://www.w3.org/2001/XMLSchema#double> .\n"));
            }
            ("text/turtle", out)
        }
        "ntriples" => {
            let mut out = String::new();
            for row in &rows {
                let iri: String = row.get(0);
                let score: f64 = row.get(1);
                out.push_str(&format!("{iri} <http://pg-ripple.io/ns#hasPageRank> \"{score:.8}\"^^<http://www.w3.org/2001/XMLSchema#double> .\n"));
            }
            ("application/n-triples", out)
        }
        "jsonld" => {
            let items: Vec<String> = rows.iter().map(|row| {
                let iri: String = row.get(0);
                let clean = iri.trim_matches(|c| c == '<' || c == '>');
                let score: f64 = row.get(1);
                format!("  {{\"@id\":\"{clean}\",\"http://pg-ripple.io/ns#hasPageRank\":{{\"@value\":{score:.8}}}}}")
            }).collect();
            (
                "application/ld+json",
                format!("[\n{}\n]", items.join(",\n")),
            )
        }
        fmt => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "PT0417", "detail": format!("unsupported export format '{fmt}'")}),
            );
        }
    };

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "build_response_error"}),
            )
        })
}

/// GET /pagerank/explain/:node_iri
///
/// Returns the score explanation tree for a node.
pub(crate) async fn pagerank_explain(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(node_iri): Path<String>,
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
    let node_esc = node_iri.replace('\'', "''");
    let sql = format!("SELECT * FROM pg_ripple.explain_pagerank('{node_esc}', 5)");
    let rows = match client.query(&sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "explain_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "depth": row.get::<_, i32>(0),
                "contributor": row.get::<_, String>(1),
                "contribution": row.get::<_, f64>(2),
                "path": row.get::<_, String>(3),
            })
        })
        .collect();
    json_response(
        StatusCode::OK,
        serde_json::json!({"node": node_iri, "contributors": results}),
    )
}

/// GET /pagerank/queue-stats
///
/// Returns IVM queue depth and timing metrics.
pub(crate) async fn pagerank_queue_stats(
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
    let sql = "SELECT COUNT(*)::BIGINT, COALESCE(MAX(ABS(delta::FLOAT8)), 0.0), \
               MIN(enqueued_at)::TEXT \
               FROM _pg_ripple.pagerank_dirty_edges";
    let row = match client.query_one(sql, &[]).await {
        Ok(r) => r,
        Err(_) => {
            return json_response(
                StatusCode::OK,
                serde_json::json!({"queued_edges": 0, "max_delta": 0}),
            );
        }
    };
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "queued_edges": row.get::<_, i64>(0),
            "max_delta": row.get::<_, f64>(1),
            "oldest_enqueue": row.get::<_, Option<String>>(2),
        }),
    )
}

/// POST /centrality/run
///
/// Triggers a centrality computation for the given metric.
pub(crate) async fn centrality_run(
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
    let req: CentralityRunRequest = match serde_json::from_slice(&bytes) {
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
    let metric_esc = req.metric.replace('\'', "''");
    let sql = format!("SELECT COUNT(*) FROM pg_ripple.centrality_run('{metric_esc}')");
    let count: i64 = match client.query_one(&sql, &[]).await {
        Ok(r) => r.get(0),
        Err(e) => {
            return redacted_error(
                "centrality_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    json_response(
        StatusCode::OK,
        serde_json::json!({"metric": req.metric, "nodes_scored": count}),
    )
}

/// GET /centrality/results
///
/// Returns centrality scores for a metric, ordered by score descending.
pub(crate) async fn centrality_results(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<CentralityResultsParams>,
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
    let metric_filter = match &params.metric {
        Some(m) => format!("WHERE cs.metric = '{}'", m.replace('\'', "''")),
        None => String::new(),
    };
    let limit = params.limit.unwrap_or(100).min(10000);
    let sql = format!(
        "SELECT d.value, cs.metric, cs.score \
         FROM _pg_ripple.centrality_scores cs \
         JOIN _pg_ripple.dictionary d ON d.id = cs.node \
         {metric_filter} \
         ORDER BY cs.score DESC \
         LIMIT {limit}"
    );
    let rows = match client.query(&sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "query_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let results: Vec<serde_json::Value> = rows.iter().map(|row| {
        let iri: String = row.get(0);
        let clean = iri.trim_matches(|c| c == '<' || c == '>');
        serde_json::json!({"node": clean, "metric": row.get::<_, String>(1), "score": row.get::<_, f64>(2)})
    }).collect();
    json_response(StatusCode::OK, serde_json::json!({"results": results}))
}

/// POST /pagerank/find-duplicates
///
/// Find candidate duplicate nodes via centrality + fuzzy matching.
pub(crate) async fn find_duplicates(
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
    let req: FindDuplicatesRequest = if bytes.is_empty() {
        FindDuplicatesRequest {
            metric: "betweenness".to_owned(),
            centrality_threshold: 0.1,
            fuzzy_threshold: 0.85,
        }
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
    let metric_esc = req.metric.replace('\'', "''");
    let sql = format!(
        "SELECT * FROM pg_ripple.pagerank_find_duplicates('{}', {}, {})",
        metric_esc, req.centrality_threshold, req.fuzzy_threshold
    );
    let rows = match client.query(&sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "find_dup_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "node_a": row.get::<_, String>(0),
                "node_b": row.get::<_, String>(1),
                "centrality_score": row.get::<_, f64>(2),
                "fuzzy_score": row.get::<_, f64>(3),
            })
        })
        .collect();
    json_response(StatusCode::OK, serde_json::json!({"duplicates": results}))
}
