//! Federation endpoint policy, SSRF allowlist, adaptive timeout, and result cache (v0.55.0+).
//!
//! Split from `federation.rs` in v0.85.0 (Q13-03).

use super::*;

// Cross-module helper: normalise_sparql_for_cache lives in decode.rs.
use super::decode::normalise_sparql_for_cache;

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
pub(super) fn cache_lookup(url: &str, sparql_text: &str) -> Option<String> {
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
pub(super) fn cache_store(url: &str, sparql_text: &str, body: &str) {
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
