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

-- ── 1. Set the promotion threshold to the minimum value (10) ──────────────────
SET pg_ripple.vp_promotion_threshold = 10;

-- Verify the GUC accepted the value.
SELECT current_setting('pg_ripple.vp_promotion_threshold')::int = 10 AS threshold_set;

-- ── 2. Insert triples below the threshold (stay in vp_rare) ───────────────────
SELECT pg_ripple.insert_triple('<http://promo.test/S1>','<http://promo.test/RARE_P>','<http://promo.test/O1>') > 0 AS i1;
SELECT pg_ripple.insert_triple('<http://promo.test/S2>','<http://promo.test/RARE_P>','<http://promo.test/O2>') > 0 AS i2;
SELECT pg_ripple.insert_triple('<http://promo.test/S3>','<http://promo.test/RARE_P>','<http://promo.test/O3>') > 0 AS i3;
SELECT pg_ripple.insert_triple('<http://promo.test/S4>','<http://promo.test/RARE_P>','<http://promo.test/O4>') > 0 AS i4;
SELECT pg_ripple.insert_triple('<http://promo.test/S5>','<http://promo.test/RARE_P>','<http://promo.test/O5>') > 0 AS i5;
SELECT pg_ripple.insert_triple('<http://promo.test/S6>','<http://promo.test/RARE_P>','<http://promo.test/O6>') > 0 AS i6;
SELECT pg_ripple.insert_triple('<http://promo.test/S7>','<http://promo.test/RARE_P>','<http://promo.test/O7>') > 0 AS i7;
SELECT pg_ripple.insert_triple('<http://promo.test/S8>','<http://promo.test/RARE_P>','<http://promo.test/O8>') > 0 AS i8;
SELECT pg_ripple.insert_triple('<http://promo.test/S9>','<http://promo.test/RARE_P>','<http://promo.test/O9>') > 0 AS i9;

-- Still below threshold (9 triples < 10); predicate should be in vp_rare.
SELECT count(*) = 9 AS nine_rows_in_rare
FROM _pg_ripple.vp_rare
WHERE p = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 3. Insert the threshold-crossing triple (triggers atomic promotion) ────────
SELECT pg_ripple.insert_triple('<http://promo.test/S10>','<http://promo.test/RARE_P>','<http://promo.test/O10>') > 0 AS i10_triggers_promotion;

-- ── 4. Verify zero orphan rows in vp_rare after promotion ─────────────────────
SELECT count(*) = 0 AS no_orphans_in_vp_rare
FROM _pg_ripple.vp_rare
WHERE p = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 5. Verify all 10 triples are visible through the public API ────────────────
SELECT count(*) = 10 AS all_ten_triples_visible
FROM pg_ripple.find_triples(NULL, '<http://promo.test/RARE_P>', NULL);

-- ── 6. Verify predicate catalog has accurate triple_count ─────────────────────
SELECT triple_count = 10 AS catalog_count_correct
FROM _pg_ripple.predicates
WHERE id = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://promo.test/RARE_P' LIMIT 1
);

-- ── 7. Insert more triples after promotion (should go to delta table) ──────────
SELECT pg_ripple.insert_triple('<http://promo.test/S11>','<http://promo.test/RARE_P>','<http://promo.test/O11>') > 0 AS i11_post_promotion;

SELECT count(*) = 11 AS eleven_triples_after_post_promotion_insert
FROM pg_ripple.find_triples(NULL, '<http://promo.test/RARE_P>', NULL);

-- ── 8. Verify GUC min/max bounds ──────────────────────────────────────────────
-- The minimum is 10; verify threshold is at least 10.
SELECT current_setting('pg_ripple.vp_promotion_threshold')::int >= 10 AS threshold_at_min;

-- Restore default.
RESET pg_ripple.vp_promotion_threshold;

