# §2.7 AI Retrieval and GraphRAG

## What and Why

Knowledge graphs and vector search are complementary: vectors excel at fuzzy semantic
similarity ("what treats headaches?"), while graph structure captures precise relationships
("which drugs interact with aspirin?"). pg_ripple combines both in a single database,
eliminating the need for a separate vector store.

This chapter is the **canonical AI and retrieval reference**. It covers:

- **Vector embeddings**: store and index entity embeddings alongside RDF triples.
- **HNSW indexes**: fast approximate nearest-neighbor search via pgvector.
- **Hybrid retrieval**: Reciprocal Rank Fusion (RRF) of SPARQL and vector results.
- **`rag_retrieve()`**: end-to-end RAG pipeline from question to LLM-ready context.
- **JSON-LD framing for LLM prompts**: structured context for grounded generation.
- **Graph-enriched embeddings**: use `owl:sameAs` canonicalization and neighborhood context.
- **Full-text broadening**: combine FTS with vector search for recall.

```admonish note
pg_ripple's vector features require the **pgvector** extension. All vector functions
gracefully degrade (return zero rows with a WARNING) when pgvector is not installed.
```

### Why Not a Separate Vector Store?

| Concern | Separate vector store | pg_ripple integrated |
|---|---|---|
| Data consistency | Sync required between stores | Single source of truth |
| ACID transactions | No transactional guarantees | Full PostgreSQL ACID |
| Hybrid queries | Two round-trips + client-side merge | Single SQL query |
| Operational cost | Two systems to manage | One PostgreSQL instance |
| Graph-aware embeddings | Not possible | `contextualize_entity()` enriches embeddings |

---

## How It Works

### The Embedding Pipeline

1. **Store entities** as RDF triples with `rdfs:label` and `rdf:type`.
2. **Embed** entities via an OpenAI-compatible API: `embed_entities()` calls the API in batches and stores vectors in `_pg_ripple.embeddings`.
3. **Index** with pgvector HNSW for approximate nearest-neighbor search.
4. **Query** with `similar_entities()`, `hybrid_search()`, or `rag_retrieve()`.

### Key Functions

| Function | Purpose |
|---|---|
| `store_embedding(iri, vec, model)` | Manually store one entity's embedding |
| `embed_entities(graph, model, batch)` | Batch-embed entities from a graph |
| `refresh_embeddings(graph, model, force)` | Re-embed stale entities |
| `similar_entities(text, k, model)` | Find k nearest entities to a text query |
| `hybrid_search(sparql, text, k, alpha, model)` | RRF fusion of SPARQL + vector results |
| `rag_retrieve(question, filter, k, model, fmt)` | End-to-end RAG with context collection |
| `contextualize_entity(iri, depth, max)` | Build text context from RDF neighborhood |
| `add_embedding_triples()` | Materialise `pg:hasEmbedding` for SHACL checks |
| `list_embedding_models()` | List stored models with counts and dimensions |

### GUC Parameters

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.embedding_api_url` | (none) | OpenAI-compatible embedding API base URL |
| `pg_ripple.embedding_api_key` | (none) | API key (superuser only, not logged) |
| `pg_ripple.embedding_model` | `text-embedding-3-small` | Default embedding model |
| `pg_ripple.embedding_dimensions` | `1536` | Vector dimension count |
| `pg_ripple.use_graph_context` | `off` | Enrich embedding input with graph neighborhood |
| `pg_ripple.auto_embed` | `off` | Auto-queue new entities for embedding |
| `pg_ripple.embedding_batch_size` | `100` | API batch size for `embed_entities()` |

---

## Worked Examples

### Setup: Configure Embedding API

```sql
-- Point to your OpenAI-compatible embedding endpoint
ALTER SYSTEM SET pg_ripple.embedding_api_url = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key = 'sk-your-key-here';
ALTER SYSTEM SET pg_ripple.embedding_model = 'text-embedding-3-small';
ALTER SYSTEM SET pg_ripple.embedding_dimensions = 1536;
SELECT pg_reload_conf();
```

```admonish warning
The API key is stored as a superuser-only GUC. It never appears in query logs or
`pg_stat_statements`. For production, consider using a local embedding service
(e.g., Ollama, vLLM) to avoid sending data to external APIs.
```

### Step 1: Embed Entities

Batch-embed all entities with an `rdfs:label`:

```sql
-- Embed all entities in the default graph
SELECT pg_ripple.embed_entities();
-- Returns: 150 (number of embeddings stored)

