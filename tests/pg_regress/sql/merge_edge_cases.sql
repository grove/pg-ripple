-- merge_edge_cases.sql — Merge worker edge-case regression tests
--
-- These tests verify correctness for scenarios that could silently corrupt data:
--
--  1. compact() on a store is non-negative and does not crash (no-op when empty delta)
--  2. compact() is idempotent — calling it twice gives the same result
--  3. Inserting after compact() goes to delta; visible immediately via the view
--  4. Deleting a delta-resident triple (before compact) removes it directly
--     (no tombstone needed — the row is in delta only)
--  5. Deleting a non-existent triple returns 0 (no crash)
--  6. Multiple compacts on the same predicate do not multiply rows
--
-- NOTE: setup.sql always runs first (pgrx special handling) which installs the
-- extension via CREATE EXTENSION pg_ripple.
-- Uses unique IRIs (<http://edge.test/…>) to avoid interference with other tests.

SET search_path TO pg_ripple, public;

-- ── 1. compact() does not crash and returns non-negative ───────────────────────
SELECT pg_ripple.compact() >= 0 AS compact_nonneg_ok;

-- ── 2. Insert one triple, compact, count ──────────────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://edge.test/S1>',
    '<http://edge.test/P1>',
    '<http://edge.test/O1>'
) > 0 AS insert_ok;

SELECT count(*) = 1 AS visible_before_compact
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);

SELECT pg_ripple.compact() >= 0 AS compact1_ok;

SELECT count(*) = 1 AS visible_after_compact
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);

-- ── 3. compact() is idempotent ─────────────────────────────────────────────────
SELECT pg_ripple.compact() >= 0 AS compact2_ok;

SELECT count(*) = 1 AS visible_after_second_compact
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);

-- ── 4. Insert after compact goes to delta; visible immediately ─────────────────
SELECT pg_ripple.insert_triple(
    '<http://edge.test/S2>',
    '<http://edge.test/P1>',
    '<http://edge.test/O2>'
) > 0 AS insert_after_compact_ok;

SELECT count(*) = 2 AS two_triples_visible
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);

-- ── 5. Delete delta-resident triple (no tombstone needed) ─────────────────────
SELECT pg_ripple.delete_triple(
    '<http://edge.test/S2>',
    '<http://edge.test/P1>',
    '<http://edge.test/O2>'
) = 1 AS delete_delta_row_ok;

SELECT count(*) = 1 AS back_to_one
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);

-- ── 6. Delete non-existent triple returns 0 ────────────────────────────────────
SELECT pg_ripple.delete_triple(
    '<http://edge.test/NOSUCH>',
    '<http://edge.test/P1>',
    '<http://edge.test/NOSUCH>'
) = 0 AS delete_nonexistent_ok;

-- ── 7. Multiple compacts do not multiply rows ──────────────────────────────────
SELECT pg_ripple.compact() >= 0 AS compact3_ok;
SELECT pg_ripple.compact() >= 0 AS compact4_ok;

SELECT count(*) = 1 AS still_one_triple
FROM pg_ripple.find_triples(NULL, '<http://edge.test/P1>', NULL);
