# GUC Reference

All pg_ripple configuration parameters are set with `ALTER SYSTEM SET`, `SET` (session-level), or in `postgresql.conf`. Reload with `SELECT pg_reload_conf()` after `ALTER SYSTEM`.

---

## General Parameters

### `pg_ripple.max_path_depth`

| | |
|---|---|
| Type | Integer |
| Default | `10` |
| Range | 1–100 |

Maximum recursion depth for SPARQL property paths (`*`, `+`). Increase for deeply nested graphs; lower for tighter resource bounds.

---

### `pg_ripple.federation_timeout`

| | |
|---|---|
| Type | Integer (milliseconds) |
| Default | `5000` |

Timeout for outbound SPARQL federation requests.

---

### `pg_ripple.export_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |

Number of rows written per batch in Parquet export operations.

---

## Embedding / Vector Parameters (v0.27.0+)

These GUCs control the pgvector integration introduced in v0.27.0. All embedding functions degrade gracefully when pgvector is absent.

---

### `pg_ripple.pgvector_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |

Master switch for all vector embedding paths. Set to `off` to disable embedding storage, similarity search, and SPARQL `pg:similar()` without uninstalling pgvector. Useful for temporarily disabling the feature.

```sql
-- Disable at session level for a bulk load
SET pg_ripple.pgvector_enabled = off;
```

---

### `pg_ripple.embedding_api_url`

| | |
|---|---|
| Type | String |
| Default | *(none)* |

Base URL for the OpenAI-compatible embeddings API. The extension appends `/embeddings` to this URL when making requests.

```sql
ALTER SYSTEM SET pg_ripple.embedding_api_url = 'https://api.openai.com/v1';
-- For Ollama (local):
ALTER SYSTEM SET pg_ripple.embedding_api_url = 'http://localhost:11434/v1';
```

---

### `pg_ripple.embedding_api_key`

| | |
|---|---|
| Type | String |
| Default | *(none)* |

Bearer token sent as `Authorization: Bearer <key>` in embedding API requests. For local models that don't require authentication, set to any non-empty string (e.g., `'local'`).

> **Security:** Avoid storing API keys in `postgresql.conf`. Use `ALTER SYSTEM` and restrict `pg_hba.conf` access, or inject the key via a session-level `SET` in application code.

---

### `pg_ripple.embedding_model`

| | |
|---|---|
| Type | String |
| Default | *(none)* |

Model name passed in the `"model"` field of embedding API requests.

```sql
ALTER SYSTEM SET pg_ripple.embedding_model = 'text-embedding-3-small';
-- or for Ollama:
ALTER SYSTEM SET pg_ripple.embedding_model = 'nomic-embed-text';
```

---

### `pg_ripple.embedding_dimensions`

| | |
|---|---|
| Type | Integer |
| Default | `1536` |
| Range | 1–65535 |

Expected output dimensions from the embedding model. Must match the model's output length. Common values:

| Model | Dimensions |
|---|---|
| `text-embedding-3-small` | 1536 |
| `text-embedding-3-large` | 3072 |
| `text-embedding-ada-002` | 1536 |
| `nomic-embed-text` (Ollama) | 768 |

---

### `pg_ripple.embedding_index_type`

| | |
|---|---|
| Type | String |
| Default | *(none — HNSW when pgvector present)* |
| Values | `hnsw`, `ivfflat` |

Index type for the `_pg_ripple.embeddings` table. HNSW is the default and recommended for most workloads. IVFFlat uses less memory but requires `lists` parameter tuning.

---

### `pg_ripple.embedding_precision`

| | |
|---|---|
| Type | String |
| Default | *(none — full float4 precision)* |
| Values | *(unset)*, `half`, `binary` |

Storage precision for embedding vectors. Reduces disk/memory usage at the cost of accuracy:

| Value | pgvector type | Notes |
|---|---|---|
| *(unset)* | `vector(N)` | Full 32-bit float; highest accuracy |
| `half` | `halfvec(N)` | 16-bit float; ~50% storage reduction |
| `binary` | `bit(N)` | 1-bit quantised; ~97% storage reduction, lower accuracy |

> **Note:** Changing precision after data is stored requires re-running the migration or manually altering the column type and re-embedding.
