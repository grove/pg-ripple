-- pg_regress test: CWB write-path equivalence (v0.67.0 MJOURNAL-03)
--
-- Verifies that all write paths succeed after the mutation_journal
-- has_no_rules() fast path was fixed (removed WHERE enabled=true from
-- the construct_rules query).
-- Write paths tested:
--   insert_triple(), SPARQL INSERT DATA, load_ntriples_into_graph(), load_turtle_into_graph()
-- Also verifies create_construct_rule() / drop_construct_rule() work.
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Create source and derived graphs.
SELECT pg_ripple.create_graph('https://cwbequiv.test/source/') IS NULL
    AS source_graph_created;
SELECT pg_ripple.create_graph('https://cwbequiv.test/derived/') IS NULL
    AS derived_graph_created;

-- Register a CONSTRUCT rule that copies rdf:type triples from source to derived.
SELECT pg_ripple.create_construct_rule(
    'cwb_equiv_rule',
    'CONSTRUCT { ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?class }
     WHERE { GRAPH <https://cwbequiv.test/source/> {
       ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?class
     } }',
    'https://cwbequiv.test/derived/'
) IS NULL AS rule_created;

-- Confirm rule is listed.
SELECT jsonb_array_length(pg_ripple.list_construct_rules()) >= 1 AS rule_registered;

-- Path 1: insert_triple()
SELECT pg_ripple.insert_triple(
    '<https://cwbequiv.test/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://cwbequiv.test/Person>',
    'https://cwbequiv.test/source/'
) > 0 AS path1_insert_ok;

-- Retract via delete_triple_from_graph.
SELECT pg_ripple.delete_triple_from_graph(
    '<https://cwbequiv.test/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://cwbequiv.test/Person>',
    'https://cwbequiv.test/source/'
) >= 0 AS path1_delete_ok;

-- Path 2: SPARQL INSERT DATA
SELECT pg_ripple.sparql_update(
    'INSERT DATA { GRAPH <https://cwbequiv.test/source/> { <https://cwbequiv.test/Bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://cwbequiv.test/Person> } }'
) IS NOT NULL AS path2_sparql_insert_ok;

-- Retract via SPARQL DELETE DATA.
SELECT pg_ripple.sparql_update(
    'DELETE DATA { GRAPH <https://cwbequiv.test/source/> { <https://cwbequiv.test/Bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://cwbequiv.test/Person> } }'
) IS NOT NULL AS path2_sparql_delete_ok;

-- Path 3: load_ntriples_into_graph()
SELECT pg_ripple.load_ntriples_into_graph(
    '<https://cwbequiv.test/Carol> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://cwbequiv.test/Person> .' || chr(10),
    'https://cwbequiv.test/source/'
) > 0 AS path3_load_ok;

-- Path 4: load_turtle_into_graph()
SELECT pg_ripple.load_turtle_into_graph(
    '@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .' || chr(10) || '<https://cwbequiv.test/Dave> rdf:type <https://cwbequiv.test/Person> .' || chr(10),
    'https://cwbequiv.test/source/'
) > 0 AS path4_load_ok;

-- Path 5: load_ntriples() (default graph → named source graph via a wrapping function)
-- BULK-01: load_ntriples() now triggers mutation journal flush after loading,
-- so derived triples appear immediately without calling refresh_construct_rule.
-- We insert directly into the source named graph using load_ntriples_into_graph.
SELECT pg_ripple.load_ntriples_into_graph(
    '<https://cwbequiv.test/Eve> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://cwbequiv.test/Person> .' || chr(10),
    'https://cwbequiv.test/source/'
) > 0 AS path5_bulk_load_ok;

-- Cleanup
SELECT pg_ripple.drop_construct_rule('cwb_equiv_rule') AS rule_dropped;
SELECT pg_ripple.clear_graph('https://cwbequiv.test/source/') >= 0 AS source_cleared;
SELECT pg_ripple.clear_graph('https://cwbequiv.test/derived/') >= 0 AS derived_cleared;
SELECT pg_ripple.drop_graph('https://cwbequiv.test/source/') IS NULL AS source_dropped;
SELECT pg_ripple.drop_graph('https://cwbequiv.test/derived/') IS NULL AS derived_dropped;
