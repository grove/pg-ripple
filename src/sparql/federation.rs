//! SPARQL federation: SERVICE clause HTTP executor.
//!
//! Fetches results from remote SPARQL endpoints (SPARQL 1.1 Protocol,
//! `application/sparql-results+json`) and returns dictionary-encoded row sets
//! that the SQL generator can inject as `VALUES` clauses.
//!
//! # Security
//!
//! Only endpoints explicitly registered in `_pg_ripple.federation_endpoints`
//! are contacted.  Any attempt to call an unregistered URL is rejected with
//! an ERROR (or silently skipped for `SERVICE SILENT`) — this prevents SSRF.
//!
//! # Connection pooling (v0.19.0)
//!
//! A thread-local `ureq::Agent` is shared across all SERVICE calls within a
//! session.  The agent reuses TCP connections and TLS sessions, controlled by
//! `pg_ripple.federation_pool_size` (default: 4 per host).
//!
//! # Result caching (v0.19.0)
//!
//! When `pg_ripple.federation_cache_ttl > 0`, remote results are stored in
//! `_pg_ripple.federation_cache` keyed on `(url, XXH3-128(sparql_text))`.
//! Cache hits skip the HTTP call entirely.  Expired rows are cleaned up by
//! the merge background worker.
//!
//! # Parallelism
//!
//! Within a PostgreSQL SPI context, true parallel HTTP is not feasible.
//! Multiple SERVICE clauses in one query are executed sequentially.
//! (The pg_ripple_http sidecar can exploit async parallelism for the HTTP
//! endpoint path, but the in-process SPI path uses sequential fallback.)

#![allow(clippy::type_complexity)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde_json::Value as Json;
use spargebra::algebra::GraphPattern;
use spargebra::term::NamedNodePattern;

use crate::dictionary;

// ─── Thread-local connection pool (v0.19.0) ──────────────────────────────────

thread_local! {
    /// Shared HTTP agent for the current PostgreSQL backend.
    /// Created lazily on first use; reuses TCP/TLS connections across calls.
    static SHARED_AGENT: RefCell<Option<ureq::Agent>> = const { RefCell::new(None) };
}

/// Return the per-thread shared ureq agent, creating it on first call.
///
/// If the `pool_size` has changed since the agent was created the agent is
/// recreated (this is rare — pool_size is a session GUC).
fn get_agent(timeout: Duration, pool_size: usize) -> ureq::Agent {
    SHARED_AGENT.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(
                ureq::AgentBuilder::new()
                    .timeout(timeout)
                    .max_idle_connections_per_host(pool_size)
                    .build(),
            );
        }
        // SAFETY: we just ensured opt.is_some()
        opt.as_ref().unwrap().clone()
    })
}

// ─── Allowlist check ─────────────────────────────────────────────────────────

/// Returns `true` when `url` is registered in `_pg_ripple.federation_endpoints`
/// with `enabled = true`.
pub(crate) fn is_endpoint_allowed(url: &str) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM _pg_ripple.federation_endpoints
            WHERE url = $1 AND enabled = true
         )",
        &[DatumWithOid::from(url)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Returns the `local_view_name` for an endpoint if set and not NULL.
///
/// When non-NULL, the SERVICE clause should be rewritten to scan the local
/// pre-materialised stream table instead of making an HTTP call.
pub(crate) fn get_local_view(url: &str) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT local_view_name FROM _pg_ripple.federation_endpoints
          WHERE url = $1 AND enabled = true AND local_view_name IS NOT NULL",
        &[DatumWithOid::from(url)],
    )
    .ok()
    .flatten()
}

// ─── Adaptive timeout (v0.19.0) ──────────────────────────────────────────────

/// Derive the effective timeout for a given endpoint.
///
/// When `pg_ripple.federation_adaptive_timeout = on`, reads the P95 latency
/// from `_pg_ripple.federation_health` and uses `max(1s, p95_ms * 3 / 1000)`.
/// Falls back to `pg_ripple.federation_timeout` when adaptive mode is off or
/// no health data is available.
pub(crate) fn effective_timeout_secs(url: &str) -> i32 {
    let base = crate::FEDERATION_TIMEOUT.get();
    let adaptive = crate::FEDERATION_ADAPTIVE_TIMEOUT.get();
    if !adaptive || !has_health_table() {
        return base;
    }
    // Approximate P95 from the last 100 successful probes (ORDER BY latency_ms DESC OFFSET 95%).
    let p95 = Spi::get_one_with_args::<i64>(
        "SELECT PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_ms)::bigint
         FROM (
             SELECT latency_ms FROM _pg_ripple.federation_health
             WHERE url = $1 AND success = true
             ORDER BY probed_at DESC
             LIMIT 100
         ) sub",
        &[DatumWithOid::from(url)],
    )
    .ok()
    .flatten();
    match p95 {
        Some(ms) if ms > 0 => {
            let derived = ((ms * 3) / 1000).max(1) as i32;
            derived.min(3600)
        }
        _ => base,
    }
}

