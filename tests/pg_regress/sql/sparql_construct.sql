-- pg_regress test: SPARQL CONSTRUCT queries (v0.5.1)
-- Namespace: https://construct.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://construct.test/%'
    );
END $$;

-- Load test data.
SELECT pg_ripple.load_ntriples(
    '<https://construct.test/alice> <https://construct.test/knows> <https://construct.test/bob> .' || E'\n' ||
    '<https://construct.test/bob>   <https://construct.test/knows> <https://construct.test/carol> .' || E'\n' ||
    '<https://construct.test/alice> <https://construct.test/name>  "Alice" .' || E'\n' ||
    '<https://construct.test/bob>   <https://construct.test/name>  "Bob" .'
) = 4 AS four_triples_loaded;

-- CONSTRUCT with explicit template: build inverse knows triples.
SELECT count(*) = 2 AS two_inverse_triples
FROM pg_ripple.sparql_construct(
    'CONSTRUCT { ?b <https://construct.test/knownBy> ?a }
     WHERE { ?a <https://construct.test/knows> ?b }'
);

-- CONSTRUCT WHERE (bare form): returns original pattern triples.
SELECT count(*) >= 1 AS construct_where_works
FROM pg_ripple.sparql_construct(
    'CONSTRUCT WHERE { <https://construct.test/alice> <https://construct.test/knows> ?o }'
);

-- CONSTRUCT result contains expected subject / predicate strings.
SELECT (result->>'s') LIKE '%bob%' AND (result->>'p') LIKE '%knownBy%'
    AS inverse_triple_is_correct
FROM pg_ripple.sparql_construct(
    'CONSTRUCT { ?b <https://construct.test/knownBy> ?a }
     WHERE { ?a <https://construct.test/knows> ?b }
         ORDER BY ?a LIMIT 1'
);

-- ── v0.9.0: CONSTRUCT / DESCRIBE Turtle and JSON-LD output formats ────────────

-- CONSTRUCT → Turtle: returns TEXT, contains triple dot.
SELECT pg_ripple.sparql_construct_turtle(
    'CONSTRUCT { ?b <https://construct.test/knownBy> ?a }
     WHERE { ?a <https://construct.test/knows> ?b }'
) LIKE '% .' AS construct_turtle_has_dot;

-- CONSTRUCT → Turtle: contains IRI from test data.
SELECT pg_ripple.sparql_construct_turtle(
    'CONSTRUCT { ?s <https://construct.test/knows> ?o }
     WHERE { ?s <https://construct.test/knows> ?o }'
) LIKE '%construct.test%' AS construct_turtle_has_iri;

-- CONSTRUCT → JSON-LD: returns JSONB array.
SELECT jsonb_typeof(pg_ripple.sparql_construct_jsonld(
    'CONSTRUCT { ?s <https://construct.test/knows> ?o }
     WHERE { ?s <https://construct.test/knows> ?o }'
)) = 'array' AS construct_jsonld_is_array;

-- CONSTRUCT → JSON-LD: array entries have @id.
SELECT count(*) >= 1 AS construct_jsonld_has_id_entries
FROM jsonb_array_elements(pg_ripple.sparql_construct_jsonld(
    'CONSTRUCT { ?s <https://construct.test/knows> ?o }
     WHERE { ?s <https://construct.test/knows> ?o }'
)) AS elem
WHERE elem ? '@id';

-- DESCRIBE → Turtle: contains alice IRI.
SELECT pg_ripple.sparql_describe_turtle(
    'DESCRIBE <https://construct.test/alice>'
) LIKE '%construct.test/alice%' AS describe_turtle_has_alice;

-- DESCRIBE → JSON-LD: returns JSONB array.
SELECT jsonb_typeof(pg_ripple.sparql_describe_jsonld(
    'DESCRIBE <https://construct.test/alice>'
)) = 'array' AS describe_jsonld_is_array;
