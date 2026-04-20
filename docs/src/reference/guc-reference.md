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

### `pg_ripple.property_path_max_depth` *(deprecated)*

| | |
|---|---|
| Type | Integer |
| Default | `64` |
| Range | 1–100 000 |
| Status | **Deprecated** since v0.38.0 — use `max_path_depth` instead |

Legacy alias for `max_path_depth`. Setting this GUC still works but emits a
deprecation notice. It will be removed in a future major release.

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

---

## v0.37.0: Tombstone GC & Error Safety

### `pg_ripple.tombstone_gc_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `sighup` (shared: requires server signal, not per-session) |

When `on`, pg_ripple automatically issues `VACUUM ANALYZE` on a predicate's tombstone table after each merge cycle if the residual tombstone count exceeds `tombstone_gc_threshold × main_row_count`. Set to `off` to disable automatic tombstone cleanup (useful when managing VACUUM manually).

### `pg_ripple.tombstone_gc_threshold`

| | |
|---|---|
| Type | String (decimal) |
| Default | `0.05` (5%) |
| Range | `0.0` – `1.0` |
| Context | `sighup` |

Tombstone-to-main-row ratio that triggers automatic `VACUUM` after a merge cycle. When the remaining tombstone count divided by the new main table row count exceeds this value, a `VACUUM ANALYZE` is scheduled on the tombstone table.

Lower values (e.g. `0.01`) trigger VACUUM more aggressively; higher values (e.g. `0.20`) allow more tombstone bloat before cleanup.

---

## v0.37.0: GUC Validator Rules

The following string-enum GUCs now reject invalid values at `SET` time with an error. Previously, invalid values were silently ignored until the execution path checked them.

| GUC | Valid values |
|---|---|
| `pg_ripple.inference_mode` | `off`, `on_demand`, `materialized` |
| `pg_ripple.enforce_constraints` | `off`, `warn`, `error` |
| `pg_ripple.rule_graph_scope` | `default`, `all` |
| `pg_ripple.shacl_mode` | `off`, `sync`, `async` |
| `pg_ripple.describe_strategy` | `cbd`, `scbd`, `simple` |

**`pg_ripple.rls_bypass` scope change (v0.37.0)**: This GUC is now registered at `PGC_POSTMASTER` scope when pg_ripple is loaded via `shared_preload_libraries`. This prevents a session from bypassing graph-level RLS with `SET LOCAL pg_ripple.rls_bypass = on`.

---

## v0.42.0: Parallel Merge Workers

### `pg_ripple.merge_workers`

| | |
|---|---|
| Type | Integer |
| Default | `1` |
| Range | `1` – `16` |
| Context | `postmaster` (startup-only; set in `postgresql.conf`) |

Number of background merge worker processes. Each worker owns a disjoint round-robin slice of VP predicates. Workers use `pg_advisory_lock` to prevent conflicts; idle workers steal work from overloaded peers. Increasing this value helps workloads with many distinct predicates (> 50).

---

## v0.42.0: Cost-Based Federation Planner

### `pg_ripple.federation_planner_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |

When `on`, pg_ripple uses VoID statistics collected from remote SPARQL endpoints to sort the SERVICE execution order by ascending estimated cost. When `off`, SERVICE clauses are executed in document order.

### `pg_ripple.federation_stats_ttl_secs`

| | |
|---|---|
| Type | Integer |
| Default | `3600` (1 hour) |
| Range | `0` – `86400` |
| Context | `userset` |

Seconds until cached VoID statistics for a remote endpoint are considered stale. Setting `0` disables caching (re-fetches on every query).

### `pg_ripple.federation_parallel_max`

| | |
|---|---|
| Type | Integer |
| Default | `4` |
| Range | `1` – `64` |
| Context | `userset` |

Maximum number of remote SERVICE clauses that pg_ripple will execute concurrently within a single query. Set to `1` to disable parallel SERVICE execution.

### `pg_ripple.federation_parallel_timeout`

| | |
|---|---|
| Type | Integer |
| Default | `60` (seconds) |
| Range | `1` – `3600` |
| Context | `userset` |

Per-endpoint timeout when executing parallel SERVICE clauses. Endpoints that do not respond within this limit return an empty result set (with a WARNING). Does not affect sequential SERVICE execution.

### `pg_ripple.federation_inline_max_rows`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Range | `1` – `1000000` |
| Context | `userset` |

Maximum number of rows in the VALUES binding table passed to a remote SERVICE clause. When the result set from the local graph exceeds this limit, pg_ripple automatically spools the bindings into a temporary table (PT620 INFO logged) and issues multiple smaller requests to the remote endpoint in batches. Set to a lower value if remote endpoints enforce query complexity limits.

### `pg_ripple.federation_allow_private`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `superuser` |

> **Security-critical GUC** — only superusers can set this.

When `off` (the default), `register_endpoint()` rejects endpoints whose hostname resolves to a loopback address (`127.0.0.0/8`), a link-local address (`169.254.0.0/16`), any RFC-1918 private range (`10/8`, `172.16/12`, `192.168/16`), or an IPv6 equivalent. This prevents server-side request forgery (SSRF) via malicious SPARQL SERVICE calls.

Set to `on` only in controlled environments where the remote endpoint is a trusted internal service (e.g., a local Fuseki instance in a Docker network).

---

## v0.42.0: owl:sameAs Safety

### `pg_ripple.sameas_max_cluster_size`

| | |
|---|---|
| Type | Integer |
| Default | `100000` |
| Range | `0` – `2147483647` |
| Context | `userset` |

Maximum number of entities in a single `owl:sameAs` equivalence cluster before canonicalization is skipped with a PT550 WARNING. A single cluster larger than this limit is usually a data quality problem (e.g., a mistakenly asserted `owl:sameAs owl:Thing`). Set to `0` to disable the check (no limit).
