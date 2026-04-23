-- examples/federation_multi_endpoint.sql
-- Multi-endpoint SPARQL federation example for pg_ripple (v0.51.0)
--
-- pg_ripple can federate queries across multiple remote SPARQL endpoints using
-- the SERVICE keyword in SPARQL 1.1 Federated Query.
--
-- Prerequisites:
--   CREATE EXTENSION pg_ripple CASCADE;
--
-- Note: Replace the endpoint URLs below with real SPARQL endpoints.

-- ── Step 1: Register remote endpoints ────────────────────────────────────────

-- Register known endpoints with optional CA-certificate pinning (v0.51.0).
-- The fingerprint is the SHA-256 of the endpoint's TLS certificate.
SELECT pg_ripple.register_federation_endpoint(
    endpoint  := 'https://dbpedia.org/sparql',
    label     := 'DBpedia',
    -- Optional: pin TLS certificate fingerprint (v0.51.0 security feature).
    -- Set PG_RIPPLE_HTTP_PIN_FINGERPRINTS env var, or pass it here:
    pin_fingerprint := NULL  -- e.g. 'sha256:AA:BB:CC:...'
);

SELECT pg_ripple.register_federation_endpoint(
    endpoint  := 'https://query.wikidata.org/sparql',
    label     := 'Wikidata'
);

-- ── Step 2: Federated query across two endpoints ──────────────────────────────

-- Find people in the local store, then fetch their birth dates from DBpedia.
SELECT result
FROM pg_ripple.sparql(
    'PREFIX dbo: <http://dbpedia.org/ontology/>
     PREFIX dbr: <http://dbpedia.org/resource/>

     SELECT ?person ?name ?birthDate
     WHERE {
       -- Local triples:
       ?person <http://schema.org/name> ?name .

       -- Remote triples from DBpedia:
       SERVICE <https://dbpedia.org/sparql> {
         ?dbpPerson dbo:birthDate ?birthDate .
         FILTER(STR(?name) = STR(?name))
       }
     }
     LIMIT 10'
);

-- ── Step 3: Cost-based federation planning ────────────────────────────────────

-- Explain the query plan to see how the federation planner distributes joins.
SELECT pg_ripple.explain_sparql(
    'PREFIX schema: <http://schema.org/>
     SELECT ?name ?homepage
     WHERE {
       ?person schema:name ?name .
       SERVICE <https://dbpedia.org/sparql> {
         ?person schema:url ?homepage .
       }
     }',
    format := ''json''
) AS federation_plan;

-- ── Step 4: Cache control ─────────────────────────────────────────────────────

-- View federation cache status (results cached for 60 minutes by default).
SELECT * FROM pg_ripple.federation_cache_stats();

-- Clear the federation result cache to force re-fetching from remote endpoints.
SELECT pg_ripple.reset_cache_stats();

-- ── Step 5: Monitor federation health ────────────────────────────────────────

-- Check which endpoints are reachable and their response times.
SELECT
    endpoint,
    label,
    last_ping_ms,
    is_healthy
FROM pg_ripple.federation_endpoint_health()
ORDER BY last_ping_ms;
