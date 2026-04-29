-- export_jsonld_node.sql
-- JSONLD-NODE-01 (v0.72.0): Tests for the pg_ripple.export_jsonld_node() function.
--
-- This function wraps export_jsonld_framed() for single-subject export,
-- automatically injecting the @id and performing recursive keyword strip.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Smoke test: function exists ───────────────────────────────────────────────

SELECT COUNT(*) = 1 AS function_exists
FROM   pg_catalog.pg_proc p
JOIN   pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE  n.nspname = 'pg_ripple'
AND    p.proname = 'export_jsonld_node';

-- ── Setup ─────────────────────────────────────────────────────────────────────

-- Load a test subject with some properties.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/node_test_subject> <https://example.org/name> "Widget" .
     <https://example.org/node_test_subject> <https://example.org/category> "Electronics" .'
) > 0 AS test_data_loaded;

-- Encode the subject IRI to get its dictionary ID.
SELECT pg_ripple.encode_term('https://example.org/node_test_subject', 0::smallint) IS NOT NULL
    AS subject_encoded;

-- ── Test: export_jsonld_node with a known subject ────────────────────────────

SELECT pg_ripple.export_jsonld_node(
    frame => '{"@context": {
        "name":     "https://example.org/name",
        "category": "https://example.org/category"
    }, "@type": "https://example.org/Product"}'::jsonb,
    subject_id => pg_ripple.encode_term('https://example.org/node_test_subject', 0::smallint)
) IS NOT NULL AS export_returns_result;

-- ── Test: unknown subject returns NULL ───────────────────────────────────────

SELECT pg_ripple.export_jsonld_node(
    frame => '{"@context": {}, "@type": "ex:Thing"}'::jsonb,
    subject_id => -999999
) IS NULL AS unknown_subject_returns_null;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.sparql_update(
    'DELETE WHERE { <https://example.org/node_test_subject> ?p ?o }'
) >= 0 AS cleanup_done;