-- Embed only entities in a specific graph
SELECT pg_ripple.embed_entities('https://example.org/graph/pubmed');

-- Override the model for this call
SELECT pg_ripple.embed_entities(NULL, 'text-embedding-3-large', 50);
```

### Step 2: Similar Entity Search

Find entities semantically similar to a question:

```sql
SELECT * FROM pg_ripple.similar_entities('knowledge graph applications', 5);
```

Returns:

| entity_id | entity_iri | distance |
|---|---|---|
| 42001 | `<https://example.org/paper/42>` | 0.12 |
| 99001 | `<https://example.org/paper/99>` | 0.18 |
| 10001 | `<https://example.org/person/alice>` | 0.31 |

### Step 3: Hybrid Search with RRF

Combine SPARQL structural queries with vector similarity:

```sql
SELECT * FROM pg_ripple.hybrid_search(
    'PREFIX dct: <http://purl.org/dc/terms/>
     PREFIX bibo: <http://purl.org/ontology/bibo/>
     SELECT ?entity WHERE {
         ?entity a bibo:AcademicArticle ;
                 dct:creator <https://example.org/person/alice> .
     }',
    'knowledge graph survey',
    10,
    0.5
);
```

Returns:

| entity_id | entity_iri | rrf_score | sparql_rank | vector_rank |
|---|---|---|---|---|
| 42001 | `<https://example.org/paper/42>` | 0.032 | 1 | 1 |
| 99001 | `<https://example.org/paper/99>` | 0.024 | 0 | 2 |

The `alpha` parameter controls weighting:
- `alpha = 1.0`: SPARQL only (graph structure)
- `alpha = 0.0`: vector only (semantic similarity)
- `alpha = 0.5`: equal weight (default)

### Step 4: End-to-End RAG with `rag_retrieve()`

The complete pipeline from question to LLM-ready context:

```sql
SELECT * FROM pg_ripple.rag_retrieve(
    'What papers discuss knowledge graphs?',
    NULL,
    5
);
```

Returns:

| entity_iri | label | context_json | distance |
|---|---|---|---|
| `<https://example.org/paper/42>` | Knowledge Graphs in Practice | `{"types": [...], "properties": [...], ...}` | 0.12 |

With a SPARQL filter to restrict candidates:

```sql
SELECT * FROM pg_ripple.rag_retrieve(
    'What papers discuss knowledge graphs?',
    '?entity a <http://purl.org/ontology/bibo/AcademicArticle> .',
    5
);
```

Get JSON-LD formatted context for LLM consumption:

```sql
SELECT * FROM pg_ripple.rag_retrieve(
    'What papers discuss knowledge graphs?',
    NULL,
    5,
    NULL,
    'jsonld'
);
```

### Building LLM Prompts with JSON-LD Framing

Use framed JSON-LD as structured context for LLM prompts:

```sql
-- Get framed JSON-LD for a specific paper
SELECT pg_ripple.export_jsonld_framed('{
    "@context": {
        "dct": "http://purl.org/dc/terms/",
        "foaf": "http://xmlns.com/foaf/0.1/",
        "bibo": "http://purl.org/ontology/bibo/",
        "schema": "https://schema.org/",
        "title": "dct:title",
        "creator": "dct:creator",
        "name": "foaf:name",
        "affiliation": "schema:affiliation",
        "cites": "bibo:cites",
        "keywords": "schema:keywords"
    },
    "@type": "bibo:AcademicArticle",
    "creator": {
        "name": {},
        "affiliation": { "name": {} }
    },
    "cites": { "title": {} }
}'::jsonb);
```

