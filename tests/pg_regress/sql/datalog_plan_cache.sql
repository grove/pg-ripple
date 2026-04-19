-- pg_regress test: Datalog rule plan cache (v0.30.0)
--
-- Tests `pg_ripple.rule_plan_cache_stats()` and verifies that:
-- 1. GUCs pg_ripple.rule_plan_cache and pg_ripple.rule_plan_cache_size exist.
-- 2. rule_plan_cache_stats() returns rows after inference.
-- 3. Cache is invalidated (entry disappears) after drop_rules().
--
-- The plan cache is populated by infer_agg() (aggregate SQL compiled once
-- and reused on subsequent calls).

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. rule_plan_cache defaults to on.
SHOW pg_ripple.rule_plan_cache;

-- 1b. rule_plan_cache_size defaults to 64.
SHOW pg_ripple.rule_plan_cache_size;

-- 1c. Both GUCs can be set.
SET pg_ripple.rule_plan_cache = false;
SHOW pg_ripple.rule_plan_cache;
SET pg_ripple.rule_plan_cache = true;
SHOW pg_ripple.rule_plan_cache;

SET pg_ripple.rule_plan_cache_size = 32;
SHOW pg_ripple.rule_plan_cache_size;
SET pg_ripple.rule_plan_cache_size = 64;
SHOW pg_ripple.rule_plan_cache_size;

-- ── Part 2: rule_plan_cache_stats() ──────────────────────────────────────────

-- 2a. Function exists and returns a well-formed result set (may be empty at start).
SELECT count(*) >= 0 AS stats_callable
FROM pg_ripple.rule_plan_cache_stats();

-- When cache is empty (no inference run yet for 'pc_test'), no rows for that rule_set.
SELECT count(*) = 0 AS no_cache_entry_before_infer
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'pc_test';

-- ── Part 3: Cache populated after infer_agg() ────────────────────────────────

-- Create a pre-test baseline for cleanup.
CREATE TEMP TABLE _pc_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- Load a COUNT aggregate rule.
SELECT pg_ripple.load_rules(
    '?x <https://example.org/pc/friendCount> ?n :- COUNT(?y WHERE ?x <https://example.org/pc/knows> ?y) = ?n .',
    'pc_test'
) > 0 AS rule_loaded;

-- Insert base triples.
SELECT pg_ripple.insert_triple(
    '<https://example.org/pc/Alice>',
    '<https://example.org/pc/knows>',
    '<https://example.org/pc/Bob>'
) > 0 AS triple_1;

SELECT pg_ripple.insert_triple(
    '<https://example.org/pc/Alice>',
    '<https://example.org/pc/knows>',
    '<https://example.org/pc/Carol>'
) > 0 AS triple_2;

-- First infer_agg() run: populates the aggregate cache (miss).
SELECT (pg_ripple.infer_agg('pc_test')->>'aggregate_derived')::bigint >= 0
    AS first_infer_agg_ok;

-- Second infer_agg() run: should hit the aggregate cache.
SELECT (pg_ripple.infer_agg('pc_test')->>'aggregate_derived')::bigint >= 0
    AS second_infer_agg_ok;

-- Cache entry should now exist for 'pc_test'.
SELECT count(*) >= 1 AS cache_entry_exists
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'pc_test';

-- After second run, hits should be >= 1 (first run is a miss; second is a hit).
SELECT hits >= 1 AS has_cache_hits
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'pc_test';

-- ── Part 4: Cache invalidation via drop_rules() ───────────────────────────────

SELECT pg_ripple.drop_rules('pc_test') >= 0 AS rules_dropped;

-- After drop_rules, the cache entry for 'pc_test' should be gone.
SELECT count(*) = 0 AS cache_cleared_after_drop
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'pc_test';

-- ── Cleanup ───────────────────────────────────────────────────────────────────
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _pc_test_baseline);
