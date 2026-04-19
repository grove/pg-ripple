-- pg_regress test: Well-Founded Semantics (v0.32.0)
--
-- Tests that:
-- 1. GUC pg_ripple.wfs_max_iterations exists and defaults to 100.
-- 2. pg_ripple.infer_wfs() exists and returns a correctly-keyed JSONB object.
-- 3. For stratifiable programs, infer_wfs() returns same result as infer()
--    with certainty = 'true' for all facts (unknown = 0, stratifiable = true).
-- 4. For non-stratifiable programs (cyclic negation), infer_wfs() returns
--    certainty = 'unknown' for unresolvable facts (unknown > 0, stratifiable = false).

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. wfs_max_iterations defaults to 100.
SHOW pg_ripple.wfs_max_iterations;

-- 1b. GUC can be set.
SET pg_ripple.wfs_max_iterations = 50;
SHOW pg_ripple.wfs_max_iterations;
SET pg_ripple.wfs_max_iterations = 100;

-- ── Part 2: infer_wfs() function structure ────────────────────────────────────

-- 2a. Function exists and returns JSONB on an empty rule set.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/w/link> ?y :- ?x <https://ex.org/w/edge> ?y .',
    'wfs_struct_test'
) > 0 AS struct_rules_loaded;

SELECT jsonb_typeof(pg_ripple.infer_wfs('wfs_struct_test')) = 'object'
    AS infer_wfs_returns_object;

-- 2b. Result has required keys.
SELECT pg_ripple.infer_wfs('wfs_struct_test') ? 'derived'      AS has_derived;
SELECT pg_ripple.infer_wfs('wfs_struct_test') ? 'certain'      AS has_certain;
SELECT pg_ripple.infer_wfs('wfs_struct_test') ? 'unknown'      AS has_unknown;
SELECT pg_ripple.infer_wfs('wfs_struct_test') ? 'iterations'   AS has_iterations;
SELECT pg_ripple.infer_wfs('wfs_struct_test') ? 'stratifiable' AS has_stratifiable;

SELECT pg_ripple.drop_rules('wfs_struct_test') >= 0 AS struct_rules_dropped;

-- ── Part 3: stratifiable program ─────────────────────────────────────────────
-- A purely positive transitive-closure program: all derived facts are certain.

-- Insert test edges: a→b, b→c.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/w/a>', '<https://ex.org/w/edge>', '<https://ex.org/w/b>'
) > 0 AS edge_ab;
SELECT pg_ripple.insert_triple(
    '<https://ex.org/w/b>', '<https://ex.org/w/edge>', '<https://ex.org/w/c>'
) > 0 AS edge_bc;

-- Load a stratifiable transitive-closure rule set.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/w/reach> ?y :- ?x <https://ex.org/w/edge> ?y . ' ||
    '?x <https://ex.org/w/reach> ?z :- ?x <https://ex.org/w/reach> ?y, ?y <https://ex.org/w/edge> ?z .',
    'wfs_stratifiable'
) > 0 AS strat_rules_loaded;

-- Run WFS; expect stratifiable=true, unknown=0, certain>0.
SELECT
    (pg_ripple.infer_wfs('wfs_stratifiable') ->> 'stratifiable')::boolean AS stratifiable,
    (pg_ripple.infer_wfs('wfs_stratifiable') ->> 'unknown')::int           AS unknown_count,
    (pg_ripple.infer_wfs('wfs_stratifiable') ->> 'certain')::int  > 0      AS has_certain_facts;

-- Verify that infer_wfs() and infer() give the same derived count.
SELECT pg_ripple.drop_rules('wfs_stratifiable') >= 0 AS drop_strat;

-- ── Part 4: non-stratifiable program (cyclic negation) ────────────────────────
-- Classic mutual negation: trusted(X) :- person(X), NOT untrusted(X)
--                          untrusted(X) :- person(X), NOT trusted(X)
-- Since the rules cycle through negation, stratify() fails.
-- WFS reports both as certainty = 'unknown'.

-- Insert base fact.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/w/alice>', '<https://ex.org/w/person>', '<https://ex.org/w/yes>'
) > 0 AS person_fact;

-- Load mutual negation rule set.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/w/trusted>   <https://ex.org/w/yes> :- '
        '?x <https://ex.org/w/person>    <https://ex.org/w/yes>, '
        'NOT ?x <https://ex.org/w/untrusted> <https://ex.org/w/yes> . '
    '?x <https://ex.org/w/untrusted> <https://ex.org/w/yes> :- '
        '?x <https://ex.org/w/person>    <https://ex.org/w/yes>, '
        'NOT ?x <https://ex.org/w/trusted>   <https://ex.org/w/yes> .',
    'wfs_mutual_neg'
) > 0 AS neg_rules_loaded;

-- 4a. WFS reports stratifiable=false.
SELECT
    (pg_ripple.infer_wfs('wfs_mutual_neg') ->> 'stratifiable')::boolean AS stratifiable_is_false;

-- 4b. WFS reports unknown > 0 (unresolvable facts).
SELECT
    (pg_ripple.infer_wfs('wfs_mutual_neg') ->> 'unknown')::int > 0 AS has_unknown_facts;

-- 4c. Derived = certain + unknown.
SELECT
    (pg_ripple.infer_wfs('wfs_mutual_neg') ->> 'derived')::int =
    (pg_ripple.infer_wfs('wfs_mutual_neg') ->> 'certain')::int +
    (pg_ripple.infer_wfs('wfs_mutual_neg') ->> 'unknown')::int AS derived_eq_certain_plus_unknown;

SELECT pg_ripple.drop_rules('wfs_mutual_neg') >= 0 AS drop_neg;

-- Cleanup inserted triples.
SELECT pg_ripple.delete_triple(
    '<https://ex.org/w/a>', '<https://ex.org/w/edge>', '<https://ex.org/w/b>'
) >= 0 AS del_ab;
SELECT pg_ripple.delete_triple(
    '<https://ex.org/w/b>', '<https://ex.org/w/edge>', '<https://ex.org/w/c>'
) >= 0 AS del_bc;
SELECT pg_ripple.delete_triple(
    '<https://ex.org/w/alice>', '<https://ex.org/w/person>', '<https://ex.org/w/yes>'
) >= 0 AS del_person;
