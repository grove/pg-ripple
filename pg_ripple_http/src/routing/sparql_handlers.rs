//! SPARQL endpoint handlers -- extracted from routing.rs (MOD-01, v0.72.0).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::common::{AppState, check_auth, redacted_error};
use crate::spi_bridge::execute_sparql_with_traceparent;
// Re-use types and constants declared in parent routing module.
use super::{
    CT_CSV, CT_FORM, CT_JSONLD, CT_NTRIPLES, CT_SPARQL_JSON, CT_SPARQL_QUERY, CT_SPARQL_UPDATE,
    CT_SPARQL_XML, CT_TSV, CT_TURTLE, SparqlParams,
};
// Helper functions live in admin_handlers (extracted sibling module).
use super::admin_handlers::{csv_escape, strip_angle, xml_escape};

// ─── SPARQL GET handler ──────────────────────────────────────────────────────

pub(crate) async fn sparql_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SparqlParams>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let query = match params.query {
        Some(q) => q,
        None => {
            return (StatusCode::BAD_REQUEST, "missing 'query' parameter").into_response();
        }
    };

    let accept = negotiate_accept(&headers, &query);
    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    execute_sparql_with_traceparent(&state, &query, false, &accept, traceparent.as_deref()).await
}

// ─── SPARQL POST handler ─────────────────────────────────────────────────────

pub(crate) async fn sparql_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            // v0.61.0 H7-6: PT404 JSON envelope for body-size rejection.
            return json_response_http(
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({
                    "error": "PT404",
                    "message": "request body exceeds maximum allowed size (10 MiB)"
                }),
            );
        }
    };
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    if content_type.starts_with(CT_SPARQL_QUERY) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql_with_traceparent(
            &state,
            &body_str,
            false,
            &accept,
            traceparent.as_deref(),
        )
        .await;
    }

    if content_type.starts_with(CT_SPARQL_UPDATE) {
        let accept = negotiate_accept(&headers, &body_str);
        return execute_sparql_with_traceparent(
            &state,
            &body_str,
            true,
            &accept,
            traceparent.as_deref(),
        )
        .await;
    }

    if content_type.starts_with(CT_FORM) {
        let params: SparqlParams = serde_urlencoded::from_str(&body_str).unwrap_or_default();
        if let Some(update) = params.update {
            let accept = negotiate_accept(&headers, &update);
            return execute_sparql_with_traceparent(
                &state,
                &update,
                true,
                &accept,
                traceparent.as_deref(),
            )
            .await;
        }
        if let Some(query) = params.query {
            let accept = negotiate_accept(&headers, &query);
            return execute_sparql_with_traceparent(
                &state,
                &query,
                false,
                &accept,
                traceparent.as_deref(),
            )
            .await;
        }
        return (
            StatusCode::BAD_REQUEST,
            "missing 'query' or 'update' parameter in form body",
        )
            .into_response();
    }

    (
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "expected application/sparql-query, application/sparql-update, or application/x-www-form-urlencoded",
    )
        .into_response()
}

// ─── SPARQL /stream handler (v0.51.0) ────────────────────────────────────────
//
// POST /sparql/stream — streams results as chunked transfer-encoded lines.
//
// • SELECT / ASK → JSON-Lines (one JSON binding object per line),
//   Content-Type: application/sparql-results+json
// • CONSTRUCT / DESCRIBE → N-Triples (one triple per line),
//   Content-Type: application/n-triples
//
// This endpoint never buffers the full result set in memory: it fetches rows
// incrementally from PostgreSQL and flushes each row to the client as soon as it
// arrives.  Clients that support chunked transfer encoding (curl, browsers, most
// HTTP clients) will receive results progressively.