// ─── Result cache (v0.19.0) ──────────────────────────────────────────────────

/// Check the federation result cache.
///
/// Returns cached JSON body string on a hit, or `None` on a miss / disabled cache.
fn cache_lookup(url: &str, sparql_text: &str) -> Option<String> {
    let ttl = crate::FEDERATION_CACHE_TTL.get();
    if ttl == 0 {
        return None;
    }
    // XXH3-128 of the SPARQL text as a 32-char hex fingerprint key.
    // Using 128-bit avoids birthday-bound collisions even at very high query volumes.
    let hash = {
        use xxhash_rust::xxh3::xxh3_128;
        format!("{:032x}", xxh3_128(sparql_text.as_bytes()))
    };
    Spi::get_one_with_args::<String>(
        "SELECT result_jsonb::text
         FROM _pg_ripple.federation_cache
         WHERE url = $1 AND query_hash = $2 AND expires_at > now()",
        &[DatumWithOid::from(url), DatumWithOid::from(hash.as_str())],
    )
    .ok()
    .flatten()
}

/// Store results in the federation result cache.
fn cache_store(url: &str, sparql_text: &str, body: &str) {
    let ttl = crate::FEDERATION_CACHE_TTL.get();
    if ttl == 0 {
        return;
    }
    let hash = {
        use xxhash_rust::xxh3::xxh3_128;
        format!("{:032x}", xxh3_128(sparql_text.as_bytes()))
    };
    // Validate that the body is valid JSON before storing.
    if serde_json::from_str::<Json>(body).is_err() {
        return;
    }
    let ttl_str = format!("{ttl} seconds");
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.federation_cache (url, query_hash, result_jsonb, expires_at)
         VALUES ($1, $2, $3::jsonb, now() + $4::interval)
         ON CONFLICT (url, query_hash) DO UPDATE
           SET result_jsonb = EXCLUDED.result_jsonb,
               cached_at    = now(),
               expires_at   = EXCLUDED.expires_at",
        &[
            DatumWithOid::from(url),
            DatumWithOid::from(hash.as_str()),
            DatumWithOid::from(body),
            DatumWithOid::from(ttl_str.as_str()),
        ],
    );
}

// ─── Remote HTTP execution ───────────────────────────────────────────────────

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

    // ── Cache check (v0.19.0) ─────────────────────────────────────────────────
    if let Some(cached_body) = cache_lookup(url, sparql_text) {
        return parse_sparql_results_json(&cached_body, max_results as usize)
            .map_err(|e| format!("federation cache parse error from {url}: {e}"));
    }

    // ── Connection pool + HTTP call (v0.19.0) ─────────────────────────────────
    let timeout = Duration::from_secs(timeout_secs.max(1) as u64);
    let pool_size = crate::FEDERATION_POOL_SIZE.get().max(1) as usize;
    let agent = get_agent(timeout, pool_size);

    let response = agent
        .get(url)
        .query("query", sparql_text)
        .set("Accept", "application/sparql-results+json")
        .call()
        .map_err(|e| format!("federation HTTP error calling {url}: {e}"))?;

    let body = response
        .into_string()
        .map_err(|e| format!("federation response read error from {url}: {e}"))?;

    let result: Result<RemoteResult, String> =
        parse_sparql_results_json(&body, max_results as usize)
            .map_err(|e| format!("federation result parse error from {url}: {e}"));

    // ── Cache store on success (v0.19.0) ──────────────────────────────────────
    if result.is_ok() {
        cache_store(url, sparql_text, &body);
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
        Err(e) => return Err(format!("federation HTTP error calling {url}: {e}")),
    };

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

