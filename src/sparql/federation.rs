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
use std::time::{Duration, Instant};

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde_json::Value as Json;
use spargebra::algebra::GraphPattern;
use spargebra::term::NamedNodePattern;

use crate::dictionary;

// ─── G-3 (v0.56.0): Federation circuit breaker ───────────────────────────────

/// State of a per-endpoint circuit breaker.
#[derive(Debug, Clone)]
enum CircuitState {
    /// Circuit is closed: requests flow normally.
    Closed,
    /// Circuit is open: requests are rejected immediately (PT605).
    Open { opened_at: Instant },
    /// Circuit is half-open: one probe request is allowed through.
    HalfOpen,
}

/// Per-endpoint circuit breaker tracking consecutive failures and state.
#[derive(Debug, Clone)]
struct CircuitBreaker {
    state: CircuitState,
    consecutive_failures: u32,
}

impl CircuitBreaker {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
        }
    }

    /// Record a successful call: reset failures and close circuit.
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
    }

    /// Record a failed call. Opens the circuit when the threshold is hit.
    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        let threshold = crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_THRESHOLD.get() as u32;
        if threshold > 0 && self.consecutive_failures >= threshold {
            self.state = CircuitState::Open {
                opened_at: Instant::now(),
            };
        }
    }

    /// Returns `true` when the circuit is open and the call should be blocked.
    fn is_open(&mut self) -> bool {
        let reset_secs =
            crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_RESET_SECONDS.get() as u64;
        if let CircuitState::Open { opened_at } = self.state {
            if opened_at.elapsed().as_secs() >= reset_secs {
                // Transition to half-open to allow one probe.
                self.state = CircuitState::HalfOpen;
                return false;
            }
            return true;
        }
        false
    }
}

thread_local! {
    /// Per-backend circuit breaker map keyed by endpoint URL.
    static CIRCUIT_BREAKERS: RefCell<HashMap<String, CircuitBreaker>> =
        RefCell::new(HashMap::new());
}

/// Check whether the circuit breaker for `url` is open.
/// Returns `true` when the call should be blocked (PT605).
fn circuit_is_open(url: &str) -> bool {
    let threshold = crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_THRESHOLD.get();
    if threshold <= 0 {
        return false; // Disabled.
    }
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        let breaker = map
            .entry(url.to_owned())
            .or_insert_with(CircuitBreaker::new);
        breaker.is_open()
    })
}

fn circuit_record_success(url: &str) {
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        if let Some(breaker) = map.get_mut(url) {
            breaker.record_success();
        }
    });
}

fn circuit_record_failure(url: &str) {
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        let breaker = map
            .entry(url.to_owned())
            .or_insert_with(CircuitBreaker::new);
        breaker.record_failure();
    });
}

// ─── Thread-local connection pool (v0.19.0) ──────────────────────────────────

thread_local! {
    /// Shared HTTP agent for the current PostgreSQL backend.
    /// Created lazily on first use; reuses TCP/TLS connections across calls.
    static SHARED_AGENT: RefCell<Option<ureq::Agent>> = const { RefCell::new(None) };
}

/// Strip the platform-specific "(os error NNN)" suffix from ureq error strings.
///
/// macOS uses ECONNREFUSED = 61, Linux uses 111.  Normalising the message makes
/// pg_regress expected outputs portable across operating systems.
fn normalize_http_err(e: impl std::fmt::Display) -> String {
    let s = format!("{e}");
    // Locate the last "(os error " pattern and strip the parenthesised suffix.
    if let Some(start) = s.rfind(" (os error ") {
        let end = s[start..]
            .find(')')
            .map(|i| start + i + 1)
            .unwrap_or(s.len());
        let mut out = s[..start].to_string();
        if end < s.len() {
            out.push_str(&s[end..]);
        }
        out
    } else {
        s
    }
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
        // opt is Some(…) because we just set it above when it was None.
        // Using unwrap_or_else with unreachable! avoids both clippy::unwrap_used
        // and clippy::expect_used while preserving the invariant documentation.
        opt.as_ref()
            .unwrap_or_else(|| unreachable!("get_agent: agent should be Some after init"))
            .clone()
    })
}

