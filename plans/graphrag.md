# pg_ripple × GraphRAG: Synergy Analysis

> **Date**: 2026-04-18
> **Status**: Research report
> **Authors**: Auto-generated from deep research on Microsoft GraphRAG v3.x and pg_ripple v0.19+

## Executive Summary

Microsoft's **GraphRAG** (v3.0, 32k+ GitHub stars) is a graph-based Retrieval-Augmented Generation system that extracts knowledge graphs from unstructured text using LLMs, performs hierarchical community detection, generates community summaries, and uses those structures to answer questions over private datasets. It substantially outperforms baseline vector-similarity RAG on two classes of queries: (1) "connect-the-dots" questions requiring traversal across disparate facts, and (2) holistic sensemaking over entire corpora.

**pg_ripple** is a PostgreSQL 18 extension implementing a high-performance RDF triple store with native SPARQL 1.1, SHACL validation, Datalog reasoning, HTAP storage, federation, JSON-LD framing, and a companion HTTP/SPARQL endpoint.

This report identifies deep structural synergies between the two systems and proposes concrete integration paths where pg_ripple serves as a **persistent, queryable, semantically-enriched knowledge graph backend** for GraphRAG pipelines — and where GraphRAG's community-structured approach enhances pg_ripple's value as an enterprise knowledge platform.

---

## 1. GraphRAG Architecture (Detailed)

### 1.1 The Problem GraphRAG Solves

Standard RAG relies on vector similarity search: given a user query, retrieve the top-k most similar text chunks from an embedding index and stuff them into the LLM context window. This fails in two scenarios:

- **Cross-document reasoning**: When the answer requires connecting facts scattered across many documents through shared entities, baseline RAG cannot "connect the dots" because the relevant chunks may not be semantically similar to the query.
- **Holistic summarization**: Questions like "What are the main themes in this dataset?" have no specific text to retrieve — there is nothing in the query to direct vector search to the right content.

**Reference**: Edge et al., "From Local to Global: A Graph RAG Approach to Query-Focused Summarization" (arXiv:2404.16130, 2024).

### 1.2 Indexing Pipeline

GraphRAG's indexing pipeline transforms unstructured text into a structured knowledge model through six phases:

| Phase | Process | Output |
|-------|---------|--------|
| 1. Compose TextUnits | Chunk documents into token-sized units (default: 1200 tokens) | `text_units` table |
| 2. Document Processing | Link documents to their constituent text units | `documents` table |
| 3. Graph Extraction | LLM extracts entities (name, type, description) and relationships (source, target, description, weight) from each text unit; optionally extract claims/covariates | `entities`, `relationships`, `covariates` tables |
| 4. Graph Augmentation | Hierarchical Leiden community detection on the entity-relationship graph | `communities` table |
| 5. Community Summarization | LLM generates reports for each community at every hierarchy level, containing executive overviews, key findings, entity/relationship references | `community_reports` table |
| 6. Text Embedding | Generate vector embeddings for entity descriptions, text unit text, and community report content | Vector store (LanceDB, Azure AI Search, CosmosDB) |

### 1.3 Knowledge Model

The GraphRAG knowledge model consists of seven core tables:

- **Document**: Input document with title, text, linked text_unit_ids
- **TextUnit**: Chunk of text with token count, linked entity/relationship/covariate IDs
- **Entity**: Extracted entity with title, type (person/place/org/event), description, frequency, degree
- **Relationship**: Edge between entities with description, weight, combined_degree
- **Covariate**: Time-bounded claims about entities (optional)
- **Community**: Leiden cluster with parent/children hierarchy, level, member entity/relationship IDs
- **Community Report**: LLM-generated summary with title, full_content, rank, findings

### 1.4 Query Modes

| Mode | Strategy | Best For |
|------|----------|----------|
| **Global Search** | Map-reduce over community reports: community summaries → intermediate point-rated responses → final aggregated answer | Holistic questions ("What are the main themes?") |
| **Local Search** | Entity-centric: find semantically-related entities → fan out to neighbors, relationships, covariates, community reports, text units → prioritize + filter → generate answer | Specific entity questions ("What are chamomile's healing properties?") |
| **DRIFT Search** | Hybrid: start with community report primer → generate follow-up questions → iterative local search refinement → hierarchical answer synthesis | Complex questions needing both breadth and depth |
| **Basic Search** | Standard top-k vector search over text units | Simple factual lookup |

