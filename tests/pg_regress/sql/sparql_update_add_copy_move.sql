-- pg_regress test: SPARQL Update ADD / COPY / MOVE (v0.48.0)
-- Tests all three graph management operations.

-- Setup: insert triples into a source named graph
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       GRAPH <http://example.org/source-graph> {
           <http://example.org/s1> <http://example.org/p1> <http://example.org/o1> .
           <http://example.org/s2> <http://example.org/p2> <http://example.org/o2> .
       }
    }'
);

-- Verify source graph has 2 triples
SELECT count(*) AS source_count
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/source-graph> { ?s ?p ?o }
     }'
);

-- Test ADD: copy source into target (source preserved)
SELECT pg_ripple.sparql_update(
    'ADD <http://example.org/source-graph> TO <http://example.org/target-add>'
) >= 0 AS add_returns_count;

-- ADD: target should now have triples
SELECT count(*) >= 2 AS add_target_has_triples
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/target-add> { ?s ?p ?o }
     }'
);

-- ADD: source should still have triples
SELECT count(*) >= 2 AS add_source_preserved
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/source-graph> { ?s ?p ?o }
     }'
);

-- Test COPY: clear target-copy then copy source to it
SELECT pg_ripple.sparql_update(
    'COPY <http://example.org/source-graph> TO <http://example.org/target-copy>'
) >= 0 AS copy_returns_count;

-- COPY: target should have triples
SELECT count(*) >= 2 AS copy_target_has_triples
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/target-copy> { ?s ?p ?o }
     }'
);

-- COPY: source should still have triples
SELECT count(*) >= 2 AS copy_source_preserved
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/source-graph> { ?s ?p ?o }
     }'
);

-- Test MOVE: move source-move to target-move
-- First populate a move source
SELECT pg_ripple.sparql_update(
    'INSERT DATA {
       GRAPH <http://example.org/source-move> {
           <http://example.org/m1> <http://example.org/mp1> <http://example.org/mo1> .
       }
    }'
);

SELECT pg_ripple.sparql_update(
    'MOVE <http://example.org/source-move> TO <http://example.org/target-move>'
) >= 0 AS move_returns_count;

-- MOVE: target should have triples
SELECT count(*) >= 1 AS move_target_has_triples
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/target-move> { ?s ?p ?o }
     }'
);

-- MOVE: source should be empty (or have 0 triples)
SELECT count(*) = 0 AS move_source_empty
FROM pg_ripple.sparql(
    'SELECT ?s ?p ?o WHERE {
       GRAPH <http://example.org/source-move> { ?s ?p ?o }
     }'
);