/// Public wrapper around `get_agent` for use by `federation_planner` (v0.42.0).
pub(crate) fn get_agent_pub(timeout: Duration, pool_size: usize) -> ureq::Agent {
    get_agent(timeout, pool_size)
}

// ─── Endpoint policy check (v0.55.0) ─────────────────────────────────────────

/// Check the federation endpoint network policy for `url`.
///
/// Three policy modes are supported:
/// - `'open'`         — allow all endpoints (development/testing only).
/// - `'allowlist'`    — only permit URLs listed in `pg_ripple.federation_allowed_endpoints`.
/// - `'default-deny'` — block RFC-1918, loopback, link-local, and `file://` URLs.
///
/// Returns `Ok(())` when the URL is permitted, or `Err(message)` when blocked.
///
/// Error messages begin with `PT606:` for observability.
pub(crate) fn check_endpoint_policy(url: &str) -> Result<(), String> {
    let policy = crate::FEDERATION_ENDPOINT_POLICY
        .get()
        .map(|c| c.to_string_lossy().to_string())
        .unwrap_or_else(|| "default-deny".to_string());

    match policy.as_str() {
        "open" => Ok(()),
        "allowlist" => {
            let allowed = crate::FEDERATION_ALLOWED_ENDPOINTS
                .get()
                .map(|c| c.to_string_lossy().to_string())
                .unwrap_or_default();
            let permitted = allowed
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .any(|entry| entry == url);
            if permitted {
                Ok(())
            } else {
                Err(format!(
                    "PT606: SERVICE endpoint blocked by federation_endpoint_policy: {url}"
                ))
            }
        }
        _ => {
            // default-deny: block private/loopback/link-local/file:// URLs.
            if url.starts_with("file://") {
                return Err(format!(
                    "PT606: SERVICE endpoint blocked by federation_endpoint_policy: {url}"
                ));
            }

            if extract_host(url).is_some_and(|host| is_blocked_host(&host)) {
                return Err(format!(
                    "PT606: SERVICE endpoint blocked by federation_endpoint_policy: {url}"
                ));
            }
            Ok(())
        }
    }
}

/// Returns `true` when `host` is a loopback, link-local, or RFC-1918 address.
fn is_blocked_host(host: &str) -> bool {
    // loopback
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host.starts_with("127.") {
        return true;
    }
    // link-local IPv4: 169.254.x.x
    if host.starts_with("169.254.") {
        return true;
    }
    // link-local IPv6: fe80::
    if host.to_lowercase().starts_with("fe80") {
        return true;
    }
    // SSRF-RFC1918-01 (v0.80.0): IPv6 Unique Local addresses fc00::/7
    // (includes both fc::/8 and fd::/8 subnets used for private networks).
    let h_lower = host.to_lowercase();
    if h_lower.starts_with("fc") || h_lower.starts_with("fd") {
        return true;
    }
    // RFC-1918: 10.x.x.x
    if host.starts_with("10.") {
        return true;
    }
    // RFC-1918: 172.16.x.x – 172.31.x.x
    if host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|s| s.parse::<u8>().ok())
        .is_some_and(|second| (16..=31).contains(&second))
    {
        return true;
    }
    // RFC-1918: 192.168.x.x
    if host.starts_with("192.168.") {
        return true;
    }
    false
}

