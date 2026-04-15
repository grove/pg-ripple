# Named Graphs

pg_ripple supports the RDF concept of named graphs: each triple belongs to either the **default graph** (internal ID `0`) or a named graph identified by an IRI.

Named graphs are stored as part of each VP table row (`g BIGINT` column). No separate graph-catalog table is required — the set of named graphs is derived from the data.

## create_graph

```sql
pg_ripple.create_graph(graph_iri TEXT) RETURNS BIGINT
```

Registers a named graph in the dictionary and returns its `BIGINT` ID. This is informational only — graphs are created implicitly when triples are inserted. Calling `create_graph` explicitly is useful when you want a graph ID before loading data.

```sql
SELECT pg_ripple.create_graph('<https://example.org/graph1>');
-- Returns the dictionary ID for the graph IRI
```

## drop_graph

```sql
pg_ripple.drop_graph(graph_iri TEXT) RETURNS BIGINT
```

Deletes all triples belonging to the named graph. Returns the number of triples deleted.

```sql
SELECT pg_ripple.drop_graph('<https://example.org/graph1>');
```

> **Warning**: This permanently deletes all triples in the graph. The operation is transactional — wrap it in `BEGIN`/`ROLLBACK` if you need a dry run.

## list_graphs

```sql
pg_ripple.list_graphs() RETURNS TABLE(graph_iri TEXT, triple_count BIGINT)
```

Returns all named graphs and how many triples each contains. The default graph (ID 0) is excluded.

```sql
SELECT * FROM pg_ripple.list_graphs();
```

## Querying named graphs in SPARQL

Use the `GRAPH` keyword to restrict patterns to a specific graph:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?s ?p ?o WHERE {
    GRAPH <https://example.org/graph1> { ?s ?p ?o }
  }
');
```

To query across all named graphs:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?g ?s ?p ?o WHERE {
    GRAPH ?g { ?s ?p ?o }
  }
');
```

## Inserting into a named graph

```sql
-- Via insert_triple
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>',
    '<https://example.org/graph1>'  -- named graph
);

-- Via bulk load (N-Quads — fourth column is the graph)
SELECT pg_ripple.load_nquads('
<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> <https://example.org/graph1> .
');

-- Via TriG
SELECT pg_ripple.load_trig('
GRAPH <https://example.org/graph1> {
  <https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
}
');
```

## Default graph

The default graph has internal ID `0`. Triples inserted without a graph argument land in the default graph. SPARQL patterns without a `GRAPH` clause match against the default graph.