This produces nested JSON that LLMs can reason about more effectively than flat triples.

### Graph-Enriched Embeddings

Use `contextualize_entity()` to build richer text for embedding:

```sql
-- Get context text for an entity
SELECT pg_ripple.contextualize_entity(
    'https://example.org/paper/42',
    1,
    20
);
```

Returns a text string like:

```
Knowledge Graphs in Practice. Type: AcademicArticle. Created by: Alice Johnson, Bob Smith. 
Cited by: Graph Neural Networks for Entity Resolution. Keywords: knowledge graph, RDF, SPARQL.
```

Enable graph-enriched embeddings globally:

```sql
SET pg_ripple.use_graph_context = 'on';

-- Now embed_entities() uses contextualize_entity() for each entity
SELECT pg_ripple.embed_entities();
```

### owl:sameAs Before Embedding

Canonicalize equivalent entities before embedding to avoid duplicates:

```sql
-- Load sameAs links
SELECT pg_ripple.load_turtle('
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .

ex:person/alice owl:sameAs <https://orcid.org/0000-0001-2345-6789> .
');

-- Run OWL RL inference to canonicalize
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');

-- Now embed — equivalent entities share a single embedding
SELECT pg_ripple.embed_entities();
```

### Full-Text Search Broadening

Combine vector search with PostgreSQL full-text search for higher recall:

```sql
-- Create FTS index on paper titles
SELECT pg_ripple.fts_index('<http://purl.org/dc/terms/title>');

-- Use FTS to find papers by keyword
SELECT * FROM pg_ripple.fts_search(
    'knowledge & graph',
    '<http://purl.org/dc/terms/title>'
);

-- Combine FTS candidates with vector search in a hybrid approach
-- Step 1: Get FTS matches
-- Step 2: Get vector matches
-- Step 3: Merge with RRF (done automatically in hybrid_search)
SELECT * FROM pg_ripple.hybrid_search(
    'PREFIX dct: <http://purl.org/dc/terms/>
     SELECT ?entity WHERE {
         ?entity dct:title ?t .
         FILTER (CONTAINS(?t, "knowledge"))
     }',
    'knowledge graph applications',
    10,
    0.6
);
```

### Storing Manual Embeddings

If you compute embeddings externally:

```sql
SELECT pg_ripple.store_embedding(
    'https://example.org/paper/42',
    ARRAY[0.1, -0.2, 0.3, 0.05, -0.15, 0.25, 0.08, -0.1, 0.2, 0.12]::float8[],
    'custom-model-v1'
);
```

### Refreshing Stale Embeddings

After updating entity labels, refresh the affected embeddings:

```sql
-- Refresh only entities whose labels changed
SELECT pg_ripple.refresh_embeddings();
-- Returns: 12 (re-embedded entities)

-- Force re-embed everything
SELECT pg_ripple.refresh_embeddings(NULL, NULL, true);
```

### Checking Embedding Coverage

```sql
-- List all embedding models and their entity counts
SELECT * FROM pg_ripple.list_embedding_models();

-- Add pg:hasEmbedding triples for SHACL completeness checks
SELECT pg_ripple.add_embedding_triples();

-- Validate embedding completeness
SELECT pg_ripple.validate();
```

---

## Common Patterns

### Pattern: Complete RAG Pipeline

```sql
-- 1. Load knowledge graph
SELECT pg_ripple.load_turtle_file('/data/domain.ttl');

-- 2. Run inference to derive additional facts
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');

-- 3. Embed entities
SELECT pg_ripple.embed_entities();

-- 4. Query with RAG
SELECT * FROM pg_ripple.rag_retrieve(
    'What drugs treat migraines?',
    '?entity a <https://example.org/Drug> .',
    5,
    NULL,
    'jsonld'
);
```

### Pattern: Periodic Re-Embedding

Schedule embedding refresh after data updates:

```sql
-- After loading new data
SELECT pg_ripple.load_turtle('...');
SELECT pg_ripple.infer('rdfs');

-- Refresh embeddings for entities with changed labels
SELECT pg_ripple.refresh_embeddings();

-- Compact HTAP tables
SELECT pg_ripple.compact();
```

