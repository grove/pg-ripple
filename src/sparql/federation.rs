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
//! # Parallelism
//!
//! Within a PostgreSQL SPI context, true parallel HTTP is not feasible.
//! Multiple SERVICE clauses in one query are executed sequentially.
//! (The pg_ripple_http sidecar can exploit async parallelism for the HTTP
//! endpoint path, but the in-process SPI path uses sequential fallback.)

#![allow(clippy::type_complexity)]

use std::time::Duration;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde_json::Value as Json;

use crate::dictionary;

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

// ─── Remote HTTP execution ───────────────────────────────────────────────────

/// Execute a SPARQL SELECT query against a remote endpoint.
///
/// Sends an HTTP GET with `query=<sparql_text>` and `Accept:
/// application/sparql-results+json`.  On success returns `(variables, rows)`;
/// each row is a `Vec<Option<String>>` of N-Triples–formatted terms.
///
/// `timeout_secs` is the per-call wall-clock budget.
/// `max_results` caps how many rows are returned; the rest are silently dropped.
pub(crate) fn execute_remote(
    url: &str,
    sparql_text: &str,
    timeout_secs: i32,
    max_results: i32,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), String> {
    type RemoteResult = (Vec<String>, Vec<Vec<Option<String>>>);

    let timeout = Duration::from_secs(timeout_secs.max(1) as u64);

    let agent = ureq::AgentBuilder::new().timeout(timeout).build();

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
    result
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
pub(crate) fn encode_results(
    variables: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> (Vec<String>, Vec<Vec<Option<i64>>>) {
    let encoded: Vec<Vec<Option<i64>>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| cell.map(|s| encode_ntriples_term(&s)))
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