/// Extract the hostname/IP from a URL string without external dependencies.
///
/// Returns `None` for malformed URLs.
fn extract_host(url: &str) -> Option<String> {
    // Strip scheme (e.g. "https://").
    let after_scheme = url.split_once("://").map(|(_, rest)| rest)?;
    // Strip path, query, fragment.
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    // Strip userinfo@
    let host_port = if let Some((_, hp)) = authority.split_once('@') {
        hp
    } else {
        authority
    };
    // IPv6 literal: [::1]:port
    if host_port.starts_with('[') {
        return host_port
            .split_once(']')
            .map(|(h, _)| h.trim_start_matches('[').to_string());
    }
    // Strip port.
    let host = host_port.split(':').next().unwrap_or(host_port);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

// ─── Database allowlist check ────────────────────────────────────────────────

/// Returns `true` when `url` is registered in `_pg_ripple.federation_endpoints`
/// with `enabled = true`.
pub(crate) fn is_endpoint_allowed(url: &str) -> bool {
    // FED-URL-01 (v0.81.0): normalise both the incoming URL and the allowlist
    // entries to lowercase scheme+host before comparison so that URLs that
    // differ only in case or trailing slash are not incorrectly rejected.
    let normalised = normalise_federation_url(url);
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM _pg_ripple.federation_endpoints
            WHERE lower(rtrim(url, '/')) = $1 AND enabled = true
         )",
        &[DatumWithOid::from(normalised.as_str())],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Normalise a federation URL for case-insensitive allowlist comparison.
///
/// Converts the scheme and host to lowercase and strips a trailing slash.
/// Path and query components are left as-is (case-sensitive).
pub(crate) fn normalise_federation_url(url: &str) -> String {
    // Parse scheme://host/path and lowercase scheme + host only.
    if let Some(after_scheme) = url.find("://") {
        let scheme = url[..after_scheme].to_lowercase();
        let rest = &url[after_scheme + 3..];
        // Split host from path at the first '/'.
        let (host_port, path) = if let Some(slash_pos) = rest.find('/') {
            (&rest[..slash_pos], &rest[slash_pos..])
        } else {
            (rest, "")
        };
        let host_lower = host_port.to_lowercase();
        // Strip trailing slash from path.
        let path_trimmed = path.trim_end_matches('/');
        format!("{scheme}://{host_lower}{path_trimmed}")
    } else {
        // Not a well-formed URL; just lowercase and strip trailing slash.
        url.to_lowercase().trim_end_matches('/').to_owned()
    }
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

/// Returns the named-graph dictionary IDs of all registered graph endpoints (v0.42.0).
///
/// Used to exclude service-data named graphs from outer BGP scans so that
/// endpoint data loaded into named graphs does not leak into outer patterns.
pub(crate) fn get_service_graph_ids() -> Vec<i64> {
    let mut result = Vec::new();
    Spi::connect(|client| {
        let rows = client.select(
            "SELECT d.id
                   FROM _pg_ripple.federation_endpoints fe
                   JOIN _pg_ripple.dictionary d
                     ON d.value = fe.graph_iri AND d.kind = 0
                  WHERE fe.graph_iri IS NOT NULL AND fe.enabled = true",
            None,
            &[],
        );
        if let Ok(rows) = rows {
            for row in rows {
                if let Ok(Some(id)) = row.get::<i64>(1) {
                    result.push(id);
                }
            }
        }
    });
    result
}

/// Returns the `graph_iri` for an endpoint if set and not NULL (v0.42.0).
///
/// When non-NULL, the SERVICE clause is satisfied by querying the local named
/// graph with that IRI instead of making an HTTP call.  This enables mock
/// endpoints for the W3C SPARQL federation test suite and offline testing.
pub(crate) fn get_graph_iri(url: &str) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT graph_iri FROM _pg_ripple.federation_endpoints
          WHERE url = $1 AND enabled = true AND graph_iri IS NOT NULL",
        &[DatumWithOid::from(url)],
    )
    .ok()
    .flatten()
}

