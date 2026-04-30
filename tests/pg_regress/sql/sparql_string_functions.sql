-- pg_regress test: SPARQL string functions (STRLEN, SUBSTR, REPLACE, UCASE, LCASE)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test data.
SELECT pg_ripple.load_ntriples(
    '<https://strfn.test/a> <https://strfn.test/name> "Hello World" .' || E'\n' ||
    '<https://strfn.test/b> <https://strfn.test/name> "foo bar" .' || E'\n' ||
    '<https://strfn.test/c> <https://strfn.test/name> "Test123" .'
) = 3 AS three_triples_loaded;

-- 1. STRLEN filter via BIND.
SELECT COUNT(*) = 3 AS strlen_filter_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://strfn.test/name> ?n .
        BIND(STRLEN(?n) AS ?len)
        FILTER(?len > 6)
    }
$$);

-- 2. UCASE transformation.
SELECT COUNT(*) = 3 AS ucase_works
FROM pg_ripple.sparql($$
    SELECT ?x ?upper WHERE {
        ?x <https://strfn.test/name> ?n .
        BIND(UCASE(?n) AS ?upper)
    }
$$);

-- 3. LCASE transformation.
SELECT COUNT(*) = 3 AS lcase_works
FROM pg_ripple.sparql($$
    SELECT ?x ?lower WHERE {
        ?x <https://strfn.test/name> ?n .
        BIND(LCASE(?n) AS ?lower)
    }
$$);

-- 4. CONTAINS filter.
SELECT COUNT(*) = 1 AS contains_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://strfn.test/name> ?n .
        FILTER(CONTAINS(?n, "World"))
    }
$$);

-- 5. STRSTARTS filter.
SELECT COUNT(*) = 1 AS strstarts_works
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        ?x <https://strfn.test/name> ?n .
        FILTER(STRSTARTS(?n, "Test"))
    }
$$);
