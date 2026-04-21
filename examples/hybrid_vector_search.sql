-- hybrid_vector_search.sql — End-to-end vector + SPARQL hybrid search (v0.46.0)
--
-- This example demonstrates the pg:similar() + SPARQL property-path pattern:
--   1. Load an entity graph with embeddings.
--   2. Run vector similarity search to find nearest neighbours.
--   3. Combine with SPARQL property-path constraints to filter by graph topology.
--
-- Prerequisites:
--   - pgvector installed: CREATE EXTENSION IF NOT EXISTS vector;
--   - pg_ripple installed with vector support (pg_ripple.pgvector_enabled = on)
--
-- Run this in a database with pg_ripple and pgvector installed:
--   psql -f examples/hybrid_vector_search.sql

-- ── Setup ─────────────────────────────────────────────────────────────────────

CREATE EXTENSION IF NOT EXISTS pg_ripple;
CREATE EXTENSION IF NOT EXISTS vector;

-- ── Step 1: Load entity graph ─────────────────────────────────────────────────

SELECT pg_ripple.load_turtle($TTL$
@prefix ex: <http://example.org/entity/> .
@prefix schema: <http://schema.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:ml        a schema:ResearchTopic ; rdfs:label "Machine Learning" .
ex:dl        a schema:ResearchTopic ; rdfs:label "Deep Learning" .
ex:nlp       a schema:ResearchTopic ; rdfs:label "Natural Language Processing" .
ex:cv        a schema:ResearchTopic ; rdfs:label "Computer Vision" .
ex:rl        a schema:ResearchTopic ; rdfs:label "Reinforcement Learning" .
ex:kg        a schema:ResearchTopic ; rdfs:label "Knowledge Graphs" .
ex:sparql    a schema:ResearchTopic ; rdfs:label "SPARQL Query Language" .

-- Relationships between topics.
ex:dl        schema:isPartOf ex:ml .
ex:nlp       schema:isPartOf ex:ml .
ex:cv        schema:isPartOf ex:ml .
ex:rl        schema:isPartOf ex:ml .
ex:sparql    schema:isPartOf ex:kg .
$TTL$, false);

-- ── Step 2: Store embeddings (using pre-computed placeholder vectors) ──────────
-- In production, replace these with real embeddings from your model.

SELECT pg_ripple.store_embedding(
    'http://example.org/entity/ml',
    ARRAY[0.9, 0.1, 0.2, 0.3]::float4[]
);

SELECT pg_ripple.store_embedding(
    'http://example.org/entity/dl',
    ARRAY[0.85, 0.15, 0.25, 0.35]::float4[]
);

SELECT pg_ripple.store_embedding(
    'http://example.org/entity/nlp',
    ARRAY[0.7, 0.3, 0.1, 0.5]::float4[]
);

SELECT pg_ripple.store_embedding(
    'http://example.org/entity/kg',
    ARRAY[0.2, 0.8, 0.6, 0.1]::float4[]
);

SELECT pg_ripple.store_embedding(
    'http://example.org/entity/sparql',
    ARRAY[0.15, 0.85, 0.65, 0.05]::float4[]
);

-- ── Step 3: Hybrid search — similar entities filtered by graph constraints ────

-- Find the top-3 topics most similar to "Machine Learning" that are also
-- subfields of Machine Learning (via schema:isPartOf property path).
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX ex: <http://example.org/entity/>
  PREFIX schema: <http://schema.org/>
  PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
  SELECT ?topic ?label ?similarity WHERE {
    ?topic a schema:ResearchTopic .
    ?topic rdfs:label ?label .
    ?topic schema:isPartOf ex:ml .
    BIND(pg:similar(ex:ml, ?topic) AS ?similarity)
    FILTER(?similarity > 0.5)
  }
  ORDER BY DESC(?similarity)
  LIMIT 3
$SPARQL$);

-- ── Step 4: Pure vector search (no graph constraints) ─────────────────────────

SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX ex: <http://example.org/entity/>
  PREFIX schema: <http://schema.org/>
  PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
  SELECT ?topic ?label ?similarity WHERE {
    ?topic a schema:ResearchTopic .
    ?topic rdfs:label ?label .
    BIND(pg:similar(ex:kg, ?topic) AS ?similarity)
    FILTER(?similarity > 0.0)
  }
  ORDER BY DESC(?similarity)
  LIMIT 5
$SPARQL$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
-- DROP EXTENSION pg_ripple CASCADE;
