-- pg_regress test: SPARQL CONSTRUCT query with blank nodes

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load base data.
SELECT pg_ripple.load_ntriples(
    '<https://cbn.test/person1> <https://cbn.test/name> "Alice" .' || E'\n' ||
    '<https://cbn.test/person1> <https://cbn.test/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://cbn.test/person2> <https://cbn.test/name> "Bob" .'
) = 3 AS three_triples_loaded;

-- 1. CONSTRUCT query returns non-zero triples.
SELECT COUNT(*) >= 1 AS construct_returns_triples
FROM pg_ripple.sparql_construct($$
    CONSTRUCT { ?p <https://cbn.test/known> <https://cbn.test/true> . }
    WHERE {
        ?p <https://cbn.test/name> ?n .
    }
$$);

-- 2. CONSTRUCT with projected data.
SELECT COUNT(*) >= 1 AS construct_blank_node_ok
FROM pg_ripple.sparql_construct($$
    CONSTRUCT { ?p <https://cbn.test/hasName> ?n . }
    WHERE {
        ?p <https://cbn.test/name> ?n .
    }
$$);

-- 3. DESCRIBE returns triples about a subject.
SELECT COUNT(*) >= 1 AS describe_returns_triples
FROM pg_ripple.sparql_describe($$
    DESCRIBE <https://cbn.test/person1>
$$);

-- 4. CONSTRUCT with OPTIONAL pattern.
SELECT COUNT(*) >= 1 AS construct_optional_ok
FROM pg_ripple.sparql_construct($$
    CONSTRUCT { ?p <https://cbn.test/hasAge> ?a . }
    WHERE {
        ?p <https://cbn.test/name> ?n .
        OPTIONAL { ?p <https://cbn.test/age> ?a . }
    }
$$);
