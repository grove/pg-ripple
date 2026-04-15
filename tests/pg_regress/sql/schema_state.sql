-- pg_regress test: migration script coverage
--
-- Verifies that a fresh CREATE EXTENSION pg_ripple (latest version) produces
-- the complete schema that should result from applying every migration script
-- in sequence from v0.1.0 to the current version.
--
-- If any migration introduced a column, table, or index that is missing here,
-- this test will fail — protecting users who run ALTER EXTENSION pg_ripple UPDATE
-- from a broken schema.
--
-- NOTE: setup.sql (run before this file) already does DROP/CREATE EXTENSION.

-- ── Dictionary table ──────────────────────────────────────────────────────────

-- All columns present from v0.1.0 base schema
SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'dictionary'
ORDER BY ordinal_position;

-- ── Columns introduced by 0.3.0 → 0.4.0 migration ───────────────────────────

-- qt_s, qt_p, qt_o must all exist and be nullable BIGINTs
SELECT
    column_name,
    data_type,
    is_nullable
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'dictionary'
  AND column_name  IN ('qt_s', 'qt_p', 'qt_o')
ORDER BY column_name;

-- ── Predicates table ──────────────────────────────────────────────────────────

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'predicates'
ORDER BY ordinal_position;

-- ── vp_rare table (including source column from v0.1.0 base) ─────────────────

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'vp_rare'
ORDER BY ordinal_position;

-- ── Statement ID sequence ─────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relname = 'statement_id_seq'
      AND c.relkind = 'S'
      AND n.nspname = '_pg_ripple'
) AS statement_id_seq_exists;

-- ── predicate_stats view ─────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.views
    WHERE table_schema = 'pg_ripple'
      AND table_name   = 'predicate_stats'
) AS predicate_stats_view_exists;

-- ── v0.6.0: predicates.htap column ───────────────────────────────────────────

SELECT column_name, data_type, is_nullable
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'predicates'
  AND column_name  = 'htap';

-- ── v0.6.0: subject_patterns and object_patterns tables ──────────────────────

SELECT table_name
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name IN ('subject_patterns', 'object_patterns')
ORDER BY table_name;

-- ── v0.6.0: cdc_subscriptions table ──────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
      AND table_name   = 'cdc_subscriptions'
) AS cdc_subscriptions_exists;
