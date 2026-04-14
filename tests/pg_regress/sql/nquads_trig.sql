-- pg_regress test: N-Quads round-trip and TriG named-graph import

-- Load N-Quads with named graphs
SELECT pg_ripple.load_nquads(
    '<https://example.org/a> <https://example.org/b> <https://example.org/c> <https://example.org/g1> .' || chr(10) ||
    '<https://example.org/d> <https://example.org/e> <https://example.org/f> <https://example.org/g2> .' || chr(10) ||
    '<https://example.org/x> <https://example.org/y> <https://example.org/z> .' || chr(10)
) = 3 AS nquads_loaded;

-- List graphs shows at least two named graphs (may include graphs from earlier tests)
SELECT count(*) >= 2 AS two_or_more_named_graphs
FROM pg_ripple.list_graphs();

-- Export all as N-Quads should include graph column for named-graph triples
SELECT pg_ripple.export_nquads(NULL) LIKE '%<https://example.org/g1>%' AS nquads_export_has_g1;
SELECT pg_ripple.export_nquads(NULL) LIKE '%<https://example.org/g2>%' AS nquads_export_has_g2;

-- Export single graph
SELECT pg_ripple.export_nquads('<https://example.org/g1>') LIKE '%<https://example.org/a>%' AS g1_export_has_a;

-- Triple count matches
SELECT pg_ripple.triple_count() >= 3 AS count_at_least_three;

-- TriG load: named graph via GRAPH {} block
SELECT pg_ripple.load_trig(
    '@prefix ex: <https://example.org/> .' || chr(10) ||
    'GRAPH ex:trig_graph {' || chr(10) ||
    '  ex:p1 ex:q1 ex:r1 .' || chr(10) ||
    '}' || chr(10)
) = 1 AS trig_loaded;

SELECT count(*) >= 1 AS trig_graph_listed
FROM pg_ripple.list_graphs()
WHERE graph_iri = '<https://example.org/trig_graph>';

-- Blank node document-scoping: same blank node in two loads must get different IDs
SELECT pg_ripple.load_ntriples(
    '_:b0 <https://example.org/type> <https://example.org/Thing> .' || chr(10)
) = 1 AS bnode_load1;

SELECT pg_ripple.load_ntriples(
    '_:b0 <https://example.org/type> <https://example.org/Thing> .' || chr(10)
) = 1 AS bnode_load2;

-- Two loads of the same blank node label should produce at least 2 triples
-- (because they are scoped to different generations and get different IDs)
SELECT count(*) >= 2 AS bnode_isolation
FROM pg_ripple.find_triples(NULL, '<https://example.org/type>', '<https://example.org/Thing>');
