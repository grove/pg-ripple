-- shacl_datalog_quality.sql — End-to-end SHACL + Datalog interaction (v0.46.0)
--
-- This example demonstrates the SHACL + Datalog interaction pattern:
--   1. Load a bibliographic graph.
--   2. Define SHACL shapes to validate data quality.
--   3. Run SPARQL to list constraint violations.
--   4. Apply Datalog RDFS rules to infer implicit triples.
--   5. Re-check shapes against the enriched graph.
--
-- Run this in a database with pg_ripple installed:
--   psql -f examples/shacl_datalog_quality.sql

-- ── Setup ─────────────────────────────────────────────────────────────────────

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- ── Step 1: Load a bibliographic graph ───────────────────────────────────────

SELECT pg_ripple.load_turtle($TTL$
@prefix bib: <http://example.org/bib/> .
@prefix schema: <http://schema.org/> .
@prefix dc: <http://purl.org/dc/terms/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

bib:book1 a schema:Book ;
    dc:title "Foundations of Databases" ;
    dc:creator bib:author1 ;
    schema:isbn "978-0-201-00798-4" ;
    dc:date "1995"^^xsd:gYear .

bib:book2 a schema:Book ;
    dc:title "SPARQL 1.1 Query Language" .
    -- Missing: dc:creator, dc:date (these will be violations)

bib:author1 a schema:Person ;
    schema:name "Abiteboul, Hull, Vianu" .
$TTL$, false);

-- ── Step 2: Define SHACL shapes ───────────────────────────────────────────────

SELECT pg_ripple.load_turtle($SHACL$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix schema: <http://schema.org/> .
@prefix dc: <http://purl.org/dc/terms/> .

<http://example.org/shapes/BookShape>
    a sh:NodeShape ;
    sh:targetClass schema:Book ;
    sh:property [
        sh:path dc:title ;
        sh:minCount 1 ;
        sh:datatype <http://www.w3.org/2001/XMLSchema#string> ;
    ] ;
    sh:property [
        sh:path dc:creator ;
        sh:minCount 1 ;
        sh:message "A book must have at least one creator." ;
    ] ;
    sh:property [
        sh:path dc:date ;
        sh:minCount 1 ;
        sh:message "A book must have a publication date." ;
    ] .
$SHACL$, false);

-- ── Step 3: Run SPARQL to list SHACL violations ───────────────────────────────

SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX sh: <http://www.w3.org/ns/shacl#>
  PREFIX schema: <http://schema.org/>
  SELECT ?shape ?message ?focus WHERE {
    ?report a sh:ValidationReport .
    ?report sh:result ?r .
    ?r sh:sourceShape ?shape ;
       sh:resultMessage ?message ;
       sh:focusNode ?focus .
  }
$SPARQL$);

-- Expected: bib:book2 missing dc:creator and dc:date violations.

-- ── Step 4: Apply Datalog RDFS rules ─────────────────────────────────────────

-- Load RDFS + OWL RL rules and materialize inference.
SELECT pg_ripple.materialize_owl_rl();

-- ── Step 5: Re-check shapes after inference ───────────────────────────────────

-- After inference, check if any implicit triples resolve violations.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX schema: <http://schema.org/>
  PREFIX dc: <http://purl.org/dc/terms/>
  SELECT ?book ?title ?creator WHERE {
    ?book a schema:Book .
    ?book dc:title ?title .
    OPTIONAL { ?book dc:creator ?creator }
  } ORDER BY ?title
$SPARQL$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
-- DROP EXTENSION pg_ripple CASCADE;
