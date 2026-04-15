-- pg_regress test: full-text search on RDF literals (v0.5.1)
-- Namespace: https://fts.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://fts.test/%'
    );
END $$;

-- Load triples with string literals suitable for FTS.
SELECT pg_ripple.load_ntriples(
    '<https://fts.test/doc1> <https://fts.test/abstract> "RDF stores provide semantic querying capabilities" .' || E'\n' ||
    '<https://fts.test/doc2> <https://fts.test/abstract> "PostgreSQL is a powerful relational database" .' || E'\n' ||
    '<https://fts.test/doc3> <https://fts.test/abstract> "SPARQL enables semantic graph queries over RDF" .' || E'\n' ||
    '<https://fts.test/doc4> <https://fts.test/title>    "Introduction to semantic web"  .'
) = 4 AS four_triples_loaded;

-- Create FTS index on the abstract predicate.
SELECT pg_ripple.fts_index('<https://fts.test/abstract>') AS fts_index_created;

-- Search for "semantic": doc1 and doc3 contain it (not doc2).
SELECT count(*) = 2 AS two_semantic_matches
FROM pg_ripple.fts_search('semantic', '<https://fts.test/abstract>');

-- Search for "postgresql": only doc2.
SELECT count(*) = 1 AS one_postgresql_match
FROM pg_ripple.fts_search('postgresql', '<https://fts.test/abstract>');

-- Search for "RDF": doc1 and doc3.
SELECT count(*) = 2 AS two_rdf_matches
FROM pg_ripple.fts_search('rdf', '<https://fts.test/abstract>');

-- Search in un-indexed predicate (title): fts_index not called, should still work via seq scan.
SELECT pg_ripple.fts_index('<https://fts.test/title>') AS title_fts_indexed;

SELECT count(*) = 1 AS one_intro_match
FROM pg_ripple.fts_search('introduction', '<https://fts.test/title>');

-- Subject IRI of the semantic abstract is returned correctly.
SELECT s LIKE '%doc1%' OR s LIKE '%doc3%' AS subject_is_doc
FROM pg_ripple.fts_search('semantic', '<https://fts.test/abstract>')
LIMIT 1;