### 1.5 Extensibility: Bring Your Own Graph (BYOG)

GraphRAG supports importing pre-built knowledge graphs through its BYOG capability:
- Provide `entities.parquet` and `relationships.parquet` (and optionally `text_units.parquet`)
- Run only the downstream workflows: `create_communities`, `create_community_reports`, `generate_text_embeddings`
- **This is the primary integration point for pg_ripple**

### 1.6 Indexing Methods

- **Standard GraphRAG**: LLM-powered entity/relationship extraction with rich descriptions (~75% of indexing cost)
- **FastGraphRAG**: NLP-based extraction (NLTK/spaCy) — cheaper but noisier, no descriptions

---

## 2. Synergy Map

### 2.1 pg_ripple as GraphRAG's Knowledge Graph Backend

The deepest synergy is architectural: GraphRAG needs a graph, and pg_ripple *is* a graph. GraphRAG currently stores its knowledge model as Parquet files on disk. pg_ripple can serve as a **live, queryable, transactionally-consistent, reasoning-capable** replacement for that static storage layer.

```
┌─────────────────────────────────────────────────────┐
│              GraphRAG Pipeline                       │
│                                                      │
│  Documents → TextUnits → LLM Entity Extraction       │
│                              │                       │
│                              ▼                       │
│                   ┌─────────────────────┐            │
│                   │    pg_ripple        │            │
│                   │  (RDF Triple Store) │            │
│                   │                     │            │
│                   │  entities → RDF     │            │
│                   │  relationships → RDF│            │
│                   │  communities → RDF  │            │
│                   │  text_units → RDF   │            │
│                   └─────────┬───────────┘            │
│                             │                        │
│            ┌────────────────┼────────────────┐       │
│            ▼                ▼                ▼       │
│     SPARQL Query     Datalog Reasoning   SHACL      │
│     (Local Search)   (Inference)        (Quality)   │
│            │                │                │       │
│            └────────────────┼────────────────┘       │
│                             ▼                        │
│                      LLM Context Window              │
│                             │                        │
│                             ▼                        │
│                        Response                      │
└─────────────────────────────────────────────────────┘
```

#### Key Advantages Over Parquet-on-Disk

| Concern | Parquet (Default) | pg_ripple |
|---------|------------------|-----------|
| Query language | Python pandas / SQL | SPARQL 1.1 (native graph queries) |
| Transactional consistency | None (file-based) | Full ACID (PostgreSQL WAL) |
| Concurrent access | Single-writer | MVCC (many readers + writers) |
| Incremental updates | Full re-index required | INSERT/DELETE individual triples |
| Reasoning | None | Datalog (RDFS, OWL-RL), SHACL validation |
| Cross-graph federation | None | SERVICE clause to remote SPARQL endpoints |
| Storage efficiency | Good compression | VP tables + dictionary encoding (33% reduction) |
| Query optimization | None | Cost-based PostgreSQL optimizer + plan cache |
| Provenance | text_unit_ids array | RDF-star statement identifiers, named graphs |
| Real-time updates | Batch only | HTAP (delta/main split, background merge) |

### 2.2 Semantic Enrichment via Reasoning

GraphRAG's entity extraction is powerful but purely empirical — it captures only what the LLM identifies in the text. pg_ripple adds **deductive reasoning**:

#### 2.2.1 RDFS/OWL-RL Inference

```sql
-- Load OWL-RL rules after GraphRAG entities are imported as RDF
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');
```

This automatically derives:
- **Subclass transitivity**: If "Machine Learning" `rdfs:subClassOf` "Artificial Intelligence", and an entity is typed as "Machine Learning", it is also classified under "AI"
- **Domain/range inference**: If a `worksAt` relationship has `rdfs:domain` Person and `rdfs:range` Organization, pg_ripple infers the types of source/target entities
- **Transitive properties**: `partOf`, `locatedIn`, `subOrganizationOf` chains are materialized
- **Symmetric properties**: `collaboratesWith`, `relatedTo` are bi-directional