pub(crate) async fn sparql_stream_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    use axum::body::Body as AxumBody;
    use tokio_stream::StreamExt as _;
    use tokio_stream::wrappers::ReceiverStream;

    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    let query_text = String::from_utf8_lossy(&body_bytes).to_string();

    let query_lower = query_text.trim().to_lowercase();
    let is_construct = query_lower.starts_with("construct") || query_lower.starts_with("describe");

    let content_type = if is_construct {
        CT_NTRIPLES
    } else {
        CT_SPARQL_JSON
    };

    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            return redacted_error(
                "service_unavailable",
                &format!("pool error: {e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    // Use a channel so we can stream rows as they arrive from PostgreSQL.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::convert::Infallible>>(64);

    tokio::spawn(async move {
        if is_construct {
            // CONSTRUCT / DESCRIBE: stream as N-Triples (one "<s> <p> <o> .\n" per row).
            let rows = client
                .query(
                    "SELECT s, p, o FROM pg_ripple.sparql_construct($1)",
                    &[&query_text],
                )
                .await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        let s: String = row.get(0);
                        let p: String = row.get(1);
                        let o: String = row.get(2);
                        let line = format!("{s} {p} {o} .\n");
                        if tx.send(Ok(line.into_bytes())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("# error: {e}\n");
                    let _ = tx.send(Ok(msg.into_bytes())).await;
                }
            }
        } else {
            // SELECT / ASK: stream as JSON-Lines (one binding JSON object per line).
            let sql = if query_lower.starts_with("ask") {
                "SELECT json_build_object('boolean', pg_ripple.sparql_ask($1))::text"
            } else {
                "SELECT row_to_json(t)::text FROM (SELECT result FROM pg_ripple.sparql($1)) t"
            };
            let rows = client.query(sql, &[&query_text]).await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        let line_str: String = row.get(0);
                        let line = format!("{line_str}\n");
                        if tx.send(Ok(line.into_bytes())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("{{\"error\":\"{}\"}}\n", e.to_string().replace('"', "'"));
                    let _ = tx.send(Ok(msg.into_bytes())).await;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|chunk| chunk.map(axum::body::Bytes::from));

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .header("transfer-encoding", "chunked")
        .body(AxumBody::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ─── Content negotiation ─────────────────────────────────────────────────────

/// Build a JSON response with the given status code (used in main.rs handlers).
pub(crate) fn json_response_http(status: StatusCode, body: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(crate) fn negotiate_accept(headers: &HeaderMap, query: &str) -> String {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let query_lower = query.trim().to_lowercase();
    let is_construct = query_lower.starts_with("construct") || query_lower.starts_with("describe");

    // Explicit accept header takes precedence.
    for candidate in accept
        .split(',')
        .map(|s| s.split(';').next().unwrap_or("").trim())
    {
        match candidate {
            CT_SPARQL_JSON | CT_SPARQL_XML | CT_CSV | CT_TSV | CT_TURTLE | CT_NTRIPLES
            | CT_JSONLD => return candidate.to_owned(),
            _ => {}
        }
    }

    // Default by query type.
    if is_construct {
        CT_TURTLE.to_owned()
    } else {
        CT_SPARQL_JSON.to_owned()
    }
}

// ─── Result formatters ───────────────────────────────────────────────────────

pub(crate) fn format_select_results(results: &[serde_json::Value], accept: &str) -> Response {
    match accept {
        CT_SPARQL_JSON => format_select_json(results),
        CT_SPARQL_XML => format_select_xml(results),
        CT_CSV => format_select_csv(results),
        CT_TSV => format_select_tsv(results),
        _ => format_select_json(results),
    }
}

pub(crate) fn format_select_json(results: &[serde_json::Value]) -> Response {
    // W3C SPARQL Results JSON format.
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let bindings: Vec<serde_json::Value> = results
        .iter()
        .map(|row| {
            let mut binding = serde_json::Map::new();
            if let Some(obj) = row.as_object() {
                for (key, val) in obj {
                    if let Some(s) = val.as_str() {
                        let mut term = serde_json::Map::new();
                        if s.starts_with("http://") || s.starts_with("https://") {
                            term.insert("type".to_owned(), "uri".into());
                            term.insert("value".to_owned(), s.into());
                        } else if s.starts_with("_:") {
                            term.insert("type".to_owned(), "bnode".into());
                            term.insert(
                                "value".to_owned(),
                                s.strip_prefix("_:").unwrap_or(s).into(),
                            );
                        } else {
                            term.insert("type".to_owned(), "literal".into());
                            term.insert("value".to_owned(), s.into());
                        }
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    } else if val.is_number() {
                        let mut term = serde_json::Map::new();
                        term.insert("type".to_owned(), "literal".into());
                        term.insert("value".to_owned(), val.to_string().into());
                        term.insert(
                            "datatype".to_owned(),
                            "http://www.w3.org/2001/XMLSchema#integer".into(),
                        );
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    } else if val.is_boolean() {
                        let mut term = serde_json::Map::new();
                        term.insert("type".to_owned(), "literal".into());
                        term.insert("value".to_owned(), val.to_string().into());
                        term.insert(
                            "datatype".to_owned(),
                            "http://www.w3.org/2001/XMLSchema#boolean".into(),
                        );
                        binding.insert(key.clone(), serde_json::Value::Object(term));
                    }
                }
            }
            serde_json::Value::Object(binding)
        })
        .collect();

    let body = serde_json::json!({
        "head": { "vars": vars },
        "results": { "bindings": bindings }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_SPARQL_JSON)
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

pub(crate) fn format_select_xml(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let mut xml = String::from("<?xml version=\"1.0\"?>\n");
    xml.push_str("<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n");
    xml.push_str("  <head>\n");
    for v in &vars {
        xml.push_str(&format!("    <variable name=\"{v}\"/>\n"));
    }
    xml.push_str("  </head>\n");
    xml.push_str("  <results>\n");

    for row in results {
        xml.push_str("    <result>\n");
        if let Some(obj) = row.as_object() {
            for (key, val) in obj {
                xml.push_str(&format!("      <binding name=\"{key}\">"));
                if let Some(s) = val.as_str() {
                    if s.starts_with("http://") || s.starts_with("https://") {
                        xml.push_str(&format!("<uri>{}</uri>", xml_escape(s)));
                    } else if s.starts_with("_:") {
                        xml.push_str(&format!(
                            "<bnode>{}</bnode>",
                            xml_escape(s.strip_prefix("_:").unwrap_or(s))
                        ));
                    } else {
                        xml.push_str(&format!("<literal>{}</literal>", xml_escape(s)));
                    }
                } else {
                    xml.push_str(&format!("<literal>{}</literal>", val));
                }
                xml.push_str("</binding>\n");
            }
        }
        xml.push_str("    </result>\n");
    }

    xml.push_str("  </results>\n");
    xml.push_str("</sparql>\n");

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_SPARQL_XML)
        .body(Body::from(xml))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

pub(crate) fn format_select_csv(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let mut csv = vars.join(",");
    csv.push('\n');

    for row in results {
        if let Some(obj) = row.as_object() {
            let vals: Vec<String> = vars
                .iter()
                .map(|v| {
                    obj.get(v)
                        .and_then(|val| val.as_str().map(csv_escape))
                        .unwrap_or_default()
                })
                .collect();
            csv.push_str(&vals.join(","));
            csv.push('\n');
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_CSV)
        .body(Body::from(csv))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

pub(crate) fn format_select_tsv(results: &[serde_json::Value]) -> Response {
    let vars: Vec<String> = results
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().map(|k| format!("?{k}")).collect())
        .unwrap_or_default();

    let mut tsv = vars.join("\t");
    tsv.push('\n');

    for row in results {
        if let Some(obj) = row.as_object() {
            let vals: Vec<String> = results
                .first()
                .and_then(|r| r.as_object())
                .map(|first| first.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
                .iter()
                .map(|v| {
                    obj.get(v)
                        .and_then(|val| val.as_str().map(String::from))
                        .unwrap_or_default()
                })
                .collect();
            tsv.push_str(&vals.join("\t"));
            tsv.push('\n');
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", CT_TSV)
        .body(Body::from(tsv))
        .unwrap_or_else(|e| {
            tracing::error!("response build error: {e}");
            redacted_error(
                "internal_server_error",
                &format!("response build failed: {e}"),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
}

pub(crate) fn format_ask_result(result: bool, accept: &str) -> Response {
    match accept {
        CT_SPARQL_XML => {
            let xml = format!(
                "<?xml version=\"1.0\"?>\n\
                 <sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n\
                   <head/>\n\
                   <boolean>{result}</boolean>\n\
                 </sparql>\n"
            );
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_SPARQL_XML)
                .body(Body::from(xml))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
        _ => {
            let body = serde_json::json!({
                "head": {},
                "boolean": result
            });
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_SPARQL_JSON)
                .body(Body::from(body.to_string()))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
    }
}

pub(crate) fn format_graph_results(triples: &[(String, String, String)], accept: &str) -> Response {
    match accept {
        CT_NTRIPLES => {
            let body: String = triples
                .iter()
                .map(|(s, p, o)| format!("{s} {p} {o} .\n"))
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_NTRIPLES)
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
        CT_JSONLD => {
            let graph: Vec<serde_json::Value> = triples
                .iter()
                .map(|(s, p, o)| {
                    serde_json::json!({
                        "@id": strip_angle(s),
                        p.trim_start_matches('<').trim_end_matches('>'): strip_angle(o)
                    })
                })
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_JSONLD)
                .body(Body::from(
                    serde_json::to_string(&graph).unwrap_or_default(),
                ))
                .unwrap_or_else(|e| {
                    tracing::error!("response build error: {e}");
                    redacted_error(
                        "internal_server_error",
                        &format!("response build failed: {e}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )
                })
        }
        _ => {
            // Default: Turtle
            let body: String = triples
                .iter()
                .map(|(s, p, o)| format!("{s} {p} {o} .\n"))
                .collect();
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", CT_TURTLE)
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
    }
}

// ─── RAG endpoint (v0.28.0) ──────────────────────────────────────────────────
