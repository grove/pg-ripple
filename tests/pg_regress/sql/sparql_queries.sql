-- pg_regress test: SPARQL query engine (v0.3.0)
-- Uses a unique predicate namespace to avoid interference from other tests.

-- Setup: load a small dataset with unique-to-this-test predicates.
SELECT pg_ripple.load_ntriples(
    '<https://q.test/alice> <https://q.test/knows> <https://q.test/bob> .' || E'\n' ||
    '<https://q.test/bob>   <https://q.test/knows> <https://q.test/carol> .' || E'\n' ||
    '<https://q.test/alice> <https://q.test/name>  "Alice" .' || E'\n'
) = 3 AS three_triples_loaded;

-- SELECT with bound predicate (knows): exactly 2 triples.
SELECT COUNT(*) AS knows_count
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <https://q.test/knows> ?o }'
);

-- SELECT DISTINCT subjects of knows triples: exactly 2 distinct subjects.
SELECT COUNT(*) AS knows_subjects
FROM pg_ripple.sparql(
    'SELECT DISTINCT ?s WHERE { ?s <https://q.test/knows> ?o }'
);

-- LIMIT 1 must return exactly one row.
SELECT COUNT(*) AS limit_count
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <https://q.test/knows> ?o } LIMIT 1'
);

-- ASK for a subject that was never stored: must be false.
SELECT pg_ripple.sparql_ask(
    'ASK { <https://q.test/nobody> <https://q.test/knows> ?o }'
) AS ask_nobody;

-- ASK for alice: must be true.
SELECT pg_ripple.sparql_ask(
    'ASK { <https://q.test/alice> <https://q.test/knows> ?o }'
) AS ask_alice_knows;

-- ASK any triple with the test predicate: must be true.
SELECT pg_ripple.sparql_ask(
    'ASK { ?s <https://q.test/knows> ?o }'
) AS ask_any_knows;

-- sparql_explain must return a string that starts with the generated-SQL marker.
SELECT pg_ripple.sparql_explain(
    'SELECT ?s ?o WHERE { ?s <https://q.test/knows> ?o }',
    FALSE
) LIKE '-- Generated SQL --%' AS explain_ok;

-- SELECT subjects of knows triples in deterministic alphabetical order.
SELECT result->>'s' AS subject
FROM pg_ripple.sparql(
    'SELECT ?s ?o WHERE { ?s <https://q.test/knows> ?o }'
)
ORDER BY 1;

-- SELECT with literal pattern (name triple): exactly 1 row.
SELECT COUNT(*) AS name_count
FROM pg_ripple.sparql(
    'SELECT ?o WHERE { <https://q.test/alice> <https://q.test/name> ?o }'
);
