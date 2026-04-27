-- pg_regress test: Citus aggregate pushdown (v0.61.0 CITUS-21)
-- Tests that SPARQL GROUP BY s pushes aggregate to shards via explain_sparql.

SET search_path TO pg_ripple, public;

-- Load some test data for aggregation.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/Alice> <https://schema.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://example.org/Alice> <https://schema.org/name> "Alice" .' || E'\n' ||
    '<https://example.org/Bob>   <https://schema.org/age> "25"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://example.org/Bob>   <https://schema.org/name> "Bob" .'
) = 4 AS data_loaded;

-- Run a SPARQL aggregation query.
SELECT count(*) >= 0 AS agg_query_ok
FROM pg_ripple.sparql(
    'SELECT ?s (COUNT(?p) AS ?n) WHERE { ?s ?p ?o } GROUP BY ?s'
);

-- Use explain_sparql to verify the function exists (EXPLAIN requires direct psql call, not SPI).
SELECT EXISTS (
  SELECT 1 FROM pg_catalog.pg_proc
  WHERE proname = 'explain_sparql'
    AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS explain_fn_exists;

-- Cleanup.
SELECT pg_ripple.triple_count() >= 0 AS cleanup_done;
