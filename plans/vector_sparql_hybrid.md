# Vector + SPARQL Hybrid Search: Analysis & pg_ripple Integration Strategy

> **Date**: 2026-04-18
> **Status**: Research report
> **Audience**: pg_ripple developers and stakeholders

## Executive Summary

**Hybrid search** — combining structured graph queries (SPARQL) with vector similarity search — is emerging as the dominant retrieval paradigm for AI-powered applications. The insight is simple: SPARQL excels at precise, structured reasoning over relationships and constraints, while vector search excels at fuzzy semantic matching across unstructured content. Neither alone is sufficient for modern Retrieval-Augmented Generation (RAG), question answering, and recommendation workloads.

For **pg_ripple**, the hybrid search opportunity is uniquely compelling. pg_ripple already runs inside PostgreSQL, and **pgvector** is the most popular PostgreSQL extension for vector similarity search (14k+ GitHub stars, part of the core offering of every major managed Postgres provider). Because both extensions share the same PostgreSQL process, the integration can achieve **zero-copy, in-process joins** between SPARQL-generated relational plans and vector similarity scans — a capability no other triplestore or vector database can match.

This report analyzes the vector + SPARQL hybrid landscape, surveys the competitive environment, maps concrete integration points with pgvector, and proposes a phased implementation strategy for pg_ripple.

---

## 1. The Case for Hybrid Search

### 1.1 Limitations of Pure Vector Search

Vector similarity search retrieves documents based on semantic proximity in embedding space. While powerful for natural language queries, it has well-documented failure modes:

| Limitation | Example |
|---|---|
| **No structural reasoning** | "Find all drugs that interact with aspirin AND are approved by the FDA" requires multi-hop join logic, not cosine similarity |
| **No negation** | "Find proteins NOT associated with cancer" cannot be expressed as a vector query |
| **No aggregation** | "Count the number of papers per author in graph theory" requires GROUP BY semantics |
| **No provenance** | Vector search returns ranked lists without explaining *why* a result is relevant |
| **Hallucination amplification** | Embedding models encode training-data biases; similarity scores provide no factual grounding |

### 1.2 Limitations of Pure SPARQL

SPARQL excels at precise, structured queries but struggles with:

| Limitation | Example |
|---|---|
| **No fuzzy matching** | `FILTER regex(?label, "asprin")` won't find "aspirin" (typo) or "acetylsalicylic acid" (synonym) |
| **No semantic similarity** | "Find entities similar to Paris" (ambiguous: city? person? agreement?) requires embedding-space reasoning |
| **No unstructured content** | SPARQL operates on triples; free-text descriptions, PDFs, and images need a different retrieval model |
| **Cold-start brittleness** | Users must know the ontology (predicate names, class hierarchy) to write effective queries |

### 1.3 The Hybrid Advantage

Hybrid search combines both paradigms, enabling queries like:

```
"Find all drugs (SPARQL: ?drug rdf:type :Drug) that are semantically similar
 to 'anti-inflammatory agents' (vector: cosine < 0.3) AND have known
 interactions with aspirin (SPARQL: ?drug :interactsWith :aspirin)"
```

This query is impossible in either paradigm alone. The SPARQL engine handles the structural constraints (type filtering, join traversal), while the vector engine handles the semantic matching. The fusion of results is where the value lies.

---

## 2. pgvector: The PostgreSQL Vector Engine

### 2.1 Overview

**pgvector** (v0.8.2, MIT license) is the standard PostgreSQL extension for vector similarity search. It provides:

- **Data types**: `vector` (up to 16,000 dimensions), `halfvec` (half-precision, up to 16,000 dims), `bit` (binary vectors, up to 64,000 dims), `sparsevec` (sparse vectors, up to 16,000 non-zero elements)
- **Distance functions**: L2 (`<->`), negative inner product (`<#>`), cosine (`<=>`), L1/taxicab (`<+>`), Hamming (`<~>`), Jaccard (`<%>`)
- **Index types**: HNSW (hierarchical navigable small world graphs) and IVFFlat (inverted file with flat quantization)
- **Filtering**: Iterative index scans (v0.8.0+) allow efficient pre-filtering and post-filtering with `WHERE` clauses
- **Hybrid search**: Built-in support for combining pgvector with PostgreSQL full-text search via Reciprocal Rank Fusion (RRF)

