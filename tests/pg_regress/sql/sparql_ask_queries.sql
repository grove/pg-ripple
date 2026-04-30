-- pg_regress test: SPARQL ASK queries

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test data.
SELECT pg_ripple.load_ntriples(
    '<https://ask.test/alice> <https://ask.test/type> <https://ask.test/Person> .' || E'\n' ||
    '<https://ask.test/alice> <https://ask.test/name> "Alice" .' || E'\n' ||
    '<https://ask.test/alice> <https://ask.test/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .'
) = 3 AS three_triples_loaded;

-- 1. ASK returns true when pattern matches.
SELECT (pg_ripple.sparql_ask($$
    ASK { <https://ask.test/alice> <https://ask.test/type> <https://ask.test/Person> . }
$$)) = true AS ask_true_works;

-- 2. ASK returns false when pattern doesn't match.
SELECT (pg_ripple.sparql_ask($$
    ASK { <https://ask.test/nonexistent> <https://ask.test/type> <https://ask.test/Person> . }
$$)) = false AS ask_false_works;

-- 3. ASK with FILTER.
SELECT (pg_ripple.sparql_ask($$
    ASK {
        <https://ask.test/alice> <https://ask.test/age> ?a .
        FILTER(?a > 25)
    }
$$)) = true AS ask_with_filter_works;

-- 4. ASK with failing FILTER.
SELECT (pg_ripple.sparql_ask($$
    ASK {
        <https://ask.test/alice> <https://ask.test/age> ?a .
        FILTER(?a > 50)
    }
$$)) = false AS ask_failing_filter_works;
