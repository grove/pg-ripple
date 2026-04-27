# Vector Embeddings and Hybrid Search

Vector search and graph queries answer different questions. Vector search is good at *"things that look like X"*; graph queries are good at *"things in this exact relationship to X"*. Real-world questions usually need both — *"prescriptions semantically similar to ibuprofen, taken by patients in the cardiology cohort"*. pg_ripple does both in one query.

This chapter is the practical reference. It covers:

- Storing vector embeddings alongside RDF entities.
- Building HNSW indexes with `pgvector`.
- Running pure similarity, pure SPARQL, and hybrid (RRF) search.
- The `pg:similar()` SPARQL function — vector search inside a SPARQL pattern.
- Federating to external vector stores (Weaviate, Qdrant, Pinecone, remote pgvector).

For higher-level decision-making, start with [AI Overview](ai-overview.md).
For an end-to-end RAG pipeline, see [RAG Pipeline](../user-guide/rag-pipeline.md).

---

## Setup

```sql
-- 1. pgvector is required.
CREATE EXTENSION IF NOT EXISTS vector;

-- 2. Point pg_ripple at an OpenAI-compatible embedding API.
ALTER SYSTEM SET pg_ripple.embedding_api_url      = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key_env  = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.embedding_model        = 'text-embedding-3-small';
ALTER SYSTEM SET pg_ripple.embedding_dimensions   = 1536;
SELECT pg_reload_conf();
```

API keys are read from the named environment variable at call time and **never** stored in the database.

---

## Embedding entities

Every entity that has an `rdfs:label` (or `skos:prefLabel`, or any of the configured label predicates) can be embedded:

```sql
-- Bulk-embed every labelled entity.
SELECT pg_ripple.embed_entities();

-- Or embed only one named graph.
SELECT pg_ripple.embed_entities('https://example.org/products');

-- Manually store a precomputed vector.
SELECT pg_ripple.store_embedding(
    iri    := '<https://example.org/aspirin>',
    vec    := '[...1536 floats...]'::vector,
    model  := 'text-embedding-3-small'
);
```

Set `pg_ripple.use_graph_context = on` to enrich each entity's embedding input with its 1-hop graph neighbourhood — this dramatically improves recall on entities whose labels alone are ambiguous (*"Apple"* the company vs the fruit).

Set `pg_ripple.auto_embed = on` to enqueue any newly-inserted labelled entity for background embedding. The merge-worker drains the queue.

---

## The three search modes

### 1. Pure similarity

```sql
SELECT entity_iri, similarity
FROM pg_ripple.similar_entities('headache medication', k := 10);
```

Equivalent to the cosine-distance HNSW lookup you would write by hand against `_pg_ripple.embeddings`, but with text-to-vector handled for you.

### 2. Pure SPARQL

```sql
SELECT * FROM pg_ripple.sparql($$
    SELECT ?drug WHERE {
        ?drug a <https://example.org/Drug> ;
              <https://example.org/treats> <https://example.org/headache> .
    }
$$);
```

### 3. Hybrid (Reciprocal Rank Fusion)

Hybrid search returns the *fused* ranking of (a) the SPARQL query's top-k matches and (b) the vector query's top-k matches, using Reciprocal Rank Fusion. This catches both the exact relational matches *and* the fuzzy semantic neighbours.

```sql
SELECT entity_iri, score
FROM pg_ripple.hybrid_search(
    sparql := 'SELECT ?d WHERE { ?d a <https://example.org/Drug> }',
    text   := 'headache medication',
    k      := 10,
    alpha  := 0.5  -- 0.0 = pure SPARQL, 1.0 = pure vector
);
```

`alpha` is the relative weight given to the vector ranking. Tune by inspection; 0.5 is a sensible default.

---

## `pg:similar()` — vector search **inside** SPARQL

The `pg:similar()` SPARQL function returns the cosine similarity between an entity and a free-text query. It is callable in `BIND`, `FILTER`, and `ORDER BY`.

```sparql
PREFIX pg: <http://pg-ripple.io/fn/>

SELECT ?drug ?score WHERE {
    ?drug a <https://example.org/Drug> .
    BIND(pg:similar(?drug, "anti-inflammatory") AS ?score)
    FILTER(?score > 0.7)
}
ORDER BY DESC(?score)
LIMIT 20
```

This is the most expressive form — graph constraints and similarity score live in the same query plan, with no client-side post-processing.

---

## Vector federation — Weaviate, Qdrant, Pinecone, remote pgvector

If you already operate a vector store and do not want to migrate, register it as a **vector federation endpoint**. `hybrid_search()` will blend results from local pg_ripple *and* the remote service.

```sql
SELECT pg_ripple.register_vector_endpoint(
    url      := 'https://qdrant.internal:6333',
    api_type := 'qdrant'
);

SELECT pg_ripple.register_vector_endpoint('https://weaviate.internal:8080', 'weaviate');
SELECT pg_ripple.register_vector_endpoint('https://my-index.pinecone.io', 'pinecone');
```

Supported `api_type` values: `pgvector`, `weaviate`, `qdrant`, `pinecone`. Endpoints are stored in `_pg_ripple.vector_endpoints` and `register_vector_endpoint()` is idempotent.

A federated `hybrid_search()` call automatically fans out, gathers per-endpoint top-k results, and re-fuses them.

---

## GUCs at a glance

| GUC | Default | Purpose |
|---|---|---|
| `pg_ripple.pgvector_enabled` | `on` | Master switch. When off, vector functions emit a WARNING and return zero rows. |
| `pg_ripple.embedding_api_url` | empty | OpenAI-compatible endpoint base URL |
| `pg_ripple.embedding_api_key_env` | `PG_RIPPLE_EMBEDDING_API_KEY` | Env var name for the API key |
| `pg_ripple.embedding_model` | `text-embedding-3-small` | Default model |
| `pg_ripple.embedding_dimensions` | `1536` | Vector size |
| `pg_ripple.embedding_batch_size` | `100` | API batch size for `embed_entities()` |
| `pg_ripple.use_graph_context` | `off` | Include 1-hop neighbours in embedding input |
| `pg_ripple.auto_embed` | `off` | Background-embed newly-inserted entities |

---

## Tuning HNSW

The HNSW index built by pg_ripple is a standard pgvector index on `_pg_ripple.embeddings(vector vector_cosine_ops)`. Tune it as you would any pgvector index:

| Setting | Default | Effect |
|---|---|---|
| `m` (build) | 16 | Higher = better recall, more memory |
| `ef_construction` (build) | 64 | Higher = better recall, slower build |
| `hnsw.ef_search` (query) | 40 | Higher = better recall, slower queries |

See [Vector Index Trade-offs](../reference/vector-index-tradeoffs.md) for measured benchmarks across these knobs.

---

## Graceful degradation

Every vector function in pg_ripple checks for pgvector at call time. If pgvector is missing, or `pgvector_enabled = off`, the function emits one `WARNING` and returns zero rows. Your application code can call vector functions in environments (CI, dev) that do not have pgvector without crashing.

---

## See also

- [AI Overview](ai-overview.md)
- [RAG Pipeline](../user-guide/rag-pipeline.md)
- [Knowledge-Graph Embeddings](knowledge-graph-embeddings.md) — graph-structural embeddings, complementary to text embeddings.
- [Vector Index Trade-offs](../reference/vector-index-tradeoffs.md)
- [Embedding function reference](../reference/embedding-functions.md)
