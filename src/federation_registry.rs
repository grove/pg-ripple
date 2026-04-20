//! pg_ripple SQL API — SPARQL Federation endpoint registry (v0.16.0)

/// Check whether a URL's hostname resolves to a private/loopback/link-local address.
///
/// When `pg_ripple.federation_allow_private` is `false` (default), this function
/// returns `Err` with a PT621 message if the resolved IP is in RFC 1918,
/// loopback (127.x), link-local (169.254.x), or IPv6 link-local ranges (v0.42.0).
fn check_private_ip(url: &str) -> Result<(), String> {
    if crate::FEDERATION_ALLOW_PRIVATE.get() {
        return Ok(());
    }

    // Extract hostname from the URL.
    let host = extract_host(url);
    if host.is_empty() {
        return Ok(()); // Cannot extract host — let the HTTP call fail later.
    }

    // Try to resolve the host to IP addresses.
    use std::net::ToSocketAddrs;
    let addrs_result = format!("{host}:80").to_socket_addrs();
    let addrs = match addrs_result {
        Ok(a) => a,
        Err(_) => return Ok(()), // DNS failure — let the HTTP call fail later.
    };

    for addr in addrs {
        let ip = addr.ip();
        if is_private_ip(ip) {
            return Err(format!(
                "PT621: register_endpoint: endpoint URL '{url}' resolves to a \
                 private/loopback/link-local address ({ip}); \
                 set pg_ripple.federation_allow_private = true to override"
            ));
        }
    }
    Ok(())
}

/// Extract the hostname from an HTTP/HTTPS URL.
fn extract_host(url: &str) -> String {
    // Simple extraction: strip scheme, take up to first '/' or ':' (port).
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or("");
    // Strip path.
    let host_and_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    // Strip port.
    if let Some(bracket_end) = host_and_port.rfind(']') {
        // IPv6 literal like [::1]:8080
        return host_and_port[..=bracket_end]
            .trim_matches(['[', ']'])
            .to_owned();
    }
    host_and_port
        .split(':')
        .next()
        .unwrap_or(host_and_port)
        .to_owned()
}

