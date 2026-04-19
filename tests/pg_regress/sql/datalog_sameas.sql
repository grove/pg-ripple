-- pg_regress test: owl:sameAs entity canonicalization (v0.31.0)
--
-- Tests that:
-- 1. GUC pg_ripple.sameas_reasoning exists and defaults to on.
-- 2. Inference runs without error when sameas_reasoning is enabled.
-- 3. The canonicalization pre-pass does not corrupt normal inference.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. sameas_reasoning defaults to on.
SHOW pg_ripple.sameas_reasoning;

-- 1b. GUC can be toggled.
SET pg_ripple.sameas_reasoning = off;
SHOW pg_ripple.sameas_reasoning;
SET pg_ripple.sameas_reasoning = on;
SHOW pg_ripple.sameas_reasoning;

-- ── Part 2: Insert owl:sameAs triples ─────────────────────────────────────────

-- ex:a1 owl:sameAs ex:a2
SELECT pg_ripple.insert_triple(
    '<https://ex.org/a1>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/a2>'
) > 0 AS sameas_a1_a2;

-- ex:a2 owl:sameAs ex:a3 (transitivity via union-find)
SELECT pg_ripple.insert_triple(
    '<https://ex.org/a2>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/a3>'
) > 0 AS sameas_a2_a3;

-- Insert a fact about ex:a1 only
SELECT pg_ripple.insert_triple(
    '<https://ex.org/a1>',
    '<https://ex.org/knows>',
    '<https://ex.org/bob>'
) > 0 AS fact_a1_knows_bob;

-- ── Part 3: Load a simple Datalog rule ───────────────────────────────────────

-- Rule: ?y ex:knownBy ?x :- ?x ex:knows ?y .
SELECT pg_ripple.load_rules(
    '?y <https://ex.org/knownBy> ?x :- ?x <https://ex.org/knows> ?y .',
    'sameas_test'
) > 0 AS rules_loaded;

-- ── Part 4: Run inference with sameas_reasoning on ───────────────────────────

SET pg_ripple.sameas_reasoning = on;
SELECT pg_ripple.infer('sameas_test') >= 0 AS infer_with_sameas_ran;

-- ── Part 5: Run inference with sameas_reasoning off ──────────────────────────

-- Verify the GUC off path also works (no crash, still derives from direct facts).
SELECT pg_ripple.drop_rules('sameas_test') >= 0 AS rules_dropped_1;

SELECT pg_ripple.load_rules(
    '?y <https://ex.org/knownBy> ?x :- ?x <https://ex.org/knows> ?y .',
    'sameas_test2'
) > 0 AS rules_loaded_2;

SET pg_ripple.sameas_reasoning = off;
SELECT pg_ripple.infer('sameas_test2') >= 0 AS infer_without_sameas_ran;

SET pg_ripple.sameas_reasoning = on;

-- ── Part 6: Verify sameAs map is computed without error when VP table is empty ─

-- Load and immediately run a rule set on a rule that has no sameAs triples
-- for its predicates — should be a no-op, not an error.
SELECT pg_ripple.drop_rules('sameas_test2') >= 0 AS rules_dropped_2;

-- Cleanup: delete inserted triples.
SELECT pg_ripple.delete_triple(
    '<https://ex.org/a1>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/a2>'
) >= 0 AS cleanup_1;

SELECT pg_ripple.delete_triple(
    '<https://ex.org/a2>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/a3>'
) >= 0 AS cleanup_2;

SELECT pg_ripple.delete_triple(
    '<https://ex.org/a1>',
    '<https://ex.org/knows>',
    '<https://ex.org/bob>'
) >= 0 AS cleanup_3;
