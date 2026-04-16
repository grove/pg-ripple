-- Migration 0.15.0 → 0.16.0: SPARQL Federation
--
-- New schema objects:
--   _pg_ripple.federation_endpoints  — endpoint allowlist (SSRF protection)
--   _pg_ripple.federation_health     — rolling probe log for health monitoring
--
-- New SQL functions:
--   pg_ripple.register_endpoint(url, local_view_name)
--   pg_ripple.remove_endpoint(url)
--   pg_ripple.disable_endpoint(url)
--   pg_ripple.list_endpoints()
--
-- New GUCs:
--   pg_ripple.federation_timeout       (default: 30 seconds)
--   pg_ripple.federation_max_results   (default: 10,000 rows)
--   pg_ripple.federation_on_error      (default: 'warning')
--
-- SPARQL engine changes:
--   SERVICE <url> { ... }  — executes remote SPARQL SELECT via HTTP GET
--   SERVICE SILENT <url> { ... } — same but returns empty results on failure
--   SERVICE + local_view_name  — rewrites to scan pre-materialised stream table
--
-- Security note: only endpoints registered via register_endpoint() may be
-- contacted.  Unregistered URLs raise an ERROR to prevent SSRF attacks.

-- Federation endpoint allowlist
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_endpoints (
    url             TEXT    NOT NULL PRIMARY KEY,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    local_view_name TEXT
);

-- Federation health log (rolling probe log; used by health-based endpoint skipping)
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_health (
    id          BIGSERIAL   PRIMARY KEY,
    url         TEXT        NOT NULL,
    success     BOOLEAN     NOT NULL,
    latency_ms  BIGINT      NOT NULL DEFAULT 0,
    probed_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_federation_health_url_time
    ON _pg_ripple.federation_health (url, probed_at DESC);
