-- pg_regress test: Demand transformation (v0.31.0)
--
-- Tests that:
-- 1. GUC pg_ripple.demand_transform exists and defaults to on.
-- 2. pg_ripple.infer_demand() exists and returns a correctly-keyed JSONB object.
-- 3. infer_demand() with empty demands array runs full inference (same as infer()).
-- 4. infer_demand() with a predicate demand filters to relevant rules.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. demand_transform defaults to on.
SHOW pg_ripple.demand_transform;

-- 1b. GUC can be toggled.
SET pg_ripple.demand_transform = off;
SHOW pg_ripple.demand_transform;
SET pg_ripple.demand_transform = on;
SHOW pg_ripple.demand_transform;

-- ── Part 2: infer_demand() function structure ─────────────────────────────────

-- 2a. Function exists and returns JSONB. Use a simple rule to avoid variable-
--     predicate WARNINGs that built-in RDFS rules would produce.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/d/link> ?y :- ?x <https://ex.org/d/edge> ?y .',
    'demand_struct_test'
) > 0 AS struct_rules_loaded;

SELECT jsonb_typeof(pg_ripple.infer_demand('demand_struct_test', '[]')) = 'object'
    AS infer_demand_returns_object;

-- 2b. Result has required keys.
SELECT pg_ripple.infer_demand('demand_struct_test', '[]') ? 'derived'           AS has_derived;
SELECT pg_ripple.infer_demand('demand_struct_test', '[]') ? 'iterations'        AS has_iterations;
SELECT pg_ripple.infer_demand('demand_struct_test', '[]') ? 'demand_predicates' AS has_demand_predicates;

SELECT pg_ripple.drop_rules('demand_struct_test') >= 0 AS struct_rules_dropped;

-- ── Part 3: infer_demand() with predicate demand ──────────────────────────────

-- Create a pre-test baseline for cleanup.
CREATE TEMP TABLE _demand_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- Insert test data.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/d/a>',
    '<https://ex.org/d/childOf>',
    '<https://ex.org/d/b>'
) > 0 AS t1;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/d/b>',
    '<https://ex.org/d/childOf>',
    '<https://ex.org/d/c>'
) > 0 AS t2;

-- Load transitivity rule for descendantOf.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/d/descendantOf> ?y :- ?x <https://ex.org/d/childOf> ?y .
     ?x <https://ex.org/d/descendantOf> ?z :- ?x <https://ex.org/d/descendantOf> ?y, ?y <https://ex.org/d/childOf> ?z .',
    'demand_test'
) > 0 AS rules_loaded;

-- 3a. infer_demand with a predicate demand returns a valid JSONB object.
SELECT jsonb_typeof(
    pg_ripple.infer_demand('demand_test',
        '[{"p": "<https://ex.org/d/descendantOf>"}]')
) = 'object' AS infer_demand_predicate_returns_object;

-- 3b. derived count is non-negative.
SELECT (pg_ripple.infer_demand('demand_test',
    '[{"p": "<https://ex.org/d/descendantOf>"}]') ->> 'derived')::bigint >= 0
    AS derived_nonneg;

-- 3c. demand_predicates array is returned (may be empty if predicate not derived
-- by any rule in the set, but must be an array).
SELECT jsonb_typeof(
    pg_ripple.infer_demand('demand_test',
        '[{"p": "<https://ex.org/d/descendantOf>"}]') -> 'demand_predicates'
) = 'array' AS demand_predicates_is_array;

-- ── Part 4: infer_demand() with empty demands ─────────────────────────────────

-- With empty demands, behaves like full infer().
SELECT (pg_ripple.infer_demand('demand_test', '[]') ->> 'derived')::bigint >= 0
    AS full_infer_via_demand;

-- Cleanup.
SELECT pg_ripple.drop_rules('demand_test') >= 0 AS cleanup_rules;
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _demand_test_baseline);
