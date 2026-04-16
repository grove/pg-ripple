-- Migration 0.18.0 → 0.19.0: Federation Performance
--
-- New features (compiled from Rust):
--   - Connection pooling: thread-local ureq Agent reuses TCP/TLS across SERVICE calls
--     (pg_ripple.federation_pool_size GUC, default: 4 per endpoint)
--   - Result caching: _pg_ripple.federation_cache stores remote results with TTL
--     (pg_ripple.federation_cache_ttl GUC, default: 0 = disabled)
--   - Query rewriting: explicit variable projection instead of SELECT *
--   - Partial result handling: pg_ripple.federation_on_partial GUC
--   - Adaptive timeout: pg_ripple.federation_adaptive_timeout GUC
--   - Batch SERVICE calls: two SERVICE clauses to same endpoint → one HTTP request
--   - Result deduplication: per-call HashMap avoids redundant dictionary lookups
--   - Endpoint complexity hints: pg_ripple.set_endpoint_complexity()
--
-- Schema changes:

-- Add complexity column to federation_endpoints for query planning hints.
ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS complexity TEXT NOT NULL DEFAULT 'normal'
    CHECK (complexity IN ('fast', 'normal', 'slow'));

-- Create the federation result cache table.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache (
    url          TEXT        NOT NULL,
    query_hash   BIGINT      NOT NULL,
    result_jsonb JSONB       NOT NULL,
    cached_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (url, query_hash)
);

CREATE INDEX IF NOT EXISTS idx_federation_cache_expires
    ON _pg_ripple.federation_cache (expires_at);