This expands GraphRAG's knowledge graph with **implicit relationships** that the LLM never explicitly stated, improving both Local Search coverage and community structure quality.

#### 2.2.2 Custom Datalog Rules

```sql
-- Domain-specific inference rules
SELECT pg_ripple.load_rules('
  ?x :indirectlyFundedBy ?z :-
    ?x :fundedBy ?y,
    ?y :fundedBy ?z.
  
  ?x :competitor ?y :-
    ?x :operatesIn ?sector,
    ?y :operatesIn ?sector,
    ?x != ?y.
', 'domain_rules');

SELECT pg_ripple.infer('domain_rules');
```

These derived relationships directly enrich GraphRAG's entity graph, producing richer community structures and more informative community reports.

### 2.3 Data Quality via SHACL Validation

GraphRAG's LLM extraction is inherently noisy. pg_ripple's SHACL validation can enforce graph quality constraints:

```turtle
# SHACL shape for GraphRAG entities
ex:EntityShape a sh:NodeShape ;
    sh:targetClass graphrag:Entity ;
    sh:property [
        sh:path graphrag:title ;
        sh:minCount 1 ;
        sh:maxLength 500 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path graphrag:type ;
        sh:minCount 1 ;
        sh:in ( "person" "organization" "geo" "event" ) ;
    ] ;
    sh:property [
        sh:path graphrag:description ;
        sh:minCount 1 ;
    ] .

# SHACL shape for GraphRAG relationships
ex:RelationshipShape a sh:NodeShape ;
    sh:targetClass graphrag:Relationship ;
    sh:property [
        sh:path graphrag:source ;
        sh:minCount 1 ;
        sh:class graphrag:Entity ;
    ] ;
    sh:property [
        sh:path graphrag:target ;
        sh:minCount 1 ;
        sh:class graphrag:Entity ;
    ] ;
    sh:property [
        sh:path graphrag:weight ;
        sh:minCount 1 ;
        sh:datatype xsd:float ;
        sh:minInclusive 0.0 ;
    ] .
```

Benefits:
- **Reject malformed entities** (missing title, invalid type)
- **Enforce referential integrity** (relationship endpoints must exist)
- **Detect anomalies** (negative weights, duplicate entities with different types)
- **Async validation** mode processes the queue in the background without blocking ingestion

### 2.4 SPARQL as GraphRAG's Query Language

GraphRAG's Local Search performs entity lookup + neighborhood traversal. SPARQL is purpose-built for this:

```sparql
# Local Search equivalent: find entity + 2-hop neighborhood
PREFIX gr: <http://graphrag.example.org/>

SELECT ?entity ?type ?desc ?rel_desc ?neighbor ?neighbor_type ?neighbor_desc
WHERE {
    ?entity gr:title "Novorossiya" ;
            gr:type ?type ;
            gr:description ?desc .
    
    ?rel gr:source ?entity ;
         gr:target ?neighbor ;
         gr:description ?rel_desc ;
         gr:weight ?weight .
    
    ?neighbor gr:type ?neighbor_type ;
              gr:description ?neighbor_desc .
    
    FILTER(?weight > 1.0)
}
ORDER BY DESC(?weight)
LIMIT 50
```

```sparql
# Global Search equivalent: retrieve community reports at level 2
PREFIX gr: <http://graphrag.example.org/>

SELECT ?community ?title ?summary ?rank
WHERE {
    ?report a gr:CommunityReport ;
            gr:community ?community ;
            gr:level 2 ;
            gr:title ?title ;
            gr:summary ?summary ;
            gr:rank ?rank .
}
ORDER BY DESC(?rank)
```

#### Property Paths for Transitive Traversal

```sparql
# Find all entities transitively connected to a target
PREFIX gr: <http://graphrag.example.org/>

SELECT ?connected ?distance
WHERE {
    <http://graphrag.example.org/entity/Novorossiya>
        (gr:relatedTo)+ ?connected .
}
```

