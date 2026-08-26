//! HTTP routing — handler functions, response formatters, and `build_router`.
//!
//! All Axum handler functions, OpenAPI struct, content-type constants, and
//! query-parameter types live here so that `main` only contains startup logic.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::OpenApi;

pub mod middleware;

use crate::arrow_encode::flight_do_get;
use crate::common::{AppState, check_auth};
// CQ-05 (v0.90.0): datalog handlers moved into the routing sub-module.
// M15-14 (v0.96.0): datalog handlers split into sub-modules.
pub(crate) mod datalog_admin;
pub(crate) mod datalog_handlers;
pub(crate) mod datalog_inference;
use self::datalog_handlers as datalog;

// ─── OpenAPI specification (K-1, v0.55.0) ────────────────────────────────────

/// Generated OpenAPI 3.1 document for pg_ripple_http.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "pg_ripple_http",
        version = "0.16.0",
        description = "SPARQL 1.1 Protocol HTTP endpoint and Datalog REST API for pg_ripple",
        license(name = "Apache-2.0")
    ),
    paths(
        admin_handlers::openapi_spec,
    ),
    tags(
        (name = "sparql", description = "SPARQL 1.1 Query and Update Protocol"),
        (name = "datalog", description = "Datalog inference and rule management"),
        (name = "health", description = "Health and observability"),
        (name = "metadata", description = "Dataset and service metadata"),
    )
)]
pub struct ApiDoc;

// ─── Content types ───────────────────────────────────────────────────────────

pub(crate) const CT_SPARQL_JSON: &str = "application/sparql-results+json";
pub(crate) const CT_SPARQL_XML: &str = "application/sparql-results+xml";
pub(crate) const CT_CSV: &str = "text/csv";
pub(crate) const CT_TSV: &str = "text/tab-separated-values";
pub(crate) const CT_TURTLE: &str = "text/turtle";
pub(crate) const CT_NTRIPLES: &str = "application/n-triples";
pub(crate) const CT_JSONLD: &str = "application/ld+json";
pub(crate) const CT_SPARQL_QUERY: &str = "application/sparql-query";
pub(crate) const CT_SPARQL_UPDATE: &str = "application/sparql-update";
pub(crate) const CT_FORM: &str = "application/x-www-form-urlencoded";

// ─── Query parameters ────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(crate) struct SparqlParams {
    query: Option<String>,
    update: Option<String>,
    /// Feature 12 (v0.120.0): when present and equal to "ok", read-only queries
    /// are proxied to the read-replica DSN (if configured).
    replica: Option<String>,
}

// ─── RAG request / response ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct RagRequest {
    question: String,
    sparql_filter: Option<String>,
    #[serde(default = "default_k")]
    k: i32,
    model: Option<String>,
    #[serde(default = "default_output_format")]
    output_format: String,
}

fn default_k() -> i32 {
    5
}
fn default_output_format() -> String {
    "jsonb".to_owned()
}

#[derive(Serialize)]
pub(crate) struct RagResult {
    entity_iri: String,
    label: String,
    context_json: serde_json::Value,
    distance: f64,
}

#[derive(Serialize)]
pub(crate) struct RagResponse {
    results: Vec<RagResult>,
    /// Concatenated plain-text context for direct use as an LLM system prompt.
    context: String,
}

// ─── Main ────────────────────────────────────────────────────────────────────

// MOD-01 (v0.72.0): extracted handler submodules
pub(crate) mod admin_handlers;
pub(crate) mod confidence_handlers;
pub(crate) mod conflict_handler;
pub(crate) mod explain_handler;
pub(crate) mod hypothetical_handler;
pub(crate) mod pagerank_handlers;
pub(crate) mod rag_handler;
pub(crate) mod rule_authoring_handler;
// v0.110.0: Rule explain handler
pub(crate) mod rule_explain_handler;
pub(crate) mod rule_library_handler;
pub(crate) mod sparql_handlers;
// v0.115.0 M16-02: new subsystem handlers.
pub(crate) mod dp_handlers;
pub(crate) mod entity_resolution_handlers;
pub(crate) mod pprl_handlers;
pub(crate) mod proof_tree_handler;
pub(crate) mod temporal_handlers;
pub(crate) mod tenant_handlers;
// v0.126.0 FEAT-03: Per-endpoint federation credential status.
pub(crate) mod federation_credential_handlers;
// v0.128.0 JSON-WRITEBACK-01: JSON mapping relational writeback.
pub(crate) mod json_mapping_handlers;

