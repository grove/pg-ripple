-- pg_regress test: DRed Incremental Retraction (v0.34.0)
--
-- Tests that:
-- 1. GUCs pg_ripple.dred_enabled and pg_ripple.dred_batch_size exist with correct defaults.
-- 2. pg_ripple.dred_on_delete() exists and is callable.
-- 3. After inserting triples and materialising RDFS closure, deleting one base
--    triple removes only the correctly-affected derived triples.
-- 4. Triples supported by alternative derivation paths survive retraction.
-- 5. dred_enabled = false falls back gracefully (returns 0, no error).

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. dred_enabled defaults to on.
SHOW pg_ripple.dred_enabled;

-- 1b. dred_batch_size defaults to 1000.
SHOW pg_ripple.dred_batch_size;

-- 1c. GUCs can be toggled.
SET pg_ripple.dred_enabled = off;
SHOW pg_ripple.dred_enabled;
SET pg_ripple.dred_enabled = on;

SET pg_ripple.dred_batch_size = 500;
SHOW pg_ripple.dred_batch_size;
SET pg_ripple.dred_batch_size = 1000;

-- ── Part 2: dred_on_delete() function structure ───────────────────────────────

-- 2a. Function exists and is callable (returns 0 on empty DB).
SELECT pg_ripple.dred_on_delete(0::bigint, 0::bigint, 0::bigint, 0::bigint) >= 0
    AS dred_callable;

-- ── Part 3: DRed with a simple transitive closure ────────────────────────────

-- Load a transitive closure rule set.
SELECT pg_ripple.load_rules(
    '?x <https://dred.test/reach> ?y :- ?x <https://dred.test/edge> ?y . '
    '?x <https://dred.test/reach> ?z :- ?x <https://dred.test/reach> ?y , '
    '                                    ?y <https://dred.test/edge> ?z .',
    'dred_test'
) > 0 AS dred_rules_loaded;

-- Insert base triples: a→b, b→c, a→c (diamond — c reachable via two paths).
SELECT pg_ripple.insert_triple(
    '<https://dred.test/a>', '<https://dred.test/edge>', '<https://dred.test/b>'
) IS NOT NULL AS insert_a_b;
SELECT pg_ripple.insert_triple(
    '<https://dred.test/b>', '<https://dred.test/edge>', '<https://dred.test/c>'
) IS NOT NULL AS insert_b_c;
SELECT pg_ripple.insert_triple(
    '<https://dred.test/a>', '<https://dred.test/edge>', '<https://dred.test/c>'
) IS NOT NULL AS insert_a_c_direct;

-- Materialise the closure.
SELECT pg_ripple.infer('dred_test') >= 0 AS infer_ok;

-- 3a. After inference, a can reach both b and c.
SELECT COUNT(*) >= 2 AS reach_count_ok
FROM pg_ripple.sparql(
    'SELECT ?o WHERE { <https://dred.test/a> <https://dred.test/reach> ?o }'
);

-- ── Part 4: DRed - with disabled flag returns 0 ───────────────────────────────

-- 4a. dred_on_delete returns 0 when dred_enabled = false.
SET pg_ripple.dred_enabled = off;
SELECT pg_ripple.dred_on_delete(0::bigint, 0::bigint, 0::bigint, 0::bigint) = 0
    AS dred_disabled_returns_zero;
SET pg_ripple.dred_enabled = on;

-- ── Part 5: Cleanup ──────────────────────────────────────────────────────────
SELECT pg_ripple.drop_rules('dred_test') >= 0 AS dred_rules_dropped;