pg_ripple compiles property paths to `WITH RECURSIVE ... CYCLE` — efficient and cycle-safe on PostgreSQL 18.

### 2.5 JSON-LD Framing for LLM Context Windows

GraphRAG must assemble context windows for LLM prompts. pg_ripple's JSON-LD framing produces **pre-shaped, nested JSON** directly from the triple store:

```sql
-- Frame entity + neighborhood as nested JSON-LD for LLM context
SELECT pg_ripple.export_jsonld_framed(
    '{
        "@context": { "gr": "http://graphrag.example.org/" },
        "@type": "gr:Entity",
        "gr:title": "Novorossiya",
        "gr:description": {},
        "gr:hasRelationship": {
            "@type": "gr:Relationship",
            "gr:target": {
                "@type": "gr:Entity",
                "gr:title": {},
                "gr:description": {}
            },
            "gr:description": {},
            "gr:weight": {}
        }
    }'::jsonb,
    'http://graphrag.example.org/default',
    '@always', false, true
);
```

This eliminates the Python post-processing currently required to assemble GraphRAG context — the triple store produces the exact nested structure the LLM prompt needs.

### 2.6 Provenance via RDF-star and Named Graphs

GraphRAG tracks provenance through `text_unit_ids` arrays. pg_ripple offers richer alternatives:

#### RDF-star Statement-Level Metadata

```sparql
# Attach confidence score and source text unit to a specific relationship
<< :Novorossiya :targets :PrivatBank >> gr:confidence 0.87 ;
    gr:sourceTextUnit :tu_4523 ;
    gr:extractedBy "gpt-4-turbo" ;
    gr:extractedAt "2024-06-15T10:30:00Z"^^xsd:dateTime .
```

Each triple can carry its own provenance metadata (which LLM extracted it, when, with what confidence, from which text unit) without bloating the main graph.

#### Named Graphs for Versioning

```sql
-- Each indexing run stored in a separate named graph
SELECT pg_ripple.create_graph('http://graphrag.example.org/runs/2024-06-15');
SELECT pg_ripple.load_turtle(
    '<http://graphrag.example.org/entity/42> a gr:Entity ...',
    'http://graphrag.example.org/runs/2024-06-15'
);

-- Query across versions or a specific run
SELECT pg_ripple.sparql('
    SELECT ?entity ?run WHERE {
        GRAPH ?run { ?entity a gr:Entity }
    }
');
```

This enables:
- **Incremental indexing**: new documents add triples to a new named graph; reasoning runs across all graphs
- **A/B testing**: compare entity graphs produced by different LLMs or prompt versions
- **Temporal queries**: "What entities were extracted from last week's documents?"

### 2.7 Full-Text Search for Hybrid Retrieval

GraphRAG's Basic Search mode uses vector similarity. pg_ripple adds GIN-indexed full-text search:

```sql
-- Create FTS index on entity descriptions
SELECT pg_ripple.fts_index('http://graphrag.example.org/description');

-- Hybrid search: keyword + graph traversal
SELECT pg_ripple.fts_search(
    'terrorism & bank',
    'http://graphrag.example.org/description'
);
```

This enables a hybrid retrieval strategy: use FTS to find relevant entities by keyword, then use SPARQL to traverse their neighborhoods — combining vector, keyword, and graph search in a single system.

### 2.8 Federation for Multi-Source Knowledge Graphs

GraphRAG operates on a single private corpus. pg_ripple's federation extends this to external knowledge bases:

```sparql
# Enrich GraphRAG entities with Wikidata descriptions
PREFIX wd: <http://www.wikidata.org/entity/>
PREFIX wdt: <http://www.wikidata.org/prop/direct/>

SELECT ?entity ?local_desc ?wikidata_desc
WHERE {
    ?entity a gr:Entity ;
            gr:title ?name ;
            gr:description ?local_desc .
    
    SERVICE <https://query.wikidata.org/sparql> {
        ?wd_entity rdfs:label ?name ;
                   schema:description ?wikidata_desc .
        FILTER(LANG(?wikidata_desc) = "en")
    }
}
```

