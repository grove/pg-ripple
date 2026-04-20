-- Migration 0.42.0 → 0.43.0: Named-graph registry for empty-graph enumeration
--
-- Adds _pg_ripple.named_graphs table that tracks named graph IRIs that have
-- been explicitly loaded, even if the graph has zero triples. This is needed
-- for GRAPH ?var { COUNT(*) } queries to enumerate empty named graphs and
-- return count=0 (per SPARQL semantics) rather than omitting those graphs.
--
-- All graph-aware bulk loaders (load_turtle_into_graph, load_rdfxml_into_graph,
-- load_ntriples_into_graph, etc.) now register the graph IRI in this table.

CREATE TABLE IF NOT EXISTS _pg_ripple.named_graphs (
    graph_id BIGINT NOT NULL PRIMARY KEY
);
CREATE INDEX IF NOT EXISTS idx_named_graphs_id ON _pg_ripple.named_graphs (graph_id);
