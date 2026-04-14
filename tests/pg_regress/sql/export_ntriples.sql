-- pg_regress test: N-Triples bulk load and export round-trip

-- Load a small N-Triples dataset
SELECT pg_ripple.load_ntriples(
    '<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .' || chr(10) ||
    '<https://example.org/bob> <https://example.org/knows> <https://example.org/carol> .' || chr(10) ||
    '<https://example.org/alice> <https://example.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || chr(10)
) = 3 AS loaded_three_triples;

-- Verify count
SELECT pg_ripple.triple_count() >= 3 AS count_at_least_three;

-- Verify find_triples works after load
SELECT count(*) >= 1 AS alice_knows_found
FROM pg_ripple.find_triples('<https://example.org/alice>', '<https://example.org/knows>', NULL);

-- Export default graph as N-Triples; verify it contains expected content
SELECT pg_ripple.export_ntriples(NULL) LIKE '%<https://example.org/alice>%' AS export_contains_alice;
SELECT pg_ripple.export_ntriples(NULL) LIKE '%<https://example.org/knows>%' AS export_contains_knows;

-- Turtle load test
SELECT pg_ripple.load_turtle(
    '@prefix ex: <https://example.org/> .' || chr(10) ||
    'ex:dave ex:friendOf ex:eve .' || chr(10)
) = 1 AS turtle_loaded;

SELECT count(*) >= 1 AS dave_found
FROM pg_ripple.find_triples('<https://example.org/dave>', NULL, NULL);

-- Language-tagged literal load
SELECT pg_ripple.load_turtle(
    '<https://example.org/alice> <https://example.org/label> "Alice"@en .' || chr(10)
) = 1 AS lang_literal_loaded;

SELECT count(*) >= 1 AS lang_literal_found
FROM pg_ripple.find_triples('<https://example.org/alice>', '<https://example.org/label>', NULL);