This allows GraphRAG to:
- **Augment extracted entities** with authoritative descriptions from Wikidata, DBpedia, or domain-specific SPARQL endpoints
- **Cross-reference** entities across multiple GraphRAG indexes stored in different pg_ripple instances
- **Link private knowledge** to public ontologies for standardized typing

### 2.9 Incremental Views for Real-Time GraphRAG

pg_ripple's SPARQL views create live, incrementally-updated materializations:

```sql
-- Live view of high-degree entities (potential community hubs)
SELECT pg_ripple.create_sparql_view(
    'high_degree_entities',
    'SELECT ?entity (COUNT(?rel) AS ?degree)
     WHERE { ?rel gr:source ?entity }
     GROUP BY ?entity
     HAVING (COUNT(?rel) > 10)',
    '10s', true
);

-- Live view tracking community membership changes
SELECT pg_ripple.create_sparql_view(
    'community_entity_counts',
    'SELECT ?community (COUNT(?entity) AS ?size)
     WHERE { ?community gr:hasMember ?entity }
     GROUP BY ?community',
    '30s', true
);
```

These views update automatically as new triples are ingested, enabling **streaming GraphRAG** — where new documents are continuously indexed, and community structures evolve in near-real-time.

### 2.10 HTAP for Concurrent Read/Write GraphRAG

GraphRAG's default pipeline is batch-oriented: index first, query later. pg_ripple's HTAP storage eliminates this distinction:

- **Delta partition**: accepts real-time triple writes (new entities, relationships)
- **Main partition**: BRIN-indexed read-optimized copy for query performance
- **Background merge worker**: periodically consolidates delta into main
- **Query path**: `(main EXCEPT tombstones) UNION ALL delta` — always sees all data

This enables a **live GraphRAG** scenario where:
1. New documents arrive continuously
2. Entity extraction writes triples to delta
3. Queries (Local/Global/DRIFT) read from main + delta simultaneously
4. Community detection runs periodically on the evolving graph
5. No downtime or re-indexing required

---

## 3. Integration Architecture Proposals

### 3.1 Proposal A: pg_ripple as GraphRAG Storage Provider

GraphRAG v3.0's architecture uses a **factory pattern** for storage providers. The integration would implement a custom `TableProvider` that reads/writes GraphRAG's knowledge model tables to pg_ripple via SQL/SPARQL.

```
GraphRAG Python Pipeline
    │
    ├── storage_factory.register("pg_ripple", PgRippleTableProvider)
    │
    ├── Indexing writes entities/relationships as RDF triples
    │   via pg_ripple.load_turtle() or pg_ripple.insert_triple()
    │
    ├── Query engine reads via pg_ripple.sparql()
    │   or pg_ripple_http POST /sparql
    │
    └── Community detection reads entity graph via SPARQL CONSTRUCT
        → runs Leiden in Python
        → writes communities back as RDF
```

**Implementation effort**: Medium. Requires a Python `graphrag-storage-pg-ripple` package implementing GraphRAG's `TableProvider` interface, mapping between GraphRAG's tabular model and RDF.

### 3.2 Proposal B: pg_ripple as BYOG Source

The simpler approach: use GraphRAG's existing BYOG (Bring Your Own Graph) capability. pg_ripple exports its RDF graph as Parquet-compatible tables for GraphRAG to consume.

```
pg_ripple (existing RDF knowledge graph)
    │
    ├── SPARQL CONSTRUCT → entities.parquet
    │   SELECT ?id ?title ?type ?description
    │   WHERE { ?id a gr:Entity ; gr:title ?title ; ... }
    │
    ├── SPARQL CONSTRUCT → relationships.parquet
    │   SELECT ?id ?source ?target ?description ?weight
    │   WHERE { ?id a gr:Relationship ; gr:source ?source ; ... }
    │
    └── GraphRAG BYOG workflow:
        workflows: [create_communities, create_community_reports, generate_text_embeddings]
```

**Implementation effort**: Low. A SQL function or Python script that runs SPARQL queries against pg_ripple and writes results as Parquet. GraphRAG handles community detection and summarization.

### 3.3 Proposal C: pg_ripple HTTP as GraphRAG Query Backend

For query-time integration, GraphRAG's query engine can call pg_ripple_http directly instead of loading Parquet files into memory:

