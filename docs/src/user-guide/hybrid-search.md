# Hybrid Search (Vector + SPARQL)

pg_ripple v0.27.0 introduces **vector embedding storage and similarity search** integrated directly with the SPARQL query engine. You can store high-dimensional embeddings for RDF entities, search by semantic similarity, and combine vector search with SPARQL graph patterns in a single query.

---

## Overview

The hybrid search feature adds three capabilities:

1. **Embedding storage** — store float vectors alongside entities in `_pg_ripple.embeddings`.
2. **Similarity search** — find the *k* nearest entities using cosine distance (via pgvector).
3. **SPARQL `pg:similar()` extension** — use semantic similarity inside SPARQL `BIND` expressions.

All paths degrade gracefully: when pgvector is absent or the integration is disabled, functions emit a `WARNING` and return zero rows/void rather than raising an ERROR. CI environments without pgvector are fully supported.

---

## Prerequisites

| Requirement | Notes |
|---|---|
| pgvector extension | `CREATE EXTENSION vector;` — optional; without it, embeddings are stored as BYTEA stubs |
| Embedding API | Any OpenAI-compatible `/embeddings` endpoint (OpenAI, Ollama, Azure OpenAI, etc.) |
| pg_ripple ≥ 0.27.0 | Run migration `pg_ripple--0.26.0--0.27.0.sql` on existing installs |

---

## Quick Start

### 1. Install pgvector and configure

```sql
CREATE EXTENSION IF NOT EXISTS vector;

-- Point at your embedding API
ALTER SYSTEM SET pg_ripple.embedding_api_url = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key  = 'sk-...';
ALTER SYSTEM SET pg_ripple.embedding_model    = 'text-embedding-3-small';
ALTER SYSTEM SET pg_ripple.embedding_dimensions = 1536;
SELECT pg_reload_conf();
```

### 2. Load some RDF data

```sql
SELECT pg_ripple.load_ntriples(
    '<https://pharma.example/aspirin>   rdfs:label "aspirin" .
     <https://pharma.example/ibuprofen> rdfs:label "ibuprofen" .
     <https://pharma.example/naproxen>  rdfs:label "naproxen" .'
);
```

### 3. Embed entities in batch

```sql
-- Embeds all entities that have rdfs:label and stores results
SELECT pg_ripple.embed_entities() AS entities_embedded;
```

### 4. Search by similarity

```sql
-- Find the 5 entities most similar to "pain relief"
SELECT entity_iri, score
FROM pg_ripple.similar_entities('pain relief', k := 5);
```

### 5. Hybrid SPARQL

```sql
SELECT *
FROM pg_ripple.sparql(
    'PREFIX pg:  <http://pg-ripple.org/functions/>
     PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     SELECT ?drug ?score WHERE {
       ?drug rdf:type <https://pharma.example/Drug> .
       BIND(pg:similar(?drug, "anti-inflammatory", 10) AS ?score)
       FILTER(?score < 0.4)
     }
     ORDER BY ?score'
);
```

---

## Configuration GUCs

See the [GUC Reference](guc-reference.md) for the full list of `pg_ripple.embedding_*` parameters.

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.pgvector_enabled` | `on` | Master switch — set to `off` to disable all embedding paths |
| `pg_ripple.embedding_api_url` | *(none)* | Base URL for the OpenAI-compatible embeddings API |
| `pg_ripple.embedding_api_key` | *(none)* | Bearer token for the API |
| `pg_ripple.embedding_model` | *(none)* | Model name passed to the API |
| `pg_ripple.embedding_dimensions` | `1536` | Expected output dimensions; must match the model |
| `pg_ripple.embedding_index_type` | *(none)* | Index type: `hnsw` (default when pgvector present) or `ivfflat` |
| `pg_ripple.embedding_precision` | *(none)* | Storage precision: unset = full float4, `half` = halfvec, `binary` = bit |

---

## SQL Functions

See the [Embedding Functions Reference](embedding-functions.md) for full signatures and examples.

| Function | Description |
|---|---|
| `pg_ripple.store_embedding(iri, vector)` | Upsert a single embedding |
| `pg_ripple.similar_entities(query, k, model)` | Return *k* nearest entities |
| `pg_ripple.embed_entities(graph, model, batch_size)` | Batch-embed all entities in a graph |
| `pg_ripple.refresh_embeddings(graph, model, force)` | Re-embed stale or missing entries |

---

## SPARQL Extension: `pg:similar()`

The `pg:similar()` function is usable in `BIND` expressions:

```sparql
PREFIX pg: <http://pg-ripple.org/functions/>

BIND(pg:similar(?entity, "search text", 10) AS ?score)
FILTER(?score < 0.5)
```

**Arguments:**

| Position | Type | Description |
|---|---|---|
| 1 | IRI / variable | Entity whose embedding to score against the query |
| 2 | String literal | Query text |
| 3 | Integer literal | Number of neighbors (*k*) |

**Return value:** cosine distance (0 = identical, 2 = maximally dissimilar), or NULL when pgvector is absent.

---

## Graceful Degradation

All embedding functions check two preconditions before doing any real work:

1. `pg_ripple.pgvector_enabled` must be `on`
2. The pgvector extension must be installed

If either check fails, the function emits a `WARNING` with an error code (`PT603` for missing pgvector, `PT605` for disabled integration) and returns an empty result set or void — it does **not** raise an ERROR.

This means CI pipelines without pgvector will have green regress tests.

---

## Error Codes

| Code | Description |
|---|---|
| PT601 | Embedding API URL not configured |
| PT602 | Embedding dimension mismatch |
| PT603 | pgvector extension not installed |
| PT604 | Embedding API request failed |
| PT605 | Entity has no embedding |
| PT606 | No stale embeddings to refresh |
