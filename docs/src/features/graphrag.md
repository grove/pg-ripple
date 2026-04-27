# GraphRAG End-to-End

[Microsoft GraphRAG](https://github.com/microsoft/graphrag) is a popular open-source pipeline that turns unstructured documents into a structured knowledge graph of entities, relationships, text units, and community summaries. pg_ripple is a **first-class storage and query backend** for GraphRAG: you can ingest GraphRAG output directly, enrich it with Datalog rules, validate it with SHACL, and export it back as Parquet for downstream tools — all without leaving PostgreSQL.

This page is the canonical end-to-end guide. It assumes you already have a working GraphRAG output (the standard `entities.parquet`, `relationships.parquet`, `text_units.parquet` files).

---

## What pg_ripple adds to GraphRAG

| Stock GraphRAG | With pg_ripple |
|---|---|
| Output stored in Parquet on disk | Output stored as queryable RDF in PostgreSQL |
| Search via custom Python pipeline | Search via SPARQL, SHACL, vector + RAG |
| No incremental update — rebuild the whole graph | Incremental: load new documents into a named graph and re-infer |
| Quality issues surface only at query time | SHACL catches missing labels, dangling relationships, etc. on insert |
| One process owns the data | Multiple readers, transactional writers, full ACID |

---

## The data model

GraphRAG is mapped to four RDF classes:

| Class | What it represents |
|---|---|
| `gr:Entity` | A named entity (person, organisation, location, event, …) |
| `gr:Relationship` | A directed, weighted edge between two entities |
| `gr:TextUnit` | A chunk of source text mentioning entities |
| `gr:Community` | A detected cluster of related entities |
| `gr:CommunityReport` | An LLM-generated summary of a community |

Key properties:

| Property | Domain | Range |
|---|---|---|
| `gr:title` | `gr:Entity` | `xsd:string` |
| `gr:type` | `gr:Entity` | `xsd:string` (PERSON, ORG, …) |
| `gr:description` | any | `xsd:string` |
| `gr:source` | `gr:Relationship` | `gr:Entity` |
| `gr:target` | `gr:Relationship` | `gr:Entity` |
| `gr:weight` | `gr:Relationship` | `xsd:float` |
| `gr:text` | `gr:TextUnit` | `xsd:string` |
| `gr:tokenCount` | `gr:TextUnit` | `xsd:integer` |

The full ontology, including all SHACL shapes, lives at `examples/graphrag_byog.sql`.

---

## End-to-end pipeline

```
   GraphRAG Python pipeline
            │
            ▼
   entities.parquet                    │
   relationships.parquet               │  Step 1: import
   text_units.parquet                  │
            │                          │
            ▼                          │
   pg_ripple.import_graphrag_parquet() ▼
   ──────────────────────────────────────────
            │
            ▼
   Step 2: enrich with Datalog
   pg_ripple.load_rules_builtin('graphrag-enrichment')
   pg_ripple.infer('graphrag-enrichment')
   → derives gr:coworker, gr:collaborates,
     gr:indirectReport, gr:relatedOrg
            │
            ▼
   Step 3: validate with SHACL
   pg_ripple.load_shacl(...)
   pg_ripple.shacl_validate()
   → catches missing labels, dangling references
            │
            ▼
   Step 4: query
   - SPARQL for relationship walks
   - rag_context() for grounded LLM prompts
   - hybrid_search() for similarity + structure
            │
            ▼
   Step 5 (optional): export back to Parquet
   pg_ripple.export_graphrag_entities()
   pg_ripple.export_graphrag_relationships()
   → for downstream Microsoft GraphRAG tooling
```

---

## Step 1 — Import GraphRAG output

```sql
-- Register the gr: prefix once.
SELECT pg_ripple.register_prefix('gr', 'https://graphrag.org/ns/');

-- Import the three core files. Each file becomes triples in the named graph.
SELECT pg_ripple.import_graphrag_parquet(
    entities_path     := '/data/graphrag/entities.parquet',
    relationships_path:= '/data/graphrag/relationships.parquet',
    text_units_path   := '/data/graphrag/text_units.parquet',
    target_graph      := 'https://example.org/kb-2026-04'
);
-- Returns the count of triples inserted.
```

For ad-hoc loading, use `load_turtle()` directly:

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix gr:  <https://graphrag.org/ns/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

<https://example.org/entity/alice>
    rdf:type  gr:Entity ;
    gr:title  "Alice" ;
    gr:type   "PERSON" .
$TTL$);
```

---

## Step 2 — Enrich with Datalog

The bundled `graphrag-enrichment` rule set derives four useful relationships from the raw GraphRAG output:

| Derived property | Meaning |
|---|---|
| `gr:coworker` | Two entities both have relationships targeting the same organisation |
| `gr:collaborates` | Two entities co-occur in the same text unit |
| `gr:indirectReport` | Transitive closure of `gr:manages` |
| `gr:relatedOrg` | Two organisations share an entity bridge |

```sql
SELECT pg_ripple.load_rules_builtin('graphrag-enrichment');
SELECT pg_ripple.infer('graphrag-enrichment');
-- Returns the count of derived triples (source = 1).
```

You can write your own rules — see [Reasoning & Inference](reasoning-and-inference.md) — but the bundled set is a good starting point.

---

## Step 3 — Validate with SHACL

GraphRAG output is LLM-generated and can have quality issues: entities without titles, relationships pointing at deleted entities, weights outside [0, 1]. Catch them with SHACL:

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix gr:   <https://graphrag.org/ns/> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

gr:EntityShape a sh:NodeShape ;
    sh:targetClass gr:Entity ;
    sh:property [ sh:path gr:title ; sh:minCount 1 ; sh:datatype xsd:string ] ;
    sh:property [ sh:path gr:type  ; sh:in ( "PERSON" "ORG" "GEO" "EVENT" ) ] .

gr:RelationshipShape a sh:NodeShape ;
    sh:targetClass gr:Relationship ;
    sh:property [ sh:path gr:source ; sh:minCount 1 ; sh:class gr:Entity ] ;
    sh:property [ sh:path gr:target ; sh:minCount 1 ; sh:class gr:Entity ] ;
    sh:property [ sh:path gr:weight ; sh:datatype xsd:float ] .
$TTL$);

-- Validate everything currently in the store.
SELECT focus_node, message FROM pg_ripple.shacl_validate();
```

