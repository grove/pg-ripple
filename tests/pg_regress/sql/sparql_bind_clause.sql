-- pg_regress test: SPARQL BIND and COALESCE edge cases

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test data.
SELECT pg_ripple.load_ntriples(
    '<https://bind.test/alice> <https://bind.test/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://bind.test/bob> <https://bind.test/age> "25"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://bind.test/carol> <https://bind.test/name> "Carol" .'
) = 3 AS three_triples_loaded;

-- 1. BIND with arithmetic.
SELECT COUNT(*) = 2 AS bind_arithmetic_works
FROM pg_ripple.sparql($$
    SELECT ?person ?decade WHERE {
        ?person <https://bind.test/age> ?age .
        BIND(?age / 10 AS ?decade)
        FILTER(?decade >= 2)
    }
$$);

-- 2. BIND with STR().
SELECT COUNT(*) = 2 AS bind_str_works
FROM pg_ripple.sparql($$
    SELECT ?person ?s WHERE {
        ?person <https://bind.test/age> ?age .
        BIND(STR(?age) AS ?s)
    }
$$);

-- 3. COALESCE returns first non-null binding.
SELECT COUNT(*) >= 1 AS coalesce_works
FROM pg_ripple.sparql($$
    SELECT ?person ?val WHERE {
        ?person <https://bind.test/name> ?val .
        FILTER(COALESCE(?val, "default") != "")
    }
$$);

-- 4. IF expression in BIND.
SELECT COUNT(*) = 2 AS if_bind_works
FROM pg_ripple.sparql($$
    SELECT ?person ?label WHERE {
        ?person <https://bind.test/age> ?age .
        BIND(IF(?age > 28, "senior", "junior") AS ?label)
    }
$$);
