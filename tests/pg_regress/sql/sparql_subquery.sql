-- pg_regress test: SPARQL subqueries (SELECT inside SELECT)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test data.
SELECT pg_ripple.load_ntriples(
    '<https://subq.test/a> <https://subq.test/score> "10"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://subq.test/a> <https://subq.test/type> <https://subq.test/Widget> .' || E'\n' ||
    '<https://subq.test/b> <https://subq.test/score> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://subq.test/b> <https://subq.test/type> <https://subq.test/Widget> .' || E'\n' ||
    '<https://subq.test/c> <https://subq.test/score> "20"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://subq.test/c> <https://subq.test/type> <https://subq.test/Gadget> .'
) = 6 AS six_triples_loaded;

-- 1. Subquery with LIMIT inside outer query.
SELECT COUNT(*) >= 1 AS subquery_limit_works
FROM pg_ripple.sparql($$
    SELECT ?item ?score WHERE {
        ?item <https://subq.test/type> <https://subq.test/Widget> .
        {
            SELECT ?item ?score WHERE {
                ?item <https://subq.test/score> ?score .
            }
            ORDER BY DESC(?score)
            LIMIT 2
        }
    }
$$);

-- 2. Subquery for aggregation inside outer query.
SELECT COUNT(*) = 2 AS subquery_agg_works
FROM pg_ripple.sparql($$
    SELECT ?item WHERE {
        ?item <https://subq.test/type> <https://subq.test/Widget> .
        {
            SELECT (AVG(?s) AS ?avg) WHERE {
                ?x <https://subq.test/score> ?s .
            }
        }
    }
$$);

-- 3. Triple count is unchanged after read-only queries.
SELECT pg_ripple.triple_count() >= 6 AS triple_count_ok;
