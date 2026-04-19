-- Migration 0.31.0 → 0.32.0: Well-Founded Semantics & Tabling
--
-- New SQL objects:
--   1. _pg_ripple.tabling_cache — session-scoped result cache for Datalog/SPARQL
--   2. pg_ripple.infer_wfs(rule_set TEXT) RETURNS JSONB — WFS inference
--   3. pg_ripple.tabling_stats() RETURNS TABLE(...) — cache hit/miss stats
--
-- New GUCs (registered in Rust _PG_init, no SQL required):
--   pg_ripple.wfs_max_iterations  INTEGER DEFAULT 100
--   pg_ripple.tabling             BOOLEAN DEFAULT true
--   pg_ripple.tabling_ttl         INTEGER DEFAULT 300
--
-- Schema changes:
--   None to VP tables, predicates, dictionary, or rules tables.
--   The tabling_cache table is created lazily on first use (via
--   tabling::ensure_tabling_catalog()), but we also create it here so that
--   the table exists immediately after ALTER EXTENSION ... UPDATE.

CREATE TABLE IF NOT EXISTS _pg_ripple.tabling_cache (
    goal_hash   BIGINT      NOT NULL PRIMARY KEY,
    result      JSONB       NOT NULL,
    computed_ms FLOAT8      NOT NULL DEFAULT 0,
    hits        BIGINT      NOT NULL DEFAULT 0,
    cached_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.tabling_cache IS
    'Memoisation cache for infer_wfs() and SPARQL sub-query results (v0.32.0). '
    'Invalidated on triple insert/delete, load_rules(), and drop_rules().';