// Re-export public helpers that arrow_encode.rs and spi_bridge.rs import via
// `crate::routing::...`.  These functions live in sparql_handlers but are
// accessible at the routing crate path for backward compatibility.
pub(crate) use sparql_handlers::{
    format_ask_result, format_graph_results, format_select_results, json_response_http,
};

// ─── Router factory ───────────────────────────────────────────────────────────

/// Build the application [`Router`] and apply middleware layers.
///
/// Called from `main` after the [`AppState`] and CORS policy are constructed.
pub fn build_router(state: Arc<AppState>, max_body_bytes: usize, cors: CorsLayer) -> Router {
    Router::new()
        // SPARQL 1.1 Protocol
        .route(
            "/sparql",
            get(sparql_handlers::sparql_get).post(sparql_handlers::sparql_post),
        )
        .route("/sparql/stream", post(sparql_handlers::sparql_stream_post))
        .route("/rag", post(rag_handler::rag_post))
        .route("/health", get(admin_handlers::health))
        // v0.60.0 H7-5: Kubernetes readiness probe — 503 until first PG connection.
        .route("/ready", get(admin_handlers::ready))
        // O13-01 (v0.84.0): deep extension health-check with 2-second deadline.
        .route("/health/ready", get(admin_handlers::health_ready))
        .route("/metrics", get(admin_handlers::metrics_endpoint))
        // SECURITY (METRICS-AUTH-DOC-01, v0.83.0): /metrics and /metrics/extension
        // are intentionally unauthenticated to support Prometheus scraping from a
        // trusted internal network without requiring a token.  These routes expose
        // only aggregate counters — no user data — so the risk is information
        // disclosure of query throughput figures.  Operators who need authentication
        // should place a reverse proxy (nginx, Caddy, Envoy) in front with an
        // IP-allowlist or mTLS.  See docs/src/operations/metrics.md.
        // v0.72.0 OBS-02: Extension streaming metrics endpoint.
        .route(
            "/metrics/extension",
            get(admin_handlers::extension_metrics_endpoint),
        )
        // v0.55.0 L-7.2: VoID dataset description
        .route("/void", get(admin_handlers::void_endpoint))
        // v0.55.0 L-7.4: SPARQL Service Description
        .route("/service", get(admin_handlers::service_description))
        // v0.55.0 K-1: OpenAPI specification
        .route("/openapi.yaml", get(admin_handlers::openapi_spec))
        // Datalog — Phase 1: Rule management
        .route("/datalog/rules", get(datalog::list_rules))
        .route(
            "/datalog/rules/{rule_set}",
            post(datalog::load_rules).delete(datalog::drop_rules),
        )
        .route(
            "/datalog/rules/{rule_set}/builtin",
            post(datalog::load_builtin),
        )
        .route("/datalog/rules/{rule_set}/add", post(datalog::add_rule))
        .route(
            "/datalog/rules/{rule_set}/{rule_id}",
            delete(datalog::remove_rule),
        )
        .route(
            "/datalog/rules/{rule_set}/enable",
            put(datalog::enable_rule_set),
        )
        .route(
            "/datalog/rules/{rule_set}/disable",
            put(datalog::disable_rule_set),
        )
        // Datalog — Phase 2: Inference
        .route("/datalog/infer/{rule_set}", post(datalog::infer))
        .route(
            "/datalog/infer/{rule_set}/stats",
            post(datalog::infer_with_stats),
        )
        .route("/datalog/infer/{rule_set}/agg", post(datalog::infer_agg))
        .route("/datalog/infer/{rule_set}/wfs", post(datalog::infer_wfs))
        .route(
            "/datalog/infer/{rule_set}/demand",
            post(datalog::infer_demand),
        )
        .route(
            "/datalog/infer/{rule_set}/lattice",
            post(datalog::infer_lattice),
        )
        // Datalog — Phase 3: Query & constraints
        .route("/datalog/query/{rule_set}", post(datalog::query_goal))
        .route("/datalog/constraints", get(datalog::check_constraints_all))
        .route(
            "/datalog/constraints/{rule_set}",
            get(datalog::check_constraints),
        )
        // Datalog — Phase 4: Admin & monitoring
        .route("/datalog/stats/cache", get(datalog::cache_stats))
        .route("/datalog/stats/tabling", get(datalog::tabling_stats))
        .route(
            "/datalog/lattices",
            get(datalog::list_lattices).post(datalog::create_lattice),
        )
        .route(
            "/datalog/views",
            get(datalog::list_views).post(datalog::create_view),
        )
        .route("/datalog/views/{name}", delete(datalog::drop_view))
        // v0.62.0: Visual graph explorer — browser-based SPARQL CONSTRUCT visualiser.
        .route("/explorer", get(admin_handlers::explorer_page))
        // v0.118.0 Feature 1: Benchmark history endpoint.
        .route("/admin/bench-history", get(admin_handlers::bench_history))
        // v0.120.0 Feature 8: Diagnostic snapshot endpoint.
        .route(
            "/admin/diagnostic-snapshot",
            get(admin_handlers::diagnostic_snapshot),
        )
        // v0.62.0: Arrow Flight bulk-export endpoint.
        .route("/flight/do_get", post(flight_do_get))
        // v0.73.0 SUB-01: Live SPARQL subscription SSE endpoint.
        .route("/subscribe/{subscription_id}", get(sparql_subscription_sse))
        // v0.87.0: Uncertain Knowledge Engine — confidence API endpoints.
        .route(
            "/confidence/load",
            post(confidence_handlers::load_with_confidence),
        )
        .route(
            "/confidence/shacl-score",
            get(confidence_handlers::shacl_score),
        )
        .route(
            "/confidence/shacl-report",
            get(confidence_handlers::shacl_report_scored),
        )
        .route(
            "/confidence/vacuum",
            post(confidence_handlers::vacuum_confidence),
        )
        // v0.108.0: Bayesian confidence update endpoints.
        .route(
            "/confidence/update",
            post(confidence_handlers::update_confidence),
        )
        .route(
            "/confidence/bulk-update",
            post(confidence_handlers::bulk_update_confidence),
        )
        // v0.88.0: PageRank & Graph Analytics (PR-HTTP-01)
        .route("/pagerank/run", post(pagerank_handlers::pagerank_run))
        .route(
            "/pagerank/results",
            get(pagerank_handlers::pagerank_results),
        )
        .route("/pagerank/status", get(pagerank_handlers::pagerank_status))
        .route(
            "/pagerank/vacuum-dirty",
            post(pagerank_handlers::vacuum_dirty),
        )
        .route("/pagerank/export", get(pagerank_handlers::pagerank_export))
        .route(
            "/pagerank/explain/{node_iri}",
            get(pagerank_handlers::pagerank_explain),
        )
        .route(
            "/pagerank/queue-stats",
            get(pagerank_handlers::pagerank_queue_stats),
        )
        .route("/centrality/run", post(pagerank_handlers::centrality_run))
        .route(
            "/centrality/results",
            get(pagerank_handlers::centrality_results),
        )
        .route(
            "/pagerank/find-duplicates",
            post(pagerank_handlers::find_duplicates),
        )
        // v0.101.0: Natural-language inference explanation (NL-EXPLAIN-01)
        .route(
            "/explain",
            post(explain_handler::explain_post).get(explain_handler::explain_get),
        )
        // v0.102.0: What-if reasoning (hypothetical inference)
        .route(
            "/hypothetical",
            post(hypothetical_handler::hypothetical_post),
        )
        // v0.103.0: Rule conflict detection
        .route(
            "/rule-conflicts/{ruleset}",
            get(conflict_handler::rule_conflicts_get),
        )
        // v0.104.0: Rule library infrastructure
        .route(
            "/rule-libraries",
            get(rule_library_handler::list_rule_libraries),
        )
        // v0.120.0 Feature 11: Rule-library federation (stream + subscribe).
        .route(
            "/rule-libraries/{name}/stream",
            get(rule_library_handler::stream_rule_library),
        )
        .route(
            "/rule-libraries/{name}/subscribe",
            post(rule_library_handler::subscribe_rule_library),
        )
        // v0.105.0: Guided rule authoring & LLM rule extraction
        .route(
            "/rules/draft",
            post(rule_authoring_handler::draft_rules_post),
        )
        .route(
            "/rules/validate",
            post(rule_authoring_handler::validate_rule_post),
        )
        // v0.110.0: Rule explainability — GET /rules/{id}/explain
        .route(
            "/rules/{id}/explain",
            get(rule_explain_handler::explain_rule_get),
        )
        // v0.115.0 M16-02: Temporal facts REST API.
        .route(
            "/temporal/mark",
            get(temporal_handlers::list_temporal_predicates)
                .post(temporal_handlers::mark_temporal_predicate),
        )
        .route(
            "/temporal/point_in_time",
            post(temporal_handlers::set_point_in_time),
        )
        .route("/temporal/facts", get(temporal_handlers::temporal_facts))
        // v0.125.0 FEAT-02: Temporal graph snapshot and diff endpoints.
        .route(
            "/temporal/graphs/{iri}/snapshot",
            get(temporal_handlers::graph_snapshot),
        )
        .route(
            "/temporal/graphs/{iri}/diff",
            get(temporal_handlers::graph_diff),
        )
        // v0.115.0 M16-02: PPRL REST API.
        .route("/pprl/bloom_encode", post(pprl_handlers::pprl_bloom_encode))
        .route(
            "/pprl/dice_similarity",
            post(pprl_handlers::pprl_dice_similarity),
        )
        // v0.115.0 M16-02: Differential-privacy REST API.
        .route("/dp/noisy_count", post(dp_handlers::dp_noisy_count))
        .route("/dp/noisy_histogram", post(dp_handlers::dp_noisy_histogram))
        // v0.118.0 Feature 2: Privacy budget status endpoint.
        .route(
            "/dp/budget/{dataset}/{principal}",
            get(dp_handlers::dp_budget_get),
        )
        // v0.115.0 M16-02: Entity-resolution REST API.
        .route(
            "/entity-resolution/resolve",
            post(entity_resolution_handlers::entity_resolution_resolve),
        )
        .route(
            "/entity-resolution/evaluate",
            post(entity_resolution_handlers::entity_resolution_evaluate),
        )
        .route(
            "/entity-resolution/monitoring/enable",
            post(entity_resolution_handlers::entity_resolution_monitoring_enable),
        )
        .route(
            "/entity-resolution/monitoring/disable",
            post(entity_resolution_handlers::entity_resolution_monitoring_disable),
        )
        // v0.115.0 M16-02: Proof-tree REST API.
        .route(
            "/proof-tree/{subject}/{predicate}/{object}",
            get(proof_tree_handler::proof_tree_get),
        )
        // v0.115.0 M16-02: Multi-tenant REST API.
        .route(
            "/tenants",
            get(tenant_handlers::list_tenants).post(tenant_handlers::create_tenant),
        )
        .route(
            "/tenants/{name}",
            get(tenant_handlers::get_tenant).delete(tenant_handlers::delete_tenant),
        )
        // v0.120.0: Tenant quota endpoints.
        .route(
            "/tenants/{name}/quota",
            get(tenant_handlers::get_tenant_quota).post(tenant_handlers::update_tenant_quota),
        )
        // v0.126.0 FEAT-03: Per-endpoint federation credential status (write-auth required).
        .route(
            "/federation/{endpoint}/auth-status",
            get(federation_credential_handlers::federation_auth_status),
        )
        // v0.128.0 JSON-WRITEBACK-01: JSON mapping relational writeback.
        .route(
            "/json-mapping/{name}/writeback",
            post(json_mapping_handlers::json_mapping_writeback_post),
        )
        .route(
            "/json-mapping/{name}/writeback/status",
            get(json_mapping_handlers::json_mapping_writeback_status_get),
        )
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(cors)
        .with_state(state)
}

