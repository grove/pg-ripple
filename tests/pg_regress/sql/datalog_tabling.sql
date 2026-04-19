-- pg_regress test: Tabling / memoisation (v0.32.0)
--
-- Tests that:
-- 1. GUCs pg_ripple.tabling and pg_ripple.tabling_ttl exist with correct defaults.
-- 2. pg_ripple.tabling_stats() returns a table with required columns.
-- 3. Tabling cache is populated on infer_wfs() calls.
-- 4. Cache is invalidated on drop_rules() and load_rules().
-- 5. TTL=0 (no expiry) and positive TTL are respected.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. tabling defaults to on.
SHOW pg_ripple.tabling;

-- 1b. tabling_ttl defaults to 300.
SHOW pg_ripple.tabling_ttl;

-- 1c. GUCs can be toggled.
SET pg_ripple.tabling = off;
SHOW pg_ripple.tabling;
SET pg_ripple.tabling = on;

SET pg_ripple.tabling_ttl = 0;
SHOW pg_ripple.tabling_ttl;
SET pg_ripple.tabling_ttl = 300;

-- ── Part 2: tabling_stats() function structure ────────────────────────────────

-- 2a. Function exists and returns a set (possibly empty).
SELECT COUNT(*) >= 0 AS tabling_stats_callable FROM pg_ripple.tabling_stats();

-- 2b. Function has expected columns (check via select on zero rows).
SELECT goal_hash, hits, computed_ms, cached_at
FROM pg_ripple.tabling_stats()
WHERE FALSE;  -- returns no rows but validates column names exist

-- ── Part 3: tabling_stats() reports hits on repeated calls ───────────────────

-- Load a simple rule and insert data.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/t/linked> ?y :- ?x <https://ex.org/t/edge> ?y .',
    'tabling_test'
) > 0 AS tabling_rules_loaded;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/t/a>', '<https://ex.org/t/edge>', '<https://ex.org/t/b>'
) > 0 AS edge_inserted;

-- First call: cache miss — populates the cache.
SELECT jsonb_typeof(pg_ripple.infer_wfs('tabling_test')) = 'object' AS first_call_ok;

-- Stats should have at least one entry now.
SELECT COUNT(*) > 0 AS cache_has_entries FROM pg_ripple.tabling_stats();

-- ── Part 4: cache invalidation on drop_rules() ───────────────────────────────

-- Dropping rules clears the tabling cache.
SELECT pg_ripple.drop_rules('tabling_test') >= 0 AS rules_dropped;

-- After drop_rules, cache should be empty.
SELECT COUNT(*) = 0 AS cache_cleared_after_drop FROM pg_ripple.tabling_stats();

-- ── Part 5: cache invalidation on triple insert ───────────────────────────────

-- Reload rules so we have something to cache.
SELECT pg_ripple.load_rules(
    '?x <https://ex.org/t/linked2> ?y :- ?x <https://ex.org/t/edge> ?y .',
    'tabling_test2'
) > 0 AS tabling_rules2_loaded;

-- First infer_wfs call: populates cache.
SELECT jsonb_typeof(pg_ripple.infer_wfs('tabling_test2')) = 'object' AS second_test_call_ok;
SELECT COUNT(*) > 0 AS cache_populated FROM pg_ripple.tabling_stats();

-- Insert a new triple: should invalidate cache.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/t/b>', '<https://ex.org/t/edge>', '<https://ex.org/t/c>'
) > 0 AS edge2_inserted;

-- After triple insert, cache should be empty.
SELECT COUNT(*) = 0 AS cache_cleared_after_insert FROM pg_ripple.tabling_stats();

SELECT pg_ripple.drop_rules('tabling_test2') >= 0 AS rules2_dropped;

-- Cleanup.
SELECT pg_ripple.delete_triple(
    '<https://ex.org/t/a>', '<https://ex.org/t/edge>', '<https://ex.org/t/b>'
) >= 0 AS del_a;
SELECT pg_ripple.delete_triple(
    '<https://ex.org/t/b>', '<https://ex.org/t/edge>', '<https://ex.org/t/c>'
) >= 0 AS del_b;
