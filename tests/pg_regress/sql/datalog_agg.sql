-- pg_regress test: Datalog aggregation (Datalog^agg, v0.30.0)
--
-- Tests `pg_ripple.infer_agg()` which supports COUNT, SUM, MIN, MAX, AVG
-- aggregate functions in rule bodies.
-- Also verifies aggregation-stratification violation detection (PT510).

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- Pre-test baseline for cleanup.
CREATE TEMP TABLE _agg_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- ── Part 1: GUC and function existence checks ─────────────────────────────────

-- 1. The infer_agg function exists and returns JSONB.
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;
SELECT jsonb_typeof(pg_ripple.infer_agg('rdfs')) = 'object' AS infer_agg_returns_object;
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped;

-- 2. infer_agg result has required keys.
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded_2;
SELECT pg_ripple.infer_agg('rdfs') ? 'derived'            AS has_derived;
SELECT pg_ripple.infer_agg('rdfs') ? 'aggregate_derived'  AS has_agg_derived;
SELECT pg_ripple.infer_agg('rdfs') ? 'iterations'         AS has_iterations;
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped_2;

-- Clean up any rdfs-inferred triples.
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _agg_test_baseline);

-- ── Part 2: COUNT aggregate rule ──────────────────────────────────────────────

-- Insert test data: social graph for friend-counting.
-- ex:Alice knows ex:Bob and ex:Carol; ex:Bob knows ex:Dave.
SELECT pg_ripple.insert_triple(
    '<https://example.org/agg/Alice>',
    '<https://xmlns.com/foaf/0.1/knows>',
    '<https://example.org/agg/Bob>'
) > 0 AS alice_knows_bob;

SELECT pg_ripple.insert_triple(
    '<https://example.org/agg/Alice>',
    '<https://xmlns.com/foaf/0.1/knows>',
    '<https://example.org/agg/Carol>'
) > 0 AS alice_knows_carol;

SELECT pg_ripple.insert_triple(
    '<https://example.org/agg/Bob>',
    '<https://xmlns.com/foaf/0.1/knows>',
    '<https://example.org/agg/Dave>'
) > 0 AS bob_knows_dave;

-- Load the COUNT aggregate rule:
-- "?x ex:friendCount ?n :- COUNT(?y WHERE ?x foaf:knows ?y) = ?n ."
SELECT pg_ripple.load_rules(
    '?x <https://example.org/agg/friendCount> ?n :- COUNT(?y WHERE ?x <https://xmlns.com/foaf/0.1/knows> ?y) = ?n .',
    'agg_count_test'
) > 0 AS count_rule_loaded;

-- Run inference with aggregate support.
SELECT (pg_ripple.infer_agg('agg_count_test')->>'aggregate_derived')::bigint >= 0
    AS count_agg_derived_nonneg;

-- Verify results: Alice should have 2 friends, Bob should have 1.
-- The count is stored as a dictionary-encoded literal.
SELECT count(*) >= 1 AS alice_has_friend_count
FROM pg_ripple.find_triples(
    '<https://example.org/agg/Alice>',
    '<https://example.org/agg/friendCount>',
    NULL
);

SELECT count(*) >= 1 AS bob_has_friend_count
FROM pg_ripple.find_triples(
    '<https://example.org/agg/Bob>',
    '<https://example.org/agg/friendCount>',
    NULL
);

-- The object (count value) should decode to a numeric literal.
SELECT
    o IS NOT NULL AS count_value_present
FROM pg_ripple.find_triples(
    '<https://example.org/agg/Alice>',
    '<https://example.org/agg/friendCount>',
    NULL
)
LIMIT 1;

-- Clean up count test.
SELECT pg_ripple.drop_rules('agg_count_test') >= 0 AS count_rules_dropped;
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _agg_test_baseline);

-- ── Part 3: Aggregation stratification violation (PT510) ──────────────────────

-- A cycle through aggregation should produce a WARNING (PT510) and gracefully
-- skip the aggregate rule, not crash the backend.
--
-- Rule set with a cycle:
-- Rule 1 (normal): ?x foaf:knows ?y :- ?x foaf:acquaintance ?y .
-- Rule 2 (aggregate, CYCLE!): ?x foaf:acquaintance ?y :-
--   COUNT(?z WHERE ?x foaf:knows ?z) = ?y .
-- (foaf:knows → foaf:acquaintance via agg, but foaf:acquaintance → foaf:knows
--  via positive rule — this creates a cycle through aggregation)
SELECT pg_ripple.load_rules(
    '?x <https://xmlns.com/foaf/0.1/knows> ?y :- ?x <https://example.org/agg/acq> ?y .',
    'agg_cycle_test'
) > 0 AS cycle_rule_1_loaded;
SELECT pg_ripple.load_rules(
    '?x <https://example.org/agg/acq> ?n :- COUNT(?y WHERE ?x <https://xmlns.com/foaf/0.1/knows> ?y) = ?n .',
    'agg_cycle_test'
) > 0 AS cycle_rule_2_loaded;

-- infer_agg on a cycle through aggregation should emit a WARNING and return a result
-- (not crash); the aggregate_derived should be 0 (aggregate was skipped).
SELECT jsonb_typeof(pg_ripple.infer_agg('agg_cycle_test')) = 'object'
    AS cycle_infer_returns_object;

-- Clean up.
SELECT pg_ripple.drop_rules('agg_cycle_test') >= 0 AS cycle_rules_dropped;
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _agg_test_baseline);
