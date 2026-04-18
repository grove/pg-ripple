-- sparql_protocol.sql — pg_regress placeholder for SPARQL Protocol tests (v0.15.0)
--
-- The full W3C SPARQL 1.1 Protocol tests require the pg_ripple_http companion
-- binary and HTTP requests (curl).  This file tests the SQL-level functions that
-- back the HTTP endpoint, verifying they produce correct output formats.

-- Seed a known triple so the next query is deterministic.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/proto_seed> <https://example.org/proto_prop> "seed" .'
);

-- Verify sparql() returns JSONB results suitable for JSON serialization
SELECT result FROM pg_ripple.sparql(
    'SELECT ?s WHERE { <https://example.org/proto_seed> ?p ?o . BIND(<https://example.org/proto_seed> AS ?s) } LIMIT 1'
);

-- Verify sparql_ask returns boolean
SELECT pg_ripple.load_ntriples(
    '<https://example.org/test_proto> <https://example.org/type> <https://example.org/Thing> .
');
SELECT pg_ripple.sparql_ask('ASK { <https://example.org/test_proto> ?p ?o }') AS ask_true;

-- Verify sparql_construct returns triples
SELECT * FROM pg_ripple.sparql_construct('
    CONSTRUCT { ?s <https://example.org/has> ?o }
    WHERE { ?s <https://example.org/type> ?o }
') LIMIT 2;

-- Verify sparql_describe returns triples
SELECT * FROM pg_ripple.sparql_describe('
    DESCRIBE <https://example.org/test_proto>
') LIMIT 5;

-- Verify sparql_update works for protocol update path
SELECT pg_ripple.sparql_update('
    INSERT DATA { <https://example.org/proto_new> <https://example.org/value> "test" }
');
SELECT pg_ripple.sparql_ask('ASK { <https://example.org/proto_new> ?p ?o }') AS update_verified;