/// Returns true if the given IP address is private, loopback, or link-local.
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Loopback: 127.0.0.0/8
            if octets[0] == 127 {
                return true;
            }
            // RFC 1918: 10.0.0.0/8
            if octets[0] == 10 {
                return true;
            }
            // RFC 1918: 172.16.0.0/12
            if octets[0] == 172 && (octets[1] >= 16 && octets[1] <= 31) {
                return true;
            }
            // RFC 1918: 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            // Link-local: 169.254.0.0/16
            if octets[0] == 169 && octets[1] == 254 {
                return true;
            }
            false
        }
        std::net::IpAddr::V6(v6) => {
            // Loopback: ::1
            if v6.is_loopback() {
                return true;
            }
            // IPv6 link-local: fe80::/10
            let segs = v6.segments();
            if (segs[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // Unique local: fc00::/7
            if (segs[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            false
        }
    }
}

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── v0.16.0: SPARQL Federation ────────────────────────────────────────────

    /// Register a remote SPARQL endpoint in the federation allowlist.
    ///
    /// Only registered endpoints can be contacted via SERVICE clauses.
    /// Attempting to call an unregistered endpoint raises an ERROR (SSRF protection).
    ///
    /// `local_view_name` — optional name of a pg_ripple SPARQL view stream table
    /// that pre-materialises the same data.  When set, SERVICE clauses targeting
    /// this URL are rewritten to scan the local table instead of making HTTP calls.
    ///
    /// `complexity` (v0.19.0) — optional hint for query planning: `'fast'`, `'normal'`
    /// (default), or `'slow'`.  Fast endpoints execute first in multi-endpoint queries.
    ///
    /// `graph_iri` (v0.42.0) — optional named-graph IRI.  When set, SERVICE clauses
    /// targeting this URL are satisfied by querying the local named graph with that IRI
    /// instead of making an HTTP call.  Useful for mock endpoints and offline testing.
    #[pg_extern]
    fn register_endpoint(
        url: &str,
        local_view_name: default!(Option<&str>, "NULL"),
        complexity: default!(Option<&str>, "NULL"),
        graph_iri: default!(Option<&str>, "NULL"),
    ) {
        // v0.22.0 M-13: Reject non-http/https URL schemes to prevent file://, gopher://, etc.
        let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
        if !scheme_ok {
            pgrx::error!(
                "register_endpoint: URL scheme must be http or https; got: {}",
                url
            );
        }

        // v0.42.0: Reject private/loopback/link-local IPs unless allowed by GUC.
        if let Err(msg) = super::check_private_ip(url) {
            pgrx::error!("{}", msg);
        }

        let local_view = local_view_name.unwrap_or("");
        let cx = complexity.unwrap_or("normal");
        if local_view.is_empty() {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, complexity, graph_iri)
                 VALUES ($1, true, $2, $3)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, complexity = $2, graph_iri = $3",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(cx),
                    pgrx::datum::DatumWithOid::from(graph_iri),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
        } else {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, local_view_name, complexity, graph_iri)
                 VALUES ($1, true, $2, $3, $4)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, local_view_name = $2, complexity = $3, graph_iri = $4",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(local_view_name),
                    pgrx::datum::DatumWithOid::from(cx),
                    pgrx::datum::DatumWithOid::from(graph_iri),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
        }

        // v0.42.0: Attempt to fetch VoID statistics for the newly registered endpoint.
        // This is best-effort — failures are logged but do not abort the registration.
        // Only attempt for real HTTP endpoints (not mock graph_iri endpoints).
        if graph_iri.is_none() {
            let _ = std::panic::catch_unwind(|| {
                crate::sparql::federation_planner::refresh_endpoint_stats(url);
            });
        }
    }

    /// Set the complexity hint for a registered endpoint (v0.19.0).
    ///
    /// Allowed values: `'fast'`, `'normal'`, `'slow'`.
    /// Fast endpoints execute first in queries with multiple SERVICE clauses
    /// targeting different endpoints, enabling earlier failure detection.
    #[pg_extern]
    fn set_endpoint_complexity(url: &str, complexity: &str) {
        Spi::run_with_args(
            "UPDATE _pg_ripple.federation_endpoints SET complexity = $2 WHERE url = $1",
            &[
                pgrx::datum::DatumWithOid::from(url),
                pgrx::datum::DatumWithOid::from(complexity),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("set_endpoint_complexity failed: {e}"));
    }

    /// Remove a remote SPARQL endpoint from the federation allowlist.
    ///
    /// After removal, SERVICE clauses targeting this URL will raise an ERROR.
    #[pg_extern]
    fn remove_endpoint(url: &str) {
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.federation_endpoints WHERE url = $1",
            &[pgrx::datum::DatumWithOid::from(url)],
        )
        .unwrap_or_else(|e| pgrx::error!("remove_endpoint failed: {e}"));
    }

    /// Disable a remote SPARQL endpoint without removing it.
    ///
    /// Disabled endpoints are excluded from SERVICE queries (like not being
    /// registered) but can be re-enabled with `register_endpoint()`.
    #[pg_extern]
    fn disable_endpoint(url: &str) {
        Spi::run_with_args(
            "UPDATE _pg_ripple.federation_endpoints SET enabled = false WHERE url = $1",
            &[pgrx::datum::DatumWithOid::from(url)],
        )
        .unwrap_or_else(|e| pgrx::error!("disable_endpoint failed: {e}"));
    }

    /// List all registered federation endpoints.
    ///
    /// Returns (url, enabled, local_view_name, complexity) for every endpoint in the allowlist.
    #[pg_extern]
    fn list_endpoints() -> TableIterator<
        'static,
        (
            name!(url, String),
            name!(enabled, bool),
            name!(local_view_name, Option<String>),
            name!(complexity, String),
        ),
    > {
        let mut rows: Vec<(String, bool, Option<String>, String)> = Vec::new();
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT url, enabled, local_view_name, complexity
                     FROM _pg_ripple.federation_endpoints
                     ORDER BY url",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_endpoints SPI error: {e}"));
            for row in result {
                let url: String = row.get(1).ok().flatten().unwrap_or_default();
                let enabled: bool = row.get(2).ok().flatten().unwrap_or(false);
                let local_view: Option<String> = row.get(3).ok().flatten();
                let cx: String = row
                    .get(4)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "normal".to_owned());
                rows.push((url, enabled, local_view, cx));
            }
        });
        TableIterator::new(rows)
    }
}
