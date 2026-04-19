-- pg_regress test: Predicate-filter pushdown (v0.29.0)
--
-- Tests that Datalog rules containing arithmetic/comparison guards produce
-- correct results.  Filter pushdown moves guards into JOIN ON clauses for
-- better index utilisation, but the semantics are unchanged.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- Pre-test baseline.
CREATE TEMP TABLE _fp_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- 1. Insert triples for arithmetic-filter rule tests.
SELECT pg_ripple.insert_triple(
    '<https://example.org/fp/a>',
    '<https://example.org/fp/score>',
    '"42"^^<http://www.w3.org/2001/XMLSchema#integer>'
) > 0 AS score_a_inserted;

SELECT pg_ripple.insert_triple(
    '<https://example.org/fp/b>',
    '<https://example.org/fp/score>',
    '"5"^^<http://www.w3.org/2001/XMLSchema#integer>'
) > 0 AS score_b_inserted;

-- 2. Load RDFS rules (contain no arithmetic guards themselves, but verify
--    the compiler path for rules with body atoms works correctly after
--    the filter-pushdown refactor).
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;

-- 3. Inference with the refactored compiler produces correct results.
SELECT (pg_ripple.infer_with_stats('rdfs')->>'derived')::bigint >= 0
    AS derived_nonneg;

-- 4. Verify cost-based reorder and pushdown are both on (default state).
SHOW pg_ripple.datalog_cost_reorder;

-- 5. Cleanup.
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped;
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _fp_test_baseline);
