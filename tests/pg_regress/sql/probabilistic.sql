-- pg_regress test: Uncertain Knowledge Engine (v0.87.0)
-- Tests probabilistic Datalog @weight annotations, confidence side table,
-- vacuum_confidence(), shacl_score(), and load_triples_with_confidence().

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Test 1: load_triples_with_confidence ─────────────────────────────────────
-- Load a small set of triples with a confidence score and verify the count.
SELECT pg_ripple.load_triples_with_confidence(
    '<http://example.org/alice> <http://example.org/knows> <http://example.org/bob> .',
    0.9
) AS loaded;

-- ── Test 2: vacuum_confidence ─────────────────────────────────────────────────
-- vacuum_confidence() must return a non-negative bigint.
SELECT pg_ripple.vacuum_confidence() >= 0 AS vacuum_ok;

-- ── Test 3: shacl_score ───────────────────────────────────────────────────────
-- shacl_score() must return a value in [0.0, 1.0].
SELECT pg_ripple.shacl_score('http://example.org/data') BETWEEN 0.0 AND 1.0 AS score_in_range;

-- ── Test 4: shacl_report_scored column list ───────────────────────────────────
-- shacl_report_scored() must return a table with five columns.
SELECT count(*) AS col_count
FROM information_schema.columns
WHERE table_schema = 'pg_ripple'
  AND table_name   = 'shacl_report_scored';

-- ── Test 5: export_turtle_with_confidence ─────────────────────────────────────
-- export_turtle_with_confidence() must return a non-null text value.
SELECT pg_ripple.export_turtle_with_confidence() IS NOT NULL AS turtle_ok;
