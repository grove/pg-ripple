//! pg_ripple SQL API — SPARQL Federation endpoint registry (v0.16.0)

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
    #[pg_extern]
    fn register_endpoint(
        url: &str,
        local_view_name: default!(Option<&str>, "NULL"),
        complexity: default!(Option<&str>, "NULL"),
    ) {
        // v0.22.0 M-13: Reject non-http/https URL schemes to prevent file://, gopher://, etc.
        let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
        if !scheme_ok {
            pgrx::error!(
                "register_endpoint: URL scheme must be http or https; got: {}",
                url
            );
        }
        let local_view = local_view_name.unwrap_or("");
        let cx = complexity.unwrap_or("normal");
        if local_view.is_empty() {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, complexity)
                 VALUES ($1, true, $2)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, complexity = $2",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(cx),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
        } else {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, local_view_name, complexity)
                 VALUES ($1, true, $2, $3)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, local_view_name = $2, complexity = $3",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(local_view_name),
                    pgrx::datum::DatumWithOid::from(cx),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
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