Set `pg_ripple.shacl_mode = 'sync'` to reject offending inserts at write time, or `'async'` to route them to a dead-letter queue. See [Validating Data Quality](validating-data-quality.md).

---

## Step 4 — Query the enriched graph

GraphRAG-specific queries pair naturally with `rag_context()`:

```sql
-- Find the LLM-ready context for a question.
SELECT pg_ripple.rag_context(
    'Who collaborates with Alice on machine learning?',
    k := 10
);
```

For purely structural queries, use SPARQL directly:

```sql
-- All co-workers of Alice's co-workers (a 2-hop search).
SELECT * FROM pg_ripple.sparql($$
    PREFIX gr: <https://graphrag.org/ns/>
    SELECT DISTINCT ?friend WHERE {
        <https://example.org/entity/alice> gr:coworker/gr:coworker ?friend .
        FILTER(?friend != <https://example.org/entity/alice>)
    }
$$);
```

---

## Step 5 — Round-trip export

If you need to feed an enriched graph back into Microsoft GraphRAG's Python tools:

```sql
SELECT pg_ripple.export_graphrag_entities('', '/tmp/entities.parquet');
SELECT pg_ripple.export_graphrag_relationships('', '/tmp/relationships.parquet');
```

Pass `''` for the default graph or a named-graph IRI to scope the export.

---

## Operational tips

- **Use a named graph per ingestion run** (`https://example.org/kb-2026-04`). When a re-ingest runs, `clear_graph()` drops the old version atomically.
- **Run inference *after* loading**, not interleaved. Inference is bulk-friendly and parallelisable; per-row inference is not.
- **Materialise embeddings after enrichment**. Datalog-derived properties improve KGE quality and `rag_context()` recall.

---

## See also

- [AI Overview](ai-overview.md)
- [Reasoning & Inference](reasoning-and-inference.md)
- [Validating Data Quality](validating-data-quality.md)
- [Cookbook: Chatbot grounded in a knowledge graph](../cookbook/grounded-chatbot.md)
- [GraphRAG function reference](../reference/graphrag-functions.md)
- [GraphRAG ontology reference](../reference/graphrag-ontology.md)