### 2.2 Key Technical Properties

| Property | Value |
|---|---|
| Max vector dimensions | 16,000 (vector), 16,000 (halfvec) |
| Storage per vector | $4d + 8$ bytes (single-precision) |
| HNSW build parameters | `m` (max connections, default 16), `ef_construction` (default 64) |
| HNSW query parameter | `ef_search` (default 40, higher = better recall) |
| IVFFlat lists | Recommended: `rows/1000` (< 1M rows), `√rows` (> 1M rows) |
| Iterative scans | `hnsw.iterative_scan = strict_order` or `relaxed_order` |
| WAL support | Full — enables replication and PITR |
| Parallel builds | `max_parallel_maintenance_workers` (default 2) |

### 2.3 Why pgvector Is the Right Partner

1. **Same process, same transaction**: pg_ripple and pgvector share the PostgreSQL backend. JOINs between VP tables and vector tables execute in-process with zero serialization overhead.
2. **ACID compliance**: Vector inserts and triple inserts can share a transaction — either both commit or neither does.
3. **Index co-location**: HNSW indexes on vectors and B-tree indexes on dictionary IDs live in the same shared buffer pool. The query planner can reason about both simultaneously.
4. **Ubiquity**: pgvector ships with every major managed Postgres provider (AWS RDS, Azure, GCP CloudSQL, Supabase, Neon, etc.). Any pg_ripple deployment that supports pgvector gains hybrid search for free.
5. **Ecosystem**: 40+ language bindings (pgvector-python, pgvector-rust, pgvector-node, etc.) and integration with every major ML framework.

---

## 3. Competitive Landscape

### 3.1 Triplestores with Vector Capabilities

| System | Vector Approach | Limitations |
|---|---|---|
| **Ontotext GraphDB** | Lucene connector for FTS; no native vector similarity | Requires external vector DB; no in-process join |
| **Stardog** | Vector similarity via external Pinecone/Weaviate connector | Cross-process latency; no transactional consistency |
| **Apache Jena / Fuseki** | Lucene FTS integration; experimental vector plugin | Java-only; no ANN index; no GPU acceleration |
| **Virtuoso** | FTS via Lucene; no vector similarity | Legacy architecture; no embedding support |
| **Oxigraph** | Pure SPARQL engine; no vector support | Rust-native but no extension ecosystem |

### 3.2 Vector Databases with Graph Capabilities

| System | Graph Approach | Limitations |
|---|---|---|
| **Weaviate** | Cross-references between objects (not a graph query language) | No SPARQL, no reasoning, no SHACL |
| **Qdrant** | Payload filtering (flat key-value, no joins) | No graph traversal, no inference |
| **Pinecone** | Metadata filtering only | No relationships, no graph queries |
| **Milvus** | Scalar filtering; no graph model | No ontology support |

### 3.3 Graph Databases with Vector Capabilities

| System | Vector Approach | Limitations |
|---|---|---|
| **Neo4j** | GDS library: FastRP, GraphSAGE, Node2Vec, HashGNN embeddings; vector index (since 5.11) | No SPARQL, no RDF, proprietary Cypher |
| **Amazon Neptune** | Neptune Analytics: vector similarity + openCypher/Gremlin | No SPARQL vector integration; Analytics is a separate service |
| **TigerGraph** | Graph + ML workbench; no native vector index | No RDF/SPARQL |

### 3.4 pg_ripple's Unique Position

**No existing system combines all of the following**:

1. ✅ Full SPARQL 1.1 query engine
2. ✅ SHACL validation
3. ✅ Datalog reasoning (OWL RL)
4. ✅ RDF-star (statement-level metadata)
5. ✅ HTAP architecture for concurrent read/write
6. ✅ **In-process pgvector integration** (same PostgreSQL backend)
7. ✅ ACID transactions spanning triples and vectors
8. ✅ PostgreSQL ecosystem (pg_stat_statements, logical replication, connection pooling)

pg_ripple + pgvector is the only stack that can execute a single SQL query plan that joins SPARQL triple patterns with vector similarity scans — with full transactional guarantees, zero serialization overhead, and the PostgreSQL query planner optimizing the entire plan.

