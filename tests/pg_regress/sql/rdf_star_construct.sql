-- pg_regress test: v0.9.0 RDF-star in CONSTRUCT / DESCRIBE output
-- Namespace: https://rdfstar.construct.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://rdfstar.construct.test/%'
    );
END $$;

-- Load test triples including an RDF-star quoted triple as object
SELECT pg_ripple.load_ntriples(
    '<https://rdfstar.construct.test/alice> <https://rdfstar.construct.test/knows> <https://rdfstar.construct.test/bob> .' || E'\n' ||
    '<https://rdfstar.construct.test/alice> <https://rdfstar.construct.test/name> "Alice" .' || E'\n' ||
    '<https://rdfstar.construct.test/bob>   <https://rdfstar.construct.test/name> "Bob" .' || E'\n' ||
    '<https://rdfstar.construct.test/stmt1> <https://rdfstar.construct.test/annotates> << <https://rdfstar.construct.test/alice> <https://rdfstar.construct.test/knows> <https://rdfstar.construct.test/bob> >> .'
) = 4 AS four_triples_loaded;

-- CONSTRUCT result as Turtle should contain the subjects
SELECT pg_ripple.sparql_construct_turtle(
    'CONSTRUCT { ?s <https://rdfstar.construct.test/knows> ?o }
     WHERE { ?s <https://rdfstar.construct.test/knows> ?o }'
) LIKE '%rdfstar.construct.test%' AS construct_turtle_has_content;

-- CONSTRUCT as Turtle contains a triple dot
SELECT pg_ripple.sparql_construct_turtle(
    'CONSTRUCT { ?s <https://rdfstar.construct.test/knows> ?o }
     WHERE { ?s <https://rdfstar.construct.test/knows> ?o }'
) LIKE '% .' AS construct_turtle_has_dot;

-- CONSTRUCT result as JSON-LD is an array
SELECT jsonb_typeof(pg_ripple.sparql_construct_jsonld(
    'CONSTRUCT { ?s <https://rdfstar.construct.test/knows> ?o }
     WHERE { ?s <https://rdfstar.construct.test/knows> ?o }'
)) = 'array' AS construct_jsonld_is_array;

-- CONSTRUCT JSON-LD contains @id
SELECT count(*) >= 1 AS construct_jsonld_has_id
FROM jsonb_array_elements(pg_ripple.sparql_construct_jsonld(
    'CONSTRUCT { ?s <https://rdfstar.construct.test/knows> ?o }
     WHERE { ?s <https://rdfstar.construct.test/knows> ?o }'
)) AS elem
WHERE elem ? '@id';

-- DESCRIBE as Turtle for alice should contain alice
SELECT pg_ripple.sparql_describe_turtle(
    'DESCRIBE <https://rdfstar.construct.test/alice>'
) LIKE '%rdfstar.construct.test/alice%' AS describe_turtle_has_alice;

-- DESCRIBE as JSON-LD is an array
SELECT jsonb_typeof(pg_ripple.sparql_describe_jsonld(
    'DESCRIBE <https://rdfstar.construct.test/alice>'
)) = 'array' AS describe_jsonld_is_array;

-- Verify that the RDF-star quoted triple was loaded (the annotates predicate exists)
SELECT count(*) >= 1 AS rdfstar_triple_loaded
FROM pg_ripple.find_triples(
    '<https://rdfstar.construct.test/stmt1>',
    '<https://rdfstar.construct.test/annotates>',
    NULL
);
