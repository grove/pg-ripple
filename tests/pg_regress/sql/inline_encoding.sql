-- pg_regress test: inline value encoding for xsd:integer, xsd:boolean, xsd:date, xsd:dateTime (v0.5.1)
-- Uses unique predicate namespace https://inline.test/ to avoid interference.

-- Clean up any inline.test data from previous runs.
DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value IN (
            'https://inline.test/age',
            'https://inline.test/active',
            'https://inline.test/birthdate',
            'https://inline.test/created'
        )
    );
END $$;

-- Load triples with typed literals.
-- xsd:integer values: 18, 25, 42
-- xsd:boolean values: true, false
-- xsd:date: 1990-01-15
-- xsd:dateTime: 2023-06-01T00:00:00Z
SELECT pg_ripple.load_ntriples(
    '<https://inline.test/alice> <https://inline.test/age> "25"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://inline.test/bob>   <https://inline.test/age> "18"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://inline.test/carol> <https://inline.test/age> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://inline.test/alice> <https://inline.test/active> "true"^^<http://www.w3.org/2001/XMLSchema#boolean> .' || E'\n' ||
    '<https://inline.test/bob>   <https://inline.test/active> "false"^^<http://www.w3.org/2001/XMLSchema#boolean> .' || E'\n' ||
    '<https://inline.test/alice> <https://inline.test/birthdate> "1990-01-15"^^<http://www.w3.org/2001/XMLSchema#date> .' || E'\n' ||
    '<https://inline.test/alice> <https://inline.test/created>   "2023-06-01T00:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> .'
) = 7 AS seven_triples_loaded;

-- Verify that inline-encoded integers decode to the correct N-Triples literal form.
-- The ?age variable should return values like "25"^^<xsd:integer>.
SELECT result->>'age' LIKE '%25%' AS alice_age_decoded
FROM pg_ripple.sparql(
    'SELECT ?age WHERE { <https://inline.test/alice> <https://inline.test/age> ?age }'
);

-- Verify boolean decodes correctly.
SELECT result->>'active' LIKE '%true%' AS alice_active_true
FROM pg_ripple.sparql(
    'SELECT ?active WHERE { <https://inline.test/alice> <https://inline.test/active> ?active }'
);

-- Verify xsd:date decodes correctly.
SELECT result->>'birthdate' LIKE '%1990-01-15%' AS alice_birthdate_decoded
FROM pg_ripple.sparql(
    'SELECT ?birthdate WHERE { <https://inline.test/alice> <https://inline.test/birthdate> ?birthdate }'
);

-- Verify xsd:dateTime decodes correctly.
SELECT result->>'created' LIKE '%2023-06-01%' AS alice_created_decoded
FROM pg_ripple.sparql(
    'SELECT ?created WHERE { <https://inline.test/alice> <https://inline.test/created> ?created }'
);

-- FILTER comparison: people with age > 20 (alice=25, carol=42; bob=18 excluded).
SELECT COUNT(*) = 2 AS two_over_20
FROM pg_ripple.sparql(
    'SELECT ?p ?age WHERE {
       ?p <https://inline.test/age> ?age .
       FILTER(?age > "20"^^<http://www.w3.org/2001/XMLSchema#integer>)
     }'
);

-- FILTER comparison: people with age <= 25 (alice=25, bob=18; carol=42 excluded).
SELECT COUNT(*) = 2 AS two_at_most_25
FROM pg_ripple.sparql(
    'SELECT ?p ?age WHERE {
       ?p <https://inline.test/age> ?age .
       FILTER(?age <= "25"^^<http://www.w3.org/2001/XMLSchema#integer>)
     }'
);

-- ORDER BY age: should come back in order bob(18), alice(25), carol(42).
SELECT (
    SELECT string_agg(result->>'p', ',' ORDER BY result->>'age')
    FROM pg_ripple.sparql(
        'SELECT ?p ?age WHERE { ?p <https://inline.test/age> ?age } ORDER BY ?age'
    )
) LIKE '%bob%alice%carol%' AS ordered_by_age_correctly;

-- All inline-encoded integers across the store; the dictionary should NOT contain
-- a row for "25"^^xsd:integer (it is stored inline, not in the dict table).
SELECT NOT EXISTS (
    SELECT 1 FROM _pg_ripple.dictionary
    WHERE value = '25' AND kind = 3
      AND datatype = 'http://www.w3.org/2001/XMLSchema#integer'
) AS integer_25_not_in_dictionary;
