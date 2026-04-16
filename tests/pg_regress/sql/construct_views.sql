-- pg_regress test: SPARQL CONSTRUCT Views (v0.18.0)
--
-- Tests: catalog table existence/schema; list_construct_views empty;
-- pg_trickle-absent error; wrong query form rejected; unbound variable error;
-- blank node in template error.

-- ── Catalog table exists ──────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'construct_views'
) AS construct_views_catalog_exists;

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'construct_views'
  AND column_name IN ('name','sparql','generated_sql','schedule','decode','template_count','stream_table','created_at')
ORDER BY column_name;

-- ── list_construct_views: empty initially ─────────────────────────────────────

SELECT pg_ripple.list_construct_views() = '[]'::jsonb AS construct_views_initially_empty;

-- ── pg_trickle absent: create_construct_view raises descriptive error ─────────

SELECT pg_ripple.create_construct_view(
    'test_cv',
    'CONSTRUCT { ?s <https://example.org/p> ?o } WHERE { ?s <https://example.org/p> ?o }'
) IS NULL AS create_construct_view_without_pgtrickle_errors;

-- ── pg_trickle absent: drop_construct_view raises descriptive error ───────────

SELECT pg_ripple.drop_construct_view('test_cv') IS NULL AS drop_construct_view_without_pgtrickle_errors;

-- ── Wrong query form: SELECT query rejected ───────────────────────────────────

SELECT pg_ripple.create_construct_view(
    'bad_cv',
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o }'
) IS NULL AS select_query_rejected_by_construct_view;

-- ── Unbound variable in template: error listing unbound vars ─────────────────

SELECT pg_ripple.create_construct_view(
    'unbound_cv',
    'CONSTRUCT { ?s <https://example.org/q> ?unbound } WHERE { ?s <https://example.org/p> ?o }'
) IS NULL AS unbound_variable_rejected;

-- ── Blank node in template: descriptive error ─────────────────────────────────

SELECT pg_ripple.create_construct_view(
    'blank_cv',
    'CONSTRUCT { _:b0 <https://example.org/p> ?o } WHERE { ?s <https://example.org/p> ?o }'
) IS NULL AS blank_node_in_template_rejected;
