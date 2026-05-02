//! Remote SPARQL endpoint HTTP execution (v0.16.0+).
//!
//! Split from `federation.rs` in v0.85.0 (Q13-03).

// Bring in all pub(crate) items from mod.rs (types, constants, imports, and
// re-exported functions from sibling modules).
use super::*;

// Cross-module helpers that are pub(super) in their source modules.
use super::circuit::{
    circuit_is_open, circuit_record_failure, circuit_record_success, get_agent, normalize_http_err,
};
use super::decode::update_federation_stats;
use super::policy::{cache_lookup, cache_store};

/// Execute a SPARQL SELECT query against a remote endpoint.
///
/// Sends an HTTP GET with `query=<sparql_text>` and `Accept:
/// application/sparql-results+json`.  On success returns `(variables, rows)`;
/// each row is a `Vec<Option<String>>` of N-Triples–formatted terms.
///
/// `timeout_secs` is the per-call wall-clock budget.
/// `max_results` caps how many rows are returned; the rest are silently dropped.
///
/// When a cached result is available (v0.19.0), the HTTP call is skipped.
/// When the call fails mid-stream and `allow_partial = true` (v0.19.0),
/// rows received up to the failure point are returned.
pub(crate) fn execute_remote(
    url: &str,
    sparql_text: &str,
    timeout_secs: i32,
    max_results: i32,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), String> {
    type RemoteResult = (Vec<String>, Vec<Vec<Option<String>>>);

    // ── G-3 (v0.56.0): Circuit breaker check ──────────────────────────────────
    if circuit_is_open(url) {
        pgrx::debug1!("federation circuit breaker open for {url}: returning PT605");
        return Err(format!(
            "PT605: federation circuit breaker open for endpoint {url}; try again later"
        ));
    }

    // ── Endpoint policy check (v0.55.0) ───────────────────────────────────────
    if let Err(e) = check_endpoint_policy(url) {
        if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Relaxed) {
            crate::shmem::FED_BLOCKED_COUNT
                .get()
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        return Err(e);
    }

    // v0.55.0 G-4: increment total call counter.
    if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Relaxed) {
        crate::shmem::FED_CALL_COUNT
            .get()
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // ── Cache check (v0.19.0) ─────────────────────────────────────────────────
    if let Some(cached_body) = cache_lookup(url, sparql_text) {
        return parse_sparql_results_json(&cached_body, max_results as usize)
            .map_err(|e| format!("federation cache parse error from {url}: {e}"));
    }

    // ── Connection pool + HTTP call (v0.19.0) ─────────────────────────────────
    let timeout = Duration::from_secs(timeout_secs.max(1) as u64);
    let pool_size = crate::FEDERATION_POOL_SIZE.get().max(1) as usize;
    let agent = get_agent(timeout, pool_size);

    // FED-COST-01 (v0.82.0): measure HTTP call latency for federation stats.
    let call_start = std::time::Instant::now();

    let response = agent
        .get(url)
        .query("query", sparql_text)
        .set("Accept", "application/sparql-results+json")
        .call()
        .map_err(|e| {
            let msg = format!(
                "federation HTTP error calling {url}: {}",
                normalize_http_err(e)
            );
            circuit_record_failure(url);
            msg
        })?;

    // FED-BODY-STREAM-01 (v0.82.0): pre-check Content-Length before buffering body.
    // Reject immediately if Content-Length exceeds federation_max_response_bytes
    // rather than allocating a large buffer and checking after.
    let max_bytes = crate::FEDERATION_MAX_RESPONSE_BYTES.get();
    if max_bytes >= 0
        && let Some(cl_str) = response.header("content-length")
        && let Ok(content_len) = cl_str.parse::<i64>()
        && content_len > max_bytes as i64
    {
        circuit_record_failure(url);
        return Err(format!(
            "PT543: federation response from {url} Content-Length {content_len} \
                         exceeds pg_ripple.federation_max_response_bytes ({max_bytes})"
        ));
    }
    let body = response.into_string().map_err(|e| {
        format!(
            "federation response read error from {url}: {}",
            normalize_http_err(e)
        )
    })?;

    // FED-TRUNC-01 (v0.81.0): post-read truncation check (handles cases where
    // Content-Length was absent or inaccurate).
    let body = if max_bytes >= 0 && body.len() > max_bytes as usize {
        pgrx::warning!(
            "PT543: federation response from {url} is {} bytes, exceeding \
             pg_ripple.federation_max_response_bytes ({}); \
             attempting partial result recovery",
            body.len(),
            max_bytes
        );
        // Truncate and attempt partial parse for complete JSON objects.
        let truncated = &body[..max_bytes as usize];
        let partial = parse_sparql_results_json_partial(truncated, max_results as usize);
        let row_count = partial.1.len();
        pgrx::warning!(
            "PT543: federation {url}: recovered {row_count} complete rows from truncated response"
        );
        return Ok(partial);
    } else {
        body
    };

    let result: Result<RemoteResult, String> =
        parse_sparql_results_json(&body, max_results as usize)
            .map_err(|e| format!("federation result parse error from {url}: {e}"));

    // ── Cache store on success (v0.19.0) ──────────────────────────────────────
    if result.is_ok() {
        cache_store(url, sparql_text, &body);
        // G-3 (v0.56.0): record success to reset circuit breaker failure counter.
        circuit_record_success(url);
        // FED-COST-01b (v0.82.0): update federation_stats with call latency.
        let latency_ms = call_start.elapsed().as_millis() as f64;
        let row_count = result
            .as_ref()
            .map(|(_, rows)| rows.len() as i64)
            .unwrap_or(0);
        update_federation_stats(url, latency_ms, row_count, false);
    } else if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Relaxed) {
        // v0.55.0 G-4: increment error counter on parse failure.
        crate::shmem::FED_ERROR_COUNT
            .get()
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // G-3 (v0.56.0): parse failure counts as a circuit breaker failure.
        circuit_record_failure(url);
        // FED-COST-01b: record error latency.
        let latency_ms = call_start.elapsed().as_millis() as f64;
        update_federation_stats(url, latency_ms, 0, true);
    }

    result
}

