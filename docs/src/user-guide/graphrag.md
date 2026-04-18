# GraphRAG Integration (v0.26.0)

pg_ripple supports **GraphRAG BYOG** (Bring Your Own Graph) — storing and querying
Microsoft GraphRAG knowledge graphs directly in PostgreSQL using RDF triples.

## Overview

[GraphRAG](https://github.com/microsoft/graphrag) produces structured knowledge graphs
with entities, relationships, and text units.  pg_ripple stores these as native RDF
triples, enabling SPARQL queries, Datalog enrichment, SHACL validation, and Parquet
export — all within the database.

## Quick start

```sql
-- 1. Register the gr: prefix
SELECT pg_ripple.register_prefix('gr', 'https://graphrag.org/ns/');

-- 2. Load entities as Turtle
SELECT pg_ripple.load_turtle($TTL$
@prefix gr:  <https://graphrag.org/ns/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

<https://example.org/entity/alice>
    rdf:type  gr:Entity ;
    gr:title  "Alice" ;
    gr:type   "PERSON" .
$TTL$);

-- 3. Query via SPARQL
SELECT * FROM pg_ripple.sparql(
    'SELECT ?e ?t WHERE { ?e a <https://graphrag.org/ns/Entity> ; <https://graphrag.org/ns/title> ?t }'
);

-- 4. Export to Parquet for downstream tools
SELECT pg_ripple.export_graphrag_entities('', '/tmp/entities.parquet');
```

## Data model

| Class | Description |
|---|---|
| `gr:Entity` | A named entity (person, org, location, event, …) |
| `gr:Relationship` | A directed relationship between two entities |
| `gr:TextUnit` | A chunk of source text mentioning entities |
| `gr:Community` | A detected community of related entities |
| `gr:CommunityReport` | A summary report for a community |

Key properties:

| Property | Domain | Range |
|---|---|---|
| `gr:title` | `gr:Entity` | `xsd:string` |
| `gr:type` | `gr:Entity` | `xsd:string` |
| `gr:description` | any | `xsd:string` |
| `gr:source` | `gr:Relationship` | `gr:Entity` |
| `gr:target` | `gr:Relationship` | `gr:Entity` |
| `gr:weight` | `gr:Relationship` | `xsd:float` |
| `gr:text` | `gr:TextUnit` | `xsd:string` |
| `gr:tokenCount` | `gr:TextUnit` | `xsd:integer` |
| `gr:frequency` | `gr:Entity` | `xsd:integer` |
| `gr:degree` | `gr:Entity` | `xsd:integer` |

## Named graph storage

Load entities into a dedicated named graph to keep GraphRAG data isolated:

```sql
SELECT pg_ripple.load_trig($TRIG$
@prefix gr: <https://graphrag.org/ns/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

<https://myapp.org/graphs/graphrag> {
    <https://myapp.org/entities/e1>
        rdf:type gr:Entity ;
        gr:title "ACME Corp" ;
        gr:type "ORGANIZATION" .
}
$TRIG$);
```

Then export from only that graph:

```sql
SELECT pg_ripple.export_graphrag_entities(
    'https://myapp.org/graphs/graphrag',
    '/var/data/entities.parquet'
);
```

Pass an empty string `''` to export from the default graph.

## SHACL validation

Load the bundled shapes to enforce schema constraints before export:

```sql
-- Load shapes from the shipped file
SELECT pg_ripple.load_shacl(pg_read_file('/path/to/graphrag_shapes.ttl'));

-- Validate
SELECT pg_ripple.validate(NULL)::jsonb ->> 'conforms';
```

See [GraphRAG Enrichment](graphrag-enrichment.md) for Datalog enrichment rules.

## Parquet export

Three functions write Parquet files compatible with the GraphRAG pipeline:

| Function | Output columns |
|---|---|
| `export_graphrag_entities(graph, path)` | `id, title, type, description, text_unit_ids, frequency, degree` |
| `export_graphrag_relationships(graph, path)` | `id, source, target, description, weight, combined_degree, text_unit_ids` |
| `export_graphrag_text_units(graph, path)` | `id, text, n_tokens, document_id, entity_ids, relationship_ids` |

All functions return the number of rows written and require superuser.

## Python CLI

A convenience script `scripts/graphrag_export.py` handles connection management and
multi-table export:

```bash
python scripts/graphrag_export.py \
    --pg-url "postgresql://localhost/mydb" \
    --graph-iri "https://myapp.org/graphs/graphrag" \
    --output-dir /var/data/graphrag_export \
    --enrich-with-datalog \
    --validate
```

## Full walkthrough

See `examples/graphrag_byog.sql` for a complete step-by-step example.
