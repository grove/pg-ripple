-- Example: LLM-Augmented SPARQL Repair (v0.57.0)
-- Demonstrates pg_ripple.repair_sparql() for fixing broken SPARQL queries.
--
-- Prerequisites:
--   SET pg_ripple.llm_endpoint = 'https://api.openai.com/v1/chat/completions';
--   SET pg_ripple.llm_api_key_env = 'OPENAI_API_KEY';
--   SET pg_ripple.llm_model = 'gpt-4o-mini';

SET search_path TO pg_ripple, public;

-- Example 1: Repair a query with a missing closing brace.
SELECT pg_ripple.repair_sparql(
    $$SELECT ?person ?name WHERE {
        ?person rdf:type <http://schema.org/Person> .
        ?person <http://schema.org/name> ?name$$,
    'SPARQL parse error: unexpected end of input, expected }'
) AS repaired_query;

-- Example 2: Repair a query with a typo in a prefix.
SELECT pg_ripple.repair_sparql(
    $$PREFIX schema: <http://schema.org/>
SELECT ?s WHERE { ?s schema:nme ?o }$$,
    'No results returned — did you mean schema:name?'
) AS repaired_query;

-- Example 3: Use mock endpoint for testing without API key.
SET pg_ripple.llm_endpoint = 'mock';
SELECT pg_ripple.repair_sparql(
    'SELECT ?s WHERE { ?s ?p ?o',
    'parse error'
) AS mock_repair;
RESET pg_ripple.llm_endpoint;
