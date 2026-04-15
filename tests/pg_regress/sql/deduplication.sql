-- deduplication.sql — Explicit dedup functions and merge-time dedup tests
--
-- Covers:
--   1. deduplicate_predicate() — removes duplicates from a single predicate VP table
--   2. deduplicate_all() — deduplicates all predicates
--   3. dedup_on_merge — verify no duplicates after merge when GUC is enabled
--   4. vp_rare deduplication via deduplicate_predicate and deduplicate_all
--
-- Uses unique IRIs (<http://dedup.test/…>) to avoid interference with other tests.
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.

SET search_path TO pg_ripple, public;

-- ── 1. Insert duplicate triples (same s, p, o, g) ────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
) > 0 AS dup1_ok;

SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
) > 0 AS dup2_ok;

SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
) > 0 AS dup3_ok;

-- Before dedup: 3 rows visible (duplicates present in delta).
SELECT count(*) AS count_before_dedup
FROM pg_ripple.find_triples(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
);

-- ── 2. deduplicate_predicate() ────────────────────────────────────────────────
-- Returns the number of rows removed (should be 2: keep 1 of 3).
SELECT pg_ripple.deduplicate_predicate('<http://dedup.test/P1>') AS rows_removed;

-- After dedup: exactly 1 row remains.
SELECT count(*) AS count_after_dedup
FROM pg_ripple.find_triples(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
);

-- ── 3. Idempotency — second call removes 0 rows ───────────────────────────────
SELECT pg_ripple.deduplicate_predicate('<http://dedup.test/P1>') AS idempotent_run;

-- Triple is still present.
SELECT count(*) AS count_still_1
FROM pg_ripple.find_triples(
    '<http://dedup.test/S1>',
    '<http://dedup.test/P1>',
    '<http://dedup.test/O1>'
);

-- ── 4. deduplicate_all() ──────────────────────────────────────────────────────
-- Insert duplicates on a different predicate (will stay rare if below threshold).
SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S2>',
    '<http://dedup.test/P2>',
    '<http://dedup.test/O2>'
) > 0 AS rare_dup1;
SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S2>',
    '<http://dedup.test/P2>',
    '<http://dedup.test/O2>'
) > 0 AS rare_dup2;

-- deduplicate_all removes dupes across all predicates including rare ones.
SELECT pg_ripple.deduplicate_all() >= 1 AS dedup_all_removed_some;

-- Exactly 1 copy of the rare triple remains.
SELECT count(*) AS rare_dedup_count
FROM pg_ripple.find_triples(
    '<http://dedup.test/S2>',
    '<http://dedup.test/P2>',
    '<http://dedup.test/O2>'
);

-- ── 5. Merge-time dedup (dedup_on_merge = on) ────────────────────────────────
-- Insert duplicates into a new predicate, compact with dedup enabled,
-- confirm only 1 row survives in main.
-- Set threshold = 1 so P3 immediately gets a dedicated HTAP VP table.
SET pg_ripple.vp_promotion_threshold = 1;
SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S3>',
    '<http://dedup.test/P3>',
    '<http://dedup.test/O3>'
) > 0 AS merge_dup1;
SELECT pg_ripple.insert_triple(
    '<http://dedup.test/S3>',
    '<http://dedup.test/P3>',
    '<http://dedup.test/O3>'
) > 0 AS merge_dup2;

-- Enable dedup_on_merge and compact.
SET pg_ripple.dedup_on_merge = true;
SELECT pg_ripple.compact() >= 0 AS compact_with_dedup_ok;
RESET pg_ripple.dedup_on_merge;
RESET pg_ripple.vp_promotion_threshold;

-- After merge+dedup: exactly 1 copy in main.
SELECT count(*) AS count_after_merge_dedup
FROM pg_ripple.find_triples(
    '<http://dedup.test/S3>',
    '<http://dedup.test/P3>',
    '<http://dedup.test/O3>'
);
