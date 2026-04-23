-- Migration 0.41.0 → 0.42.0: Parallel Merge, Cost-Based Federation & Live CDC
--
-- New features:
--   • Parallel merge worker pool: pg_ripple.merge_workers GUC (startup only)
--   • owl:sameAs cluster size bound: pg_ripple.sameas_max_cluster_size GUC
--   • VoID statistics catalog: _pg_ripple.endpoint_stats table
--   • Cost-based federation planner: pg_ripple.federation_planner_enabled,
--     pg_ripple.federation_parallel_max, pg_ripple.federation_parallel_timeout
--   • Federation result streaming: pg_ripple.federation_inline_max_rows
--   • IP/CIDR allowlist: pg_ripple.federation_allow_private GUC
--   • Federation stats TTL: pg_ripple.federation_stats_ttl_secs GUC
--   • Named CDC subscriptions: _pg_ripple.subscriptions table,
--     pg_ripple.create_subscription(), pg_ripple.drop_subscription(),
--     pg_ripple.list_subscriptions()
--   • Named-graph local SERVICE execution: graph_iri column on federation_endpoints
--     (originally from the W3C test suite work, kept here for compatibility)
--
-- All new GUCs are registered at CREATE EXTENSION / shared_preload_libraries time
-- by the Rust _PG_init function; no SQL registration is needed here.

-- Add graph_iri column to federation_endpoints if not already present
-- (added during W3C test work; kept for backward compatibility).
ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS graph_iri TEXT;

-- VoID statistics catalog (v0.42.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.endpoint_stats (
    endpoint_url         TEXT        NOT NULL PRIMARY KEY,
    total_triples        BIGINT      NOT NULL DEFAULT 0,
    predicate_stats_json TEXT        NOT NULL DEFAULT '{}',
    distinct_subjects    BIGINT      NOT NULL DEFAULT 0,
    distinct_objects     BIGINT      NOT NULL DEFAULT 0,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Named CDC subscription registry (v0.42.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.subscriptions (
    name            TEXT        NOT NULL PRIMARY KEY,
    filter_sparql   TEXT,
    filter_shape    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Add UNIQUE constraint on vp_rare(p, s, o, g) required for ON CONFLICT idempotent inserts.
-- Safe to run on existing data (duplicate quads are not possible in a correctly-built store).
ALTER TABLE _pg_ripple.vp_rare
    ADD CONSTRAINT IF NOT EXISTS vp_rare_psog_unique UNIQUE (p, s, o, g);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.42.0', '0.41.0')
ON CONFLICT DO NOTHING;
