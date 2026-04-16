-- pg_regress test: SPARQL Graph Management (LOAD/CLEAR/DROP/CREATE) (v0.12.0)
-- Namespace: https://graphmgmt.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://graphmgmt.test/%'
    );
END $$;

-- ── CREATE GRAPH ─────────────────────────────────────────────────────────────

-- CREATE GRAPH registers a named graph in the dictionary.
SELECT pg_ripple.sparql_update(
    'CREATE GRAPH <https://graphmgmt.test/g1>'
) = 0 AS create_graph_ok;

-- Creating an existing graph with SILENT is a no-op (does not error).
SELECT pg_ripple.sparql_update(
    'CREATE SILENT GRAPH <https://graphmgmt.test/g1>'
) = 0 AS create_silent_ok;

-- ── CLEAR GRAPH ──────────────────────────────────────────────────────────────

-- Insert triples into a named graph and the default graph.
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       GRAPH <https://graphmgmt.test/g1> {
         <https://graphmgmt.test/s1> <https://graphmgmt.test/p> <https://graphmgmt.test/o1> .
         <https://graphmgmt.test/s2> <https://graphmgmt.test/p> <https://graphmgmt.test/o2>
       }
     }'
) = 2 AS two_in_g1;

SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       <https://graphmgmt.test/s3> <https://graphmgmt.test/p> <https://graphmgmt.test/o3>
     }'
) = 1 AS one_in_default;

-- Named graph g1 has 2 triples.
SELECT count(*) = 2 AS g1_has_two
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { GRAPH <https://graphmgmt.test/g1> { ?s <https://graphmgmt.test/p> ?o } }'
);

-- Default graph has 1 triple.
SELECT count(*) = 1 AS default_has_one
FROM pg_ripple.find_triples(NULL, '<https://graphmgmt.test/p>', NULL);

-- CLEAR the named graph.
SELECT pg_ripple.sparql_update(
    'CLEAR GRAPH <https://graphmgmt.test/g1>'
) = 2 AS cleared_two;

-- Named graph g1 is now empty.
SELECT count(*) = 0 AS g1_empty
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { GRAPH <https://graphmgmt.test/g1> { ?s <https://graphmgmt.test/p> ?o } }'
);

-- Default graph still has its triple.
SELECT count(*) = 1 AS default_unchanged
FROM pg_ripple.find_triples(NULL, '<https://graphmgmt.test/p>', NULL);

-- CLEAR DEFAULT removes triples from the default graph.
SELECT pg_ripple.sparql_update(
    'CLEAR DEFAULT'
) >= 0 AS clear_default_ok;

SELECT count(*) = 0 AS default_empty
FROM pg_ripple.find_triples('<https://graphmgmt.test/s3>', '<https://graphmgmt.test/p>', NULL);

-- ── DROP GRAPH ────────────────────────────────────────────────────────────────

-- Insert fresh triples into g1, then DROP it.
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       GRAPH <https://graphmgmt.test/g1> {
         <https://graphmgmt.test/a> <https://graphmgmt.test/b> <https://graphmgmt.test/c>
       }
     }'
) = 1 AS one_in_g1_again;

SELECT pg_ripple.sparql_update(
    'DROP GRAPH <https://graphmgmt.test/g1>'
) = 1 AS drop_g1_deleted_one;

-- After DROP, g1 has no triples.
SELECT count(*) = 0 AS g1_dropped
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { GRAPH <https://graphmgmt.test/g1> { ?s ?p ?o } }'
);

-- DROP SILENT on non-existent graph is a no-op.
SELECT pg_ripple.sparql_update(
    'DROP SILENT GRAPH <https://graphmgmt.test/nonexistent>'
) = 0 AS drop_silent_ok;
