-- pg_regress test: Magic sets goal-directed inference (v0.29.0)
--
-- Tests `pg_ripple.infer_goal(rule_set, goal)` which uses a simplified magic
-- sets transformation to derive only facts relevant to a goal triple pattern.
-- Also verifies the `pg_ripple.magic_sets` GUC.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- Pre-test baseline: record max(i) in vp_rare for cleanup.
CREATE TEMP TABLE _mg_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- 1. GUC pg_ripple.magic_sets exists and has a default value.
SHOW pg_ripple.magic_sets;

-- 2. The infer_goal function exists and returns a JSONB object.
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;

SELECT jsonb_typeof(pg_ripple.infer_goal('rdfs', '?x ?p ?y')) = 'object'
    AS result_is_object;

-- 3. JSONB result contains required keys.
SELECT pg_ripple.infer_goal('rdfs', '?x ?p ?y') ? 'derived'    AS has_derived;
SELECT pg_ripple.infer_goal('rdfs', '?x ?p ?y') ? 'iterations' AS has_iterations;
SELECT pg_ripple.infer_goal('rdfs', '?x ?p ?y') ? 'matching'   AS has_matching;

-- 4. matching count is non-negative.
SELECT (pg_ripple.infer_goal('rdfs', '?x ?p ?y')->>'matching')::bigint >= 0
    AS matching_nonneg;

-- 5. magic_sets GUC can be disabled for fallback mode.
SET pg_ripple.magic_sets = false;
SHOW pg_ripple.magic_sets;
SELECT jsonb_typeof(pg_ripple.infer_goal('rdfs', '?x ?p ?y')) = 'object'
    AS fallback_result_is_object;
-- Restore default.
SET pg_ripple.magic_sets = true;

-- 6. Verify no orphaned magic temp tables remain after infer_goal.
--    Magic temp tables use a naming pattern _magic_*.
SELECT count(*) = 0 AS no_magic_temp_tables
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind = 'r'
  AND n.nspname = 'pg_temp'
  AND c.relname LIKE '_magic_%';

-- 7. Cleanup.
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _mg_test_baseline);
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped;