/// Execute a SPARQL SELECT query, returning partial results on connection failures.
///
/// When `allow_partial = true`, a connection drop mid-response returns however
/// many rows were parsed rather than an error.  Emits a WARNING naming the
/// endpoint, the row count received, and the error.
pub(crate) fn execute_remote_partial(
    url: &str,
    sparql_text: &str,
    timeout_secs: i32,
    max_results: i32,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), String> {
    // ── Cache check ───────────────────────────────────────────────────────────
    if let Some(cached_body) = cache_lookup(url, sparql_text) {
        return parse_sparql_results_json(&cached_body, max_results as usize)
            .map_err(|e| format!("federation cache parse error: {e}"));
    }

    let timeout = Duration::from_secs(timeout_secs.max(1) as u64);
    let pool_size = crate::FEDERATION_POOL_SIZE.get().max(1) as usize;
    let agent = get_agent(timeout, pool_size);

    let response = match agent
        .get(url)
        .query("query", sparql_text)
        .set("Accept", "application/sparql-results+json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            return Err(format!(
                "federation HTTP error calling {url}: {}",
                normalize_http_err(&e)
            ));
        }
    };

    // FED-BODY-STREAM-01 (v0.82.0): pre-check Content-Length before buffering.
    let fed_max = crate::FEDERATION_MAX_RESPONSE_BYTES.get();
    if fed_max >= 0
        && let Some(cl_str) = response.header("content-length")
        && let Ok(content_len) = cl_str.parse::<i64>()
        && content_len > fed_max as i64
    {
        return Ok((vec![], vec![]));
    }
    // Read body — on truncation, attempt partial parse.
    let body = match response.into_string() {
        Ok(b) => b,
        Err(e) => {
            // Connection dropped while reading body — try best-effort parse on
            // whatever was buffered by ureq before the error.
            pgrx::warning!(
                "SERVICE {url}: connection dropped while reading response ({e}); \
                 attempting partial result recovery"
            );
            // ureq does not expose partial reads; we cannot recover partial JSON here.
            // Return empty with warning.
            return Ok((vec![], vec![]));
        }
    };

    // Attempt full parse first; on failure try partial extraction.
    match parse_sparql_results_json(&body, max_results as usize) {
        Ok(result) => {
            cache_store(url, sparql_text, &body);
            Ok(result)
        }
        Err(_) => {
            // H-13: if the response body is very large, skip partial recovery
            // to avoid the rfind heuristic incorrectly truncating valid JSON.
            let max_partial_bytes = crate::FEDERATION_PARTIAL_RECOVERY_MAX_BYTES.get() as usize;
            if body.len() > max_partial_bytes {
                pgrx::warning!(
                    "SERVICE {url}: partial response too large for recovery ({} bytes > {} limit); returning empty",
                    body.len(),
                    max_partial_bytes
                );
                return Ok((vec![], vec![]));
            }
            // Body may be truncated JSON.  Try to extract partial rows.
            let partial = parse_sparql_results_json_partial(&body, max_results as usize);
            let row_count = partial.1.len();
            pgrx::warning!("SERVICE {url}: result parse error; using {row_count} partial rows");
            Ok(partial)
        }
    }
}