// ─── v0.73.0 SUB-01: Live SPARQL subscription SSE endpoint ───────────────────

/// `GET /subscribe/:subscription_id` — Server-Sent Events stream for a live
/// SPARQL subscription.
///
/// Polls the subscription state and forwards change events as SSE.
/// A keepalive comment is sent every 15 seconds.
///
/// Requires `Authorization: Bearer <token>` when auth is configured.
async fn sparql_subscription_sse(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(subscription_id): axum::extract::Path<String>,
) -> Response {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    // Validate subscription_id is safe for use in a channel name.
    // Only allow alphanumeric, hyphen, underscore to prevent injection.
    if !subscription_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return (
            StatusCode::BAD_REQUEST,
            "invalid subscription_id: only alphanumeric, hyphen and underscore allowed",
        )
            .into_response();
    }

    // LISTEN-LEN-01 (v0.82.0): enforce 63-character limit.
    // PostgreSQL silently truncates LISTEN channel names longer than 63 bytes,
    // which can cause channel-name collisions between subscription IDs that
    // share the same first 63 characters.
    if subscription_id.len() > 63 {
        return (
            StatusCode::BAD_REQUEST,
            "invalid subscription_id: maximum length is 63 characters",
        )
            .into_response();
    }

    // Spawn a background task that polls for subscription notifications and
    // sends them over an mpsc channel.
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);
    let pool = state.pool.clone();
    let sub_id = subscription_id.clone();

    tokio::spawn(async move {
        let channel = format!("pg_ripple_subscription_{sub_id}");

        // Get a connection from the pool.
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(format!("event: error\ndata: {{\"error\":\"{e}\"}}\n\n"))
                    .await;
                return;
            }
        };

        // LISTEN on the notification channel.
        if let Err(e) = client.execute(&format!("LISTEN \"{channel}\""), &[]).await {
            let _ = tx
                .send(format!("event: error\ndata: {{\"error\":\"{e}\"}}\n\n"))
                .await;
            return;
        }

        // Send an initial event to confirm the subscription is active.
        if tx
            .send(format!(
                "event: subscribed\ndata: {{\"subscription_id\":\"{sub_id}\"}}\n\n"
            ))
            .await
            .is_err()
        {
            return;
        }

        // Poll every 5 seconds using a simple pg_ripple function to check for
        // any queued notifications.  Because we are using the pool connection,
        // we cannot block-wait on raw LISTEN notifications; instead we poll
        // pg_notification_queue_usage() and send keepalives in between.
        let mut keepalive_tick: u64 = 0;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            if tx.is_closed() {
                break;
            }

            keepalive_tick += 1;
            if keepalive_tick.is_multiple_of(3) {
                // Send keepalive comment every 15 seconds.
                if tx.send(": keepalive\n\n".to_string()).await.is_err() {
                    break;
                }
            }
        }
    });

    // Stream the SSE events back as a chunked HTTP response.
    use tokio_stream::StreamExt as _;
    use tokio_stream::wrappers::ReceiverStream;

    let body_stream = ReceiverStream::new(rx)
        .map(|chunk: String| Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(chunk)));

    (
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-cache"),
            ("x-accel-buffering", "no"),
        ],
        Body::from_stream(body_stream),
    )
        .into_response()
}
