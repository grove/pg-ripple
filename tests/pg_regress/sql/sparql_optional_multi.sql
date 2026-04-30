-- pg_regress test: SPARQL OPTIONAL with multiple patterns

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test data: some persons with optional attributes.
SELECT pg_ripple.load_ntriples(
    '<https://opt2.test/alice> <https://opt2.test/type> <https://opt2.test/Person> .' || E'\n' ||
    '<https://opt2.test/alice> <https://opt2.test/name> "Alice" .' || E'\n' ||
    '<https://opt2.test/alice> <https://opt2.test/email> "alice@ex.com" .' || E'\n' ||
    '<https://opt2.test/bob> <https://opt2.test/type> <https://opt2.test/Person> .' || E'\n' ||
    '<https://opt2.test/bob> <https://opt2.test/name> "Bob" .' || E'\n' ||
    '<https://opt2.test/carol> <https://opt2.test/type> <https://opt2.test/Person> .'
) = 6 AS six_triples_loaded;

-- 1. OPTIONAL: all persons, with email if available.
SELECT COUNT(*) = 3 AS optional_preserves_all_persons
FROM pg_ripple.sparql($$
    SELECT ?p ?email WHERE {
        ?p <https://opt2.test/type> <https://opt2.test/Person> .
        OPTIONAL { ?p <https://opt2.test/email> ?email . }
    }
$$);

-- 2. Persons without name (carol).
SELECT COUNT(*) = 1 AS optional_null_binding
FROM pg_ripple.sparql($$
    SELECT ?p WHERE {
        ?p <https://opt2.test/type> <https://opt2.test/Person> .
        OPTIONAL { ?p <https://opt2.test/name> ?name . }
        FILTER(!BOUND(?name))
    }
$$);

-- 3. Nested OPTIONAL.
SELECT COUNT(*) = 3 AS nested_optional_ok
FROM pg_ripple.sparql($$
    SELECT ?p ?name ?email WHERE {
        ?p <https://opt2.test/type> <https://opt2.test/Person> .
        OPTIONAL {
            ?p <https://opt2.test/name> ?name .
            OPTIONAL { ?p <https://opt2.test/email> ?email . }
        }
    }
$$);