/// Best-effort partial JSON parser for truncated SPARQL results bodies.
///
/// Extracts variable names and as many binding rows as could be parsed before
/// the truncation.  Returns empty sets when headers are missing.
fn parse_sparql_results_json_partial(
    body: &str,
    max_results: usize,
) -> (Vec<String>, Vec<Vec<Option<String>>>) {
    // Try to extract variables from head.vars even if results are truncated.
    let doc: Json = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            // Attempt to fix truncated JSON by scanning for the last complete binding.
            // Find the last closing '}' that is part of a binding.
            // This is a best-effort heuristic for '{"head":{...},"results":{"bindings":[...'.
            if let Some(bracket_pos) = body.rfind("},") {
                let fixed = format!("{}{}", &body[..=bracket_pos], "]}}}");
                match serde_json::from_str(&fixed) {
                    Ok(v) => v,
                    Err(_) => return (vec![], vec![]),
                }
            } else {
                return (vec![], vec![]);
            }
        }
    };

    let vars_arr = doc
        .get("head")
        .and_then(|h| h.get("vars"))
        .and_then(|v| v.as_array());
    let variables: Vec<String> = match vars_arr {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        None => return (vec![], vec![]),
    };

    let bindings_arr = doc
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array());
    let bindings = match bindings_arr {
        Some(arr) => arr,
        None => return (variables, vec![]),
    };

    let mut rows: Vec<Vec<Option<String>>> = Vec::with_capacity(bindings.len().min(max_results));
    for binding in bindings.iter().take(max_results) {
        let mut row: Vec<Option<String>> = Vec::with_capacity(variables.len());
        for var in &variables {
            let term = binding.get(var);
            row.push(term.and_then(sparql_result_term_to_ntriples));
        }
        rows.push(row);
    }
    (variables, rows)
}

/// Parse a `application/sparql-results+json` document.
///
/// Returns `(variables, rows)` where each row is `Vec<Option<String>>` with
/// N-Triples–formatted terms (bound values) or `None` (unbound).
fn parse_sparql_results_json(
    body: &str,
    max_results: usize,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), String> {
    type Rows = Vec<Vec<Option<String>>>;

    let doc: Json = serde_json::from_str(body).map_err(|e| format!("JSON parse error: {e}"))?;

    let vars_arr = doc
        .get("head")
        .and_then(|h| h.get("vars"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing head.vars in SPARQL results JSON".to_string())?;

    let variables: Vec<String> = vars_arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();

    let bindings_arr = doc
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .ok_or_else(|| "missing results.bindings in SPARQL results JSON".to_string())?;

    let mut rows: Rows = Vec::with_capacity(bindings_arr.len().min(max_results));

    for binding in bindings_arr.iter().take(max_results) {
        let mut row: Vec<Option<String>> = Vec::with_capacity(variables.len());
        for var in &variables {
            let term = binding.get(var);
            row.push(term.and_then(sparql_result_term_to_ntriples));
        }
        rows.push(row);
    }

    Ok((variables, rows))
}

/// Convert one SPARQL results JSON term object to an N-Triples–formatted string.
///
/// Handles `uri`, `literal` (with optional `xml:lang` or `datatype`), and
/// `bnode` term types.  Returns `None` for unrecognised or missing data.
fn sparql_result_term_to_ntriples(term: &Json) -> Option<String> {
    let ty = term.get("type")?.as_str()?;
    let value = term.get("value")?.as_str()?;
    match ty {
        "uri" => Some(format!("<{value}>")),
        "bnode" => Some(format!("_:{value}")),
        "literal" => {
            if let Some(lang) = term.get("xml:lang").and_then(|l| l.as_str()) {
                Some(format!(r#""{value}"@{lang}"#))
            } else if let Some(dt) = term.get("datatype").and_then(|d| d.as_str()) {
                // Plain xsd:string is represented as an undecorated literal.
                if dt == "http://www.w3.org/2001/XMLSchema#string" {
                    Some(format!(r#""{value}""#))
                } else {
                    Some(format!(r#""{value}"^^<{dt}>"#))
                }
            } else {
                Some(format!(r#""{value}""#))
            }
        }
        _ => None,
    }
}

// ─── Result encoding ─────────────────────────────────────────────────────────
