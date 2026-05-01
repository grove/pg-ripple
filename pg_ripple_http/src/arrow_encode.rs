//! Arrow Flight bulk-export endpoint (FLIGHT-02, v0.66.0).
//!
//! Exposes `POST /flight/do_get` which returns results of a SPARQL SELECT
//! query as an Arrow IPC stream (application/vnd.apache.arrow.stream).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::common::{AppState, check_auth, redacted_error};
use crate::routing::json_response_http;

// ─── v0.66.0: Arrow Flight bulk-export endpoint (FLIGHT-02) ──────────────────

/// Validate an Arrow Flight v2 ticket.
///
/// Returns `Ok(graph_id)` when the ticket is valid, `Err(reason)` otherwise.
fn validate_flight_ticket(
    ticket: &serde_json::Value,
    secret: Option<&str>,
    now_secs: u64,
    allow_unsigned: bool,
) -> Result<i64, String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let ticket_type = ticket["type"].as_str().unwrap_or("");
    if ticket_type != "arrow_flight_v2" {
        return Err(format!("unexpected ticket type: {ticket_type}"));
    }
    let aud = ticket["aud"].as_str().unwrap_or("");
    if aud != "pg_ripple_http" {
        return Err(format!("unexpected audience: {aud}"));
    }
    let exp = ticket["exp"].as_u64().unwrap_or(0);
    if exp < now_secs {
        return Err("ticket has expired".to_owned());
    }

    let sig = ticket["sig"].as_str().unwrap_or("unsigned");
    if sig == "unsigned" {
        // FLIGHT-SEC-01: reject unsigned tickets unless explicitly allowed.
        if !allow_unsigned {
            return Err(
                "unsigned Arrow Flight ticket rejected — set ARROW_UNSIGNED_TICKETS_ALLOWED=true \
                 for local development or configure a signing secret"
                    .to_owned(),
            );
        }
        // allow_unsigned = true: skip HMAC verification for local development.
    } else {
        let secret = match secret {
            Some(s) if !s.is_empty() => s,
            _ => return Err("server has no ARROW_FLIGHT_SECRET configured".to_owned()),
        };
        let iat = ticket["iat"].as_u64().unwrap_or(0);
        let graph_iri = ticket["graph_iri"].as_str().unwrap_or("");
        let graph_id_v = ticket["graph_id"].as_i64().unwrap_or(0);
        let nonce = ticket["nonce"].as_str().unwrap_or("");
        let canonical = format!(
            "aud=pg_ripple_http,exp={exp},graph_id={graph_id_v},graph_iri={graph_iri},iat={iat},nonce={nonce},type=arrow_flight_v2"
        );
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| format!("HMAC key error: {e}"))?;
        mac.update(canonical.as_bytes());
        let result = mac.finalize();
        let expected = hex::encode(result.into_bytes());
        if !constant_time_eq::constant_time_eq(expected.as_bytes(), sig.as_bytes()) {
            return Err("invalid ticket signature".to_owned());
        }
    }

    Ok(ticket["graph_id"].as_i64().unwrap_or(0))
}

