# Data Modeling

## When to use RDF-star vs reification

**Reification** (traditional RDF) represents a triple as a resource with four properties (`rdf:subject`, `rdf:predicate`, `rdf:object`, `rdf:type rdf:Statement`). It requires four extra triples per annotated statement and produces verbose query patterns.

**RDF-star** uses a quoted triple `<< s p o >>` directly as a subject or object:

```
# RDF-star (compact)
<< <ex:alice> <ex:knows> <ex:bob> >> <ex:since> "2023-01-01"^^xsd:date .

# Reification (verbose — 4 extra triples)
<ex:stmt1> rdf:type      rdf:Statement ;
           rdf:subject   <ex:alice> ;
           rdf:predicate <ex:knows> ;
           rdf:object    <ex:bob> ;
           <ex:since>    "2023-01-01"^^xsd:date .
```

Use RDF-star for edge annotations (provenance, confidence, time ranges). Use reification only for legacy compatibility with stores that do not support RDF-star.

## Named graphs for partitioning

Named graphs are a lightweight way to partition data by source, time, or topic without changing the triple structure:

```sql
-- Load provenance data into separate graphs
SELECT pg_ripple.load_nquads('
<ex:alice> <ex:knows> <ex:bob> <ex:source1> .
<ex:alice> <ex:knows> <ex:bob> <ex:source2> .
');

-- Query within a specific graph
SELECT * FROM pg_ripple.sparql('
  SELECT ?s ?p ?o WHERE {
    GRAPH <ex:source1> { ?s ?p ?o }
  }
');
```

## Blank nodes

Blank nodes are useful for anonymous intermediate resources — nodes that don't need a globally-unique IRI. Common uses:

- **List encoding**: each list element is a blank node with `rdf:first` and `rdf:rest` predicates
- **Structured values**: a measurement with multiple facets (value, unit, uncertainty)
- **Intermediate join nodes**: n-ary relationships without reification

**Pitfall**: blank nodes have load-scope identity. `_:b0` in two separate `load_ntriples()` calls gets two different dictionary IDs. If you need stable cross-load blank nodes, use IRI-based identifiers instead.

## Subject-position vs object-position quoted triples

pg_ripple supports both:

```
# Object-position: annotating the statement as an object
<ex:carol> <ex:asserted> << <ex:alice> <ex:knows> <ex:bob> >> .

# Subject-position: the statement has properties
<< <ex:alice> <ex:knows> <ex:bob> >> <ex:since> "2023"^^xsd:gYear .
```

Both are stored via `encode_triple()` in the dictionary and can be retrieved with `decode_triple()` or `get_statement()`.

## LPG-style edge properties via RDF-star

RDF-star maps cleanly onto LPG edge properties:

| LPG concept | RDF-star encoding |
|---|---|
| Node `alice` | `<ex:alice>` |
| Edge `alice --[KNOWS]--> bob` | `<ex:alice> <ex:knows> <ex:bob>` |
| Edge property `since = 2023` | `<< <ex:alice> <ex:knows> <ex:bob> >> <ex:since> "2023"^^xsd:gYear` |
| Node property `name = "Alice"` | `<ex:alice> <ex:name> "Alice"` |

This makes pg_ripple a natural backend for LPG data once the Cypher/GQL query layer is added (v0.13.0).

## Interop format guide (v0.9.0)

Choose the right serialization format for the tool or context you are integrating with:

| Tool / Context | Recommended format | pg_ripple function |
|---|---|---|
| Protégé / OWL ontologies | RDF/XML | `load_rdfxml()` |
| Linked Data Platform (LDP) REST APIs | JSON-LD | `export_jsonld()` / `sparql_construct_jsonld()` |
| Command-line pipelines, streaming | N-Triples or N-Quads | `export_ntriples()` / `export_nquads()` |
| Human-readable files, Git storage | Turtle | `export_turtle()` / `sparql_construct_turtle()` |
| Large graph export (memory-efficient) | Streaming Turtle | `export_turtle_stream()` |
| SPARQL query results for APIs | JSON-LD CONSTRUCT | `sparql_construct_jsonld()` |

### Protégé → RDF/XML

Protégé saves ontologies in OWL/RDF/XML by default.  Load them directly:

```sql
-- Read the file into PostgreSQL (superuser only)
SELECT pg_ripple.load_rdfxml(pg_read_file('/data/ontology.owl'));
```

### Linked Data Platform → JSON-LD

REST APIs built on LDP typically serve JSON-LD.  Use `export_jsonld()` to get the current state:

```sql
SELECT pg_ripple.export_jsonld('https://myapp.example.org/graph/users');
```

For SPARQL-driven responses:

```sql
SELECT pg_ripple.sparql_construct_jsonld('
  CONSTRUCT { ?s ?p ?o }
  WHERE     { ?s a <https://schema.org/Person> ; ?p ?o }
');
```

### CLI / shell pipelines → N-Triples or N-Quads

For processing with `rapper`, `riot`, `rdfpipe`, or `awk`/`grep` on the command line:

```bash
psql -c "COPY (SELECT pg_ripple.export_ntriples()) TO STDOUT" > snapshot.nt
```

For multi-graph exports:

```bash
psql -c "COPY (SELECT pg_ripple.export_nquads(NULL)) TO STDOUT" > snapshot.nq
```

