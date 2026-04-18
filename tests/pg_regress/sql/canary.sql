-- pg_regress test: canary() health check function (v0.25.0)
-- Verifies that pg_ripple.canary() returns a well-formed JSONB object with
-- expected keys and sensible values.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- canary() must return a JSONB object.
SELECT jsonb_typeof(pg_ripple.canary()) AS canary_type;

-- The result must contain all four expected keys.
SELECT (pg_ripple.canary() ? 'merge_worker')    AS has_merge_worker,
       (pg_ripple.canary() ? 'cache_hit_rate')  AS has_cache_hit_rate,
       (pg_ripple.canary() ? 'catalog_consistent') AS has_catalog_consistent,
       (pg_ripple.canary() ? 'orphaned_rare_rows')  AS has_orphaned_rare_rows;

-- catalog_consistent must be true on a healthy database.
SELECT (pg_ripple.canary()->>'catalog_consistent')::boolean AS catalog_consistent;

-- orphaned_rare_rows must be 0 on a healthy database.
SELECT (pg_ripple.canary()->>'orphaned_rare_rows')::bigint AS orphaned_rare_rows;

-- cache_hit_rate must be between 0.0 and 1.0.
SELECT (pg_ripple.canary()->>'cache_hit_rate')::float8 BETWEEN 0.0 AND 1.0 AS hit_rate_in_range;