```
GraphRAG Query Engine
    │
    ├── Local Search:
    │   POST http://localhost:7878/sparql
    │   Body: SELECT entity neighborhood query
    │   → Receives JSON results
    │   → Assembles LLM context window
    │
    ├── Global Search:
    │   POST http://localhost:7878/sparql
    │   Body: SELECT community reports at level N
    │   → Map-reduce over results
    │
    └── DRIFT Search:
        POST http://localhost:7878/sparql
        Body: Iterative entity-community traversal queries
```

**Implementation effort**: Low-Medium. Requires a custom `ContextBuilder` that issues SPARQL queries instead of reading DataFrames.

### 3.4 Proposal D: Bidirectional Integration

The most ambitious approach: GraphRAG community structure is materialized back into pg_ripple as RDF, enabling SPARQL-native GraphRAG queries.

```
┌──────────────┐     entities/rels     ┌──────────────┐
│              │ ───────────────────▶  │              │
│  pg_ripple   │                       │   GraphRAG   │
│  (RDF Store) │  ◀───────────────── │  (LLM+Leiden) │
│              │  communities/reports  │              │
└──────┬───────┘                       └──────────────┘
       │
       ▼
  SPARQL queries combine:
  - Entity graph (original RDF)
  - Community structure (GraphRAG-derived)
  - Inferred triples (Datalog)
  - Validated constraints (SHACL)
  - External knowledge (Federation)
```

---

## 4. Specific Feature-to-Feature Synergy Matrix

| pg_ripple Feature | GraphRAG Component | Synergy |
|---|---|---|
| VP storage + dictionary encoding | Entity/relationship storage | 10× faster integer joins vs. string-based Parquet; 33% storage reduction |
| SPARQL 1.1 SELECT | Local Search context building | Native graph pattern matching replaces Python DataFrame traversal |
| SPARQL property paths | DRIFT Search entity traversal | `WITH RECURSIVE ... CYCLE` for arbitrary-depth neighbor exploration |
| Datalog + OWL-RL reasoning | Entity extraction enrichment | Derives implicit relationships the LLM missed; expands community connections |
| SHACL validation | Entity/relationship quality | Rejects malformed extractions; enforces type consistency |
| JSON-LD framing | LLM context window assembly | Pre-shaped nested JSON directly from triple store; no post-processing |
| Named graphs | Index versioning / A/B testing | Each extraction run in its own graph; temporal queries across versions |
| RDF-star | Provenance tracking | Statement-level confidence, source text unit, extraction model metadata |
| Full-text search (GIN) | Basic Search / hybrid retrieval | Keyword + graph search in single system; no separate vector store needed |
| Federation (SERVICE clause) | Knowledge enrichment | Augment entities with Wikidata, DBpedia; cross-reference multiple indexes |
| HTAP (delta/main) | Real-time indexing | Continuous entity ingestion without query degradation |
| SPARQL views | Monitoring / streaming | Live materializations of community statistics, entity counts, anomalies |
| Plan cache | Query performance | Repeated GraphRAG queries (same patterns, different bindings) hit cache |
| pg_ripple_http | Query-time API | Standard SPARQL Protocol endpoint for any GraphRAG client |
| Export (Turtle, JSON-LD, N-Triples) | BYOG / interoperability | Export graph in any format for GraphRAG or other consumers |
| Bulk loading | Batch entity import | >50K triples/sec via `load_turtle()` for large GraphRAG indexes |
| Graph-level RLS | Multi-tenant GraphRAG | Row-level security per named graph; different users see different entity subsets |

---

## 5. Use Cases

### 5.1 Enterprise Document Intelligence

**Scenario**: A company wants to analyze thousands of internal documents (reports, emails, policies).

1. GraphRAG extracts entities and relationships via LLM
2. pg_ripple stores the graph with SHACL-enforced quality
3. Datalog rules infer organizational hierarchies, project dependencies
4. Users query via SPARQL or GraphRAG's query modes
5. Federation links entities to public knowledge bases
6. Named graphs separate by department; RLS controls access

### 5.2 Scientific Literature Analysis

