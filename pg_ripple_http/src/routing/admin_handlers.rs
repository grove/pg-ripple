//! Admin/observability/explorer handlers -- extracted from routing.rs (MOD-01, v0.72.0).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use super::sparql_handlers::json_response_http;
use crate::common::{AppState, check_auth, redacted_error};

// Re-import routing types for ApiDoc
use super::ApiDoc;
use utoipa::OpenApi as _;

/// Build timestamp recorded at compile time (RFC 3339).
/// BUILD-TIME-FIELD-01 (v0.83.0): populated by build.rs with an RFC-3339
/// timestamp, or falls back to the Cargo package version string prefixed with
/// "build-version=" when SOURCE_DATE_EPOCH is unset.
const BUILD_TIME: &str = option_env!("BUILD_TIMESTAMP").unwrap_or(concat!("build-version=", env!("CARGO_PKG_VERSION")));

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

// ─── Metrics endpoint ────────────────────────────────────────────────────────

pub(crate) async fn metrics_endpoint(State(state): State<Arc<AppState>>) -> Response {
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
         pg_ripple_http_sparql_queries_by_result_size_total{{result_size_bucket=\"large\"}} {}\n",
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
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4")
        .body(Body::from(body))
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

// ─── v0.62.0: Visual graph explorer ─────────────────────────────────────────

/// Serve the browser-based visual graph explorer at `/explorer`.
///
/// EXPLORER-AUTH-01 (v0.80.0): authentication is required. Unauthenticated
/// requests receive HTTP 401 so that the full RDF graph cannot be browsed
/// without credentials.
///
/// The explorer is a single-page application that accepts a SPARQL CONSTRUCT
/// query, renders the resulting triples as a force-directed graph using
/// sigma.js, and allows clicking any node to expand its neighbourhood.
pub(crate) async fn explorer_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    // EXPLORER-AUTH-01: require authentication before serving the explorer UI.
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>pg_ripple Graph Explorer</title>
  <style>
    body { margin: 0; font-family: sans-serif; display: flex; flex-direction: column; height: 100vh; background: #1a1a2e; color: #eee; }
    #toolbar { padding: 10px; background: #16213e; display: flex; gap: 8px; align-items: center; border-bottom: 1px solid #0f3460; }
    #toolbar label { font-size: 13px; color: #a0aec0; }
    #query { flex: 1; padding: 6px 10px; border-radius: 4px; border: 1px solid #0f3460; background: #0f3460; color: #eee; font-family: monospace; font-size: 13px; }
    #run-btn { padding: 6px 16px; border-radius: 4px; border: none; background: #e94560; color: #fff; cursor: pointer; font-size: 13px; }
    #run-btn:hover { background: #c73652; }
    #status { font-size: 12px; color: #a0aec0; padding: 4px; }
    #canvas { flex: 1; background: #0d1117; }
    #info-panel { position: fixed; right: 10px; top: 60px; width: 300px; background: #16213e; border: 1px solid #0f3460; border-radius: 6px; padding: 12px; display: none; font-size: 12px; max-height: 80vh; overflow-y: auto; }
    .node-label { font-weight: bold; color: #e94560; margin-bottom: 6px; word-break: break-all; }
    .triple-row { margin: 4px 0; padding: 4px; background: #0f3460; border-radius: 3px; word-break: break-all; }
  </style>
</head>
<body>
  <div id="toolbar">
    <label>SPARQL CONSTRUCT:</label>
    <input id="query" type="text" value="CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 100" placeholder="Enter SPARQL CONSTRUCT query..." />
    <button id="run-btn" onclick="runQuery()">Run</button>
    <span id="status"></span>
  </div>
  <canvas id="canvas"></canvas>
  <div id="info-panel">
    <div class="node-label" id="info-title"></div>
    <div id="info-triples"></div>
    <button onclick="expandNode()" style="margin-top:8px;padding:4px 10px;border-radius:3px;border:none;background:#e94560;color:#fff;cursor:pointer;font-size:12px;">Expand</button>
  </div>

  <script>
    const SPARQL_ENDPOINT = '/sparql';
    let graph = { nodes: {}, edges: [] };
    let canvas, ctx, selectedNode = null;
    let positions = {};
    let velocities = {};
    let animFrame = null;

    function init() {
      canvas = document.getElementById('canvas');
      ctx = canvas.getContext('2d');
      canvas.width = canvas.offsetWidth;
      canvas.height = canvas.offsetHeight;
      canvas.addEventListener('click', onCanvasClick);
      window.addEventListener('resize', () => { canvas.width = canvas.offsetWidth; canvas.height = canvas.offsetHeight; draw(); });
    }

    async function runQuery() {
      const q = document.getElementById('query').value.trim();
      if (!q) return;
      document.getElementById('status').textContent = 'Running...';
      try {
        const resp = await fetch('/sparql', {
          method: 'POST',
          headers: {'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json'},
          body: 'query=' + encodeURIComponent(q)
        });
        if (!resp.ok) throw new Error(await resp.text());
        const data = await resp.json();
        buildGraph(data);
        document.getElementById('status').textContent = graph.edges.length + ' triples, ' + Object.keys(graph.nodes).length + ' nodes';
      } catch(e) {
        document.getElementById('status').textContent = 'Error: ' + e.message;
      }
    }

    function buildGraph(results) {
      graph = { nodes: {}, edges: [] };
      positions = {};
      velocities = {};
      const W = canvas.width, H = canvas.height;
      for (const row of results) {
        const s = row.s && row.s.value || row.s || null;
        const p = row.p && row.p.value || row.p || null;
        const o = row.o && row.o.value || row.o || null;
        if (!s || !p || !o) continue;
        if (!graph.nodes[s]) { graph.nodes[s] = { id: s, triples: [] }; positions[s] = { x: Math.random()*W, y: Math.random()*H }; velocities[s] = { x: 0, y: 0 }; }
        if (!graph.nodes[o]) { graph.nodes[o] = { id: o, triples: [] }; positions[o] = { x: Math.random()*W, y: Math.random()*H }; velocities[o] = { x: 0, y: 0 }; }
        graph.nodes[s].triples.push({ p, o });
        graph.edges.push({ s, p, o });
      }
      if (animFrame) cancelAnimationFrame(animFrame);
      simulate();
    }

    function simulate() {
      const nodes = Object.keys(graph.nodes);
      if (nodes.length === 0) return;
      for (let i = 0; i < 5; i++) forceStep(nodes);
      draw();
      animFrame = requestAnimationFrame(simulate);
    }

    function forceStep(nodes) {
      const k = 100, W = canvas.width, H = canvas.height;
      for (const a of nodes) {
        let fx = 0, fy = 0;
        for (const b of nodes) {
          if (a === b) continue;
          const dx = positions[a].x - positions[b].x, dy = positions[a].y - positions[b].y;
          const dist = Math.max(Math.sqrt(dx*dx+dy*dy), 1);
          fx += (k*k/dist) * (dx/dist);
          fy += (k*k/dist) * (dy/dist);
        }
        for (const e of graph.edges) {
          let other = null;
          if (e.s === a) other = e.o;
          else if (e.o === a) other = e.s;
          if (!other) continue;
          const dx = positions[a].x - positions[other].x, dy = positions[a].y - positions[other].y;
          const dist = Math.max(Math.sqrt(dx*dx+dy*dy), 1);
          fx -= (dist*dist/k) * (dx/dist);
          fy -= (dist*dist/k) * (dy/dist);
        }
        // Centre gravity
        fx += (W/2 - positions[a].x) * 0.01;
        fy += (H/2 - positions[a].y) * 0.01;
        velocities[a].x = (velocities[a].x + fx) * 0.85;
        velocities[a].y = (velocities[a].y + fy) * 0.85;
        positions[a].x = Math.max(20, Math.min(W-20, positions[a].x + velocities[a].x * 0.1));
        positions[a].y = Math.max(20, Math.min(H-20, positions[a].y + velocities[a].y * 0.1));
      }
    }

    function shortLabel(iri) {
      if (!iri) return '';
      const s = iri.replace(/^<|>$/g, '');
      const h = s.lastIndexOf('#'), sl = s.lastIndexOf('/');
      const cut = Math.max(h, sl);
      return cut >= 0 ? s.slice(cut+1) : s.slice(-20);
    }

    function draw() {
      if (!ctx) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.strokeStyle = '#0f3460';
      ctx.lineWidth = 1;
      for (const e of graph.edges) {
        if (!positions[e.s] || !positions[e.o]) continue;
        ctx.beginPath();
        ctx.moveTo(positions[e.s].x, positions[e.s].y);
        ctx.lineTo(positions[e.o].x, positions[e.o].y);
        ctx.stroke();
      }
      for (const [id, node] of Object.entries(graph.nodes)) {
        const p = positions[id];
        if (!p) continue;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 8, 0, Math.PI*2);
        ctx.fillStyle = id === selectedNode ? '#e94560' : '#4361ee';
        ctx.fill();
        ctx.fillStyle = '#eee';
        ctx.font = '11px sans-serif';
        ctx.fillText(shortLabel(id), p.x+10, p.y+4);
      }
    }

    function onCanvasClick(e) {
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left, my = e.clientY - rect.top;
      for (const [id] of Object.entries(graph.nodes)) {
        const p = positions[id];
        if (!p) continue;
        if ((mx-p.x)*(mx-p.x)+(my-p.y)*(my-p.y) < 100) {
          selectedNode = id;
          showInfo(id);
          draw();
          return;
        }
      }
      selectedNode = null;
      document.getElementById('info-panel').style.display = 'none';
      draw();
    }

    function showInfo(id) {
      const node = graph.nodes[id];
      const panel = document.getElementById('info-panel');
      document.getElementById('info-title').textContent = id.replace(/^<|>$/g,'');
      const tbody = document.getElementById('info-triples');
      tbody.innerHTML = (node.triples||[]).slice(0,20).map(t =>
        '<div class="triple-row"><b>' + shortLabel(t.p) + '</b> → ' + shortLabel(t.o) + '</div>'
      ).join('');
      panel.style.display = 'block';
    }

    async function expandNode() {
      if (!selectedNode) return;
      const iri = selectedNode.replace(/^<|>$/g, '');
      const q = 'CONSTRUCT { <' + iri + '> ?p ?o } WHERE { <' + iri + '> ?p ?o } LIMIT 50';
      document.getElementById('query').value = q;
      document.getElementById('status').textContent = 'Expanding...';
      try {
        const resp = await fetch('/sparql', {
          method: 'POST',
          headers: {'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json'},
          body: 'query=' + encodeURIComponent(q)
        });
        if (!resp.ok) throw new Error(await resp.text());
        const data = await resp.json();
        for (const row of data) {
          const s = row.s && row.s.value || row.s || null;
          const p = row.p && row.p.value || row.p || null;
          const o = row.o && row.o.value || row.o || null;
          if (!s || !p || !o) continue;
          const W = canvas.width, H = canvas.height;
          if (!graph.nodes[s]) { graph.nodes[s] = { id: s, triples: [] }; positions[s] = { x: Math.random()*W, y: Math.random()*H }; velocities[s] = { x: 0, y: 0 }; }
          if (!graph.nodes[o]) { graph.nodes[o] = { id: o, triples: [] }; positions[o] = { x: Math.random()*W, y: Math.random()*H }; velocities[o] = { x: 0, y: 0 }; }
          graph.nodes[s].triples.push({ p, o });
          const exists = graph.edges.some(e => e.s === s && e.p === p && e.o === o);
          if (!exists) graph.edges.push({ s, p, o });
        }
        document.getElementById('status').textContent = graph.edges.length + ' triples, ' + Object.keys(graph.nodes).length + ' nodes';
      } catch(e) {
        document.getElementById('status').textContent = 'Error: ' + e.message;
      }
    }

    window.onload = init;
  </script>
</body>
</html>"#;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
