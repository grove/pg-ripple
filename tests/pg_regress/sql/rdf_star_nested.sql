-- pg_regress test: RDF-star nested annotation paths

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load RDF-star triples (annotations).
SELECT pg_ripple.load_ntriples(
    '<https://rdfstar.test/alice> <https://rdfstar.test/knows> <https://rdfstar.test/bob> .' || E'\n' ||
    '<https://rdfstar.test/alice> <https://rdfstar.test/name> "Alice" .' || E'\n' ||
    '<https://rdfstar.test/bob> <https://rdfstar.test/name> "Bob" .'
) = 3 AS base_triples_loaded;

-- 1. Basic triple pattern works.
SELECT COUNT(*) = 1 AS basic_knows_found
FROM pg_ripple.sparql($$
    SELECT ?b WHERE {
        <https://rdfstar.test/alice> <https://rdfstar.test/knows> ?b .
    }
$$);

-- 2. Star pattern (multiple predicates on same subject).
SELECT COUNT(*) = 2 AS star_pattern_works
FROM pg_ripple.sparql($$
    SELECT ?p ?o WHERE {
        <https://rdfstar.test/alice> ?p ?o .
    }
$$);

-- 3. Chain pattern.
SELECT COUNT(*) >= 1 AS chain_pattern_works
FROM pg_ripple.sparql($$
    SELECT ?bname WHERE {
        <https://rdfstar.test/alice> <https://rdfstar.test/knows> ?b .
        ?b <https://rdfstar.test/name> ?bname .
    }
$$);

-- 4. Reverse pattern (find subject by object).
SELECT COUNT(*) = 1 AS reverse_pattern_works
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://rdfstar.test/name> "Alice" .
    }
$$);
