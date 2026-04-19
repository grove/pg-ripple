-- pg_regress test: Subsumption checking in infer_with_stats (v0.29.0)
--
-- Tests that `pg_ripple.infer_with_stats()` returns an `eliminated_rules`
-- JSONB key.  When rules are loaded, subsumption checking removes redundant
-- rules whose body predicate set is a superset of another rule's body predicates.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;

SET search_path TO pg_ripple, public;

-- Pre-test baseline.
CREATE TEMP TABLE _ss_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- 1. infer_with_stats returns a JSONB object with the eliminated_rules key.
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;

SELECT jsonb_typeof(pg_ripple.infer_with_stats('rdfs')) = 'object'
    AS result_is_object;

-- 2. eliminated_rules key is present (may be empty array).
SELECT pg_ripple.infer_with_stats('rdfs') ? 'eliminated_rules'
    AS has_eliminated_rules;

-- 3. eliminated_rules is a JSON array.
SELECT jsonb_typeof(pg_ripple.infer_with_stats('rdfs')->'eliminated_rules') = 'array'
    AS eliminated_is_array;

-- 4. Standard keys still present after v0.29.0 change.
SELECT pg_ripple.infer_with_stats('rdfs') ? 'derived'    AS has_derived;
SELECT pg_ripple.infer_with_stats('rdfs') ? 'iterations' AS has_iterations;

-- 5. Cleanup.
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped;
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _ss_test_baseline);
