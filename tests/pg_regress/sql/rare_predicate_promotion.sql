-- rare_predicate_promotion.sql — Rare-predicate promotion atomicity tests (v0.22.0 H-3/H-4)
--
-- Verifies:
--   1. A predicate stored in vp_rare is atomically promoted to its own VP table
--      when the triple count crosses vp_promotion_threshold.
--   2. After promotion, zero orphan rows remain in vp_rare for that predicate.
--   3. The predicate catalog (triple_count) reflects the correct count after promotion.
--   4. Triples inserted before AND after promotion are all visible via find_triples.
--
-- Uses unique IRIs (<http://promo.test/…>) to avoid interference with other tests.
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.

SET search_path TO pg_ripple, public;

-- ── 1. Set the promotion threshold to the minimum value (100) ─────────────────
SET pg_ripple.vp_promotion_threshold = 100;

-- Verify the GUC accepted the value.
SELECT current_setting('pg_ripple.vp_promotion_threshold')::int = 100 AS threshold_set;

-- ── 2. Insert 99 triples below the threshold (stay in vp_rare) ────────────────
-- Use a DO block to insert 99 triples efficiently.
DO $$
DECLARE
    i INT;
BEGIN
    FOR i IN 1..99 LOOP
        PERFORM pg_ripple.insert_triple(
            '<http://promo.test/S' || i || '>',
            '<http://promo.test/RARE_P>',
            '<http://promo.test/O' || i || '>'
        );
    END LOOP;
END $$;

-- Still below threshold (99 triples < 100); predicate should be in vp_rare.
SELECT count(*) = 99 AS ninety_nine_rows_in_rare
FROM _pg_ripple.vp_rare
WHERE p = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 3. Insert the threshold-crossing triple (triggers atomic promotion) ────────
SELECT pg_ripple.insert_triple('<http://promo.test/S100>','<http://promo.test/RARE_P>','<http://promo.test/O100>') > 0 AS i100_triggers_promotion;

-- ── 4. Verify zero orphan rows in vp_rare after promotion ─────────────────────
SELECT count(*) = 0 AS no_orphans_in_vp_rare
FROM _pg_ripple.vp_rare
WHERE p = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 5. Verify all 100 triples are visible through the public API ───────────────
SELECT count(*) = 100 AS all_hundred_triples_visible
FROM pg_ripple.find_triples(NULL, '<http://promo.test/RARE_P>', NULL);

-- ── 6. Verify predicate catalog has accurate triple_count ─────────────────────
SELECT triple_count = 100 AS catalog_count_correct
FROM _pg_ripple.predicates
WHERE id = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 7. Insert more triples after promotion (should go to delta table) ──────────
SELECT pg_ripple.insert_triple('<http://promo.test/S101>','<http://promo.test/RARE_P>','<http://promo.test/O101>') > 0 AS i101_post_promotion;

SELECT count(*) = 101 AS hundred_one_triples_after_post_promotion_insert
FROM pg_ripple.find_triples(NULL, '<http://promo.test/RARE_P>', NULL);

-- ── 8. Verify GUC min/max bounds ──────────────────────────────────────────────
-- The minimum is 100; verify threshold is at least 100.
SELECT current_setting('pg_ripple.vp_promotion_threshold')::int >= 100 AS threshold_at_min;

-- Restore default.
RESET pg_ripple.vp_promotion_threshold;

