-- pg_regress test: SPARQL ASK Views (v0.18.0)
--
-- Tests: catalog table existence/schema; list_ask_views empty;
-- pg_trickle-absent error; wrong query form rejected.

-- ── Catalog table exists ──────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'ask_views'
) AS ask_views_catalog_exists;

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'ask_views'
  AND column_name IN ('name','sparql','generated_sql','schedule','stream_table','created_at')
ORDER BY column_name;

-- ── list_ask_views: empty initially ───────────────────────────────────────────

SELECT pg_ripple.list_ask_views() = '[]'::jsonb AS ask_views_initially_empty;

-- ── pg_trickle absent: create_ask_view raises descriptive error ───────────────

SELECT pg_ripple.create_ask_view(
    'test_av',
    'ASK { ?s ?p ?o }'
) IS NULL AS create_ask_view_without_pgtrickle_errors;

-- ── pg_trickle absent: drop_ask_view raises descriptive error ─────────────────

SELECT pg_ripple.drop_ask_view('test_av') IS NULL AS drop_ask_view_without_pgtrickle_errors;

-- ── Wrong query form: CONSTRUCT query rejected ────────────────────────────────

SELECT pg_ripple.create_ask_view(
    'bad_av',
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }'
) IS NULL AS construct_query_rejected_by_ask_view;