/// Returns all registered endpoints that have a `graph_iri` set (v0.42.0).
///
/// Used to expand `SERVICE ?variable` clauses: each registered graph endpoint
/// becomes one arm of a UNION, binding the variable to the endpoint URL.
pub(crate) fn get_all_graph_endpoints() -> Vec<(String, String)> {
    let mut result = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT url, graph_iri FROM _pg_ripple.federation_endpoints
                  WHERE enabled = true AND graph_iri IS NOT NULL
                  ORDER BY url",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("get_all_graph_endpoints SPI error: {e}"));
        for row in rows {
            if let (Ok(Some(url)), Ok(Some(giri))) = (row.get::<String>(1), row.get::<String>(2)) {
                result.push((url, giri));
            }
        }
    });
    result
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
    // FED-CACHE-01 (v0.81.0): normalise the SPARQL text before hashing so that
    // whitespace-variant queries share a cache entry.
    let normalised = normalise_sparql_for_cache(sparql_text);
    // XXH3-128 of the SPARQL text as a 32-char hex fingerprint key.
    // Using 128-bit avoids birthday-bound collisions even at very high query volumes.
    let hash = {
        use xxhash_rust::xxh3::xxh3_128;
        format!("{:032x}", xxh3_128(normalised.as_bytes()))
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
    // FED-CACHE-01: normalise before hashing.
    let normalised = normalise_sparql_for_cache(sparql_text);
    let hash = {
        use xxhash_rust::xxh3::xxh3_128;
        format!("{:032x}", xxh3_128(normalised.as_bytes()))
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

/// FED-COST-01b (v0.82.0): update `_pg_ripple.federation_stats` after a federation call.
///
/// Uses an upsert to accumulate call_count, error_count, and latency for P50/P95
/// approximation. P50 is approximated as the running average; P95 uses the max.
/// No-op when the table doesn't exist (older migration not run yet).
fn update_federation_stats(url: &str, latency_ms: f64, row_count: i64, is_error: bool) {
    let error_delta: i64 = if is_error { 1 } else { 0 };
    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.federation_stats AS fs \
           (endpoint_url, call_count, error_count, \
            total_latency_ms, max_latency_ms, row_estimate, updated_at) \
         VALUES ($1, 1, $2, $3, $3, $4, now()) \
         ON CONFLICT (endpoint_url) DO UPDATE SET \
           call_count      = fs.call_count + 1, \
           error_count     = fs.error_count + $2, \
           total_latency_ms = fs.total_latency_ms + $3, \
           max_latency_ms  = GREATEST(fs.max_latency_ms, $3), \
           p50_ms          = (fs.total_latency_ms + $3) / (fs.call_count + 1), \
           p95_ms          = GREATEST(fs.max_latency_ms, $3), \
           row_estimate    = CASE WHEN $2 = 0 THEN $4 ELSE fs.row_estimate END, \
           updated_at      = now()",
        &[
            DatumWithOid::from(url),
            DatumWithOid::from(error_delta),
            DatumWithOid::from(latency_ms),
            DatumWithOid::from(row_count),
        ],
    );
}

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
///
/// ENUM-02 (v0.74.0): complexity column is now SMALLINT (1=fast, 2=normal, 3=slow).
/// The query casts back to text for backward-compatible return type.
#[allow(dead_code)]
pub(crate) fn get_endpoint_complexity(url: &str) -> String {
    Spi::get_one_with_args::<String>(
        "SELECT CASE complexity
              WHEN 1 THEN 'fast'
              WHEN 2 THEN 'normal'
              WHEN 3 THEN 'slow'
              ELSE 'normal'
          END
          FROM _pg_ripple.federation_endpoints
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

/// FED-CACHE-01 (v0.81.0): Normalise a SPARQL query string for use as a cache key.
///
/// - Collapses all whitespace runs to a single space.
/// - Lowercases SPARQL keywords.
/// - Trims leading/trailing whitespace.
///
/// This ensures that whitespace-variant queries (e.g. extra newlines, tabs)
/// share the same cache entry.
fn normalise_sparql_for_cache(sparql: &str) -> String {
    // Attempt canonical form via spargebra Display; fall back to simple whitespace collapse.
    if let Ok(q) = spargebra::SparqlParser::new().parse_query(sparql) {
        return format!("{q}");
    }
    // Fallback: collapse whitespace and lowercase keywords.
    sparql.split_whitespace().collect::<Vec<_>>().join(" ")
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

// ─── v0.28.0: Vector endpoint federation ─────────────────────────────────────

/// Register an external vector service endpoint for SPARQL SERVICE federation.
///
/// `api_type` must be one of `'pgvector'`, `'weaviate'`, `'qdrant'`, or `'pinecone'`.
///
/// Returns a WARNING (not ERROR) if the URL is already registered (idempotent upsert).
pub fn register_vector_endpoint(url: &str, api_type: &str) {
    let valid_types = ["pgvector", "weaviate", "qdrant", "pinecone"];
    if !valid_types.contains(&api_type) {
        pgrx::warning!(
            "pg_ripple.register_vector_endpoint: unknown api_type '{}'; \
             must be one of: pgvector, weaviate, qdrant, pinecone",
            api_type
        );
        return;
    }

    pgrx::Spi::run_with_args(
        "INSERT INTO _pg_ripple.vector_endpoints (url, api_type, enabled) \
         VALUES ($1, $2, true) \
         ON CONFLICT (url) DO UPDATE SET api_type = EXCLUDED.api_type, enabled = true",
        &[
            pgrx::datum::DatumWithOid::from(url),
            pgrx::datum::DatumWithOid::from(api_type),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("register_vector_endpoint: SPI error: {e}"));
}

/// Returns `true` when `url` is registered in `_pg_ripple.vector_endpoints`
/// with `enabled = true`.
#[allow(dead_code)]
pub fn is_vector_endpoint_registered(url: &str) -> bool {
    pgrx::Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM _pg_ripple.vector_endpoints
            WHERE url = $1 AND enabled = true
         )",
        &[pgrx::datum::DatumWithOid::from(url)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Query an external vector service endpoint with a similarity query.
///
/// Returns a list of `(entity_id, entity_iri, score)` triples by:
/// 1. Calling the external API with `query_text` and `k`.
/// 2. Resolving returned IRIs against the local dictionary.
/// 3. Returning only entities known to the local dictionary.
///
/// When the endpoint is unavailable or times out, emits a WARNING and returns
/// an empty vector (graceful degradation per the v0.28.0 spec).
///
/// Currently supports Weaviate GraphQL, Qdrant REST, and Pinecone REST APIs.
/// The `pgvector` api_type is handled locally (no HTTP call needed).
#[allow(dead_code)]
pub fn query_vector_endpoint(url: &str, query_text: &str, k: i32) -> Vec<(i64, String, f64)> {
    if !is_vector_endpoint_registered(url) {
        pgrx::warning!(
            "pg_ripple.vector_endpoint: endpoint not registered (PT607): {url}; \
             use pg_ripple.register_vector_endpoint() to register it"
        );
        return Vec::new();
    }

    // Get api_type for this endpoint.
    let api_type: String = pgrx::Spi::get_one_with_args::<String>(
        "SELECT api_type FROM _pg_ripple.vector_endpoints WHERE url = $1",
        &[pgrx::datum::DatumWithOid::from(url)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| "unknown".to_owned());

    let timeout_ms = crate::VECTOR_FEDERATION_TIMEOUT_MS.get() as u64;
    let timeout = std::time::Duration::from_millis(timeout_ms);

    match api_type.as_str() {
        "pgvector" => {
            // pgvector is local — fall back to the local embeddings table.
            pgrx::warning!(
                "pg_ripple.query_vector_endpoint: api_type 'pgvector' is local; \
                 use pg_ripple.similar_entities() instead"
            );
            Vec::new()
        }
        "weaviate" => query_weaviate_endpoint(url, query_text, k, timeout),
        "qdrant" => query_qdrant_endpoint(url, query_text, k, timeout),
        "pinecone" => query_pinecone_endpoint(url, query_text, k, timeout),
        other => {
            pgrx::warning!("pg_ripple.query_vector_endpoint: unsupported api_type '{other}'");
            Vec::new()
        }
    }
}

/// Query a Weaviate v4 GraphQL `/v1/graphql` endpoint.
#[allow(dead_code)]
fn query_weaviate_endpoint(
    base_url: &str,
    query_text: &str,
    k: i32,
    timeout: std::time::Duration,
) -> Vec<(i64, String, f64)> {
    let endpoint = format!("{}/v1/graphql", base_url.trim_end_matches('/'));
    let gql = serde_json::json!({
        "query": format!(
            r#"{{ Get {{ Entity(nearText: {{concepts: ["{query_text}"]}}, limit: {k}) {{ _additional {{ id certainty }} iri }} }} }}"#
        )
    });
    let body_str = match serde_json::to_string(&gql) {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_weaviate_endpoint: JSON serialization error: {e}");
            return Vec::new();
        }
    };

    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let response = match agent
        .post(&endpoint)
        .set("Content-Type", "application/json")
        .send_string(&body_str)
    {
        Ok(r) => r,
        Err(e) => {
            pgrx::warning!("pg_ripple.query_vector_endpoint (weaviate): request failed: {e}");
            return Vec::new();
        }
    };

    // FED-BODY-STREAM-01 (v0.82.0): pre-check Content-Length before buffering.
    if let Some(cl_str) = response.header("content-length")
        && let Ok(cl) = cl_str.parse::<i64>()
    {
        let limit = crate::FEDERATION_MAX_RESPONSE_BYTES.get();
        if limit >= 0 && cl > limit as i64 {
            pgrx::warning!("query_weaviate_endpoint: Content-Length {cl} exceeds limit");
            return Vec::new();
        }
    }
    let body = match response.into_string() {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_weaviate_endpoint: response read error: {e}");
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            pgrx::warning!("query_weaviate_endpoint: JSON parse error: {e}");
            return Vec::new();
        }
    };

    // Parse Weaviate response: data.Get.Entity[].{iri, _additional.certainty}
    let entities = json
        .pointer("/data/Get/Entity")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    resolve_iri_scores(entities, "iri", "_additional/certainty")
}

/// Query a Qdrant REST `/collections/{name}/points/search` endpoint.
#[allow(dead_code)]
fn query_qdrant_endpoint(
    base_url: &str,
    query_text: &str,
    k: i32,
    timeout: std::time::Duration,
) -> Vec<(i64, String, f64)> {
    // Qdrant requires a pre-embedded query vector. We embed via the local API
    // if configured, otherwise we return empty with a WARNING.
    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    if api_url.is_empty() {
        pgrx::warning!(
            "pg_ripple.query_vector_endpoint (qdrant): embedding API URL not configured; \
             set pg_ripple.embedding_api_url to enable Qdrant federation"
        );
        return Vec::new();
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_guc = crate::EMBEDDING_MODEL.get();
    let model = model_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("text-embedding-3-small");

    let embedding =
        match crate::sparql::embedding::call_embedding_api_pub(query_text, model, api_url, api_key)
        {
            Ok(v) => v,
            Err(e) => {
                pgrx::warning!("query_qdrant_endpoint: embedding API error: {e}");
                return Vec::new();
            }
        };

    let endpoint = format!(
        "{}/collections/entities/points/search",
        base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "vector": embedding,
        "limit": k,
        "with_payload": true
    });
    let body_str = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_qdrant_endpoint: JSON serialization error: {e}");
            return Vec::new();
        }
    };

    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let response = match agent
        .post(&endpoint)
        .set("Content-Type", "application/json")
        .send_string(&body_str)
    {
        Ok(r) => r,
        Err(e) => {
            pgrx::warning!("pg_ripple.query_vector_endpoint (qdrant): request failed: {e}");
            return Vec::new();
        }
    };

    // FED-BODY-STREAM-01 (v0.82.0): pre-check Content-Length before buffering.
    if let Some(cl_str) = response.header("content-length")
        && let Ok(cl) = cl_str.parse::<i64>()
    {
        let limit = crate::FEDERATION_MAX_RESPONSE_BYTES.get();
        if limit >= 0 && cl > limit as i64 {
            pgrx::warning!("query_qdrant_endpoint: Content-Length {cl} exceeds limit");
            return Vec::new();
        }
    }
    let body = match response.into_string() {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_qdrant_endpoint: response read error: {e}");
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            pgrx::warning!("query_qdrant_endpoint: JSON parse error: {e}");
            return Vec::new();
        }
    };

    let results = json
        .pointer("/result")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    resolve_iri_scores(results, "payload/iri", "score")
}

/// Query a Pinecone REST `/query` endpoint.
#[allow(dead_code)]
fn query_pinecone_endpoint(
    base_url: &str,
    query_text: &str,
    k: i32,
    timeout: std::time::Duration,
) -> Vec<(i64, String, f64)> {
    // Like Qdrant, Pinecone requires a pre-embedded vector.
    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    if api_url.is_empty() {
        pgrx::warning!(
            "pg_ripple.query_vector_endpoint (pinecone): embedding API URL not configured"
        );
        return Vec::new();
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_guc = crate::EMBEDDING_MODEL.get();
    let model = model_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("text-embedding-3-small");

    let embedding =
        match crate::sparql::embedding::call_embedding_api_pub(query_text, model, api_url, api_key)
        {
            Ok(v) => v,
            Err(e) => {
                pgrx::warning!("query_pinecone_endpoint: embedding API error: {e}");
                return Vec::new();
            }
        };

    let endpoint = format!("{}/query", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "vector": embedding,
        "topK": k,
        "includeMetadata": true
    });
    let body_str = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_pinecone_endpoint: JSON serialization error: {e}");
            return Vec::new();
        }
    };

    let pinecone_key_guc = crate::EMBEDDING_API_KEY.get();
    let pinecone_key = pinecone_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let mut req = agent
        .post(&endpoint)
        .set("Content-Type", "application/json");
    if !pinecone_key.is_empty() {
        req = req.set("Api-Key", pinecone_key);
    }

    let response = match req.send_string(&body_str) {
        Ok(r) => r,
        Err(e) => {
            pgrx::warning!("pg_ripple.query_vector_endpoint (pinecone): request failed: {e}");
            return Vec::new();
        }
    };

    // FED-BODY-STREAM-01 (v0.82.0): pre-check Content-Length before buffering.
    if let Some(cl_str) = response.header("content-length")
        && let Ok(cl) = cl_str.parse::<i64>()
    {
        let limit = crate::FEDERATION_MAX_RESPONSE_BYTES.get();
        if limit >= 0 && cl > limit as i64 {
            pgrx::warning!("query_pinecone_endpoint: Content-Length {cl} exceeds limit");
            return Vec::new();
        }
    }
    let body = match response.into_string() {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("query_pinecone_endpoint: response read error: {e}");
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            pgrx::warning!("query_pinecone_endpoint: JSON parse error: {e}");
            return Vec::new();
        }
    };

    let matches = json
        .pointer("/matches")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Pinecone: matches[].{id (iri), score, metadata.iri}
    resolve_iri_scores(matches, "metadata/iri", "score")
}

/// Resolve a list of JSON result objects with IRI and score fields into
/// dictionary-encoded `(entity_id, entity_iri, score)` triples.
///
/// `iri_path` is a JSON pointer relative to each result object.
/// `score_path` is a JSON pointer for the score field.
#[allow(dead_code)]
fn resolve_iri_scores(
    items: Vec<serde_json::Value>,
    iri_path: &str,
    score_path: &str,
) -> Vec<(i64, String, f64)> {
    items
        .iter()
        .filter_map(|item| {
            let iri_ptr = format!("/{}", iri_path.replace('.', "/"));
            let score_ptr = format!("/{}", score_path.replace('.', "/"));
            let iri = item.pointer(&iri_ptr).and_then(|v| v.as_str())?.to_owned();
            let score = item
                .pointer(&score_ptr)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let bare_iri = iri.trim_start_matches('<').trim_end_matches('>');
            let entity_id = crate::dictionary::encode(bare_iri, crate::dictionary::KIND_IRI);
            if entity_id == 0 {
                return None; // Not in local dictionary — skip.
            }
            Some((entity_id, iri, score))
        })
        .collect()
}
