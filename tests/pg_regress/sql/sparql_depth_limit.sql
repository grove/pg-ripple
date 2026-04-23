-- sparql_depth_limit.sql
-- Test SPARQL query complexity limits (PT440) introduced in v0.51.0.
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Test 1: GUCs exist and have expected defaults ─────────────────────────────
SELECT current_setting('pg_ripple.sparql_max_algebra_depth') AS depth_limit;

SELECT current_setting('pg_ripple.sparql_max_triple_patterns') AS pattern_limit;

-- ── Test 2: A simple query succeeds with default limits ───────────────────────
SELECT pg_ripple.insert_triple(
    '<http://t.example.org/a>',
    '<http://t.example.org/p>',
    '<http://t.example.org/b>'
) AS triple_id;

SELECT count(*) AS result_count
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <http://t.example.org/p> ?o }'
);

-- ── Test 3: Disabling limits (0 = unlimited) works ────────────────────────────
SET pg_ripple.sparql_max_algebra_depth = 0;
SET pg_ripple.sparql_max_triple_patterns = 0;

SELECT count(*) AS result_count_unlimited
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <http://t.example.org/p> ?o }'
);

RESET pg_ripple.sparql_max_algebra_depth;
RESET pg_ripple.sparql_max_triple_patterns;

-- ── Test 4: Verify the limits round-trip via SET/SHOW ────────────────────────
SET pg_ripple.sparql_max_algebra_depth = 128;
SELECT current_setting('pg_ripple.sparql_max_algebra_depth') AS custom_depth;
RESET pg_ripple.sparql_max_algebra_depth;


-- ── Test 1: default limits allow normal queries ───────────────────────────────
-- With default limits (algebra_depth=256, triple_patterns=4096) a simple
-- query should succeed without error.
SELECT count(*) AS result_count
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <http://example.org/p> ?o }'
);

-- ── Test 2: lowering algebra depth limit ─────────────────────────────────────
-- Set a very low depth limit so the complexity check fires.
SET pg_ripple.sparql_max_algebra_depth = 1;

-- A simple SELECT with a FILTER (depth 2) should now be rejected.
-- We use DO $$ to catch the expected error.
DO $$
BEGIN
    PERFORM result FROM pg_ripple.sparql(
        'SELECT ?s WHERE { ?s <http://example.org/p> ?o . FILTER(?o != <http://example.org/x>) }'
    );
    RAISE EXCEPTION 'expected PT440 error but query succeeded';
EXCEPTION
    WHEN OTHERS THEN
        IF SQLERRM LIKE '%PT440%' THEN
            RAISE NOTICE 'OK: PT440 error raised as expected';
        ELSE
            RAISE; -- unexpected error
        END IF;
END;
$$;

-- Restore default.
RESET pg_ripple.sparql_max_algebra_depth;

-- ── Test 3: lowering triple-pattern limit ────────────────────────────────────
SET pg_ripple.sparql_max_triple_patterns = 1;

-- A query with 2 triple patterns should be rejected.
DO $$
BEGIN
    PERFORM result FROM pg_ripple.sparql(
        'SELECT ?s ?o WHERE { ?s <http://example.org/p> ?o . ?o <http://example.org/p> ?s }'
    );
    RAISE EXCEPTION 'expected PT440 error but query succeeded';
EXCEPTION
    WHEN OTHERS THEN
        IF SQLERRM LIKE '%PT440%' THEN
            RAISE NOTICE 'OK: PT440 triple-pattern limit fired as expected';
        ELSE
            RAISE;
        END IF;
END;
$$;

-- Restore default.
RESET pg_ripple.sparql_max_triple_patterns;

-- ── Test 4: disabling limits (0 = unlimited) ──────────────────────────────────
SET pg_ripple.sparql_max_algebra_depth = 0;
SET pg_ripple.sparql_max_triple_patterns = 0;

SELECT count(*) AS result_count
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <http://example.org/p> ?o }'
);

RESET pg_ripple.sparql_max_algebra_depth;
RESET pg_ripple.sparql_max_triple_patterns;