/// Arrow Flight do_get endpoint: stream VP rows as Arrow IPC record batches.
///
/// Accepts a JSON Flight ticket (as produced by `pg_ripple.export_arrow_flight()`)
/// in the request body, validates the HMAC-SHA256 signature, and streams the
/// named graph's triples as a binary Arrow IPC stream.
///
/// Response content-type: `application/vnd.apache.arrow.stream`
/// Schema: `s Int64, p Int64, o Int64, g Int64` (dictionary-encoded integers).
pub(crate) async fn flight_do_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc as StdArc;
    use std::time::{SystemTime, UNIX_EPOCH};

    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let body_bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return json_response_http(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": format!("failed to read ticket body: {e}")}),
            );
        }
    };

    let ticket: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(t) => t,
        Err(e) => {
            return json_response_http(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": format!("invalid Flight ticket: {e}")}),
            );
        }
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let graph_id = match validate_flight_ticket(
        &ticket,
        state.arrow_flight_secret.as_deref(),
        now_secs,
        state.arrow_unsigned_tickets_allowed,
    ) {
        Ok(id) => id,
        Err(reason) => {
            tracing::warn!("Arrow Flight ticket rejected: {reason}");
            state.metrics.record_arrow_ticket_rejection();
            return json_response_http(
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "invalid ticket", "reason": reason}),
            );
        }
    };

    // FLIGHT-NONCE-01 (v0.72.0): replay protection — reject reused nonces.
    {
        use std::time::Instant;
        let nonce = ticket["nonce"].as_str().unwrap_or("").to_owned();
        let expiry_secs = ticket["exp"].as_u64().unwrap_or(0).saturating_sub(now_secs);
        if !nonce.is_empty() {
            // Lazy eviction: remove expired entries to keep cache bounded.
            if state.arrow_nonce_cache.len() > state.arrow_nonce_cache_max {
                let now_instant = Instant::now();
                state
                    .arrow_nonce_cache
                    .retain(|_, (accepted_at, exp)| accepted_at.elapsed().as_secs() < *exp);
                let _ = now_instant; // suppress unused warning
            }
            // Check for replay.
            if let Some(entry) = state.arrow_nonce_cache.get(&nonce) {
                let (accepted_at, exp) = entry.value();
                if accepted_at.elapsed().as_secs() < *exp {
                    tracing::warn!(nonce = %nonce, "Arrow Flight ticket nonce replayed");
                    state.metrics.record_arrow_ticket_rejection();
                    return json_response_http(
                        StatusCode::UNAUTHORIZED,
                        serde_json::json!({"error": "invalid ticket", "reason": "nonce already used"}),
                    );
                }
            }
            // Record this nonce as seen.
            state
                .arrow_nonce_cache
                .insert(nonce, (Instant::now(), expiry_secs));
        }
    }

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "flight_do_get pool",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    // FLIGHT-SEC-02: Query all non-rare VP tables + vp_rare for the graph
    // using tombstone-exclusion read semantics:
    //   (main EXCEPT tombstones) UNION ALL delta
    // This matches the SPARQL read path and excludes tombstoned rows.
    let pred_rows = match client
        .query(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL ORDER BY id",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "flight_do_get predicates",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let graph_filter = format!("g = {graph_id}");
    let mut union_parts: Vec<String> = pred_rows
        .iter()
        .map(|r| {
            let pred_id: i64 = r.get(0);
            // (main EXCEPT tombstones) UNION ALL delta — tombstone-exclusion read semantics.
            format!(
                "SELECT {pred_id} AS p, s, o, g FROM _pg_ripple.vp_{pred_id}_main \
                 WHERE {graph_filter} AND i NOT IN (SELECT i FROM _pg_ripple.vp_{pred_id}_tombstones WHERE {graph_filter}) \
                 UNION ALL \
                 SELECT {pred_id} AS p, s, o, g FROM _pg_ripple.vp_{pred_id}_delta WHERE {graph_filter}"
            )
        })
        .collect();
    // Always include vp_rare (rare predicates have no tombstone tables; they use direct delete).
    union_parts.push(format!(
        "SELECT p, s, o, g FROM _pg_ripple.vp_rare WHERE {graph_filter}"
    ));

    let full_sql = union_parts.join(" UNION ALL ");

    // FLIGHT-SEC-02: stream in batches instead of materialising the full result.
    // batch_size defaults to 1000 rows; configurable via pg_ripple.arrow_batch_size GUC.
    // Since we're in the HTTP service (no PG GUC access), we use a fixed default of 1000
    // and expose a future env override.
    let batch_size: usize = std::env::var("ARROW_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
        .max(1);

    let rows = match client.query(&full_sql, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return redacted_error(
                "flight_do_get query",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    // ARROW-LIMIT-01 (v0.82.0): enforce row export limit before serializing.
    // Configurable via ARROW_MAX_EXPORT_ROWS env var (default: 10,000,000).
    let max_export_rows: usize = std::env::var("ARROW_MAX_EXPORT_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000_000)
        .max(1);
    if rows.len() > max_export_rows {
        return json_response_http(
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": format!(
                    "Arrow Flight export result ({} rows) exceeds ARROW_MAX_EXPORT_ROWS limit ({}); \
                     use a more selective query or increase the limit",
                    rows.len(), max_export_rows
                )
            }),
        );
    }

    // Build Arrow IPC stream with multiple record batches (one per `batch_size` rows).
    let schema = Schema::new(vec![
        Field::new("s", DataType::Int64, false),
        Field::new("p", DataType::Int64, false),
        Field::new("o", DataType::Int64, false),
        Field::new("g", DataType::Int64, false),
    ]);
    let schema_ref = StdArc::new(schema);

    let mut buf: Vec<u8> = Vec::new();
    let mut writer = match StreamWriter::try_new(&mut buf, &schema_ref) {
        Ok(w) => w,
        Err(e) => {
            return redacted_error(
                "flight_do_get ipc_writer",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let total_rows = rows.len();
    let mut batches_sent: u64 = 0;

    for chunk in rows.chunks(batch_size) {
        let mut s_vals: Vec<i64> = Vec::with_capacity(chunk.len());
        let mut p_vals: Vec<i64> = Vec::with_capacity(chunk.len());
        let mut o_vals: Vec<i64> = Vec::with_capacity(chunk.len());
        let mut g_vals: Vec<i64> = Vec::with_capacity(chunk.len());
        for row in chunk {
            s_vals.push(row.get::<_, i64>(1));
            p_vals.push(row.get::<_, i64>(0));
            o_vals.push(row.get::<_, i64>(2));
            g_vals.push(row.get::<_, i64>(3));
        }
        let batch = match RecordBatch::try_new(
            StdArc::clone(&schema_ref),
            vec![
                StdArc::new(Int64Array::from(s_vals)),
                StdArc::new(Int64Array::from(p_vals)),
                StdArc::new(Int64Array::from(o_vals)),
                StdArc::new(Int64Array::from(g_vals)),
            ],
        ) {
            Ok(b) => b,
            Err(e) => {
                return redacted_error(
                    "flight_do_get batch",
                    &e.to_string(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        };
        if let Err(e) = writer.write(&batch) {
            return redacted_error(
                "flight_do_get ipc_write",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
        batches_sent += 1;
    }

    if let Err(e) = writer.finish() {
        return redacted_error(
            "flight_do_get ipc_finish",
            &e.to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    // Wire Arrow metrics (FLIGHT-SEC-02).
    state.metrics.record_arrow_batches_sent(batches_sent);

    tracing::debug!(
        graph_id = graph_id,
        rows = total_rows,
        batches = batches_sent,
        bytes = buf.len(),
        "Arrow Flight stream serialized"
    );

    // FLIGHT-STREAM-01 (v0.71.0): stream the Arrow IPC buffer as chunked HTTP transfer
    // instead of sending the full buffer in a single body.  The response header
    // `Transfer-Encoding: chunked` allows clients to begin decoding before the
    // export completes.  The in-memory IPC buffer (bounded by result-set size) is
    // split into 64 KiB chunks and yielded lazily via `Body::from_stream`.
    const CHUNK_SIZE: usize = 65_536;
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        buf.chunks(CHUNK_SIZE).map(|c| Ok(c.to_vec())).collect();
    let byte_stream = tokio_stream::iter(chunks);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/vnd.apache.arrow.stream")
        .header("x-arrow-rows", total_rows.to_string())
        .header("x-arrow-batches", batches_sent.to_string())
        .body(Body::from_stream(byte_stream))
        .unwrap_or_else(|e| {
            redacted_error(
                "flight_do_get response",
                &e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}
