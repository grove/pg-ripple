-- sparql_csv_tsv.sql
-- Test SPARQL CSV and TSV output formats (v0.51.0).
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Setup: small test graph ───────────────────────────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://csv.example.org/Alice>',
    '<http://csv.example.org/name>',
    '"Alice"'
) AS triple_id;

-- ── Test 1: sparql_csv exists and returns rows ────────────────────────────────
SELECT count(*) >= 1 AS csv_has_rows
FROM pg_ripple.sparql_csv(
    'SELECT ?s ?name WHERE {
       ?s <http://csv.example.org/name> ?name
     }'
);

-- ── Test 2: sparql_tsv exists and returns rows ────────────────────────────────
SELECT count(*) >= 1 AS tsv_has_rows
FROM pg_ripple.sparql_tsv(
    'SELECT ?s ?name WHERE {
       ?s <http://csv.example.org/name> ?name
     }'
);

-- ── Test 3: CSV first row is the header ──────────────────────────────────────
SELECT (array_agg(line ORDER BY ordinality))[1] LIKE '?%' AS first_row_is_header
FROM pg_ripple.sparql_csv(
    'SELECT ?s ?name WHERE {
       ?s <http://csv.example.org/name> ?name
     }'
) WITH ORDINALITY;

-- ── Test 4: Empty result returns no rows ─────────────────────────────────────
SELECT count(*) AS empty_result_rows
FROM pg_ripple.sparql_csv(
    'SELECT ?s WHERE { ?s <http://csv.example.org/nonexistent> ?o }'
);

SELECT pg_ripple.insert_triple(
    '<http://example.org/Bob>',
    '<http://schema.org/name>',
    '"Bob"'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/age>',
    '"30"^^<http://www.w3.org/2001/XMLSchema#integer>'
);

-- ── Test 1: CSV header and data rows ─────────────────────────────────────────
SELECT line FROM pg_ripple.sparql_csv(
    'SELECT ?s ?name WHERE {
       ?s <http://schema.org/name> ?name
     } ORDER BY ?name'
);

-- ── Test 2: TSV header and data rows ─────────────────────────────────────────
SELECT line FROM pg_ripple.sparql_tsv(
    'SELECT ?s ?name WHERE {
       ?s <http://schema.org/name> ?name
     } ORDER BY ?name'
);

-- ── Test 3: CSV with a value containing a comma (must be quoted) ──────────────
SELECT pg_ripple.insert_triple(
    '<http://example.org/Carol>',
    '<http://schema.org/name>',
    '"Smith, Carol"'
);

SELECT line FROM pg_ripple.sparql_csv(
    'SELECT ?name WHERE {
       <http://example.org/Carol> <http://schema.org/name> ?name
     }'
);

-- ── Test 4: CSV for ASK query (returns boolean result) ───────────────────────
SELECT line FROM pg_ripple.sparql_csv(
    'SELECT ?s WHERE { ?s <http://schema.org/name> "Alice" }'
);

-- ── Test 5: Empty result set ──────────────────────────────────────────────────
SELECT count(*) AS line_count FROM pg_ripple.sparql_csv(
    'SELECT ?s WHERE { ?s <http://example.org/nonexistent> ?o }'
);