**Scenario**: Researchers index a corpus of scientific papers.

1. GraphRAG extracts: authors, methods, findings, citations, datasets
2. pg_ripple stores with OWL-RL reasoning: derive co-authorship networks, method lineages
3. SPARQL property paths find transitive citation chains
4. Community reports summarize research clusters
5. Federation enriches with PubMed/ORCID data via SPARQL endpoints
6. JSON-LD framing produces structured outputs for downstream tools

### 5.3 Real-Time News Intelligence

**Scenario**: Continuous news feed analysis with live GraphRAG.

1. New articles arrive via CDC/streaming
2. LLM extracts entities and relationships → pg_ripple delta partition
3. SPARQL views track emerging entity clusters in real-time
4. Periodic community re-detection updates summaries
5. DRIFT Search combines latest entities with historical community context
6. RDF-star tracks extraction timestamp and confidence per fact

### 5.4 Multi-Source Knowledge Fusion

**Scenario**: Combine GraphRAG extractions from multiple data sources.

1. Source A (internal docs) → GraphRAG → pg_ripple graph `source_a`
2. Source B (customer data) → GraphRAG → pg_ripple graph `source_b`
3. Source C (industry reports) → GraphRAG → pg_ripple graph `source_c`
4. SPARQL queries across all named graphs
5. Datalog rules merge entities with `owl:sameAs` links
6. Community detection runs on the unified graph
7. Global Search produces cross-source holistic summaries

---

## 6. Competitive Advantages

### 6.1 vs. GraphRAG + Neo4j

Several GraphRAG integrations exist with Neo4j. pg_ripple offers:

- **No separate infrastructure**: everything runs inside PostgreSQL — no additional database to manage, backup, or scale
- **ACID transactions**: Neo4j Community lacks multi-statement transactions
- **SQL interoperability**: join graph data with relational tables (user accounts, access logs, etc.)
- **Standards compliance**: W3C SPARQL 1.1, SHACL, JSON-LD — not a proprietary query language
- **Cost**: PostgreSQL is free; Neo4j Enterprise is licensed per core

### 6.2 vs. GraphRAG + pgvector

The common approach of storing GraphRAG outputs in PostgreSQL with pgvector provides vector search but no graph intelligence:

- **No graph traversal**: pgvector does cosine similarity, not relationship navigation
- **No reasoning**: no inference engine, no rule-based enrichment
- **No schema validation**: no SHACL-like constraint enforcement
- **No federation**: no SERVICE clause to external knowledge bases
- **Flat storage**: no VP optimization for graph patterns

pg_ripple + pgvector combined provides the complete stack: graph traversal, reasoning, validation, vector similarity, and full-text search — all in PostgreSQL.

### 6.3 vs. Standalone Triple Stores (Blazegraph, Virtuoso, Oxigraph)

- **Operational simplicity**: pg_ripple is a PostgreSQL extension, not a separate service
- **Ecosystem access**: all PostgreSQL extensions (pgvector, PostGIS, pg_cron) are available
- **HTAP architecture**: purpose-built for mixed read/write workloads
- **Background workers**: native PostgreSQL workers for merge, validation, inference
- **Proven backup/replication**: pg_basebackup, streaming replication, pg_dump

---

## 7. Implementation Roadmap

### Phase 1: BYOG Export ✅ Implemented in v0.26.0

- Build a SQL function `pg_ripple.export_graphrag_entities()` → Parquet-compatible output
- Build `pg_ripple.export_graphrag_relationships()` → Parquet-compatible output
- Document the BYOG workflow with pg_ripple as source
- **Value**: Any existing pg_ripple knowledge graph can be used with GraphRAG immediately

### Phase 2: GraphRAG Importer ✅ Implemented in v0.26.0

- Build a Python package `graphrag-storage-pg-ripple` implementing GraphRAG's `TableProvider`
- Map GraphRAG knowledge model tables to an RDF ontology (`graphrag:Entity`, `graphrag:Relationship`, etc.)
- Support both read and write paths
- **Value**: GraphRAG's indexing pipeline stores directly into pg_ripple

### Phase 3: Query Integration (In Progress)

