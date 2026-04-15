# Serialization & Export

pg_ripple (v0.9.0) supports exporting RDF data to Turtle, JSON-LD, N-Triples, and N-Quads formats, and importing from RDF/XML.  SPARQL CONSTRUCT and DESCRIBE queries can also return results directly in Turtle or JSON-LD.

---

## Import

### load_rdfxml

```sql
pg_ripple.load_rdfxml(data TEXT) RETURNS BIGINT
```

Parses [RDF/XML](https://www.w3.org/TR/rdf-syntax-grammar/) data from a string and stores all triples in the default graph.  Returns the number of triples loaded.

RDF/XML is the original W3C-standard RDF serialization and is produced by many ontology editors such as [Protégé](https://protege.stanford.edu/).

```sql
SELECT pg_ripple.load_rdfxml('<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:ex="https://example.org/">
  <rdf:Description rdf:about="https://example.org/alice">
    <ex:name>Alice</ex:name>
    <ex:knows rdf:resource="https://example.org/bob"/>
  </rdf:Description>
</rdf:RDF>');
-- Returns 2
```

**Note:** RDF/XML does not support named graphs; all triples are loaded into the default graph.

---

## Export

### export_turtle

```sql
pg_ripple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT
```

Exports triples as a [Turtle](https://www.w3.org/TR/turtle/) document.  Triples are grouped by subject and emitted as compact Turtle blocks.  All prefix declarations from the prefix registry are included as `@prefix` lines.

RDF-star quoted triples are serialized in Turtle-star `<< s p o >>` notation.

```sql
-- Export the default graph
SELECT pg_ripple.export_turtle();

-- Export a named graph
SELECT pg_ripple.export_turtle('https://example.org/my-graph');
```

**Example output:**

```turtle
@prefix ex: <https://example.org/> .

<https://example.org/alice>
    <https://example.org/knows> <https://example.org/bob> ;
    <https://example.org/name> "Alice" .
```

### export_jsonld

```sql
pg_ripple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB
```

Exports triples as a [JSON-LD](https://www.w3.org/TR/json-ld11/) expanded-form document.  Each subject becomes one array entry with all its predicates and objects.

```sql
SELECT pg_ripple.export_jsonld();
-- Returns: [{"@id": "https://example.org/alice", "https://example.org/name": [{"@value": "Alice"}], ...}]
```

JSON-LD is well-suited for use in REST APIs and Linked Data Platform (LDP) contexts.

### export_ntriples

```sql
pg_ripple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT
```

Exports triples as [N-Triples](https://www.w3.org/TR/n-triples/) text (one triple per line).

### export_nquads

```sql
pg_ripple.export_nquads(graph TEXT DEFAULT NULL) RETURNS TEXT
```

Exports quads as [N-Quads](https://www.w3.org/TR/n-quads/) text.  Pass `NULL` to export all graphs.

---

## Streaming Export

For large graphs, use the streaming variants that return `SETOF TEXT` — one line per row.  This avoids building the full document in memory.

### export_turtle_stream

```sql
pg_ripple.export_turtle_stream(graph TEXT DEFAULT NULL) RETURNS SETOF TEXT
```

Yields `@prefix` declarations first, then one flat Turtle triple per line.

```sql
COPY (SELECT line FROM pg_ripple.export_turtle_stream()) TO '/tmp/output.ttl';
```

### export_jsonld_stream

```sql
pg_ripple.export_jsonld_stream(graph TEXT DEFAULT NULL) RETURNS SETOF TEXT
```

Yields one NDJSON line per subject.  Each line is a complete JSON object.

```sql
COPY (SELECT line FROM pg_ripple.export_jsonld_stream()) TO '/tmp/output.ndjson';
```

---

## SPARQL CONSTRUCT & DESCRIBE Output Formats

By default, `sparql_construct()` and `sparql_describe()` return JSONB rows.  The v0.9.0 format-specific variants return the same triples directly as Turtle or JSON-LD.

### sparql_construct_turtle

```sql
pg_ripple.sparql_construct_turtle(query TEXT) RETURNS TEXT
```

Executes a SPARQL CONSTRUCT query and returns the result as a Turtle document.  RDF-star quoted triples use Turtle-star `<< s p o >>` notation.

```sql
SELECT pg_ripple.sparql_construct_turtle('
  CONSTRUCT { ?s <https://schema.org/knows> ?o }
  WHERE     { ?s <https://schema.org/knows> ?o }
');
```

### sparql_construct_jsonld

```sql
pg_ripple.sparql_construct_jsonld(query TEXT) RETURNS JSONB
```

Executes a SPARQL CONSTRUCT query and returns the result as a JSON-LD expanded-form array.

```sql
SELECT pg_ripple.sparql_construct_jsonld('
  CONSTRUCT { ?s ?p ?o }
  WHERE     { ?s ?p ?o }
  LIMIT 100
');
```

### sparql_describe_turtle

```sql
pg_ripple.sparql_describe_turtle(query TEXT, strategy TEXT DEFAULT 'cbd') RETURNS TEXT
```

Executes a SPARQL DESCRIBE query and returns the description as Turtle text.  `strategy` may be `'cbd'` (Concise Bounded Description, default), `'scbd'` (Symmetric CBD), or `'simple'`.

### sparql_describe_jsonld

```sql
pg_ripple.sparql_describe_jsonld(query TEXT, strategy TEXT DEFAULT 'cbd') RETURNS JSONB
```

Executes a SPARQL DESCRIBE query and returns the description as JSON-LD.

---

## RDF-star Serialization

All export functions handle RDF-star quoted triples transparently:

- **N-Triples / N-Quads**: use `<< s p o >>` notation (N-Triples-star / N-Quads-star)
- **Turtle**: use `<< s p o >>` notation (Turtle-star)
- **JSON-LD**: quoted triples are represented as `{"@value": "<< s p o >>", "@type": "rdf:Statement"}`

---

## Format Guide

| Format | Import | Export | Named Graphs | RDF-star |
|--------|--------|--------|-------------|---------|
| N-Triples | `load_ntriples` | `export_ntriples` | No | Yes (N-Triples-star) |
| N-Quads | `load_nquads` | `export_nquads` | Yes | No |
| Turtle | `load_turtle` | `export_turtle` | No | Yes (Turtle-star) |
| TriG | `load_trig` | — | Yes | No |
| RDF/XML | `load_rdfxml` | — | No | No |
| JSON-LD | — | `export_jsonld` | No | Partial |

**Tip:** Use RDF/XML for Protégé ontologies, JSON-LD for REST APIs, and Turtle for human-readable files.