/// Encode remote query results into dictionary IDs.
///
/// Each N-Triples–formatted term string is encoded via the dictionary so that
/// the resulting IDs are join-compatible with locally stored triples.
///
/// Returns `(variables, encoded_rows)`.
///
/// # Deduplication (v0.19.0)
///
/// A per-call `HashMap<String, i64>` avoids redundant dictionary lookups for
/// the same term appearing in multiple rows.  Particularly effective for result
/// sets with high-cardinality repeated values (e.g. a common subject IRI).
pub(crate) fn encode_results(
    variables: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> (Vec<String>, Vec<Vec<Option<i64>>>) {
    // Per-call deduplication cache (v0.19.0).
    let mut term_cache: HashMap<String, i64> = HashMap::new();

    let encoded: Vec<Vec<Option<i64>>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| {
                    cell.map(|s| {
                        if let Some(&id) = term_cache.get(&s) {
                            id
                        } else {
                            let id = encode_ntriples_term(&s);
                            term_cache.insert(s, id);
                            id
                        }
                    })
                })
                .collect()
        })
        .collect();
    (variables, encoded)
}

/// Encode a single N-Triples–formatted term to a dictionary ID.
///
/// Handles IRIs (`<…>`), blank nodes (`_:…`), plain literals (`"…"`),
/// language-tagged literals (`"…"@lang`), and typed literals (`"…"^^<dt>`).
fn encode_ntriples_term(term: &str) -> i64 {
    if let Some(iri) = term.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        dictionary::encode(iri, dictionary::KIND_IRI)
    } else if let Some(bnode) = term.strip_prefix("_:") {
        dictionary::encode(bnode, dictionary::KIND_BLANK)
    } else if term.starts_with('"') {
        // Literal — may have lang tag or datatype.
        if let Some((lit_body, lang)) = split_lang_literal(term) {
            dictionary::encode_lang_literal(lit_body, lang)
        } else if let Some((lit_body, dt_iri)) = split_typed_literal(term) {
            dictionary::encode_typed_literal(lit_body, dt_iri)
        } else {
            // Plain string literal — strip outer quotes.
            let plain = term
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(term);
            dictionary::encode(plain, dictionary::KIND_LITERAL)
        }
    } else {
        // Unrecognised format — encode as-is as a plain literal.
        dictionary::encode(term, dictionary::KIND_LITERAL)
    }
}

/// Split `"value"@lang` into `("value", "lang")`.
fn split_lang_literal(term: &str) -> Option<(&str, &str)> {
    // term looks like: "value"@lang
    let at = term.rfind("\"@")?;
    let lit = &term[1..at]; // strip leading '"' and trailing '"@...'
    let lang = &term[at + 2..];
    if lang.is_empty() {
        None
    } else {
        Some((lit, lang))
    }
}

/// Split `"value"^^<dt>` into `("value", "dt_iri")`.
fn split_typed_literal(term: &str) -> Option<(&str, &str)> {
    // term looks like: "value"^^<dt>
    let hat = term.rfind("\"^^<")?;
    let lit = &term[1..hat];
    let rest = &term[hat + 4..]; // skip '^^<'
    let dt = rest.strip_suffix('>')?;
    if dt.is_empty() { None } else { Some((lit, dt)) }
}

// ─── Health monitoring ───────────────────────────────────────────────────────

/// Record a probe outcome in `_pg_ripple.federation_health`.
///
/// No-op when the table doesn't exist (pg_trickle not installed or
/// `enable_federation_health()` not yet called).
pub(crate) fn record_health(url: &str, success: bool, latency_ms: i64) {
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.federation_health (url, success, latency_ms, probed_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT DO NOTHING",
        &[
            DatumWithOid::from(url),
            DatumWithOid::from(success),
            DatumWithOid::from(latency_ms),
        ],
    );
}

/// Returns `true` when the federation_health table exists.
pub(crate) fn has_health_table() -> bool {
    Spi::get_one::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = '_pg_ripple' AND c.relname = 'federation_health'
         )",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Returns `true` when the endpoint's recent success rate is above 10%.
/// Always returns `true` when the health table doesn't exist.
pub(crate) fn is_endpoint_healthy(url: &str) -> bool {
    if !has_health_table() {
        return true;
    }
    // Look at last 5 minutes; if success rate < 10%, skip.
    let rate = Spi::get_one_with_args::<f64>(
        "SELECT COALESCE(
            AVG(CASE WHEN success THEN 1.0 ELSE 0.0 END),
            1.0  -- assume healthy if no data
         ) AS success_rate
         FROM _pg_ripple.federation_health
         WHERE url = $1
           AND probed_at >= now() - INTERVAL '5 minutes'",
        &[DatumWithOid::from(url)],
    )
    .unwrap_or(None)
    .unwrap_or(1.0);

    rate >= 0.1
}

