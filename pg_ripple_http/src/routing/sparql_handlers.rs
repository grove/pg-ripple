//! SPARQL endpoint handlers -- extracted from routing.rs (MOD-01, v0.72.0).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::common::{AppState, check_auth, check_auth_write, json_error, redacted_error};
use crate::spi_bridge::{
    execute_sparql_with_bindings, execute_sparql_with_traceparent,
    execute_sparql_with_traceparent_routed,
};
// Re-use types and constants declared in parent routing module.
use super::{
    CT_CSV, CT_FORM, CT_JSONLD, CT_NTRIPLES, CT_SPARQL_JSON, CT_SPARQL_QUERY, CT_SPARQL_UPDATE,
    CT_SPARQL_XML, CT_TSV, CT_TURTLE, SparqlParams,
};
// Helper functions live in admin_handlers (extracted sibling module).
use super::admin_handlers::{csv_escape, strip_angle, xml_escape};

#[derive(serde::Deserialize)]
pub(crate) struct SparqlBindingsRequest {
    pub(crate) query: String,
    pub(crate) bindings: serde_json::Value,
    #[serde(default)]
    pub(crate) prefix_mode: Option<String>,
    #[serde(default)]
    pub(crate) replica: Option<String>,
    #[serde(default)]
    pub(crate) timeout_ms: Option<u64>,
}

/// POST /sparql/bindings — typed initial bindings for the public SQL overload.
pub(crate) async fn sparql_bindings_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return json_error(
                "PT413",
                "request body too large",
                StatusCode::PAYLOAD_TOO_LARGE,
            );
        }
    };
    let request: SparqlBindingsRequest = match serde_json::from_slice(&body_bytes) {
        Ok(request) => request,
        Err(error) => {
            return json_error(
                "PT400",
                format!("invalid bindings request: {error}"),
                StatusCode::BAD_REQUEST,
            );
        }
    };
    if !request.bindings.is_object() {
        return json_error(
            "PT0570",
            "bindings must be a JSON object",
            StatusCode::BAD_REQUEST,
        );
    }
    if let Some(mode) = request.prefix_mode.as_deref()
        && !matches!(mode, "strict" | "registered")
    {
        return json_error(
            "PT400",
            "prefix_mode must be 'strict' or 'registered'",
            StatusCode::BAD_REQUEST,
        );
    }

    let accept = negotiate_accept(&headers, &request.query);
    let traceparent = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if crate::stream::is_streamable(&request.query, &accept) {
        return crate::stream::stream_sparql_with_bindings(
            &state,
            &request.query,
            &request.bindings,
            &accept,
            traceparent.as_deref(),
            request.replica.as_deref() == Some("ok"),
            request.timeout_ms,
            request.prefix_mode.as_deref(),
        )
        .await;
    }
    execute_sparql_with_bindings(
        &state,
        &request.query,
        &request.bindings,
        &accept,
        traceparent.as_deref(),
        request.replica.as_deref() == Some("ok"),
        request.timeout_ms,
        request.prefix_mode.as_deref(),
    )
    .await
}

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
            // HTTP-ERR-01 (v0.80.0): return JSON error instead of plain text.
            return json_error(
                "PT400",
                "missing 'query' parameter",
                StatusCode::BAD_REQUEST,
            );
        }
    };

    // Feature 12 (v0.120.0): read-replica routing.
    let use_replica = params.replica.as_deref() == Some("ok");

    let accept = negotiate_accept(&headers, &query);
    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    execute_sparql_with_traceparent_routed(
        &state,
        &query,
        false,
        &accept,
        traceparent.as_deref(),
        use_replica,
        params.timeout_ms,
    )
    .await
}

// ─── SPARQL POST handler ─────────────────────────────────────────────────────

