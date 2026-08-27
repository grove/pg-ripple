//! Diagnostic snapshot handler (v0.120.0, split to sub-module v0.122.0).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::common::{AppState, check_auth_admin, redacted_error};
use crate::routing::json_response_http;

// ─── Feature 8 (v0.120.0): Diagnostic snapshot ───────────────────────────────

/// `GET /admin/diagnostic-snapshot`
///
/// Bundles: `_pg_ripple.*` table row counts, all non-sensitive GUC values,
/// extension version, HTTP companion version, and a Prometheus metrics snapshot.
///
/// Requires `check_auth_write` — contains schema introspection data.
pub(crate) async fn diagnostic_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth_admin(&state, &headers) {
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

    // 1. _pg_ripple.* table row counts.
    let table_counts: serde_json::Value = match client
        .query(
            "SELECT relname, reltuples::BIGINT AS estimate \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relkind = 'r' \
             ORDER BY relname",
            &[],
        )
        .await
    {
        Ok(rows) => {
            let map: serde_json::Map<String, serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get(0);
                    let count: i64 = r.get(1);
                    (
                        name,
                        serde_json::Value::Number(serde_json::Number::from(count)),
                    )
                })
                .collect();
            serde_json::Value::Object(map)
        }
        Err(_) => serde_json::json!({}),
    };

    // 2. Non-sensitive GUC values (exclude api_key, password, secret, token).
    let gucs: serde_json::Value = match client
        .query(
            "SELECT name, setting, unit, short_desc \
             FROM pg_settings \
             WHERE name LIKE 'pg_ripple.%' \
               AND name NOT ILIKE '%key%' \
               AND name NOT ILIKE '%password%' \
               AND name NOT ILIKE '%secret%' \
               AND name NOT ILIKE '%token%' \
             ORDER BY name",
            &[],
        )
        .await
    {
        Ok(rows) => {
            let map: serde_json::Map<String, serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get(0);
                    let setting: String = r.get(1);
                    let unit: Option<String> = r.get(2);
                    let desc: String = r.get(3);
                    let val = serde_json::json!({
                        "value": setting,
                        "unit": unit,
                        "description": desc,
                    });
                    (name, val)
                })
                .collect();
            serde_json::Value::Object(map)
        }
        Err(_) => serde_json::json!({}),
    };

    // 3. Extension version.
    let extension_version: Option<String> = client
        .query_opt(
            "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple'",
            &[],
        )
        .await
        .ok()
        .flatten()
        .map(|r| r.get(0));

    // 4. HTTP companion version.
    let http_companion_version = env!("CARGO_PKG_VERSION");

    // 5. Prometheus metrics snapshot (key counters from AppState).
    let m = &state.metrics;
    let metrics_snapshot = serde_json::json!({
        "sparql_queries_total":     m.sparql_query_count(),
        "datalog_queries_total":    m.datalog_query_count(),
        "errors_total":             m.error_count(),
        "select_count":             m.select_count(),
        "ask_count":                m.ask_count(),
        "construct_count":          m.construct_count(),
        "describe_count":           m.describe_count(),
        "update_count":             m.update_count(),
        "pool_size":                state.pool.status().size,
        "sameas_assertions_total":  m.sameas_assertions_total(),
        "temporal_facts_total":     m.temporal_facts_total(),
    });

    let snapshot = serde_json::json!({
        "generated_at":          chrono_now_utc(),
        "extension_version":     extension_version,
        "http_companion_version": http_companion_version,
        "table_row_counts":      table_counts,
        "gucs":                  gucs,
        "metrics_snapshot":      metrics_snapshot,
    });

    json_response_http(StatusCode::OK, snapshot)
}

/// Return the current UTC time as an ISO-8601 string without the `chrono` crate.
fn chrono_now_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as rough ISO-8601 (seconds-precision).
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;
    let ss = time_of_day % 60;
    // Convert days since 1970-01-01 to calendar date.
    let (y, mo, d) = days_to_ymd(days_since_epoch as u32);
    format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn days_to_ymd(days: u32) -> (u32, u32, u32) {
    // Gregorian calendar conversion from days since 1970-01-01.
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
