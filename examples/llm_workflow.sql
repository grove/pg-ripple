-- examples/llm_workflow.sql
-- LLM-to-SPARQL workflow example for pg_ripple (v0.51.0)
--
-- This example shows how to integrate an LLM (language model) with pg_ripple
-- to answer natural-language questions about an RDF knowledge graph.
--
-- Workflow:
--   1. User asks a natural-language question
--   2. LLM converts it to a SPARQL query (via pg_ripple.llm_to_sparql())
--   3. SPARQL query runs against the triple store
--   4. Results are formatted back to natural language by the LLM
--
-- Prerequisites:
--   CREATE EXTENSION pg_ripple CASCADE;
--   SET pg_ripple.llm_endpoint = 'http://localhost:11434/api/generate';

-- ── Step 1: Load sample knowledge graph ──────────────────────────────────────

SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://schema.org/Person>'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/name>',
    '"Alice Smith"'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/jobTitle>',
    '"Senior Engineer"'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Bob>',
    '<http://schema.org/knows>',
    '<http://example.org/Alice>'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Bob>',
    '<http://schema.org/name>',
    '"Bob Jones"'
);

-- ── Step 2: Natural-language query via LLM ───────────────────────────────────

-- Convert a natural-language question to SPARQL using the configured LLM.
-- The LLM is given the graph schema and asked to produce a valid SPARQL query.
SELECT pg_ripple.llm_to_sparql(
    'Who does Bob know?',
    schema_iri := 'http://schema.org/'
) AS generated_sparql;

-- ── Step 3: Execute the SPARQL query ─────────────────────────────────────────

-- Run the generated (or manually written) SPARQL query.
SELECT result->>'s' AS person_name
FROM pg_ripple.sparql(
    'SELECT ?name
     WHERE {
       <http://example.org/Bob> <http://schema.org/knows> ?person .
       ?person <http://schema.org/name> ?name .
     }'
);

-- ── Step 4: Export results as Turtle for LLM context ─────────────────────────

-- Provide the relevant subgraph as Turtle context for the LLM to summarise.
SELECT pg_ripple.sparql_construct_turtle(
    'CONSTRUCT { ?s ?p ?o }
     WHERE {
       ?s ?p ?o .
       FILTER(?s IN (<http://example.org/Alice>, <http://example.org/Bob>))
     }'
) AS turtle_context;

-- ── Step 5: Hybrid vector + SPARQL search ────────────────────────────────────

-- Find entities semantically similar to "software developer" that are
-- also connected in the knowledge graph.
-- (Requires pgvector and the vector embedding pipeline.)
SELECT
    s.uri,
    s.similarity,
    r.result->>'name' AS name
FROM pg_ripple.hybrid_vector_sparql(
    embedding := pg_ripple.embed('software developer'),
    sparql    := 'SELECT ?uri ?name WHERE { ?uri <http://schema.org/name> ?name }',
    top_k     := 5
) s
JOIN LATERAL (
    SELECT result
    FROM pg_ripple.sparql(
        format('SELECT ?name WHERE { <%s> <http://schema.org/name> ?name }', s.uri)
    ) LIMIT 1
) r ON true;
