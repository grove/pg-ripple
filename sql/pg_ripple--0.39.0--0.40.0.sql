-- Migration 0.39.0 → 0.40.0: Parallel Merge, Cost-Based Federation & Live CDC
-- Schema changes:
--   - CREATE TABLE _pg_ripple.endpoint_stats
--   - CREATE TABLE _pg_ripple.subscriptions
--   - New GUCs: merge_workers, sameas_max_cluster_size, federation_stats_ttl_secs,
--               federation_planner_enabled, federation_parallel_max,
--               federation_parallel_timeout, federation_inline_max_rows,
--               federation_allow_private
-- Data-rewrite cost: Low (new catalog tables only; no VP table data changes)
-- Downgrade: Drop endpoint_stats and subscriptions tables; remove GUC settings;
--            CDC queue tables (_pg_ripple.cdc_queue_*) must be dropped manually.

-- VoID statistics cache per registered federation endpoint
CREATE TABLE IF NOT EXISTS _pg_ripple.endpoint_stats (
    endpoint_id    BIGINT       NOT NULL REFERENCES _pg_ripple.federation_endpoints (id) ON DELETE CASCADE,
    predicate_id   BIGINT       NOT NULL,
    triple_count   BIGINT       NOT NULL DEFAULT 0,
    distinct_s     BIGINT       NOT NULL DEFAULT 0,
    distinct_o     BIGINT       NOT NULL DEFAULT 0,
    fetched_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (endpoint_id, predicate_id)
);

-- Live CDC subscription catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.subscriptions (
    name             TEXT         NOT NULL PRIMARY KEY,
    filter_sparql    TEXT,
    filter_shape     TEXT,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    queue_table_oid  OID
);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.40.0', '0.39.0')
ON CONFLICT DO NOTHING;
