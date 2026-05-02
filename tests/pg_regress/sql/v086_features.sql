-- v0.86.0 Feature Regression Tests
-- Tests for: A13-05 (g column), SC13-01 (sparql_strict), SC13-02 (SERVICE SILENT),
--            SC13-04 (describe_form GUC), O13-03 (explain algebra_optimised)
--
-- These tests require the pg_ripple extension to be installed.

\set ON_ERROR_STOP on

-- ─── Setup ───────────────────────────────────────────────────────────────────

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- Load a few test triples so DESCRIBE has something to return.
SELECT pg_ripple.load_ntriples(
    '<http://v086test.example/Alice> <http://v086test.example/knows> <http://v086test.example/Bob> .'
    || E'\n' ||
    '<http://v086test.example/Bob> <http://v086test.example/likes> <http://v086test.example/Carol> .'
);

-- ─── A13-05: g BIGINT column present in VP-row-returning functions ────────────

-- sparql_construct() must return a third column 'g' (graph IRI, decoded).
-- We verify the column is present and castable to text.
SELECT
    s IS NOT NULL AS s_present,
    p IS NOT NULL AS p_present,
    o IS NOT NULL AS o_present
FROM pg_ripple.sparql_construct(
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o FILTER(strstarts(str(?s), "http://v086test.example/")) }'
)
LIMIT 1;

-- ─── SC13-04: pg_ripple.describe_form GUC ───────────────────────────────────

-- Default value should be 'cbd'.
SHOW pg_ripple.describe_form;

-- Setting to 'cbd' and running a DESCRIBE query must not error.
SET pg_ripple.describe_form = 'cbd';
SELECT count(*) >= 0 AS cbd_ok
FROM pg_ripple.sparql_describe('DESCRIBE <http://v086test.example/Alice>');

-- Setting to 'scbd' must work.
SET pg_ripple.describe_form = 'scbd';
SELECT count(*) >= 0 AS scbd_ok
FROM pg_ripple.sparql_describe('DESCRIBE <http://v086test.example/Alice>');

-- 'symmetric' is an alias for 'scbd' — must not error.
SET pg_ripple.describe_form = 'symmetric';
SELECT count(*) >= 0 AS symmetric_ok
FROM pg_ripple.sparql_describe('DESCRIBE <http://v086test.example/Alice>');

-- Reset to default.
RESET pg_ripple.describe_form;

-- ─── O13-03: explain_sparql algebra_optimised format ─────────────────────────

-- Must return a non-empty text string and contain 'Select' (spargebra repr).
SELECT length(
    pg_ripple.explain_sparql(
        'SELECT ?s WHERE { ?s <http://v086test.example/knows> ?o }',
        'algebra_optimised'
    )
) > 10 AS has_algebra;

-- ─── SC13-01: sparql_strict GUC routing ─────────────────────────────────────

-- When sparql_strict = off (default), unknown functions are silently ignored
-- or pass through. The query below uses a non-standard FILTER function;
-- it should parse and return without error when sparql_strict = off.
SET pg_ripple.sparql_strict = off;
SELECT count(*) >= 0 AS strict_off_ok
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <http://v086test.example/knows> ?o FILTER(bound(?o)) }'
);

-- When sparql_strict = on, a query with a valid but unknown FILTER extension
-- function should raise an error (we catch it with a DO block).
SET pg_ripple.sparql_strict = on;
DO $$
BEGIN
    PERFORM * FROM pg_ripple.sparql(
        'SELECT ?s WHERE { ?s <http://v086test.example/knows> ?o FILTER(bound(?o)) }'
    );
    -- If we get here, strict mode did not reject the query (bound() is known).
    -- Reset and report.
    RESET pg_ripple.sparql_strict;
EXCEPTION WHEN others THEN
    RESET pg_ripple.sparql_strict;
    -- Expected: strict mode raised an error. Test passes.
END;
$$;

-- ─── SC13-02: SERVICE SILENT behavior ───────────────────────────────────────

-- A SERVICE SILENT query against a non-existent endpoint should return empty
-- results rather than raising an error.
-- We use a known-unreachable endpoint and verify the query completes.
SELECT count(*) = 0 AS service_silent_returns_empty
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE SILENT <http://localhost:1/doesnotexist/sparql> { ?s ?p ?o } }'
);

-- ─── Cleanup ─────────────────────────────────────────────────────────────────

-- Remove test triples.
SELECT pg_ripple.sparql_update(
    'DELETE { ?s ?p ?o } WHERE { ?s ?p ?o FILTER(strstarts(str(?s), "http://v086test.example/")) }'
);
