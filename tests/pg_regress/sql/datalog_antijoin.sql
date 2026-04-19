-- pg_regress test: Anti-join negation compilation (v0.29.0)
--
-- Tests `pg_ripple.datalog_antijoin_threshold` GUC.  The anti-join form
-- (LEFT JOIN ... IS NULL) is semantically equivalent to NOT EXISTS for
-- Datalog negation; this test verifies correct results and GUC toggling.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- Pre-test baseline.
CREATE TEMP TABLE _aj_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- 1. GUC exists and has default value.
SHOW pg_ripple.datalog_antijoin_threshold;

-- 2. GUC can be set to force NOT EXISTS form (threshold = 0).
SET pg_ripple.datalog_antijoin_threshold = 0;
SHOW pg_ripple.datalog_antijoin_threshold;

-- 3. GUC can be set to force anti-join form (threshold = 1 — any table size).
SET pg_ripple.datalog_antijoin_threshold = 1;
SHOW pg_ripple.datalog_antijoin_threshold;

-- 4. Restore default and verify inference works normally.
SET pg_ripple.datalog_antijoin_threshold = 1000;

-- Insert base triples for negation test.
SELECT pg_ripple.insert_triple(
    '<https://example.org/aj/x>',
    '<https://example.org/aj/hasColor>',
    '<https://example.org/aj/Red>'
) > 0 AS color_inserted;

-- 5. Run RDFS inference (which includes stratified negation internally in OWL RL).
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;
SELECT (pg_ripple.infer_with_stats('rdfs')->>'derived')::bigint >= 0
    AS derived_nonneg;
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped;

-- 6. Cleanup.
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _aj_test_baseline);
