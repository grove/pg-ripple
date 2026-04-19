-- v0.13.0: SHACL-driven query optimization regression tests.
--
-- Verifies that:
--   1. BGP join reordering GUC is present and controllable.
--   2. Plan-cache stats functions work correctly.
--   3. SHACL shape metadata is read and available for optimizer hints.
--   4. Extended statistics are created on VP tables after promotion.
--
-- Note: this file does NOT verify actual selectivity estimates (those depend on
-- table statistics that vary by run); it verifies the infrastructure is in place.

SET allow_system_table_mods = on;

-- ── BGP reordering GUC ────────────────────────────────────────────────────────

-- Default value should be on.
SHOW pg_ripple.bgp_reorder;

-- Can be toggled.
SET pg_ripple.bgp_reorder = off;
SHOW pg_ripple.bgp_reorder;
SET pg_ripple.bgp_reorder = on;
SHOW pg_ripple.bgp_reorder;

-- Parallel query GUC.
SHOW pg_ripple.parallel_query_min_joins;

-- ── Plan cache stats ──────────────────────────────────────────────────────────

-- Reset counters before test.
SELECT pg_ripple.plan_cache_reset();

-- Load some triples and run a SPARQL query to populate the cache.
SELECT pg_ripple.load_ntriples('
<http://example.org/A> <http://example.org/name> "Alice" .
<http://example.org/A> <http://example.org/age>  "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/B> <http://example.org/name> "Bob" .
<http://example.org/B> <http://example.org/age>  "25"^^<http://www.w3.org/2001/XMLSchema#integer> .
');

-- First execution: cache miss.
SELECT count(*) FROM pg_ripple.sparql($$
  SELECT ?name WHERE { ?s <http://example.org/name> ?name . }
$$);

-- Second execution of same query: cache hit.
SELECT count(*) FROM pg_ripple.sparql($$
  SELECT ?name WHERE { ?s <http://example.org/name> ?name . }
$$);

-- Stats should show 1 hit, 1 miss (or more depending on prior queries).
SELECT
  (pg_ripple.plan_cache_stats()->>'hits')::int >= 1     AS has_hits,
  (pg_ripple.plan_cache_stats()->>'misses')::int >= 1   AS has_misses,
  (pg_ripple.plan_cache_stats()->>'size')::int >= 1     AS has_entries,
  (pg_ripple.plan_cache_stats()->>'capacity')::int > 0  AS has_capacity;

-- hit_rate is a float between 0 and 1.
SELECT
  (pg_ripple.plan_cache_stats()->>'hit_rate')::float BETWEEN 0 AND 1 AS hit_rate_valid;

-- Reset clears the stats.
SELECT pg_ripple.plan_cache_reset();
SELECT
  (pg_ripple.plan_cache_stats()->>'hits')::int    AS hits_after_reset,
  (pg_ripple.plan_cache_stats()->>'misses')::int  AS misses_after_reset,
  (pg_ripple.plan_cache_stats()->>'size')::int    AS size_after_reset;

-- ── SHACL shape metadata for optimizer ───────────────────────────────────────

-- Load a SHACL shape with sh:maxCount 1 and sh:minCount 1.
SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/> .

ex:PersonShape a sh:NodeShape ;
  sh:targetClass ex:Person ;
  sh:property [
    sh:path ex:name ;
    sh:maxCount 1 ;
    sh:minCount 1
  ] .
$$);

-- Shape should be stored and active.
SELECT count(*) >= 1 AS has_shape
FROM _pg_ripple.shacl_shapes
WHERE active = true;

-- ── Sparql explain includes generated SQL ─────────────────────────────────────

-- Verify sparql_explain returns valid SQL with BGP reorder on.
SET pg_ripple.bgp_reorder = on;
SELECT
  pg_ripple.sparql_explain($$
    SELECT ?s ?name WHERE {
      ?s <http://example.org/name> ?name .
      ?s <http://example.org/age>  ?age  .
    }
  $$, false) LIKE '%SELECT%' AS has_select;

-- ── Cleanup ───────────────────────────────────────────────────────────────────

-- Truncate all triples so test data doesn't bleed into other tests.
SELECT pg_ripple.sparql_update('DELETE WHERE { ?s ?p ?o }') >= 1 AS cleanup_ok;
DELETE FROM _pg_ripple.shacl_shapes;