// ─── Endpoint complexity hints (v0.19.0) ─────────────────────────────────────

/// Returns the complexity hint for an endpoint: `"fast"`, `"normal"`, or `"slow"`.
///
/// Falls back to `"normal"` when the column doesn't exist (pre-migration DB)
/// or the endpoint is not registered.
#[allow(dead_code)]
pub(crate) fn get_endpoint_complexity(url: &str) -> String {
    Spi::get_one_with_args::<String>(
        "SELECT complexity FROM _pg_ripple.federation_endpoints
          WHERE url = $1 AND enabled = true",
        &[DatumWithOid::from(url)],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| "normal".to_owned())
}

// ─── Cache maintenance (v0.19.0) ─────────────────────────────────────────────

/// Remove expired rows from `_pg_ripple.federation_cache`.
///
/// Called by the merge background worker on each polling cycle.
pub(crate) fn evict_expired_cache() {
    let _ = Spi::run("DELETE FROM _pg_ripple.federation_cache WHERE expires_at <= now()");
}

// ─── Local view variable discovery ───────────────────────────────────────────

/// Retrieve the variable names exposed by a local SPARQL view stream table.
///
/// Returns an ordered list of variable names (without the `_v_` prefix) as
/// they appear in `_pg_ripple.sparql_views.variables` for the given stream
/// table name.
pub(crate) fn get_view_variables(stream_table: &str) -> Vec<String> {
    // variables is stored as a JSONB array of strings, e.g. '["s","p","o"]'.
    let json_str = Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT variables FROM _pg_ripple.sparql_views WHERE stream_table = $1",
        &[DatumWithOid::from(stream_table)],
    )
    .ok()
    .flatten();

    match json_str {
        Some(jb) => {
            if let serde_json::Value::Array(arr) = jb.0 {
                arr.into_iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            } else {
                vec![]
            }
        }
        None => vec![],
    }
}

// ─── Variable collection for query rewriting (v0.19.0) ───────────────────────

/// Collect all variable names that appear in a `GraphPattern`.
///
/// Used by the SERVICE translator to build an explicit `SELECT ?v1 ?v2 …`
/// instead of `SELECT *`, which enables endpoints to project only the needed
/// columns and reduces data transfer when combined with caller context.
pub(crate) fn collect_pattern_variables(pattern: &GraphPattern) -> HashSet<String> {
    let mut vars = HashSet::new();
    collect_vars_recursive(pattern, &mut vars);
    vars
}

fn collect_vars_recursive(pattern: &GraphPattern, out: &mut HashSet<String>) {
    use spargebra::algebra::GraphPattern::*;
    use spargebra::term::TermPattern;
    match pattern {
        Bgp { patterns } => {
            for tp in patterns {
                if let TermPattern::Variable(v) = &tp.subject {
                    out.insert(v.as_str().to_owned());
                }
                if let NamedNodePattern::Variable(v) = &tp.predicate {
                    out.insert(v.as_str().to_owned());
                }
                if let TermPattern::Variable(v) = &tp.object {
                    out.insert(v.as_str().to_owned());
                }
            }
        }
        Join { left, right }
        | LeftJoin { left, right, .. }
        | Union { left, right }
        | Minus { left, right } => {
            collect_vars_recursive(left, out);
            collect_vars_recursive(right, out);
        }
        Filter { inner, .. }
        | Graph { inner, .. }
        | Extend { inner, .. }
        | Distinct { inner }
        | Reduced { inner }
        | Slice { inner, .. }
        | OrderBy { inner, .. } => {
            collect_vars_recursive(inner, out);
        }
        Project { variables, inner } => {
            for v in variables {
                out.insert(v.as_str().to_owned());
            }
            collect_vars_recursive(inner, out);
        }
        Group {
            inner, variables, ..
        } => {
            for v in variables {
                out.insert(v.as_str().to_owned());
            }
            collect_vars_recursive(inner, out);
        }
        Values { variables, .. } => {
            for v in variables {
                out.insert(v.as_str().to_owned());
            }
        }
        Service { inner, .. } => {
            collect_vars_recursive(inner, out);
        }
        Path {
            subject, object, ..
        } => {
            if let spargebra::term::TermPattern::Variable(v) = subject {
                out.insert(v.as_str().to_owned());
            }
            if let spargebra::term::TermPattern::Variable(v) = object {
                out.insert(v.as_str().to_owned());
            }
        }
    }
}
