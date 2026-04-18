-- explain_sparql.sql — tests for explain_sparql() (v0.23.0)
--
-- Verifies that explain_sparql():
--   1. Exists and returns text
--   2. 'sparql_algebra' format returns the SPARQL algebra tree
--   3. 'sql' format returns generated SQL (contains SELECT)
--   4. 'text' format runs EXPLAIN and returns non-empty text
--   5. 'json' format runs EXPLAIN FORMAT JSON and returns non-empty text
--
-- Note: EXPLAIN output timing values vary per run; tests use length() > 0 checks.

SET search_path TO pg_ripple, public;

-- ── Setup: load some triples for explain to plan against ─────────────────────

SELECT pg_ripple.load_ntriples(
    '<https://explain.test/alice> <https://explain.test/name>  "Alice" .'  || E'\n' ||
    '<https://explain.test/alice> <https://explain.test/knows> <https://explain.test/bob> .' || E'\n' ||
    '<https://explain.test/bob>   <https://explain.test/name>  "Bob" .'
) = 3 AS triples_loaded;

-- ── 1. sparql_algebra format ─────────────────────────────────────────────────

-- The algebra output should contain 'Select' (case-sensitive spargebra Debug).
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE { ?s <https://explain.test/name> ?name }',
    'sparql_algebra'
) LIKE '%Select%' AS algebra_contains_select;

-- ── 2. sql format ────────────────────────────────────────────────────────────

-- The SQL output must contain 'SELECT' keyword.
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE { ?s <https://explain.test/name> ?name }',
    'sql'
) ILIKE '%SELECT%' AS sql_contains_select;

-- An ASK query in sql format must also return SQL.
SELECT pg_ripple.explain_sparql(
    'ASK { <https://explain.test/alice> <https://explain.test/name> ?n }',
    'sql'
) ILIKE '%SELECT%' AS ask_sql_contains_select;

-- ── 3. text format (default) — verify it runs without error ──────────────────

-- Returns something non-empty (EXPLAIN plan text).
SELECT length(pg_ripple.explain_sparql(
    'SELECT ?s ?name WHERE { ?s <https://explain.test/name> ?name }',
    'text'
)) > 0 AS text_explain_nonempty;

-- Default (omit format argument) should also work.
SELECT length(pg_ripple.explain_sparql(
    'SELECT ?s WHERE { ?s <https://explain.test/knows> ?o }'
)) > 0 AS default_format_nonempty;

-- ── 4. json format ───────────────────────────────────────────────────────────

-- JSON EXPLAIN output should start with the SQL header.
SELECT pg_ripple.explain_sparql(
    'SELECT ?s WHERE { ?s <https://explain.test/name> ?name }',
    'json'
) LIKE '%Generated SQL%' AS json_has_sql_header;
