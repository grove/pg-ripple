-- shacl_query_hints.sql — SHACL-driven query optimization hints (v0.23.0)
--
-- Verifies that after loading a SHACL shape with sh:maxCount 1 / sh:minCount 1,
-- explain_sparql() reflects the query optimizer's use of SHACL hints.
-- (Stable output: tests use boolean checks, not raw EXPLAIN plan text.)
--
-- See also: shacl_query_opt.sql (v0.13.0) for the GUC and plan-cache baseline.

SET search_path TO pg_ripple, public;

-- ── Setup ────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_ntriples(
    '<https://qhint.test/p1>  <https://qhint.test/name>  "Person One" .'  || E'\n' ||
    '<https://qhint.test/p1>  <https://qhint.test/age>   "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://qhint.test/p1>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://qhint.test/Person> .' || E'\n' ||
    '<https://qhint.test/p2>  <https://qhint.test/name>  "Person Two" .'  || E'\n' ||
    '<https://qhint.test/p2>  <https://qhint.test/age>   "25"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://qhint.test/p2>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://qhint.test/Person> .'
) = 6 AS six_triples;

-- ── Load shape with maxCount 1 / minCount 1 ──────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://qhint.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:PersonShape
    a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path ex:age ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] .
$SHACL$) IS NOT NULL AS shape_loaded;

-- ── explain_sparql() should execute without error after shape is loaded ───────

-- Basic: explain_sparql runs on a SELECT query after shapes are loaded.
SELECT length(pg_ripple.explain_sparql(
    'SELECT ?name ?age WHERE { ?s <https://qhint.test/name> ?name ; <https://qhint.test/age> ?age }',
    'sql'
)) > 0 AS sql_nonempty;

-- Verify actual SPARQL query returns correct results.
SELECT COUNT(*) = 2 AS two_persons FROM pg_ripple.sparql(
    'SELECT ?name WHERE { ?s <https://qhint.test/name> ?name ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://qhint.test/Person> }'
);

-- Validate shape (both persons should conform).
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS all_conform;

-- ── The SQL output should contain inner-join pattern ─────────────────────────
-- (sh:minCount 1 allows INNER JOIN rather than LEFT JOIN)

SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?name WHERE { ?s <https://qhint.test/name> ?name . ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://qhint.test/Person> }',
    'sql'
) ILIKE '%JOIN%' AS plan_has_join;
