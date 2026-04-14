-- pg_regress test: named graphs

-- Create two named graphs
SELECT pg_ripple.create_graph('<https://example.org/graph1>') > 0 AS graph1_created;
SELECT pg_ripple.create_graph('<https://example.org/graph2>') > 0 AS graph2_created;

-- Load a triple into graph1 via N-Quads
SELECT pg_ripple.load_nquads(
    '<https://example.org/s1> <https://example.org/p> <https://example.org/o1> <https://example.org/graph1> .' || chr(10)
) = 1 AS loaded_graph1;

-- Load a triple into graph2 via N-Quads
SELECT pg_ripple.load_nquads(
    '<https://example.org/s2> <https://example.org/p> <https://example.org/o2> <https://example.org/graph2> .' || chr(10)
) = 1 AS loaded_graph2;

-- list_graphs should include both named graphs
SELECT count(*) >= 2 AS two_graphs_listed
FROM pg_ripple.list_graphs();

-- Both graph IRIs should appear
SELECT count(*) = 1 AS graph1_in_list
FROM pg_ripple.list_graphs()
WHERE graph_iri = '<https://example.org/graph1>';

SELECT count(*) = 1 AS graph2_in_list
FROM pg_ripple.list_graphs()
WHERE graph_iri = '<https://example.org/graph2>';

-- Drop graph1 — should delete only its triple
SELECT pg_ripple.drop_graph('<https://example.org/graph1>') = 1 AS graph1_dropped;

-- graph1 should no longer be in list_graphs
SELECT count(*) = 0 AS graph1_gone
FROM pg_ripple.list_graphs()
WHERE graph_iri = '<https://example.org/graph1>';

-- graph2 triple should still exist (total count includes default + graph2)
SELECT pg_ripple.triple_count() >= 1 AS triples_remain;
