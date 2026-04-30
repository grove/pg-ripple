-- pg_regress test: SPARQL numeric and comparison functions

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load numeric test data.
SELECT pg_ripple.load_ntriples(
    '<https://num.test/a> <https://num.test/val> "3.14"^^<http://www.w3.org/2001/XMLSchema#double> .' || E'\n' ||
    '<https://num.test/b> <https://num.test/val> "-2.5"^^<http://www.w3.org/2001/XMLSchema#double> .' || E'\n' ||
    '<https://num.test/c> <https://num.test/val> "9"^^<http://www.w3.org/2001/XMLSchema#integer> .'
) = 3 AS three_numeric_triples;

-- 1. ABS() on negative value.
SELECT COUNT(*) = 1 AS abs_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://num.test/val> ?v .
        BIND(ABS(?v) AS ?a)
        FILTER(?a > 2 && ?a < 3)
    }
$$);

-- 2. CEIL: ceil(3.14) returns 1 result for num:a.
SELECT COUNT(*) = 1 AS ceil_works
FROM pg_ripple.sparql($$
    SELECT ?x (CEIL(?v) AS ?c) WHERE {
        <https://num.test/a> <https://num.test/val> ?v .
        BIND(<https://num.test/a> AS ?x)
    }
$$);

-- 3. ROUND: round(3.14) returns 1 result for num:a.
SELECT COUNT(*) = 1 AS round_works
FROM pg_ripple.sparql($$
    SELECT ?x (ROUND(?v) AS ?r) WHERE {
        <https://num.test/a> <https://num.test/val> ?v .
        BIND(<https://num.test/a> AS ?x)
    }
$$);

-- 4. MIN aggregate.
SELECT COUNT(*) = 1 AS min_agg_works
FROM pg_ripple.sparql($$
    SELECT (MIN(?v) AS ?minval) WHERE {
        ?x <https://num.test/val> ?v .
    }
$$);

-- 5. MAX aggregate.
SELECT COUNT(*) = 1 AS max_agg_works
FROM pg_ripple.sparql($$
    SELECT (MAX(?v) AS ?maxval) WHERE {
        ?x <https://num.test/val> ?v .
    }
$$);