---

## 4. Integration Architecture

### 4.1 Schema Design: Embedding Table

The core integration point is a new table that maps dictionary-encoded entities to their vector embeddings:

```sql
CREATE TABLE _pg_ripple.embeddings (
    entity_id   BIGINT NOT NULL REFERENCES _pg_ripple.dictionary(id),
    model       TEXT   NOT NULL DEFAULT 'default',
    embedding   vector(1536),         -- pgvector type; dimension varies by model
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (entity_id, model)
);

-- HNSW index for fast ANN queries
CREATE INDEX ON _pg_ripple.embeddings
    USING hnsw (embedding vector_cosine_ops);
```

Key design decisions:
- **Dictionary-encoded foreign key**: `entity_id` is an i64 from pg_ripple's XXH3-128 dictionary, enabling zero-copy joins with VP tables.
- **Multi-model support**: The `model` column allows storing embeddings from different models (OpenAI `text-embedding-3-small`, Cohere `embed-v3`, local Sentence-BERT) side by side.
- **pgvector type**: Using pgvector's native `vector` type gets us HNSW/IVFFlat indexing, all distance operators, and iterative scans for free.

### 4.2 SPARQL Extension: `pg:similar` Custom Function

Introduce a SPARQL extension function that bridges the gap:

```sparql
PREFIX pg: <http://pg-ripple.org/functions/>

SELECT ?drug ?label ?score
WHERE {
    ?drug a :Drug ;
          rdfs:label ?label ;
          :interactsWith :aspirin .
    BIND(pg:similar(?drug, "anti-inflammatory agent", 10) AS ?score)
    FILTER(?score < 0.3)
}
ORDER BY ?score
LIMIT 20
```

The `pg:similar(?entity, "query_text", k)` function:
1. Encodes `"query_text"` to a vector using the configured embedding model
2. Looks up `?entity`'s embedding in `_pg_ripple.embeddings`
3. Returns the cosine distance as a float
4. The SPARQL→SQL translator pushes this down to a pgvector operator (`<=>`)

### 4.3 SQL Translation Strategy

The SPARQL query above translates to SQL that naturally combines VP table joins with pgvector operators:

```sql
WITH drug_triples AS (
    SELECT vp_type.s AS drug_id
    FROM _pg_ripple.vp_42 vp_type        -- rdf:type
    JOIN _pg_ripple.vp_87 vp_interact     -- :interactsWith
      ON vp_type.s = vp_interact.s
    WHERE vp_type.o = 12345               -- :Drug (dictionary-encoded)
      AND vp_interact.o = 67890           -- :aspirin (dictionary-encoded)
)
SELECT
    d_drug.value   AS drug,
    d_label.value  AS label,
    e.embedding <=> $query_vec AS score   -- pgvector cosine distance
FROM drug_triples dt
JOIN _pg_ripple.vp_99 vp_label            -- rdfs:label
  ON dt.drug_id = vp_label.s
JOIN _pg_ripple.dictionary d_drug
  ON dt.drug_id = d_drug.id
JOIN _pg_ripple.dictionary d_label
  ON vp_label.o = d_label.id
JOIN _pg_ripple.embeddings e
  ON dt.drug_id = e.entity_id
WHERE e.embedding <=> $query_vec < 0.3
ORDER BY score
LIMIT 20;
```

The PostgreSQL query planner can:
- Use B-tree indexes on VP tables for the structural joins
- Use the HNSW index on `_pg_ripple.embeddings` for the vector similarity scan
- Apply iterative scans if the filter reduces the result set

### 4.4 Embedding Pipeline

```
┌──────────────┐     ┌──────────────┐     ┌──────────────────────┐
│  pg_ripple   │     │  Embedding   │     │  _pg_ripple.         │
│  dictionary  │────▶│  Service     │────▶│  embeddings          │
│  (IRIs,      │     │  (OpenAI,    │     │  (entity_id, vector) │
│   literals)  │     │   Ollama,    │     │                      │
└──────────────┘     │   HF local)  │     └──────────────────────┘
                     └──────────────┘
```

Three embedding strategies:

