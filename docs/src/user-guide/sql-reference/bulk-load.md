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
