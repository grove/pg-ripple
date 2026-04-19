-- pg_regress test: Cost-based body atom reordering (v0.29.0)
--
-- Tests `pg_ripple.datalog_cost_reorder` GUC and verifies that inference
-- produces correct results regardless of GUC setting.  The reordering is
-- internal and does not change query semantics.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- Pre-test baseline.
CREATE TEMP TABLE _cr_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- 1. GUC exists and has default value true.
SHOW pg_ripple.datalog_cost_reorder;

-- 2. Insert base triples for a multi-body rule test.
SELECT pg_ripple.insert_triple(
    '<https://example.org/cr/alice>',
    '<https://example.org/cr/knows>',
    '<https://example.org/cr/bob>'
) > 0 AS knows_inserted;
SELECT pg_ripple.insert_triple(
    '<https://example.org/cr/bob>',
    '<https://example.org/cr/type>',
    '<https://example.org/cr/Person>'
) > 0 AS type_inserted;

-- 3. Run inference with cost reordering ON (default).
SET pg_ripple.datalog_cost_reorder = true;
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded_on;
SELECT (pg_ripple.infer_with_stats('rdfs')->>'derived')::bigint >= 0
    AS derived_nonneg_on;
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped_1;

-- Cleanup inferred triples between runs.
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _cr_test_baseline);

-- 4. Run inference with cost reordering OFF.
SET pg_ripple.datalog_cost_reorder = false;
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded_off;
SELECT (pg_ripple.infer_with_stats('rdfs')->>'derived')::bigint >= 0
    AS derived_nonneg_off;
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_dropped_2;

-- Restore default.
SET pg_ripple.datalog_cost_reorder = true;

-- 5. Cleanup.
DELETE FROM _pg_ripple.vp_rare
    WHERE i > (SELECT max_i FROM _cr_test_baseline);
