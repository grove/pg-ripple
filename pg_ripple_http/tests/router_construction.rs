//! v0.128.1 emergency containment (HTTP startup): the axum router must build
//! without panicking and every registered route must resolve to a real
//! response (auth/method/handler) instead of a 404 — with no live
//! PostgreSQL connection required. Regression test for the Axum 0.7-style
//! `:capture` routes that panic Axum 0.8's router builder.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dashmap::DashMap;
use deadpool_postgres::{Config, Runtime};
use tokio_postgres::NoTls;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

use pg_ripple_http::common::AppState;
use pg_ripple_http::metrics::Metrics;
use pg_ripple_http::routing::build_router;

/// Every path registered in `routing::build_router`, with `{param}`
/// segments replaced by a literal `x`. Kept in sync with that route table.
const ROUTES: &[&str] = &[
    "/sparql",
    "/sparql/stream",
    "/rag",
    "/health",
    "/ready",
    "/health/ready",
    "/metrics",
    "/metrics/extension",
    "/void",
    "/service",
    "/openapi.yaml",
    "/datalog/rules",
    "/datalog/rules/x",
    "/datalog/rules/x/builtin",
    "/datalog/rules/x/add",
    "/datalog/rules/x/x",
    "/datalog/rules/x/enable",
    "/datalog/rules/x/disable",
    "/datalog/infer/x",
    "/datalog/infer/x/stats",
    "/datalog/infer/x/agg",
    "/datalog/infer/x/wfs",
    "/datalog/infer/x/demand",
    "/datalog/infer/x/lattice",
    "/datalog/query/x",
    "/datalog/constraints",
    "/datalog/constraints/x",
    "/datalog/stats/cache",
    "/datalog/stats/tabling",
    "/datalog/lattices",
    "/datalog/views",
    "/datalog/views/x",
    "/explorer",
    "/admin/bench-history",
    "/admin/diagnostic-snapshot",
    "/flight/do_get",
    "/subscribe/x",
    "/confidence/load",
    "/confidence/shacl-score",
    "/confidence/shacl-report",
    "/confidence/vacuum",
    "/confidence/update",
    "/confidence/bulk-update",
    "/pagerank/run",
    "/pagerank/results",
    "/pagerank/status",
    "/pagerank/vacuum-dirty",
    "/pagerank/export",
    "/pagerank/explain/x",
    "/pagerank/queue-stats",
    "/centrality/run",
    "/centrality/results",
    "/pagerank/find-duplicates",
    "/explain",
    "/hypothetical",
    "/rule-conflicts/x",
    "/rule-libraries",
    "/rule-libraries/x/stream",
    "/rule-libraries/x/subscribe",
    "/rules/draft",
    "/rules/validate",
    "/rules/x/explain",
    "/temporal/mark",
    "/temporal/point_in_time",
    "/temporal/facts",
    "/temporal/graphs/x/snapshot",
    "/temporal/graphs/x/diff",
    "/pprl/bloom_encode",
    "/pprl/dice_similarity",
    "/dp/noisy_count",
    "/dp/noisy_histogram",
    "/dp/budget/x/x",
    "/entity-resolution/resolve",
    "/entity-resolution/evaluate",
    "/entity-resolution/monitoring/enable",
    "/entity-resolution/monitoring/disable",
    "/proof-tree/x/x/x",
    "/tenants",
    "/tenants/x",
    "/tenants/x/quota",
    "/federation/x/auth-status",
    "/json-mapping/x/writeback",
    "/json-mapping/x/writeback/status",
];

/// Builds an `AppState` backed by a lazy pool pointed at a closed local port.
/// `deadpool_postgres` never connects at construction time, and connects to
/// a closed port on `127.0.0.1` fail instantly (ECONNREFUSED) rather than
/// timing out, so every handler that touches `state.pool` still returns
/// quickly without a real PostgreSQL instance.
fn test_state() -> Arc<AppState> {
    let mut cfg = Config::new();
    cfg.url = Some("postgresql://127.0.0.1:1/pg_ripple_router_test".to_owned());
    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("pool config is valid; no connection is attempted at creation");

    Arc::new(AppState {
        pool,
        auth_token: Some("test-auth-token".to_owned()),
        datalog_write_token: None,
        trust_proxy: None,
        metrics: Metrics::new(),
        ever_connected: AtomicBool::new(false),
        arrow_flight_secret: None,
        arrow_unsigned_tickets_allowed: false,
        arrow_nonce_cache: DashMap::new(),
        arrow_nonce_cache_max: 10_000,
        cors_is_permissive: false,
        metrics_token: None,
        auth_realm: "pg_ripple".to_owned(),
        replica_pool: None,
    })
}

#[tokio::test]
async fn router_builds_without_panicking_and_every_route_resolves() {
    let state = test_state();

    // Router construction must not panic — this is exactly what Axum 0.8
    // does when a route still uses the removed `:capture` syntax.
    let router = std::panic::catch_unwind(AssertUnwindSafe(|| {
        build_router(state, 1_048_576, CorsLayer::new())
    }))
    .expect("router construction panicked — check for obsolete `:capture` route syntax");

    for path in ROUTES {
        let request = Request::builder()
            .method("GET")
            .uri(*path)
            .body(Body::empty())
            .expect("request build is infallible for these static paths");

        let response = router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|e| panic!("route {path} dispatch failed: {e}"));

        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "route {path} returned 404 — stale or mistyped path pattern"
        );
    }
}
