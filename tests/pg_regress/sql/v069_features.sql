-- pg_regress test: v0.69.0 feature gate (v0.70.0 TEST-02)
--   Module restructuring regression guard (v0.69.0)
--   sparql_update / sparql_select still callable after module split
--   construct_pipeline_status() returns JSONB
--   feature_status() has expected coverage

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: Public API stability after module restructuring ──────────────────

-- 1a. sparql_update is callable (regression guard for sparql/mod.rs split).
SELECT pg_ripple.sparql_update('INSERT DATA {}') IS NOT NULL AS sparql_update_callable;

-- 1b. sparql_select is callable.
SELECT count(*) >= 0 AS sparql_select_callable
FROM pg_ripple.sparql_select('SELECT * WHERE { ?s ?p ?o } LIMIT 0'::text);

-- 1c. sparql_ask is callable.
SELECT pg_ripple.sparql_ask('ASK { ?s ?p ?o }') IN (true, false) AS sparql_ask_callable;

-- ── Part 2: Construct pipeline status ────────────────────────────────────────

-- 2a. construct_pipeline_status() is callable and returns JSONB.
SELECT pg_typeof(pg_ripple.construct_pipeline_status()) = 'jsonb'::regtype
    AS pipeline_status_is_jsonb;

-- 2b. After registering a rule, construct_pipeline_status() has at least one entry.
SELECT pg_ripple.create_graph('https://v069test.test/src/') > 0 AS g1;
SELECT pg_ripple.create_graph('https://v069test.test/dst/') > 0 AS g2;
SELECT pg_ripple.create_construct_rule(
    'v069_test_rule',
    'CONSTRUCT { ?s ?p ?o }
     WHERE { GRAPH <https://v069test.test/src/> { ?s ?p ?o } }',
    'https://v069test.test/dst/'
) IS NULL AS rule_registered;

SELECT (pg_ripple.construct_pipeline_status()->'rule_count')::int >= 1
    AS pipeline_has_rule;

-- Cleanup
SELECT pg_ripple.drop_construct_rule('v069_test_rule') AS rule_dropped;
SELECT pg_ripple.clear_graph('https://v069test.test/src/') >= 0 AS g1_cleared;
SELECT pg_ripple.clear_graph('https://v069test.test/dst/') >= 0 AS g2_cleared;
SELECT pg_ripple.drop_graph('https://v069test.test/src/') >= 0 AS g1_dropped;
SELECT pg_ripple.drop_graph('https://v069test.test/dst/') >= 0 AS g2_dropped;

-- ── Part 3: feature_status() major-area coverage ─────────────────────────────

-- 3a. feature_status() returns at least one row per major area (core areas).
SELECT count(DISTINCT feature_name) >= 5 AS has_multiple_features
FROM pg_ripple.feature_status();

-- 3b. Core features are present.
SELECT count(*) = 3 AS core_features_present
FROM pg_ripple.feature_status()
WHERE feature_name IN ('sparql_select', 'sparql_update', 'sparql_construct');
