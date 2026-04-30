-- pg_regress test: SPARQL property path alternation and sequence

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load test graph with multiple edge types.
SELECT pg_ripple.load_ntriples(
    '<https://ppath.test/a> <https://ppath.test/knows> <https://ppath.test/b> .' || E'\n' ||
    '<https://ppath.test/b> <https://ppath.test/likes> <https://ppath.test/c> .' || E'\n' ||
    '<https://ppath.test/a> <https://ppath.test/follows> <https://ppath.test/d> .' || E'\n' ||
    '<https://ppath.test/d> <https://ppath.test/knows> <https://ppath.test/e> .'
) = 4 AS four_path_triples;

-- 1. Sequence path: a knows/likes c (two-hop sequence).
SELECT COUNT(*) = 1 AS sequence_path_works
FROM pg_ripple.sparql($$
    SELECT ?end WHERE {
        <https://ppath.test/a> <https://ppath.test/knows>/<https://ppath.test/likes> ?end .
    }
$$);

-- 2. Alternation path: a (knows|follows) any.
SELECT COUNT(*) = 2 AS alternation_path_works
FROM pg_ripple.sparql($$
    SELECT ?next WHERE {
        <https://ppath.test/a> (<https://ppath.test/knows>|<https://ppath.test/follows>) ?next .
    }
$$);

-- 3. Transitive closure (knows+).
SELECT COUNT(*) >= 1 AS transitive_closure_works
FROM pg_ripple.sparql($$
    SELECT ?reachable WHERE {
        <https://ppath.test/a> <https://ppath.test/knows>+ ?reachable .
    }
$$);

-- 4. Zero-or-more path includes starting node.
SELECT COUNT(*) >= 1 AS zero_or_more_works
FROM pg_ripple.sparql($$
    SELECT ?node WHERE {
        <https://ppath.test/a> <https://ppath.test/knows>* ?node .
    }
$$);
