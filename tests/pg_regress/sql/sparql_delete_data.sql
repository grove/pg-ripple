-- pg_regress test: SPARQL DELETE DATA (v0.5.1)
-- Namespace: https://delete.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://delete.test/%'
    );
END $$;

-- Set up test data via INSERT DATA.
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       <https://delete.test/alice> <https://delete.test/member> <https://delete.test/groupA> .
       <https://delete.test/bob>   <https://delete.test/member> <https://delete.test/groupA> .
       <https://delete.test/carol> <https://delete.test/member> <https://delete.test/groupB>
     }'
) = 3 AS three_inserted;

-- Confirm triples present.
SELECT count(*) = 2 AS two_in_groupA
FROM pg_ripple.find_triples(NULL, '<https://delete.test/member>', '<https://delete.test/groupA>');

-- DELETE DATA: remove one triple.
SELECT pg_ripple.sparql_update(
    'DELETE DATA { <https://delete.test/bob> <https://delete.test/member> <https://delete.test/groupA> }'
) = 1 AS one_deleted;

-- Only alice remains in groupA.
SELECT count(*) = 1 AS one_in_groupA_after_delete
FROM pg_ripple.find_triples(NULL, '<https://delete.test/member>', '<https://delete.test/groupA>');

SELECT count(*) = 1 AS alice_still_in_groupA
FROM pg_ripple.find_triples('<https://delete.test/alice>', '<https://delete.test/member>', '<https://delete.test/groupA>');

-- carol was not deleted.
SELECT count(*) = 1 AS carol_still_in_groupB
FROM pg_ripple.find_triples('<https://delete.test/carol>', '<https://delete.test/member>', '<https://delete.test/groupB>');

-- DELETE DATA: attempt to delete a non-existent triple returns 0.
SELECT pg_ripple.sparql_update(
    'DELETE DATA { <https://delete.test/nobody> <https://delete.test/member> <https://delete.test/groupA> }'
) = 0 AS delete_nonexistent_returns_0;
