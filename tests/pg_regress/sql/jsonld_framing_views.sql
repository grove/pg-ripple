-- pg_regress test: JSON-LD Framing Views (v0.17.0)
--
-- Tests: create/drop/list framing views; pg_trickle-absent error message;
-- framing_views catalog table schema.

-- ── Catalog table exists ──────────────────────────────────────────────────────

-- The _pg_ripple.framing_views catalog table should exist after migration.
SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'framing_views'
) AS framing_views_catalog_exists;

-- Verify expected column names.
SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'framing_views'
  AND column_name IN ('name','frame','generated_construct','schedule','output_format','decode','created_at')
ORDER BY column_name;

-- ── list_framing_views: empty initially ───────────────────────────────────────

SELECT pg_ripple.list_framing_views() = '[]'::jsonb AS framing_views_initially_empty;

-- ── pg_trickle absent: create_framing_view raises descriptive error ───────────

-- create_framing_view should raise an error when pg_trickle is not installed.
SELECT pg_ripple.create_framing_view(
    'test_view',
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
) IS NULL AS create_framing_view_without_pgtrickle_errors;

-- drop_framing_view should raise an error when pg_trickle is not installed.
SELECT pg_ripple.drop_framing_view('test_view') IS NULL AS drop_framing_view_without_pgtrickle_errors;