1. **Batch embedding** (`pg_ripple.embed_entities()`):
   - Export entity labels/descriptions via SPARQL CONSTRUCT
   - Embed via external service (OpenAI API, local Ollama, Sentence-BERT)
   - Bulk COPY into `_pg_ripple.embeddings`

2. **Trigger-based incremental embedding**:
   - On dictionary insert, queue new entities for embedding
   - Background worker processes the queue asynchronously
   - Uses pgrx `BackgroundWorker` with `BGWORKER_SHMEM_ACCESS`

3. **User-supplied embeddings**:
   - Direct INSERT into `_pg_ripple.embeddings`
   - Useful for pre-computed KGE embeddings (TransE, RotatE, ComplEx)
   - Bridges the link prediction pipeline from plans/link_prediction.md

### 4.5 Hybrid Retrieval Modes

| Mode | Description | Use Case |
|---|---|---|
| **SPARQL-first** | Execute SPARQL BGP, then rank results by vector similarity | Structured query + semantic re-ranking |
| **Vector-first** | Find k-nearest entities, then filter/enrich via SPARQL | Semantic search + structural validation |
| **Fused** | Execute both in parallel, combine via RRF or weighted score | Maximum recall for RAG pipelines |
| **Graph-aware embedding** | Use graph structure to improve embeddings (GraphSAGE, R-GCN) | Embeddings that encode relational context |

---

## 5. Advanced Integration Patterns

### 5.1 SPARQL-Guided RAG

The most impactful application of hybrid search is **Retrieval-Augmented Generation** with structured knowledge:

```
User question: "What treatments exist for rheumatoid arthritis
               that don't interact with methotrexate?"

Step 1 (SPARQL): Find all treatments for rheumatoid arthritis
    ?treatment :treats :rheumatoid_arthritis .
    MINUS { ?treatment :interactsWith :methotrexate }

Step 2 (Vector): Rank treatments by semantic similarity to the query
    ORDER BY pg:similar(?treatment, "treatments for rheumatoid arthritis")

Step 3 (LLM): Generate answer using top-k results as context
```

This pipeline eliminates the hallucination problem of pure RAG: the SPARQL query guarantees structural correctness (only actual treatments, no drug interactions), while vector ranking provides relevance ordering.

### 5.2 Ontology-Aware Semantic Search

Traditional vector search treats all entities equally. pg_ripple can leverage its ontology (SHACL shapes, RDFS class hierarchy, OWL restrictions) to improve search quality:

```sparql
# Find entities similar to "Paris" but only in the context of cities
SELECT ?city ?score
WHERE {
    ?city a/rdfs:subClassOf* :City .
    BIND(pg:similar(?city, "Paris", 100) AS ?score)
}
ORDER BY ?score
LIMIT 10
```

The `a/rdfs:subClassOf*` property path leverages pg_ripple's Datalog reasoning to traverse the class hierarchy, ensuring that only cities (not people named Paris, not the Paris Agreement) are returned.

### 5.3 Graph-Contextualized Embeddings

Entity embeddings can be enriched with graph context. For each entity, generate a text representation that includes its neighborhood:

```
Entity: :aspirin
Label: "Aspirin"
Type: Drug, NSAID
Treats: Headache, Fever, Rheumatoid Arthritis
Interacts with: Warfarin, Ibuprofen, Methotrexate
Mechanism: COX-1 and COX-2 inhibition
```

This "graph-serialized text" produces embeddings that encode relational information — making vector similarity more semantically meaningful than embedding the label alone.

pg_ripple's CONSTRUCT queries can generate these contextualized descriptions automatically:

```sparql
CONSTRUCT {
    ?entity rdfs:label ?label ;
            :description ?desc ;
            rdf:type ?type ;
            :relatedTo ?related .
}
WHERE {
    ?entity rdfs:label ?label .
    OPTIONAL { ?entity :description ?desc }
    OPTIONAL { ?entity rdf:type ?type }
    OPTIONAL { ?entity ?p ?related . FILTER(?p != rdf:type) }
}
```

### 5.4 SHACL-Validated Embedding Quality

SHACL shapes can enforce constraints on the embedding table:

```turtle
:EmbeddingShape a sh:NodeShape ;
    sh:targetClass :Entity ;
    sh:property [
        sh:path :hasEmbedding ;
        sh:minCount 1 ;           # Every entity must have an embedding
        sh:message "Entity missing embedding vector" ;
    ] .
```

This ensures embedding completeness — if a new entity is added to the knowledge graph without an embedding, SHACL validation flags it.

### 5.5 Federation with External Vector Services

pg_ripple's SPARQL federation (v0.16.0+) can be extended to federate with external vector search services:

```sparql
SELECT ?entity ?score
WHERE {
    SERVICE <http://vector-service/search> {
        ?entity pg:similarTo "anti-inflammatory" ;
                pg:score ?score .
    }
    ?entity a :Drug .                    # Local structural filter
    ?entity :approvedBy :FDA .           # Local structural filter
}
```

This pattern allows pg_ripple to serve as the "structured brain" while delegating vector search to specialized services (Pinecone, Weaviate, Qdrant) when the embedding index is too large for pgvector.

---

## 6. Market Positioning

### 6.1 Target Segments

| Segment | Pain Point | pg_ripple + pgvector Solution |
|---|---|---|
| **Biomedical/Pharma** | Drug-drug interaction queries need both semantic similarity and structural constraints | SPARQL for interaction graphs + vector for mechanism-of-action similarity |
| **Enterprise Knowledge Management** | RAG pipelines hallucinate because they lack structured grounding | SPARQL CONSTRUCT feeds LLM context with guaranteed-accurate facts |
| **Financial Services** | Regulatory compliance requires precise entity resolution across jurisdictions | Vector for fuzzy entity matching + SPARQL for regulatory relationship traversal |
| **Academic/Research** | Citation networks need both structural analysis and semantic clustering | SPARQL for citation graph patterns + vector for topic similarity |
| **E-commerce** | Product recommendations need both catalog structure and user behavior signals | SPARQL for catalog constraints + vector for user preference matching |

### 6.2 Competitive Messaging

**"The only triplestore that speaks pgvector."**

Key differentiators:

1. **Zero-overhead hybrid queries**: In-process joins between SPARQL triple patterns and vector similarity — no network hop, no serialization, no eventual consistency.
2. **Transactional vector + graph updates**: Insert a triple and its embedding in one `BEGIN ... COMMIT`. No other system offers this.
3. **PostgreSQL-native**: Works with every PostgreSQL tool, monitoring system, backup strategy, and managed service. No new infrastructure to learn.
4. **Full SPARQL 1.1 + vector**: The only system that can execute a single query combining BGP matching, property paths, OPTIONAL, aggregation, subqueries, and vector similarity.
5. **Reasoning + vector**: Datalog materialization + vector search means inferred facts are also searchable by similarity.

### 6.3 Positioning Against Alternatives

| Alternative | pg_ripple advantage |
|---|---|
| Neo4j + vector index | pg_ripple has full SPARQL 1.1, SHACL, Datalog reasoning, RDF-star. Neo4j has Cypher only. |
| Weaviate / Qdrant / Pinecone | pg_ripple has structured graph querying, ontology reasoning, and ACID transactions. Vector DBs have none of these. |
| LangChain/LlamaIndex + separate vector DB + separate graph DB | pg_ripple collapses three systems into one PostgreSQL instance. Lower latency, lower ops cost, stronger consistency. |
| Ontotext GraphDB + external vector service | pg_ripple's pgvector integration is in-process; GraphDB requires cross-process round-trips. |
| Amazon Neptune Analytics | pg_ripple is open source, runs on any PostgreSQL, supports SPARQL (Neptune Analytics only supports openCypher/Gremlin for vector queries). |

---

## 7. Implementation Plan

### Phase 1: Foundation (v0.28.0) — 4–6 person-weeks

**Goal**: Core pgvector integration — embedding table, bulk loading, basic similarity function.