- Implement a custom GraphRAG `ContextBuilder` that issues SPARQL queries via pg_ripple_http
- Replace DataFrame-based context assembly with JSON-LD framing
- Add Datalog-enriched entity neighborhoods to Local Search context
- **Value**: GraphRAG queries leverage pg_ripple's full reasoning and federation capabilities

### Phase 4: Real-Time GraphRAG (High effort, high value)

- Integrate with pg_ripple's CDC/streaming capabilities
- Implement incremental community detection triggered by SPARQL view changes
- Build a GraphRAG-compatible community report regeneration pipeline using pg_ripple background workers
- **Value**: Continuous GraphRAG with no batch re-indexing

---

## 8. RDF Ontology for GraphRAG Knowledge Model

A proposed RDF mapping for GraphRAG's output tables:

```turtle
@prefix gr:    <http://graphrag.example.org/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .

# Entity
gr:Entity a rdfs:Class .
gr:title a rdf:Property ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:type a rdf:Property ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:description a rdf:Property ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:frequency a rdf:Property ; rdfs:domain gr:Entity ; rdfs:range xsd:integer .
gr:degree a rdf:Property ; rdfs:domain gr:Entity ; rdfs:range xsd:integer .

# Relationship
gr:Relationship a rdfs:Class .
gr:source a rdf:Property ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:target a rdf:Property ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:weight a rdf:Property ; rdfs:domain gr:Relationship ; rdfs:range xsd:float .

# Community
gr:Community a rdfs:Class .
gr:level a rdf:Property ; rdfs:domain gr:Community ; rdfs:range xsd:integer .
gr:parent a rdf:Property ; rdfs:domain gr:Community ; rdfs:range gr:Community .
gr:hasMember a rdf:Property ; rdfs:domain gr:Community ; rdfs:range gr:Entity .

# Community Report
gr:CommunityReport a rdfs:Class .
gr:community a rdf:Property ; rdfs:domain gr:CommunityReport ; rdfs:range gr:Community .
gr:summary a rdf:Property ; rdfs:domain gr:CommunityReport ; rdfs:range xsd:string .
gr:fullContent a rdf:Property ; rdfs:domain gr:CommunityReport ; rdfs:range xsd:string .
gr:rank a rdf:Property ; rdfs:domain gr:CommunityReport ; rdfs:range xsd:float .

# TextUnit
gr:TextUnit a rdfs:Class .
gr:text a rdf:Property ; rdfs:domain gr:TextUnit ; rdfs:range xsd:string .
gr:tokenCount a rdf:Property ; rdfs:domain gr:TextUnit ; rdfs:range xsd:integer .
gr:mentionsEntity a rdf:Property ; rdfs:domain gr:TextUnit ; rdfs:range gr:Entity .
gr:mentionsRelationship a rdf:Property ; rdfs:domain gr:TextUnit ; rdfs:range gr:Relationship .

# Provenance (via RDF-star)
gr:confidence a rdf:Property ; rdfs:range xsd:float .
gr:sourceTextUnit a rdf:Property ; rdfs:range gr:TextUnit .
gr:extractedBy a rdf:Property ; rdfs:range xsd:string .
gr:extractedAt a rdf:Property ; rdfs:range xsd:dateTime .
```

---

## 9. References

1. Edge, D., Trinh, H., Cheng, N., et al. "From Local to Global: A Graph RAG Approach to Query-Focused Summarization." arXiv:2404.16130 (2024).
2. Microsoft Research. "GraphRAG: Unlocking LLM discovery on narrative private data." Microsoft Research Blog, Feb 2024.
3. Microsoft. "GraphRAG Documentation." https://microsoft.github.io/graphrag/ (v3.0.9, 2026).
4. Microsoft. "Bring Your Own Graph." https://microsoft.github.io/graphrag/index/byog/
5. Traag, V.A., Waltman, L., van Eck, N.J. "From Louvain to Leiden: guaranteeing well-connected communities." Scientific Reports 9, 5233 (2019).
6. pg_ripple. "Implementation Plan." plans/implementation_plan.md
7. pg_ripple. "ROADMAP.md" — Phased delivery plan through v1.0.0.
