-- pg_regress test: Cost-based federation planner and VoID statistics (v0.42.0)
--
-- Tests that:
-- 1. GUCs federation_planner_enabled, federation_parallel_max, etc. exist.
-- 2. _pg_ripple.endpoint_stats table exists.
-- 3. list_federation_stats() returns endpoint stats.
-- 4. refresh_federation_stats() can be called (best-effort; no live endpoints).
-- 5. IP/CIDR allowlist rejects private-IP endpoints by default.
-- 6. federation_allow_private GUC allows override.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

SHOW pg_ripple.federation_planner_enabled;
SHOW pg_ripple.federation_parallel_max;
SHOW pg_ripple.federation_parallel_timeout;
SHOW pg_ripple.federation_inline_max_rows;
SHOW pg_ripple.federation_stats_ttl_secs;
SHOW pg_ripple.sameas_max_cluster_size;

-- ── Part 2: endpoint_stats table exists ──────────────────────────────────────

SELECT count(*) AS endpoint_stats_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'endpoint_stats';

-- Initially empty.
SELECT count(*) AS initial_stats_count FROM pg_ripple.list_federation_stats();

-- ── Part 3: Register a mock endpoint and verify stats entry ──────────────────

-- Mock endpoint using a named graph so no HTTP call is made.
SELECT pg_ripple.load_turtle('
@prefix ex: <https://ex.org/> .
ex:planet1 ex:type ex:Planet .
ex:planet2 ex:type ex:Planet .
') AS triples_loaded;

SELECT pg_ripple.register_endpoint(
    'http://mock.example.test/sparql',
    NULL, NULL,
    'https://ex.org/mock_graph'
) AS endpoint_registered;

-- After registration, stats entry may or may not exist (HTTP fetch fails for mock endpoints).
-- We verify that list_federation_stats() doesn't error.
SELECT count(*) >= 0 AS stats_accessible FROM pg_ripple.list_federation_stats();

-- ── Part 4: refresh_federation_stats (best-effort, no live endpoint) ─────────

-- refresh_federation_stats returns boolean (false if endpoint unreachable).
SELECT pg_ripple.refresh_federation_stats('http://mock.example.test/sparql') IN (true, false)
    AS refresh_did_not_crash;

-- ── Part 5: IP/CIDR allowlist ────────────────────────────────────────────────

-- Default: private IPs rejected (PT621).
SHOW pg_ripple.federation_allow_private;

-- Registering a loopback endpoint should fail with PT621.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.register_endpoint('http://127.0.0.1:3030/sparql');
        RAISE NOTICE 'no error raised';
    EXCEPTION WHEN OTHERS THEN
        -- Expected: PT621 error
        IF SQLERRM LIKE '%PT621%' OR SQLERRM LIKE '%private%' OR SQLERRM LIKE '%loopback%' THEN
            RAISE NOTICE 'PT621 correctly raised for loopback endpoint';
        ELSE
            RAISE NOTICE 'unexpected error: %', SQLERRM;
        END IF;
    END;
END;
$$;

-- Set federation_allow_private = true and try again (should succeed).
SET pg_ripple.federation_allow_private = true;
SELECT pg_ripple.register_endpoint('http://127.0.0.1:3030/sparql') IS NULL AS registered_private;
SET pg_ripple.federation_allow_private = false;

-- Cleanup.
SELECT pg_ripple.remove_endpoint('http://127.0.0.1:3030/sparql') IS NULL AS removed;
SELECT pg_ripple.remove_endpoint('http://mock.example.test/sparql') IS NULL AS mock_removed;

-- ── Part 6: Parallel SERVICE execution GUC ───────────────────────────────────

-- Verify GUC controls are accessible.
SET pg_ripple.federation_parallel_max = 2;
SHOW pg_ripple.federation_parallel_max;
SET pg_ripple.federation_parallel_max = 4;

SET pg_ripple.federation_inline_max_rows = 100;
SHOW pg_ripple.federation_inline_max_rows;
SET pg_ripple.federation_inline_max_rows = 10000;