pub(crate) async fn sparql_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SparqlParams>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth_write(&state, &headers) {
        return r;
    }

    // Feature 12 (v0.120.0): read-replica routing via query parameter.
    let use_replica = params.replica.as_deref() == Some("ok");

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
        return execute_sparql_with_traceparent_routed(
            &state,
            &body_str,
            false,
            &accept,
            traceparent.as_deref(),
            use_replica,
            params.timeout_ms,
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
        let form_params: SparqlParams = serde_urlencoded::from_str(&body_str).unwrap_or_default();
        // Form replica override takes precedence if not already set.
        let effective_use_replica = use_replica || form_params.replica.as_deref() == Some("ok");
        if let Some(update) = form_params.update {
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
        if let Some(query) = form_params.query {
            let accept = negotiate_accept(&headers, &query);
            return execute_sparql_with_traceparent_routed(
                &state,
                &query,
                false,
                &accept,
                traceparent.as_deref(),
                effective_use_replica,
                form_params.timeout_ms,
            )
            .await;
        }
        // HTTP-ERR-01 (v0.80.0): JSON error response.
        return json_error(
            "PT400",
            "missing 'query' or 'update' parameter in form body",
            StatusCode::BAD_REQUEST,
        );
    }

    json_error(
        "PT415",
        "expected application/sparql-query, application/sparql-update, or application/x-www-form-urlencoded",
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    )
}

// ─── SPARQL /stream compatibility alias (v0.51.0, direct streaming v0.134.0) ─
//
// POST /sparql/stream uses the same direct RowStream pipeline as streaming-safe
// responses from /sparql. The server, not this handler, chooses transfer framing.

pub(crate) async fn sparql_stream_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SparqlParams>,
    body: Body,
) -> Response {
    if let Err(r) = check_auth(&state, &headers) {
        return r;
    }

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return json_error(
                "PT413",
                "request body too large",
                StatusCode::PAYLOAD_TOO_LARGE,
            );
        }
    };
    let body_text = String::from_utf8_lossy(&body_bytes).to_string();
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (query_text, use_replica, timeout_ms) = if content_type.starts_with(CT_FORM) {
        let form_params: SparqlParams = match serde_urlencoded::from_str(&body_text) {
            Ok(params) => params,
            Err(_) => {
                return json_error(
                    "PT400",
                    "invalid application/x-www-form-urlencoded body",
                    StatusCode::BAD_REQUEST,
                );
            }
        };
        let Some(query) = form_params.query else {
            return json_error(
                "PT400",
                "streaming requires a 'query' parameter",
                StatusCode::BAD_REQUEST,
            );
        };
        (
            query,
            params.replica.as_deref() == Some("ok") || form_params.replica.as_deref() == Some("ok"),
            form_params.timeout_ms,
        )
    } else if content_type.starts_with(CT_SPARQL_QUERY) || content_type.is_empty() {
        (
            body_text,
            params.replica.as_deref() == Some("ok"),
            params.timeout_ms,
        )
    } else {
        return json_error(
            "PT415",
            "expected application/sparql-query or application/x-www-form-urlencoded",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        );
    };
    let accept = negotiate_stream_accept(&headers, &query_text);
    let traceparent = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok());
    crate::stream::stream_sparql(
        &state,
        &query_text,
        &accept,
        traceparent,
        use_replica,
        timeout_ms,
    )
    .await
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

    let is_construct =
        crate::stream::is_construct_query(query) || crate::stream::is_describe_query(query);

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

pub(crate) fn negotiate_stream_accept(headers: &HeaderMap, query: &str) -> String {
    let accept = headers
        .get("accept")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    for candidate in accept
        .split(',')
        .map(|value| value.split(';').next().unwrap_or("").trim())
    {
        if matches!(candidate, CT_SPARQL_JSON | CT_CSV | CT_TSV | CT_NTRIPLES) {
            return candidate.to_owned();
        }
    }
    if query_form_is_graph(query) {
        CT_NTRIPLES.to_owned()
    } else {
        CT_SPARQL_JSON.to_owned()
    }
}

fn query_form_is_graph(query: &str) -> bool {
    crate::stream::is_graph_query(query)
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