| Deliverable | Description | Effort |
|---|---|---|
| `_pg_ripple.embeddings` table | Schema with `entity_id BIGINT`, `model TEXT`, `embedding vector(N)`, HNSW index | 0.5 pw |
| `pg_ripple.embed_entities()` | SQL function: batch-embed entities by label via external HTTP API (OpenAI-compatible) | 1.5 pw |
| `pg_ripple.similar_entities()` | SQL function: find k-nearest entities by cosine similarity, returning (entity_id, distance) | 1 pw |
| SPARQL `pg:similar()` function | Register as SPARQL extension function; translate to pgvector `<=>` operator in SQL plan | 1.5 pw |
| GUC parameters | `pg_ripple.embedding_model`, `pg_ripple.embedding_dimensions`, `pg_ripple.embedding_api_url` | 0.5 pw |
| pg_regress tests | Hybrid query tests, embedding CRUD, similarity ranking, SPARQL integration | 1 pw |

### Phase 2: Advanced Hybrid (v0.29.0) — 3–5 person-weeks

**Goal**: Production-grade hybrid search with multiple fusion strategies and incremental embedding.

| Deliverable | Description | Effort |
|---|---|---|
| RRF fusion | `pg_ripple.hybrid_search(sparql, query_text, alpha)` — combines SPARQL results with vector results via Reciprocal Rank Fusion | 1.5 pw |
| Incremental embedding worker | Background worker that watches dictionary inserts and queues new entities for embedding | 1.5 pw |
| Graph-contextualized embedding | `pg_ripple.contextualize_entity()` — generates graph-serialized text for an entity using CONSTRUCT query, then embeds it | 1 pw |
| Multi-model support | Support multiple embedding models per entity (different dimensions, different providers) | 0.5 pw |
| Benchmarks | pgbench-based hybrid search benchmarks; measure latency/throughput for various query patterns | 0.5 pw |

### Phase 3: RAG Pipeline (v0.30.0) — 3–4 person-weeks

**Goal**: End-to-end RAG support — SPARQL-guided retrieval, context assembly, LLM integration.

| Deliverable | Description | Effort |
|---|---|---|
| `pg_ripple.rag_retrieve()` | SQL function: given a natural language question, run hybrid search and return structured context for LLM consumption | 1.5 pw |
| JSON-LD context output | Format hybrid search results as JSON-LD frames suitable for LLM system prompts | 0.5 pw |
| `pg_ripple_http` RAG endpoint | HTTP endpoint (`POST /rag`) that accepts a question and returns hybrid search results | 1 pw |
| SHACL embedding completeness | SHACL constraint shape that validates all entities have embeddings | 0.5 pw |
| Documentation | User guide, API reference, example notebooks | 0.5 pw |

**Total estimated effort**: 10–15 person-weeks across three releases.

---

## 8. Technical Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| pgvector not installed | Hybrid features unavailable | Runtime check via `pg_extension_config_dump`; degrade gracefully (return empty, emit WARNING) |
| Embedding API latency | Bulk embedding is slow for large graphs | Support local embedding models (Ollama, vLLM); batch API calls; async background worker |
| Dimension mismatch | Different models produce different-dimension vectors | Multi-model schema with `model` column; validate dimensions on insert |
| Stale embeddings | Entity labels change but embeddings aren't updated | Trigger-based invalidation; `pg_ripple.refresh_embeddings()` function |
| Memory pressure | Large HNSW indexes compete with VP table caches for shared_buffers | Tuning guide; halfvec support for 50% memory reduction; binary quantization for large-scale |

---

## 9. Conclusion

The convergence of structured knowledge graphs and vector similarity search is not a future trend — it is the present. Every major RAG framework (LangChain, LlamaIndex, Microsoft GraphRAG) is moving toward hybrid retrieval. The question is not *whether* to integrate vectors but *how quickly*.

pg_ripple's position inside PostgreSQL makes the pgvector integration uniquely natural and uniquely powerful. No other triplestore can offer in-process, transactional, planner-optimized joins between graph patterns and vector similarity scans. This is a structural advantage that cannot be replicated by systems running in separate processes or on separate machines.

By implementing the phased plan outlined above, pg_ripple becomes the **only system in the market** that combines:

- Full SPARQL 1.1 query engine
- SHACL validation and Datalog reasoning
- pgvector-powered hybrid semantic search
- ACID transactions spanning triples and vectors
- HTTP API for RAG pipelines
- All running in a single PostgreSQL instance

This positions pg_ripple as the ideal backend for the emerging class of **knowledge-grounded AI applications** — applications that need both the precision of structured queries and the flexibility of semantic search.
