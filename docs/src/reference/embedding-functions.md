# Embedding Functions Reference

These functions implement vector embedding storage and similarity search in pg_ripple. All functions require the pgvector extension to perform real work; without it they degrade gracefully (WARNING + empty result).

---

## `pg_ripple.store_embedding`

```sql
pg_ripple.store_embedding(
    entity_iri TEXT,
    embedding  FLOAT8[],
    model      TEXT DEFAULT NULL
) RETURNS VOID
```

Upserts a float vector for the given entity IRI into `_pg_ripple.embeddings`. If the entity already has an embedding it is replaced.

**Parameters:**

| Parameter | Description |
|---|---|
| `entity_iri` | Full IRI of the entity (must exist in the triple store dictionary) |
| `embedding` | Float8 array; length must match `pg_ripple.embedding_dimensions` |
| `model` | Model name label stored alongside the vector; defaults to `pg_ripple.embedding_model` |

**Returns:** `VOID`

**Error codes:** PT602 (dimension mismatch), PT603 (pgvector not installed)

**Example:**

```sql
SELECT pg_ripple.store_embedding(
    'https://pharma.example/aspirin',
    ARRAY[0.12, -0.34, 0.56, ...]::float8[]
);
```

---

## `pg_ripple.similar_entities`

```sql
pg_ripple.similar_entities(
    query_text TEXT,
    k          INT  DEFAULT 10,
    model      TEXT DEFAULT NULL
) RETURNS TABLE(entity_id BIGINT, entity_iri TEXT, score FLOAT8)
```

Embeds `query_text` via the configured embedding API and returns the *k* entities with the smallest cosine distance.

**Parameters:**

| Parameter | Description |
|---|---|
| `query_text` | Free-form text to embed and use as query |
| `k` | Number of nearest neighbors to return (clamped to 1–1000) |
| `model` | Override the model from `pg_ripple.embedding_model` |

**Returns:** Table of `(entity_id, entity_iri, score)` ordered by ascending cosine distance.

**Error codes:** PT601 (API URL not configured), PT603 (pgvector not installed)

**Example:**

```sql
SELECT entity_iri, score
FROM pg_ripple.similar_entities('anti-inflammatory drugs', k := 5)
ORDER BY score;
```

---

## `pg_ripple.embed_entities`

```sql
pg_ripple.embed_entities(
    graph_iri  TEXT DEFAULT '',
    model      TEXT DEFAULT NULL,
    batch_size INT  DEFAULT 100
) RETURNS BIGINT
```

Batch-embeds entities that have an `rdfs:label` (or `skos:prefLabel`) but no embedding yet. Calls the embedding API in batches and stores results via `store_embedding`.

**Parameters:**

| Parameter | Description |
|---|---|
| `graph_iri` | Named graph to scan; empty string = default graph |
| `model` | Embedding model to use (falls back to GUC) |
| `batch_size` | Number of entities to embed per API call (1–500) |

**Returns:** Number of entities successfully embedded.

**Error codes:** PT601 (API URL not configured), PT603 (pgvector not installed)

**Example:**

```sql
-- Embed all entities in the default graph
SELECT pg_ripple.embed_entities() AS embedded_count;

-- Embed entities in a specific named graph
SELECT pg_ripple.embed_entities(
    graph_iri  := 'https://myapp.org/graphs/products',
    batch_size := 200
);
```

---

## `pg_ripple.refresh_embeddings`

```sql
pg_ripple.refresh_embeddings(
    graph_iri TEXT    DEFAULT '',
    model     TEXT    DEFAULT NULL,
    force     BOOLEAN DEFAULT FALSE
) RETURNS BIGINT
```

Re-embeds entities whose embeddings are stale (label changed) or missing. When `force := TRUE`, re-embeds all entities regardless of staleness.

**Parameters:**

| Parameter | Description |
|---|---|
| `graph_iri` | Named graph to scan |
| `model` | Embedding model to use |
| `force` | When TRUE, re-embed everything; when FALSE (default) only re-embed stale entries |

**Returns:** Number of entities re-embedded.

**Error codes:** PT601 (API URL not configured), PT603 (pgvector not installed), PT606 (no stale embeddings found when not force)

**Example:**

```sql
-- Refresh only stale embeddings
SELECT pg_ripple.refresh_embeddings();

-- Force full re-embedding
SELECT pg_ripple.refresh_embeddings(force := TRUE);
```

---

## Internal Tables

### `_pg_ripple.embeddings`

Stores entity embeddings. When pgvector is installed:

| Column | Type | Description |
|---|---|---|
| `entity_id` | `BIGINT` | Dictionary integer ID for the entity IRI |
| `embedding` | `vector(N)` | Float vector, dimension = `pg_ripple.embedding_dimensions` |
| `model` | `TEXT` | Model used to generate this embedding |
| `updated_at` | `TIMESTAMPTZ` | When this embedding was last stored |

A HNSW index on `embedding` enables approximate nearest-neighbour search.

When pgvector is **absent**, the `embedding` column is `BYTEA` and all similarity functions return empty results.

---

## SPARQL Integration

The `pg:similar()` function is callable from SPARQL `BIND` expressions. See [Hybrid Search](../user-guide/hybrid-search.md) for usage.

**Function IRI:** `http://pg-ripple.org/functions/similar`

**Signature (SPARQL):**

```sparql
pg:similar(?entity_variable, "query text", k_integer)
```

Returns a numeric cosine distance, or SPARQL unbound (NULL) when pgvector is absent.
