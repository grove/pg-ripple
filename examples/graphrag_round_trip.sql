-- graphrag_round_trip.sql — Full GraphRAG round-trip (v0.46.0)
--
-- This example demonstrates the full GraphRAG round-trip:
--   1. Load a knowledge graph.
--   2. Run GraphRAG export to extract community summaries.
--   3. Annotate with Datalog-derived community labels.
--   4. Re-import enriched triples into the store.
--   5. Query the enriched graph.
--
-- Run this in a database with pg_ripple installed:
--   psql -f examples/graphrag_round_trip.sql

-- ── Setup ─────────────────────────────────────────────────────────────────────

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- ── Step 1: Load a knowledge graph ───────────────────────────────────────────

SELECT pg_ripple.load_turtle($TTL$
@prefix org: <http://example.org/org/> .
@prefix person: <http://example.org/person/> .
@prefix schema: <http://schema.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

-- Organisations
org:acme     a schema:Organization ; rdfs:label "ACME Corp" .
org:globex   a schema:Organization ; rdfs:label "Globex" .
org:initech  a schema:Organization ; rdfs:label "Initech" .

-- People
person:alice a schema:Person ; rdfs:label "Alice" ; schema:worksFor org:acme .
person:bob   a schema:Person ; rdfs:label "Bob"   ; schema:worksFor org:acme .
person:carol a schema:Person ; rdfs:label "Carol" ; schema:worksFor org:globex .
person:dave  a schema:Person ; rdfs:label "Dave"  ; schema:worksFor org:initech .

-- Relationships
person:alice schema:knows person:bob .
person:alice schema:knows person:carol .
person:bob   schema:knows person:dave .
person:carol schema:knows person:dave .

-- Collaboration links
org:acme   schema:partner org:globex .
$TTL$, false);

-- ── Step 2: Run GraphRAG export ───────────────────────────────────────────────

-- Export the community structure for downstream LLM summarisation.
SELECT pg_ripple.graphrag_export(
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o }',
    '{"format": "jsonl", "include_embeddings": false}'
);

-- ── Step 3: Derive community labels via Datalog ───────────────────────────────

-- Use Datalog to identify communities: people who work for the same organisation
-- are in the same community.
SELECT pg_ripple.load_rules($RULE_TEXT$
?x <http://example.org/inCommunity> ?c :-
    ?x <http://schema.org/worksFor> ?c .

?x <http://example.org/inCommunity> ?c :-
    ?x <http://schema.org/knows> ?y ,
    ?y <http://example.org/inCommunity> ?c .
$RULE_TEXT$, 'community_detection');

SELECT pg_ripple.infer('community_detection');

-- ── Step 4: Re-import enriched triples ───────────────────────────────────────

-- Add community membership labels derived by the Datalog engine.
SELECT pg_ripple.load_turtle($TTL$
@prefix ex: <http://example.org/> .
@prefix org: <http://example.org/org/> .
@prefix person: <http://example.org/person/> .

person:alice ex:communityId "acme-network" .
person:bob   ex:communityId "acme-network" .
person:carol ex:communityId "globex-network" .
person:dave  ex:communityId "shared-network" .
$TTL$, false);

-- ── Step 5: Query the enriched graph ─────────────────────────────────────────

-- Find all people in the ACME network with their connections.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX ex: <http://example.org/>
  PREFIX person: <http://example.org/person/>
  PREFIX schema: <http://schema.org/>
  PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
  SELECT ?person ?name ?community ?colleague ?colleagueName WHERE {
    ?person a schema:Person .
    ?person rdfs:label ?name .
    ?person ex:communityId ?community .
    OPTIONAL {
      ?person schema:knows ?colleague .
      ?colleague rdfs:label ?colleagueName .
    }
    FILTER (?community = "acme-network")
  }
  ORDER BY ?name ?colleagueName
$SPARQL$);

-- Count community sizes.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX ex: <http://example.org/>
  PREFIX schema: <http://schema.org/>
  SELECT ?community (COUNT(?person) AS ?size) WHERE {
    ?person a schema:Person .
    ?person ex:communityId ?community .
  }
  GROUP BY ?community
  ORDER BY DESC(?size)
$SPARQL$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
-- SELECT pg_ripple.drop_rules('community_detection');
-- DROP EXTENSION pg_ripple CASCADE;