### Pattern: Multi-Model Embeddings

Store embeddings from different models for comparison:

```sql
-- Embed with model A
SELECT pg_ripple.embed_entities(NULL, 'text-embedding-3-small');

-- Embed with model B
SELECT pg_ripple.embed_entities(NULL, 'text-embedding-3-large');

-- List stored models
SELECT * FROM pg_ripple.list_embedding_models();

-- Search with a specific model
SELECT * FROM pg_ripple.similar_entities('knowledge graphs', 10, 'text-embedding-3-large');
```

### Pattern: Serving RAG via HTTP

Use pg_ripple_http's `/rag` endpoint for REST access (see [§2.8](../features/apis-and-integration.md)):

```bash
curl -X POST http://localhost:8080/rag \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What treats headaches?",
    "k": 5,
    "output_format": "jsonld"
  }'
```

The response includes both structured results and a pre-formatted `context` string
ready to be injected into an LLM system prompt.

---

## Performance and Trade-offs

### Embedding Storage

Each embedding vector occupies `dimensions * 4` bytes (float32 in pgvector). For 1536-dimensional
embeddings, that is ~6 KB per entity. A graph with 1M entities uses ~6 GB for embeddings alone.

### HNSW Index Performance

| Entities | Index build time | Query latency (k=10) | Recall@10 |
|---|---|---|---|
| 10K | ~2s | <5ms | >95% |
| 100K | ~20s | <10ms | >95% |
| 1M | ~5min | <20ms | >92% |

### RRF Fusion Overhead

`hybrid_search()` executes two queries (SPARQL + vector) and fuses results in Rust.
Total overhead beyond the individual query times is <1ms for typical result sizes.

### API Call Costs

`embed_entities()` calls an external API. Batch size affects both throughput and cost:

- Larger batches reduce round-trips but increase per-request latency.
- Default batch size (100) is a good balance for OpenAI's API.
- For local models (Ollama, vLLM), increase batch size to 500+.

```admonish tip
For large initial embeddings, consider running `embed_entities()` in a separate
session with a larger `embedding_batch_size` setting to maximize throughput.
```

---

## Gotchas and Debugging

### pgvector Not Installed

All vector functions return zero rows with a WARNING when pgvector is absent:

```
WARNING: pg_ripple.similar_entities: pgvector not available (PT603)
```

Fix: install pgvector and `CREATE EXTENSION vector`.

### No Embeddings Found

If `similar_entities()` returns empty:

1. Check that `embedding_api_url` is configured:
   ```sql
   SHOW pg_ripple.embedding_api_url;
   ```
2. Check that embeddings exist:
   ```sql
   SELECT * FROM pg_ripple.list_embedding_models();
   ```
3. Run `embed_entities()` if needed.

### Dimension Mismatch

The vector dimension in `_pg_ripple.embeddings` must match `embedding_dimensions`:

```sql
SHOW pg_ripple.embedding_dimensions;
-- Must match the model's output dimension (1536 for text-embedding-3-small)
```

### Slow Vector Queries

If vector queries are slow, check that an HNSW index exists on the embeddings table.
pg_ripple creates one automatically, but it may need rebuilding after large batch inserts:

```sql
-- Rebuild the HNSW index
REINDEX INDEX _pg_ripple.embeddings_embedding_idx;
```

### API Rate Limits

`embed_entities()` respects rate limits by batching. If you hit rate limits, reduce
`embedding_batch_size`:

```sql
SET pg_ripple.embedding_batch_size = 50;
SELECT pg_ripple.embed_entities();
```

---

## Next Steps

- **[§2.6 Exporting and Sharing](../features/exporting-and-sharing.md)** — GraphRAG BYOG Parquet export pipeline.
- **[§2.4 Validating Data Quality](../features/validating-data-quality.md)** — SHACL embedding completeness shapes.
- **[§2.8 APIs and Integration](../features/apis-and-integration.md)** — serve RAG results via the HTTP endpoint.
