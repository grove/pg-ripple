# §2.6 Exporting and Sharing

## What and Why

Data in pg_ripple needs to flow **out** — to other systems, to files for archival, to
LLMs for RAG pipelines, or to Microsoft's GraphRAG framework via Parquet files.
pg_ripple supports all standard RDF serialization formats plus JSON-LD framing for
API-ready output and BYOG (Bring Your Own Graph) Parquet export for GraphRAG.

This chapter is the **canonical reference** for all export functionality, including the
GraphRAG BYOG pipeline. Other chapters cross-reference here for GraphRAG details.

---

## How It Works

### Export Formats

| Format | Function | Streaming variant | Named graph support |
|---|---|---|---|
| **N-Triples** | `export_ntriples()` | — | Per-graph or default |
| **N-Quads** | `export_nquads()` | — | Yes (all graphs) |
| **Turtle** | `export_turtle()` | `export_turtle_stream()` | Per-graph or default |
| **JSON-LD** | `export_jsonld()` | `export_jsonld_stream()` | Per-graph or default |
| **JSON-LD Framed** | `export_jsonld_framed()` | `export_jsonld_framed_stream()` | Per-graph or default |
| **SPARQL CONSTRUCT → Turtle** | `sparql_construct_turtle()` | — | Via query |
| **SPARQL CONSTRUCT → JSON-LD** | `sparql_construct_jsonld()` | — | Via query |
| **SPARQL DESCRIBE → Turtle** | `sparql_describe_turtle()` | — | Via query |
| **SPARQL DESCRIBE → JSON-LD** | `sparql_describe_jsonld()` | — | Via query |
| **Parquet (GraphRAG entities)** | `export_graphrag_entities()` | — | Per-graph |
| **Parquet (GraphRAG relationships)** | `export_graphrag_relationships()` | — | Per-graph |
| **Parquet (GraphRAG text units)** | `export_graphrag_text_units()` | — | Per-graph |

### Streaming Exports

For large graphs, streaming exports return one row per triple (or per subject for JSON-LD),
avoiding buffering the entire document in memory:

```sql
-- Stream Turtle one line at a time
SELECT * FROM pg_ripple.export_turtle_stream();

-- Stream JSON-LD one subject at a time (NDJSON)
SELECT * FROM pg_ripple.export_jsonld_stream();
```

### JSON-LD Framing

JSON-LD framing reshapes flat RDF into nested, application-friendly JSON. A **frame**
is a JSON template that specifies the desired structure:

1. pg_ripple translates the frame to a SPARQL CONSTRUCT query.
2. The CONSTRUCT query executes against the triple store.
3. The W3C embedding algorithm nests matched nodes per the frame.
4. The result is compacted with the frame's `@context`.

---

## Worked Examples

### Exporting as N-Triples

The simplest format — one triple per line:

```sql
-- Export the default graph
SELECT pg_ripple.export_ntriples(NULL);
```

Output:

```
<https://example.org/paper/42> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://purl.org/ontology/bibo/AcademicArticle> .
<https://example.org/paper/42> <http://purl.org/dc/terms/title> "Knowledge Graphs in Practice" .
<https://example.org/paper/42> <http://purl.org/dc/terms/creator> <https://example.org/person/alice> .
```

Export a specific named graph:

```sql
SELECT pg_ripple.export_ntriples('https://example.org/graph/pubmed');
```

### Exporting as N-Quads

N-Quads include the graph IRI for each triple:

```sql
-- Export all graphs (pass NULL)
SELECT pg_ripple.export_nquads(NULL);
```

### Exporting as Turtle

Compact, human-readable output with prefix declarations:

```sql
SELECT pg_ripple.export_turtle();
```

Output:

```turtle
@prefix ex: <https://example.org/> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

ex:paper/42 a bibo:AcademicArticle ;
    dct:title "Knowledge Graphs in Practice" ;
    dct:creator ex:person/alice, ex:person/bob .
```

### Exporting as JSON-LD

```sql
SELECT pg_ripple.export_jsonld();
```

Returns a JSONB array of expanded node objects:

```json
[
  {
    "@id": "https://example.org/paper/42",
    "@type": ["http://purl.org/ontology/bibo/AcademicArticle"],
    "http://purl.org/dc/terms/title": [{"@value": "Knowledge Graphs in Practice"}],
    "http://purl.org/dc/terms/creator": [
      {"@id": "https://example.org/person/alice"},
      {"@id": "https://example.org/person/bob"}
    ]
  }
]
```

### JSON-LD Framing

Shape the output into the exact JSON structure your application expects:

```sql
SELECT pg_ripple.export_jsonld_framed('{
    "@context": {
        "dct": "http://purl.org/dc/terms/",
        "foaf": "http://xmlns.com/foaf/0.1/",
        "bibo": "http://purl.org/ontology/bibo/",
        "schema": "https://schema.org/",
        "title": "dct:title",
        "creator": "dct:creator",
        "name": "foaf:name",
        "affiliation": "schema:affiliation"
    },
    "@type": "bibo:AcademicArticle",
    "creator": {
        "name": {},
        "affiliation": {
            "name": {}
        }
    }
}'::jsonb);
```

Returns nested JSON-LD:

```json
{
  "@context": {"dct": "http://purl.org/dc/terms/", "...": "..."},
  "@graph": [
    {
      "@type": "bibo:AcademicArticle",
      "title": "Knowledge Graphs in Practice",
      "creator": [
        {
          "name": "Alice Johnson",
          "affiliation": {
            "name": "Massachusetts Institute of Technology"
          }
        },
        {
          "name": "Bob Smith",
          "affiliation": {
            "name": "Stanford University"
          }
        }
      ]
    }
  ]
}
```

### Debugging Frames

See the generated SPARQL CONSTRUCT without executing:

```sql
SELECT pg_ripple.jsonld_frame_to_sparql('{
    "@context": {
        "dct": "http://purl.org/dc/terms/",
        "bibo": "http://purl.org/ontology/bibo/",
        "title": "dct:title"
    },
    "@type": "bibo:AcademicArticle",
    "title": {}
}'::jsonb);
```

### CONSTRUCT-Based Exports

Use SPARQL CONSTRUCT for selective, transformed exports:

```sql
-- Export a citation graph as Turtle
SELECT pg_ripple.sparql_construct_turtle('
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX ex:   <https://example.org/>

CONSTRUCT {
    ?paper ex:cites ?cited .
    ?paper dct:title ?title .
    ?cited dct:title ?citedTitle .
}
WHERE {
    ?paper bibo:cites ?cited ;
           dct:title ?title .
    ?cited dct:title ?citedTitle .
}
');

-- Same as JSON-LD for REST APIs
SELECT pg_ripple.sparql_construct_jsonld('
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX ex:   <https://example.org/>

CONSTRUCT {
    ?paper ex:cites ?cited .
    ?paper dct:title ?title .
}
WHERE {
    ?paper bibo:cites ?cited ;
           dct:title ?title .
}
');
```

### DESCRIBE-Based Exports

Export everything about specific entities:

```sql
-- Full description as Turtle
SELECT pg_ripple.sparql_describe_turtle('
DESCRIBE <https://example.org/paper/42>
');

-- Symmetric CBD (includes incoming links)
SELECT pg_ripple.sparql_describe_turtle(
    'DESCRIBE <https://example.org/person/alice>',
    'scbd'
);

-- As JSON-LD
SELECT pg_ripple.sparql_describe_jsonld(
    'DESCRIBE <https://example.org/paper/42>'
);
```

---

## GraphRAG BYOG Pipeline

pg_ripple is the canonical source for Microsoft GraphRAG's **Bring Your Own Graph** (BYOG)
data. The pipeline uses three export functions to produce Parquet files compatible with
GraphRAG's ingestion format.

```admonish note
This is the CANONICAL GraphRAG chapter. All other documentation that mentions GraphRAG
should cross-reference this section.
```

### Step 1: Model Entities and Relationships

GraphRAG requires entities, relationships, and text units modeled with the `gr:` prefix.
Load the GraphRAG ontology:

```sql
SELECT pg_ripple.load_turtle('
@prefix gr:   <urn:graphrag:> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex:   <https://example.org/> .

# Entity: a paper
ex:paper/42 a gr:Entity ;
    gr:title "Knowledge Graphs in Practice" ;
    gr:type "AcademicArticle" ;
    gr:description "A comprehensive survey of knowledge graph technologies and applications." ;
    gr:frequency 5 ;
    gr:degree 3 .

# Entity: an author
ex:person/alice a gr:Entity ;
    gr:title "Alice Johnson" ;
    gr:type "Person" ;
    gr:description "Researcher at MIT specializing in knowledge representation." ;
    gr:frequency 8 ;
    gr:degree 5 .

# Relationship
ex:rel/1 a gr:Relationship ;
    gr:source ex:paper/42 ;
    gr:target ex:person/alice ;
    gr:description "authored by" ;
    gr:weight "1.0"^^xsd:float ;
    gr:combinedDegree 8 .

# Text unit
ex:text/1 a gr:TextUnit ;
    gr:text "This paper surveys knowledge graph technologies..." ;
    gr:nTokens 150 ;
    gr:documentId "doc-001" .
');
```

### Step 2: Enrich with Datalog Rules

Use Datalog rules to derive additional GraphRAG metadata:

```sql
SELECT pg_ripple.load_rules('
# Derive entity frequency from triple count
?e gr:frequency ?count :-
    ?e rdf:type gr:Entity ,
    COUNT(?t WHERE ?t ?anyPred ?e) = ?count .

# Derive relationship combined degree
?r gr:combinedDegree ?deg :-
    ?r rdf:type gr:Relationship ,
    ?r gr:source ?s ,
    ?r gr:target ?t ,
    COUNT(?p1 WHERE ?s ?p1 ?_) = ?sDeg ,
    COUNT(?p2 WHERE ?t ?p2 ?_) = ?tDeg .
', 'graphrag-enrichment');

SELECT pg_ripple.infer_agg('graphrag-enrichment');
```

### Step 3: Validate with SHACL

Ensure data quality before export:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix gr:   <urn:graphrag:> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

<urn:graphrag:EntityShape> a sh:NodeShape ;
    sh:targetClass gr:Entity ;
    sh:property [
        sh:path gr:title ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path gr:type ;
        sh:minCount 1 ;
    ] .

<urn:graphrag:RelationshipShape> a sh:NodeShape ;
    sh:targetClass gr:Relationship ;
    sh:property [
        sh:path gr:source ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] ;
    sh:property [
        sh:path gr:target ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] .
');

-- Validate before export
SELECT pg_ripple.validate();
```

### Step 4: Export to Parquet

```sql
-- Export entities (requires superuser)
SELECT pg_ripple.export_graphrag_entities('', '/data/graphrag/entities.parquet');

-- Export relationships
SELECT pg_ripple.export_graphrag_relationships('', '/data/graphrag/relationships.parquet');

-- Export text units
SELECT pg_ripple.export_graphrag_text_units('', '/data/graphrag/text_units.parquet');
```

Each function returns the number of rows written. The Parquet files are directly
compatible with `pyarrow.parquet.read_table()` and GraphRAG's BYOG configuration:

```yaml
# GraphRAG settings.yaml
entity_table_path: /data/graphrag/entities.parquet
relationship_table_path: /data/graphrag/relationships.parquet
text_unit_table_path: /data/graphrag/text_units.parquet
```

### Step 5: Export from a Named Graph

For multi-tenant or versioned exports:

```sql
-- Export only entities from the "production" graph
SELECT pg_ripple.export_graphrag_entities(
    'https://example.org/graph/production',
    '/data/graphrag/prod_entities.parquet'
);

SELECT pg_ripple.export_graphrag_relationships(
    'https://example.org/graph/production',
    '/data/graphrag/prod_relationships.parquet'
);

