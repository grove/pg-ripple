-- load_into_graph.sql — pg_regress test for graph-aware bulk loaders (v0.15.0)
--
-- Tests: load_ntriples_into_graph, load_turtle_into_graph, load_rdfxml_into_graph,
--        find_triples_in_graph, triple_count_in_graph

-- Ensure clean state
SELECT pg_ripple.drop_graph('https://example.org/graph1');
SELECT pg_ripple.drop_graph('https://example.org/graph2');
SELECT pg_ripple.drop_graph('https://example.org/graph3');

-- ── Load N-Triples into a named graph ────────────────────────────────────────

SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
<https://example.org/alice> <https://example.org/name> "Alice" .
',
    'https://example.org/graph1'
) AS ntriples_loaded;

-- Verify triples are in the named graph
SELECT pg_ripple.triple_count_in_graph('https://example.org/graph1') AS graph1_count;

-- Verify triples appear via find_triples_in_graph
SELECT * FROM pg_ripple.find_triples_in_graph(NULL, NULL, NULL, 'https://example.org/graph1') ORDER BY s, p;

-- ── Load Turtle into a named graph ───────────────────────────────────────────

SELECT pg_ripple.load_turtle_into_graph(
    '@prefix ex: <https://example.org/> .
ex:carol ex:knows ex:dave .
ex:carol ex:age "30" .
',
    'https://example.org/graph2'
) AS turtle_loaded;

SELECT pg_ripple.triple_count_in_graph('https://example.org/graph2') AS graph2_count;

-- ── Load RDF/XML into a named graph ──────────────────────────────────────────

SELECT pg_ripple.load_rdfxml_into_graph(
    '<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:ex="https://example.org/">
  <rdf:Description rdf:about="https://example.org/eve">
    <ex:knows rdf:resource="https://example.org/frank"/>
  </rdf:Description>
</rdf:RDF>',
    'https://example.org/graph3'
) AS rdfxml_loaded;

SELECT pg_ripple.triple_count_in_graph('https://example.org/graph3') AS graph3_count;

-- ── Verify SPARQL can see the named graph data ──────────────────────────────

SELECT result FROM pg_ripple.sparql('
    SELECT ?s ?o WHERE {
        GRAPH <https://example.org/graph1> {
            ?s <https://example.org/knows> ?o
        }
    }
');

-- ── Verify graphs are isolated ───────────────────────────────────────────────

-- graph2 should not contain graph1's triples
SELECT result FROM pg_ripple.sparql('
    SELECT (COUNT(*) AS ?c) WHERE {
        GRAPH <https://example.org/graph2> {
            <https://example.org/alice> ?p ?o
        }
    }
');

-- Cleanup
SELECT pg_ripple.drop_graph('https://example.org/graph1');
SELECT pg_ripple.drop_graph('https://example.org/graph2');
SELECT pg_ripple.drop_graph('https://example.org/graph3');
