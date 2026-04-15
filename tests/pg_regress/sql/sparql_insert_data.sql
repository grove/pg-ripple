-- pg_regress test: SPARQL INSERT DATA (v0.5.1)
-- Namespace: https://insert.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://insert.test/%'
    );
END $$;

-- Baseline count before any inserts.
SELECT count(*) = 0 AS no_insert_triples_yet
FROM pg_ripple.find_triples(NULL, '<https://insert.test/likes>', NULL);

-- INSERT DATA: single triple.
SELECT pg_ripple.sparql_update(
    'INSERT DATA { <https://insert.test/alice> <https://insert.test/likes> <https://insert.test/chess> }'
) = 1 AS one_triple_inserted;

-- Verify the triple is now queryable.
SELECT count(*) = 1 AS alice_likes_chess
FROM pg_ripple.find_triples('<https://insert.test/alice>', '<https://insert.test/likes>', NULL);

-- INSERT DATA: multiple triples in one statement.
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       <https://insert.test/bob>   <https://insert.test/likes> <https://insert.test/tennis> .
       <https://insert.test/carol> <https://insert.test/likes> <https://insert.test/chess>
     }'
) = 2 AS two_triples_inserted;

-- Total in store: 3 (alice+bob+carol).
SELECT count(*) = 3 AS three_likes_total
FROM pg_ripple.find_triples(NULL, '<https://insert.test/likes>', NULL);

-- INSERT DATA: literal object.
SELECT pg_ripple.sparql_update(
    'INSERT DATA { <https://insert.test/alice> <https://insert.test/score> "42"^^<http://www.w3.org/2001/XMLSchema#integer> }'
) = 1 AS literal_inserted;

-- Confirm SPARQL SELECT can see the inserted literal.
SELECT count(*) = 1 AS score_visible
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://insert.test/score> "42"^^<http://www.w3.org/2001/XMLSchema#integer> }'
);