SELECT pg_ripple.export_graphrag_text_units(
    'https://example.org/graph/production',
    '/data/graphrag/prod_text_units.parquet'
);
```

---

## Common Patterns

### Pattern: API Response Formatting

Use JSON-LD framing to produce API-ready responses:

```sql
-- Papers endpoint: nested JSON with authors
SELECT pg_ripple.export_jsonld_framed('{
    "@context": {
        "title": "http://purl.org/dc/terms/title",
        "creator": "http://purl.org/dc/terms/creator",
        "name": "http://xmlns.com/foaf/0.1/name",
        "type": "@type"
    },
    "@type": "http://purl.org/ontology/bibo/AcademicArticle",
    "creator": { "name": {} }
}'::jsonb);
```

### Pattern: Scheduled Exports via CONSTRUCT Views

For continuously updated exports, create a CONSTRUCT view (requires pg_trickle):

```sql
SELECT pg_ripple.create_construct_view(
    'citation_graph',
    'PREFIX bibo: <http://purl.org/ontology/bibo/>
     PREFIX dct: <http://purl.org/dc/terms/>
     CONSTRUCT { ?p bibo:cites ?c . ?p dct:title ?t . }
     WHERE { ?p bibo:cites ?c ; dct:title ?t }',
    '30s',
    true
);

-- The view is automatically refreshed every 30 seconds
SELECT * FROM pg_ripple.construct_view_citation_graph_decoded;
```

### Pattern: Streaming Export to File

For large graphs, use `COPY` with streaming exports:

```sql
COPY (SELECT * FROM pg_ripple.export_turtle_stream())
TO '/data/export/full_graph.ttl';

COPY (SELECT * FROM pg_ripple.export_jsonld_stream())
TO '/data/export/full_graph.ndjson';
```

### Pattern: Selective Export with SPARQL

Export only a subset of the graph:

```sql
-- Export only papers from 2024
SELECT pg_ripple.sparql_construct_turtle('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX xsd:  <http://www.w3.org/2001/XMLSchema#>

CONSTRUCT { ?paper ?p ?o }
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:date ?date ;
           ?p ?o .
    FILTER (?date >= "2024-01-01"^^xsd:date)
}
');
```

---

## Performance and Trade-offs

### Buffered vs Streaming Exports

| Mode | Memory usage | Output format | Best for |
|---|---|---|---|
| Buffered (`export_turtle()`) | Entire graph in memory | Complete document | Small-medium graphs |
| Streaming (`export_turtle_stream()`) | One triple at a time | Row-per-triple | Large graphs (millions of triples) |

### Parquet Export Performance

GraphRAG Parquet export scans the relevant VP tables once per entity type. Performance
depends on the number of `gr:Entity`, `gr:Relationship`, and `gr:TextUnit` nodes:

- ~100K entities: <5 seconds
- ~1M entities: ~30 seconds
- Write path requires superuser (writes to server filesystem)

### JSON-LD Framing Cost

Framing involves executing a SPARQL CONSTRUCT query, then applying the W3C embedding
algorithm. The cost is dominated by the CONSTRUCT query; the embedding step is linear
in the number of matched nodes.

```admonish tip
Use `jsonld_frame_to_sparql()` to inspect the generated CONSTRUCT query and verify
it is efficient before calling `export_jsonld_framed()`.
```

---

## Gotchas and Debugging

### Empty Parquet Files

If `export_graphrag_entities()` returns 0, check that your data uses the correct `gr:`
prefix and that entities have `rdf:type gr:Entity`:

```sql
SELECT * FROM pg_ripple.find_triples(
    NULL,
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<urn:graphrag:Entity>'
);
```

### Framing Returns Empty Result

Ensure the frame's `@type` matches actual `rdf:type` values in the store. The type
must be a full IRI, not a prefixed name:

```sql
-- Check what types exist
SELECT * FROM pg_ripple.sparql('
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT DISTINCT ?type WHERE { ?x rdf:type ?type }
');
```

### Server-Side File Permissions

Parquet export writes to the server filesystem. Ensure the `postgres` OS user has write
permission to the output directory:

```bash
sudo mkdir -p /data/graphrag
sudo chown postgres:postgres /data/graphrag
```

### Large Export Memory

For graphs with millions of triples, buffered exports (`export_turtle()`, `export_jsonld()`)
may use significant memory. Switch to streaming variants or `COPY ... TO` with streaming.

---

## Next Steps

- **[§2.7 AI Retrieval and GraphRAG](../features/ai-retrieval-graph-rag.md)** — vector embeddings and RAG retrieval pipelines.
- **[§2.8 APIs and Integration](../features/apis-and-integration.md)** — serve exported data via the HTTP endpoint.
- **[§2.3 Querying with SPARQL](../features/querying-with-sparql.md)** — CONSTRUCT and DESCRIBE queries for selective export.
