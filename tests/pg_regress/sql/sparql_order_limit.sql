-- pg_regress test: SPARQL ORDER BY and LIMIT/OFFSET

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load ordered test data.
SELECT pg_ripple.load_ntriples(
    '<https://ord.test/a> <https://ord.test/rank> "3"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://ord.test/b> <https://ord.test/rank> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://ord.test/c> <https://ord.test/rank> "4"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://ord.test/d> <https://ord.test/rank> "2"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://ord.test/e> <https://ord.test/rank> "5"^^<http://www.w3.org/2001/XMLSchema#integer> .'
) = 5 AS five_triples_loaded;

-- 1. LIMIT returns correct number of rows.
SELECT COUNT(*) = 3 AS limit_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://ord.test/rank> ?r .
    }
    ORDER BY ?r
    LIMIT 3
$$);

-- 2. OFFSET skips rows.
SELECT COUNT(*) = 2 AS offset_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://ord.test/rank> ?r .
    }
    ORDER BY ?r
    LIMIT 10 OFFSET 3
$$);

-- 3. ORDER BY DESC.
SELECT COUNT(*) = 2 AS order_desc_limit_works
FROM pg_ripple.sparql($$
    SELECT ?x ?r WHERE {
        ?x <https://ord.test/rank> ?r .
    }
    ORDER BY DESC(?r)
    LIMIT 2
$$);

-- 4. Full result count without limit.
SELECT COUNT(*) = 5 AS full_count_ok
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://ord.test/rank> ?r .
    }
$$);
