# Bulk Loading

pg_ripple provides eight bulk-load functions covering four RDF serialization formats in both inline-string and file variants.

## load_ntriples

```sql
pg_ripple.load_ntriples(data TEXT) RETURNS BIGINT
```

Parses and loads N-Triples data from a string. Returns the number of triples loaded.

```sql
SELECT pg_ripple.load_ntriples('
<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
<https://example.org/bob>   <https://example.org/name>  "Bob" .
');
-- Returns 2
```

## load_ntriples_file

```sql
pg_ripple.load_ntriples_file(path TEXT) RETURNS BIGINT
```

Loads N-Triples data from a server-side file. The path must be readable by the PostgreSQL server process.

```sql
SELECT pg_ripple.load_ntriples_file('/data/dataset.nt');
```

## load_turtle / load_turtle_file

```sql
pg_ripple.load_turtle(data TEXT) RETURNS BIGINT
pg_ripple.load_turtle_file(path TEXT) RETURNS BIGINT
```

Parses and loads [Turtle](https://www.w3.org/TR/turtle/) data. Turtle supports prefix declarations (`@prefix`) and concise syntax for related triples.

```sql
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .

ex:alice ex:knows ex:bob ;
         ex:name  "Alice" .
');
```

## load_nquads / load_nquads_file

```sql
pg_ripple.load_nquads(data TEXT) RETURNS BIGINT
pg_ripple.load_nquads_file(path TEXT) RETURNS BIGINT
```

Loads [N-Quads](https://www.w3.org/TR/n-quads/) data. N-Quads extend N-Triples with an optional fourth component (the named graph). Triples without a graph component go into the default graph.

```sql
SELECT pg_ripple.load_nquads('
<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> <https://example.org/graph1> .
<https://example.org/bob>   <https://example.org/name>  "Bob" .
');
```

## load_trig / load_trig_file

```sql
pg_ripple.load_trig(data TEXT) RETURNS BIGINT
pg_ripple.load_trig_file(path TEXT) RETURNS BIGINT
```

Loads [TriG](https://www.w3.org/TR/trig/) data. TriG is the graph-aware extension of Turtle — triples can be grouped inside named `GRAPH { }` blocks.

```sql
SELECT pg_ripple.load_trig('
@prefix ex: <https://example.org/> .

GRAPH ex:graph1 {
    ex:alice ex:knows ex:bob .
}

ex:alice ex:name "Alice" .
');
```

## Graph-aware loaders (v0.15.0)

Starting with v0.15.0, each format has a graph-aware variant that loads triples directly into a named graph. File variants are also available (superuser-only).

### Inline loaders

```sql
pg_ripple.load_ntriples_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT
pg_ripple.load_turtle_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT
pg_ripple.load_rdfxml_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_turtle_into_graph('
@prefix ex: <https://example.org/> .
ex:alice ex:knows ex:bob .
', '<https://example.org/graph1>');
-- Returns: 1
```

### File loaders

```sql
pg_ripple.load_ntriples_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT
pg_ripple.load_turtle_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT
pg_ripple.load_rdfxml_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_ntriples_file_into_graph(
    '/data/people.nt',
    '<https://example.org/people>'
);
```

### RDF/XML loader (v0.9.0)

```sql
pg_ripple.load_rdfxml(data TEXT) RETURNS BIGINT
pg_ripple.load_rdfxml_file(path TEXT) RETURNS BIGINT
```

Parses conformant RDF/XML — the format produced by Protégé, OWL editors, and many ontology tools.

```sql
SELECT pg_ripple.load_rdfxml('
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:ex="https://example.org/">
  <rdf:Description rdf:about="https://example.org/alice">
    <ex:name>Alice</ex:name>
  </rdf:Description>
</rdf:RDF>
');
```

---

## Blank-node scoping

Each call to a bulk-load function is an independent **blank-node scope**. `_:b0` in one `load_ntriples()` call and `_:b0` in a later call get different dictionary IDs. This matches the SPARQL/RDF blank-node scoping rules and prevents accidental merging of blank nodes across document loads.

## VP promotion after bulk loads

After loading a large dataset, some predicates may cross the `pg_ripple.vp_promotion_threshold` and be automatically promoted from `vp_rare` to a dedicated VP table. This promotion is triggered automatically.

After any large load, run `ANALYZE` to help the PostgreSQL planner choose efficient join strategies:

```sql
ANALYZE _pg_ripple.vp_rare;
```

Or analyze all VP tables at once with a server-side function (available in v0.6.0+):

```sql
SELECT pg_ripple.vacuum_analyze();
```

---

## Strict mode (v0.25.0)

All bulk-load functions accept an optional `strict BOOLEAN DEFAULT false` parameter.

| Mode | Behaviour on malformed triple |
|---|---|
| `strict = false` (default) | Malformed triples emit a `WARNING` and are skipped; the remaining triples are committed |
| `strict = true` | Any parse error aborts the entire load with an `ERROR`; the transaction is rolled back |

### Examples

```sql
-- Lenient mode (default): one bad triple is skipped, others are committed
SELECT pg_ripple.load_ntriples(
    '<https://example.org/s> <https://example.org/p> <https://example.org/o> .' || chr(10) ||
    'this is malformed' || chr(10)
);
-- WARNING: skipping malformed triple on line 2
-- Result: 1

-- Strict mode: bad triple causes rollback of the entire load
SELECT pg_ripple.load_ntriples(
    '<https://example.org/s> <https://example.org/p> <https://example.org/o> .' || chr(10) ||
    'this is malformed' || chr(10),
    strict => true
);
-- ERROR: malformed triple on line 2: [...]
-- All triples from this call are rolled back
```

### When to use strict mode

- **Data pipelines**: use `strict = true` to detect upstream data quality issues early.
- **Interactive exploration**: use `strict = false` (default) to load partial data and investigate.
- **Migration**: use `strict = true` to ensure all data was loaded, then `strict = false` for recovery.

---

## File-path security (v0.25.0)

The `load_*_file()` variants (`load_turtle_file`, `load_ntriples_file`, etc.) resolve symlinks using `realpath()` and verify the canonical path lies within the PostgreSQL data directory before reading. This prevents symlink-based path traversal attacks:

```sql
-- Rejected: outside the data directory
SELECT pg_ripple.load_turtle_file('/etc/passwd');
-- ERROR: permission denied: "/etc/passwd" is outside the database cluster directory
```

Only files within `current_setting('data_directory')` can be loaded.

