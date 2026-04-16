-- pg_regress test: SPARQL DESCRIBE Views (v0.18.0)
--
-- Tests: catalog table existence/schema; list_describe_views empty;
-- pg_trickle-absent error; wrong query form rejected.

-- ── Catalog table exists ──────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'describe_views'
) AS describe_views_catalog_exists;

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'describe_views'
  AND column_name IN ('name','sparql','generated_sql','schedule','decode','strategy','stream_table','created_at')
ORDER BY column_name;

-- ── list_describe_views: empty initially ──────────────────────────────────────

SELECT pg_ripple.list_describe_views() = '[]'::jsonb AS describe_views_initially_empty;

-- ── pg_trickle absent: create_describe_view raises descriptive error ──────────

SELECT pg_ripple.create_describe_view(
    'test_dv',
    'DESCRIBE <https://example.org/resource>'
) IS NULL AS create_describe_view_without_pgtrickle_errors;

-- ── pg_trickle absent: drop_describe_view raises descriptive error ────────────

SELECT pg_ripple.drop_describe_view('test_dv') IS NULL AS drop_describe_view_without_pgtrickle_errors;

-- ── Wrong query form: SELECT query rejected ───────────────────────────────────

SELECT pg_ripple.create_describe_view(
    'bad_dv',
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o }'
) IS NULL AS select_query_rejected_by_describe_view;
