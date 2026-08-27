//! Rule library HTTP handlers (v0.104.0).
//!
//! `GET /rule-libraries`                    — list installed rule libraries
//! `GET /rule-libraries/{name}/stream`      — Arrow-Flight-style stream (Feature 11, v0.120.0)
//! `POST /rule-libraries/{name}/subscribe`  — subscribe from remote endpoint (Feature 11, v0.120.0)

use crate::common::{AppState, check_auth, check_auth_write, redacted_error};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use std::sync::Arc;
use std::time::Instant;

use super::datalog_handlers::json_response;

// ─── Handler ─────────────────────────────────────────────────────────────────

/// `GET /rule-libraries`
///
/// Returns a JSON array of installed rule library objects.  Each element has:
/// `name`, `version`, `installed_at`, `description`, `license_iri`.
///
/// An empty array is returned when no libraries are installed.
pub async fn list_rule_libraries(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

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

    let rows = match client
        .query(
            "SELECT name, version, \
                 to_char(installed_at, 'YYYY-MM-DD HH24:MI:SS') AS installed_at, \
                 coalesce(description, '') AS description, \
                 coalesce(license_iri, '') AS license_iri \
             FROM _pg_ripple.rule_libraries \
             ORDER BY name",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.metrics.record_error();
            // Table may not exist on older versions — return empty array.
            let msg = e.to_string();
            if msg.contains("does not exist") {
                state.metrics.record_datalog_query(start.elapsed());
                return json_response(StatusCode::OK, serde_json::json!([]));
            }
            return redacted_error("database_error", &msg, StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let libs: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let name: String = row.get(0);
            let version: String = row.get(1);
            let installed_at: String = row.get(2);
            let description: String = row.get(3);
            let license_iri: String = row.get(4);
            serde_json::json!({
                "name": name,
                "version": version,
                "installed_at": installed_at,
                "description": description,
                "license_iri": license_iri,
            })
        })
        .collect();

    state.metrics.record_datalog_query(start.elapsed());
    json_response(StatusCode::OK, serde_json::Value::Array(libs))
}

// ─── Feature 11 (v0.120.0): Rule-Library Federation ──────────────────────────

/// `GET /rule-libraries/{name}/stream`
///
/// Stream a rule library's rules as a newline-delimited JSON stream.
/// Each line is a JSON object with `rule_set` and `body` fields.
///
/// Uses HMAC authentication when `ARROW_FLIGHT_SECRET` is configured —
/// reuses the existing Arrow Flight auth mechanism.
pub async fn stream_rule_library(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let start = Instant::now();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_library_name"}),
        );
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

    // Verify the library is published.
    let published = match client
        .query_opt(
            "SELECT endpoint_uri FROM _pg_ripple.rule_library_federation WHERE name = $1",
            &[&name],
        )
        .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => {
            // Federation table may not exist yet.
            if e.to_string().contains("does not exist") {
                false
            } else {
                return redacted_error(
                    "federation_query_error",
                    &e.to_string(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        }
    };

    if !published {
        return json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({
                "error": "not_published",
                "detail": format!("library '{name}' is not published as a federation endpoint")
            }),
        );
    }

    // Fetch the rules for this library.
    let rows = match client
        .query(
            "SELECT body FROM _pg_ripple.rules WHERE rule_set = $1 ORDER BY id",
            &[&name],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "rules_query_error",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    // Build newline-delimited JSON stream body.
    let mut ndjson = String::new();
    for row in &rows {
        let body: String = row.get(0);
        let line = serde_json::json!({ "rule_set": name, "body": body });
        ndjson.push_str(&line.to_string());
        ndjson.push('\n');
    }

    // SEC-L-01 (v0.121.0): enforce explicit Content-Type with charset on the stream response.
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream; charset=utf-8")
        .header("x-rule-library-name", name)
        .body(Body::from(ndjson))
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "response_build_error"}),
            )
        });

    // OBS-M-02 (v0.123.0): record stream latency histogram.
    state
        .metrics
        .record_rule_library_stream_duration(start.elapsed());
    response
}

/// `POST /rule-libraries/{name}/subscribe`
///
/// Fetch a rule library from a remote Arrow Flight stream endpoint and
/// install it locally. The remote endpoint URL must be provided in the
/// request body as `{"source_uri": "https://..."}`.
pub async fn subscribe_rule_library(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }

    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "invalid_library_name"}),
        );
    }

    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "read_error"}),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct SubscribeBody {
        source_uri: String,
    }

    let req: SubscribeBody = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_json", "detail": format!("{e}")}),
            );
        }
    };

    if req.source_uri.is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "missing_source_uri"}),
        );
    }

    // Fetch the rules from the remote endpoint.
    let remote_body = match crate::outbound_policy::fetch_text(&req.source_uri).await {
        Ok(body) => body,
        Err(error) => {
            // OBS-M-02 (v0.123.0): count subscribe errors.
            state.metrics.record_rule_library_subscribe_error();
            return redacted_error("remote_fetch_error", &error, StatusCode::BAD_GATEWAY);
        }
    };

    // Parse the NDJSON stream and collect rules.
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

    let mut installed_count = 0u32;
    for line in remote_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rule_body = obj
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        if rule_body.is_empty() {
            continue;
        }
        // Insert rule into local store.
        let _ = client
            .execute(
                "INSERT INTO _pg_ripple.rules (rule_set, body) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
                &[&name, &rule_body],
            )
            .await;
        installed_count += 1;
    }

    // Record subscription in federation catalog.
    let _ = client
        .execute(
            "CREATE TABLE IF NOT EXISTS _pg_ripple.rule_library_federation ( \
               name TEXT PRIMARY KEY, endpoint_uri TEXT NOT NULL, \
               published_at TIMESTAMPTZ NOT NULL DEFAULT now() \
             )",
            &[],
        )
        .await;
    let _ = client
        .execute(
            "INSERT INTO _pg_ripple.rule_library_federation (name, endpoint_uri) \
             VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET \
             endpoint_uri = EXCLUDED.endpoint_uri, published_at = now()",
            &[&name, &req.source_uri],
        )
        .await;

    json_response(
        StatusCode::OK,
        serde_json::json!({
            "status": "subscribed",
            "name": name,
            "source_uri": req.source_uri,
            "rules_installed": installed_count,
        }),
    )
}
