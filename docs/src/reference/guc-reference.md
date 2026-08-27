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

Number of rows fetched per page when a SPARQL cursor streams results back to the
caller. This controls Rust-side peak memory: at most `export_batch_size` result
rows are decoded from dictionary IDs and held in memory simultaneously before
being forwarded. Increase for higher throughput at the cost of more memory;
decrease for tighter memory budgets.

See also: `arrow_batch_size` (Arrow IPC batching) operates independently —
the two GUCs govern different output paths and can be tuned separately.

---

### `pg_ripple.arrow_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Minimum | `1` |

Number of rows packed into each Arrow IPC `RecordBatch` during Arrow Flight
bulk export. A larger value produces fewer IPC frames and lower per-frame
overhead; a smaller value allows consumers to begin processing results sooner.

**Interaction with `export_batch_size`**: `arrow_batch_size` controls the IPC
batch granularity inside the Arrow Flight response body. `export_batch_size`
controls how many SPARQL cursor rows are fetched per page from PostgreSQL. The
two parameters operate on independent code paths and can be tuned separately.

```sql
-- Tune for high-bandwidth Arrow bulk export
SET pg_ripple.arrow_batch_size = 5000;
```

---

### `pg_ripple.vp_promotion_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Minimum | `100` |

Batch size for the COPY-phase of VP table promotion: when a rare-predicate
triple count exceeds `vp_promotion_threshold`, the promotion worker copies rows
from `_pg_ripple.vp_rare` into the new dedicated VP table in batches of this
size. Larger batches reduce promotion time at the cost of a larger WAL record
per batch; smaller batches reduce WAL pressure and allow other transactions to
proceed between batches.

**Interaction with the other batch-size GUCs**: VP promotion runs in the
background worker and is unrelated to SPARQL cursor streaming or Arrow IPC
export. Changing this GUC affects only the promotion throughput, not query
performance.

```sql
-- Reduce WAL pressure during a large promotion
SET pg_ripple.vp_promotion_batch_size = 1000;
```

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

---

## v0.46.0: TopN Push-down & Datalog Sequence Batch

### `pg_ripple.topn_pushdown`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |

When `on` (default), SPARQL SELECT queries that contain both `ORDER BY` and `LIMIT N` (with no `OFFSET > 0` and no `DISTINCT`) emit the SQL as `… ORDER BY … LIMIT N` rather than fetching all rows and discarding after decoding.

Set to `off` to disable the optimisation globally — for example, during debugging when you suspect that TopN push-down is producing incorrect results.

The `sparql_explain()` output includes a `"topn_applied": true/false` key that indicates whether push-down was applied to a specific query.

### `pg_ripple.datalog_sequence_batch`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Range | `100` – `1000000` |
| Context | `userset` |

SID (statement-ID) range reserved per parallel Datalog worker per batch. Before launching N parallel strata workers, the coordinator atomically advances the global `_pg_ripple.statement_id_seq` sequence by `N * datalog_sequence_batch`, then assigns each worker an exclusive sub-range. Workers insert triples with pre-computed SIDs without touching the shared sequence, eliminating contention.

Increase this value if parallel inference workers frequently conflict on the sequence. Decrease it to reduce unused SID gaps when inference produces fewer triples than expected per batch.



---

## v0.48.0 GUCs

### `pg_ripple.federation_max_response_bytes`

| | |
|---|---|
| Type | Integer |
| Default | `104857600` (100 MiB) |
| Range | `-1` (disabled) – `2147483647` |
| Context | `userset` |

Maximum allowed size in bytes for a federation (SERVICE) response body. When a
remote SPARQL endpoint returns a JSON response larger than this value, pg_ripple
raises error code **PT543** and aborts the query. Set to `-1` to disable the
limit (not recommended for deployments with untrusted federation endpoints).

```sql
-- Allow up to 500 MiB responses for a single query
SET pg_ripple.federation_max_response_bytes = 524288000;

-- Disable the limit (trusted internal network only)
SET pg_ripple.federation_max_response_bytes = -1;
```

---

## v0.47.0: Validated String GUCs

All six string-valued GUCs below now reject invalid values at SET time
(previously invalid values were accepted and silently ignored at runtime).

### `pg_ripple.federation_on_error`

| | |
|---|---|
| Type | String |
| Default | `warning` |
| Valid values | `warning`, `error`, `empty` |
| Context | `userset` |

Controls behaviour when a SERVICE call fails completely.  `warning` emits a
PT610 WARNING and returns an empty binding set for that endpoint.  `error`
raises an ERROR and aborts the query.  `empty` silently returns zero rows for
that endpoint.

### `pg_ripple.federation_on_partial`

| | |
|---|---|
| Type | String |
| Default | `empty` |
| Valid values | `empty`, `use` |
| Context | `userset` |

Controls behaviour when a SERVICE response stream is interrupted mid-transfer
(e.g., the remote endpoint drops the connection).  `empty` discards partial
results and returns zero rows.  `use` keeps the rows received before the error.

### `pg_ripple.sparql_overflow_action`

| | |
|---|---|
| Type | String |
| Default | `warn` |
| Valid values | `warn`, `error` |
| Context | `userset` |

Action taken when a SPARQL SELECT result set exceeds `sparql_max_rowAction taken when a SPARQL> 0`).  `warn` truncates the result set and emits a PT601
WARNING.  `error` raises an ERROR.

### `pg_ripple.tracing_exporter`

| | |
|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---t`, `otlp|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---ut` writ|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|---|--|-erhead).  `otlp` sends spans
via the OTLP gRPC protocol to the endpoint specivia tby the
`OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.

### `pg_ripple.embedding_index_type`

| | |
|---|---|
| Type | String |
| Default | `hnsw` |
| Valid values | `h| Valid values | `h| Valid values | `h| Valid values | `h| Valid val_pg_ripp| Valid values | `h| Valid values | `h| Valid values | `h| Valid values | rld index; `ivfflat` builds an IVFFlat index.
ChanginC this setCing after embeddings have been indexedChanginC this setCi`REINDEX TABLE _pg_ripple.embeddings`.

### `pg_ripple.embedding_precision`

| | |
|---|---|
| Type | String |
| Default | `single` |
| Valid values | `single`, `half`, `binary` |
| Context | `userset` |

Storage precision for emStorage precision forngle` uses pgvectorStorage precision for emStorage precision forngle` uses pgvectorStorage precision for emStorage precision forngle` uses pgvectorStorage precision for emStorage precision forngle` uses pgvectorStorage precision for emStorage precision forngle` uses pg`binary`.

---

## AI & LLM Integration Parameters (v0.49.0)

### `pg_ripple.llm_endpoint`

| | |
|---|---|
| Type | String |
| Default | `''` (empty — disabled) |
| Context | `userset` |

Base URL for an OpenAI-compatible `/v1/chat/completions` API used by `sparql_from_nl()`. When empty, calling `sparql_from_nl()` immediately raises PT700. Set to `'mock'` to use the built-in test mock without a real LLM. Examples: `https://api.openai.com/v1`, `http://localhost:11434/v1` (Ollama).

> **Note (H16-04, v0.112.0):** This GUC is a no-op within the pg_ripple extension itself. The
> HTTP companion `pg_ripple_http` reads this GUC (via the database connection) when serving the
> `/rules/{id}/explain` endpoint to handle LLM enrichment for Datalog rule explanations.
> Direct extension functions such as `sparql_from_nl()` also respect this GUC. Do not set this
> GUC to a raw API key — use `pg_ripple.llm_api_key_env` for secure key management.

---

### `pg_ripple.llm_model`

| | |
|---|---|
| Type | String |
| Default | `gpt-4o` |
| Context | `userset` |

LLM model identifier passed in the `model` field of the chat completion request body. Supported values depend on the endpoint — e.g. `gpt-4o`, `gpt-4-turbo`, `llama3`, `mistral`.

---

### `pg_ripple.llm_api_key_env`

| | |
|---|---|
| Type | String |
| Default | `PG_RIPPLE_LLM_API_KEY` |
| Context | `userset` |

Name of the environment variable from which `sparql_from_nl()` reads the Bearer API key at call time. Values must match `^[A-Z_][A-Z0-9_]*$`; raw values are rejected. For pre-0.131 migrations only, a superuser can set `pg_ripple.llm_api_key_env_allow_raw = on`. The key is never stored in the database or visible in `pg_settings`.

### `pg_ripple.llm_api_key_env_allow_raw`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `suset` |

Temporary superuser-only escape hatch for migrating raw API-key values from pre-0.131 installations. Keep it `off` after migration.

---

### `pg_ripple.llm_include_shapes`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |

When `on`, the LLM prompt sent by `sparql_from_nl()` includes a summary of active SHACL shapes as additional schema context. Disable when shapes are very large or the LLM context window is limited.




---

## v0.54.0 GUCs — High Availability & Logical Replication

### `pg_ripple.replication_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |

When `on`, starts the `logical_apply_worker` background worker that subscribes
to the `pg_ripple_pub` publication and applies incoming N-Triples batches to
the local store.  Requires `wal_level = logical` on the primary.

Requires a server restart (or SIGHUP) to take effect.

---

### `pg_ripple.replication_conflict_strategy`

| | |
|---|---|
| Type | String |
| Default | `last_writer_wins` |
| Context | `sighup` |

Conflict resolution strategy used by the logical apply worker when an incoming
triple's `(s, p, g)` already exists in the local store with a different object
or SID.

Supported values:

| Value | Behaviour |
|-------|-----------|
| `last_writer_wins` | Keep the row with the highest Statement ID (SID). This is the default and matches eventual-consistency semantics. |

---

### `pg_ripple.strict_dictionary` (D13-02, v0.86.0)

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |

When `on`, all dictionary lookups fail with a `PT400` error rather than silently inserting
new dictionary entries for unknown IRIs and literals. This is useful for read-only replicas
and validation pipelines where unknown terms indicate a data-quality problem.

When `off` (default), unknown terms are inserted into the dictionary on first access (lazy
encoding). This is appropriate for most write workloads.

---

### `pg_ripple.plan_cache_capacity` (D13-02, v0.86.0)

| | |
|---|---|
| Type | Integer |
| Default | `1024` |
| Min / Max | `1` / `32768` |
| Context | `sighup` |

Maximum number of compiled SPARQL query plans held in the in-process LRU plan cache. Each
entry stores the SQL string, projected variable list, and decoded type metadata. Cache hit
rate is visible via `pg_ripple.explain_sparql(query, 'plan_cache_stats')` or through the
`pg_ripple_plan_cache_hit_ratio` Prometheus metric.

---

### `pg_ripple.cdc_slot_cleanup_timeout_ms` (D13-02, v0.86.0)

| | |
|---|---|
| Type | Integer |
| Default | `5000` |
| Min / Max | `100` / `300000` |
| Context | `sighup` |

Timeout in milliseconds for CDC replication slot cleanup during extension uninstall or when
the `pg_ripple.cleanup_cdc_slot()` function is called. If the slot is active (a subscriber
is connected), the cleanup will wait up to this many milliseconds before returning an error.
The crash-recovery test for this scenario is in `tests/crash_recovery/cdc_slot_cleanup_during_kill.sh`.

---

## Datalog Reasoning Parameters

### `pg_ripple.inference_mode`

| | |
|---|---|
| Type | Enum string |
| Default | `'on_demand'` |
| Valid values | `off`, `on_demand`, `materialized` |
| Context | `userset` |

Controls when Datalog rules are evaluated.

| Value | Behaviour |
|---|---|
| `off` | No inference is performed. `infer()` is a no-op. |
| `on_demand` | Inference runs explicitly when `infer()` is called. |
| `materialized` | Inference results are kept materialized and incrementally updated. |

---

### `pg_ripple.enforce_constraints`

| | |
|---|---|
| Type | Enum string |
| Default | `'warn'` |
| Valid values | `off`, `warn`, `error` |
| Context | `userset` |

Controls how Datalog constraint rules (`:- body.`) are enforced.

| Value | Behaviour |
|---|---|
| `off` | Constraint violations are ignored. |
| `warn` | Violations produce a PostgreSQL WARNING. |
| `error` | Violations raise PT404 and abort the transaction. |

---

### `pg_ripple.rule_graph_scope`

| | |
|---|---|
| Type | Enum string |
| Default | `'all'` |
| Valid values | `default`, `all` |
| Context | `userset` |

Controls which graphs are searched when a Datalog rule body atom has no explicit graph annotation.

| Value | Behaviour |
|---|---|
| `default` | Unscoped atoms match only the default graph (g = 0). |
| `all` | Unscoped atoms match triples in any named graph. |

---

### `pg_ripple.magic_sets`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.29.0 |

Master switch for magic-set transformation — goal-directed inference that rewrites rules to avoid deriving irrelevant facts. Enables `infer_goal()` to perform targeted, demand-driven evaluation. Disable to debug incorrect magic-set rewrites.

---

### `pg_ripple.datalog_cost_reorder`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.29.0 |

When `on`, sorts Datalog rule body atoms by ascending estimated VP-table cardinality before SQL compilation. This places the most selective join first, reducing intermediate result sizes.

---

### `pg_ripple.datalog_antijoin_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Context | `userset` |
| Since | v0.29.0 |

Minimum VP-table row count for negated body atoms (NOT EXISTS) to use an anti-join. Below this threshold, a correlated subquery is used instead.

---

### `pg_ripple.delta_index_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `500` |
| Context | `userset` |
| Since | v0.29.0 |

Minimum semi-naive delta temp-table row count before creating a B-tree index. For small delta sets, a sequential scan is faster.

---

### `pg_ripple.rule_plan_cache`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.30.0 |

Master switch for the Datalog rule plan cache. When `on`, compiled SQL for each rule set is cached in memory and reused across `infer()` calls, avoiding repeated compilation.

---

### `pg_ripple.rule_plan_cache_size`

| | |
|---|---|
| Type | Integer |
| Default | `64` |
| Context | `userset` |
| Since | v0.30.0 |

Maximum number of rule sets whose compiled SQL is kept in the plan cache.

---

### `pg_ripple.sameas_reasoning`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.31.0 |

Master switch for `owl:sameAs` entity canonicalization. When `on`, entities linked by `owl:sameAs` are merged to their canonical representative before inference and query evaluation.

---

### `pg_ripple.demand_transform`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.31.0 |

Master switch for demand transformation — a technique that rewrites rules to propagate bindings from the query goal into the rule evaluation, reducing the search space.

---

### `pg_ripple.wfs_max_iterations`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `userset` |
| Since | v0.32.0 |

Safety cap on alternating fixpoint rounds for well-founded semantics (`infer_wfs()`). When the cap is reached, a PT520 WARNING is emitted and the partial result is returned.

---

### `pg_ripple.tabling`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.32.0 |

Master switch for the subsumptive tabling cache. Tabling memoizes intermediate query results, eliminating redundant re-evaluation of identical sub-goals in recursive SPARQL and Datalog queries.

---

### `pg_ripple.tabling_ttl`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `300` |
| Context | `userset` |
| Since | v0.32.0 |

Time-to-live in seconds for tabling cache entries. Entries are evicted after this interval. Set to `0` to keep entries indefinitely (until the session ends).

---

### `pg_ripple.datalog_max_depth`

| | |
|---|---|
| Type | Integer |
| Default | `0` (unlimited) |
| Context | `userset` |
| Since | v0.34.0 |

Maximum depth for bounded-depth Datalog fixpoint termination. When `0`, the fixpoint runs until convergence. For recursive rules on large graphs, setting a bound (e.g., `10`) limits computation time.

---

### `pg_ripple.dred_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.34.0 |

Master switch for the Delete-Rederive (DRed) algorithm. When `on`, deleting a base triple re-derives only the minimal set of affected inferred triples rather than recomputing everything. Set to `off` to always use full recompute.

---

### `pg_ripple.dred_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Context | `userset` |
| Since | v0.34.0 |

Maximum number of deleted base triples processed per DRed transaction.

---

### `pg_ripple.datalog_parallel_workers`

| | |
|---|---|
| Type | Integer |
| Default | `4` |
| Range | `1`–`64` |
| Context | `userset` |
| Since | v0.35.0 |

Maximum number of parallel background workers for Datalog stratum evaluation. Independent strata are distributed across workers for concurrent evaluation.

---

### `pg_ripple.datalog_parallel_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Context | `userset` |
| Since | v0.35.0 |

Minimum estimated total row count across all rules in a stratum before parallel group analysis is applied. Small strata are evaluated serially.

---

### `pg_ripple.lattice_max_iterations`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Context | `userset` |
| Since | v0.36.0 |

Maximum fixpoint iterations for lattice-based Datalog inference (`infer_lattice()`). See PT540 for convergence failures.

---

### `pg_ripple.datalog_max_derived`

| | |
|---|---|
| Type | Integer |
| Default | `0` (unlimited) |
| Context | `userset` |
| Since | v0.40.0 |

Maximum derived facts produced by a single `infer()` call. When the limit is hit, inference stops and returns the partial result. Useful as a safety guard in development.

---

### `pg_ripple.owl_profile`

| | |
|---|---|
| Type | Enum string |
| Default | `'RL'` |
| Valid values | `RL`, `EL`, `QL`, `off` |
| Context | `userset` |
| Since | v0.57.0 |

Active OWL 2 reasoning profile. Selects the built-in OWL rule set loaded by `load_rules_builtin('owl')`.

| Value | Profile |
|---|---|
| `RL` | OWL 2 RL — full rule set, polynomial in the size of the data |
| `EL` | OWL 2 EL — optimized for large class hierarchies (TBox-heavy ontologies) |
| `QL` | OWL 2 QL — query rewriting mode, minimal materialization |
| `off` | No built-in OWL rules loaded |

---

### `pg_ripple.probabilistic_datalog`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.57.0 |

Enable experimental probabilistic Datalog with rule confidence weights (`@weight` annotations). See [Probabilistic Reasoning](../features/uncertain-knowledge.md).

---

### `pg_ripple.datalog_citus_dispatch`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.62.0 |

When `on`, wraps Datalog stratum-iteration `INSERT…SELECT` statements in `run_command_on_all_nodes()` for distributed execution across Citus workers. Requires `pg_ripple.citus_sharding_enabled = on`.

---

### `pg_ripple.prob_datalog_cyclic`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.87.0 |

Allow probabilistic evaluation on cyclic rule sets. Cyclic probabilistic programs require approximate fixed-point evaluation; must be explicitly enabled.

---

### `pg_ripple.prob_datalog_max_iterations`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `userset` |
| Since | v0.87.0 |

Maximum semi-naive inference rounds when `prob_datalog_cyclic = on`. After this limit, a WARNING is emitted and the partial result returned.

---

### `pg_ripple.prob_datalog_convergence_delta`

| | |
|---|---|
| Type | Float |
| Default | `0.001` |
| Context | `userset` |
| Since | v0.87.0 |

Early-exit threshold for cyclic probabilistic Datalog. Iteration stops when the maximum confidence delta across all atoms falls below this value.

---

## SPARQL Query Engine Parameters

### `pg_ripple.bgp_reorder`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.13.0 |

Enable BGP (Basic Graph Pattern) join reordering based on `pg_stats` selectivity estimates. The most selective triple patterns are placed first in the join order.

---

### `pg_ripple.parallel_query_min_joins`

| | |
|---|---|
| Type | Integer |
| Default | `3` |
| Context | `userset` |
| Since | v0.13.0 |

Minimum number of VP-table joins in a SPARQL query before PostgreSQL parallel query workers are enabled for the generated SQL.

---

### `pg_ripple.sparql_strict`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.21.0 |

When `on`, raises `ERRCODE_FEATURE_NOT_SUPPORTED` for unsupported SPARQL built-in functions. When `off`, unsupported functions silently evaluate to UNDEF.

---

### `pg_ripple.wcoj_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.36.0 |

Master switch for Worst-Case Optimal Join (WCOJ / Leapfrog Triejoin) optimization. When `on`, cyclic join patterns (e.g., triangles, cliques) that exceed `wcoj_min_tables` are evaluated using the WCOJ executor rather than PostgreSQL's hash-join path.

---

### `pg_ripple.wcoj_min_tables`

| | |
|---|---|
| Type | Integer |
| Default | `3` |
| Context | `userset` |
| Since | v0.36.0 |

Minimum number of VP-table joins before WCOJ analysis is applied.

---

### `pg_ripple.wcoj_min_cardinality`

| | |
|---|---|
| Type | Integer |
| Default | `0` |
| Context | `userset` |
| Since | v0.79.0 |

Minimum VP-table cardinality before the WCOJ executor is used. Below this threshold, the standard SQL hash-join path is used instead.

---

### `pg_ripple.sparql_max_rows`

| | |
|---|---|
| Type | Integer |
| Default | `0` (unlimited) |
| Context | `userset` |
| Since | v0.40.0 |

Maximum rows returned by a SPARQL SELECT or CONSTRUCT query. When exceeded, behaviour is controlled by `sparql_overflow_action`. Set to `0` for no limit.

---

### `pg_ripple.sparql_overflow_action`

| | |
|---|---|
| Type | Enum string |
| Default | `'truncate'` |
| Valid values | `truncate`, `error` |
| Context | `userset` |
| Since | v0.40.0 |

Action taken when `sparql_max_rows` is exceeded. `truncate` returns the first N rows with a PT640 notice; `error` raises PT640 as an error.

---

### `pg_ripple.sparql_max_algebra_depth`

| | |
|---|---|
| Type | Integer |
| Default | `256` |
| Context | `userset` |
| Since | v0.51.0 |

Maximum allowed algebra tree depth for SPARQL queries. Queries exceeding this limit raise PT440.

---

### `pg_ripple.sparql_max_triple_patterns`

| | |
|---|---|
| Type | Integer |
| Default | `4096` |
| Context | `userset` |
| Since | v0.51.0 |

Maximum number of triple patterns in a single SPARQL query. Queries exceeding this limit raise PT440.

---

### `pg_ripple.describe_strategy`

| | |
|---|---|
| Type | Enum string |
| Default | `'cbd'` |
| Valid values | `cbd`, `scbd`, `simple` |
| Context | `userset` |

Algorithm for SPARQL DESCRIBE queries.

| Value | Description |
|---|---|
| `cbd` | Concise Bounded Description — all triples with the resource as subject, plus blank-node closures |
| `scbd` | Symmetric CBD — adds triples where the resource is the object |
| `simple` | Return only direct subject triples, no blank-node closure |

---

### `pg_ripple.strict_sparql_filters`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.81.0 |

When `on`, an unknown built-in function name in a FILTER expression raises ERROR (PT422) rather than evaluating to UNDEF.

---

### `pg_ripple.fuzzy_max_input_length`

| | |
|---|---|
| Type | Integer |
| Default | `4096` |
| Range | `1`–`65536` |
| Context | `userset` |
| Since | v0.89.0 |

Maximum input string length for `pg:fuzzy_match()` and `pg:token_set_ratio()`. Arguments longer than this limit raise PT0308. Prevents algorithmic complexity attacks.

---

### `pg_ripple.all_nodes_predicate_limit`

| | |
|---|---|
| Type | Integer |
| Default | `500` |
| Context | `userset` |
| Since | v0.82.0 |

Maximum number of predicates used in a wildcard property-path expansion (`*` or `+` with no explicit predicate). When the schema has more predicates, only the top-N by triple count are expanded.

---

## Storage and HTAP Parameters

### `pg_ripple.vp_promotion_threshold` *(alias: `vp_promotion_threshold`)*

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Context | `sighup` |

Minimum triple count for a predicate before it gets a dedicated VP table. Predicates below this threshold are stored in the consolidated `vp_rare` table.

---

### `pg_ripple.merge_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Context | `sighup` |

Minimum row count in a delta table before the merge worker triggers a merge cycle for that predicate.

---

### `pg_ripple.merge_interval_secs`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `60` |
| Context | `sighup` |

Maximum seconds between merge worker polling intervals. Even if delta tables are below threshold, the merge worker checks for work at this frequency.

---

### `pg_ripple.merge_retention_seconds`

| | |
|---|---|
| Type | Integer |
| Default | `60` |
| Context | `sighup` |

Seconds to keep the old main table after a merge before dropping it. Allows long-running reads against the old main to complete gracefully.

---

### `pg_ripple.latch_trigger_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Context | `sighup` |

Number of triples written in one batch before poking (latching) the merge worker to wake up early.

---

### `pg_ripple.worker_database`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `postmaster` |

Name of the database the background merge worker connects to. Must match the database where the `pg_ripple` extension is installed. Required when using `shared_preload_libraries`.

---

### `pg_ripple.dedup_on_merge`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |

When `on`, the HTAP merge deduplicates `(s, o, g)` rows using `DISTINCT ON`, keeping the row with the lowest SID. Useful for write workloads that may insert duplicates.

---

### `pg_ripple.cache_budget_mb`

| | |
|---|---|
| Type | Integer (MiB) |
| Default | `64` |
| Context | `postmaster` |

Shared-memory budget cap in megabytes for the dictionary encode cache. Must be increased in step with `dictionary_cache_size`.

---

### `pg_ripple.auto_analyze`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `sighup` |

When `on`, the background merge worker runs `ANALYZE` on each VP main table immediately after a successful merge cycle, keeping planner statistics fresh.

---

### `pg_ripple.named_graph_optimized`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `postmaster` |

When `on`, adds a `(g, s, o)` B-tree index to every dedicated VP table for fast named-graph–scoped queries. Off by default to avoid index bloat. Enable if most queries use `GRAPH` patterns.

---

### `pg_ripple.normalize_iris`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `sighup` |
| Since | v0.55.0 |

When `on`, normalizes IRI strings to NFC Unicode before dictionary encoding. Ensures that semantically equivalent IRIs differing only in Unicode normalization form map to the same dictionary entry.

---

### `pg_ripple.rls_bypass`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `postmaster` (requires `shared_preload_libraries`) |
| Access | Superuser only |
| Since | v0.37.0 |

Superuser override to bypass graph-level Row-Level Security policies. Registered at `PGC_POSTMASTER` scope — a session-level `SET` is rejected, preventing privilege escalation.

---

### `pg_ripple.predicate_cache_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.38.0 |

Enable the backend-local predicate OID cache, which stores the mapping from predicate IDs to VP table OIDs. Disable to debug catalog lookup issues.

---

### `pg_ripple.cdc_bridge_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.52.0 |

Master switch for the CDC → pg_tide outbox bridge worker. When `on`, triple inserts and deletes are serialized as JSON-LD events and published to the configured pg_tide outbox.

---

### `pg_ripple.cdc_bridge_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `sighup` |
| Since | v0.52.0 |

Maximum number of CDC notifications batched before flushing to the outbox table.

---

### `pg_ripple.cdc_bridge_flush_ms`

| | |
|---|---|
| Type | Integer (ms) |
| Default | `200` |
| Context | `sighup` |
| Since | v0.52.0 |

Maximum milliseconds between CDC bridge worker flush cycles.

---

### `pg_ripple.cdc_bridge_outbox_table`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `sighup` |
| Since | v0.52.0 |

Legacy name for the pg_tide outbox that the CDC bridge worker publishes JSON-LD events to. The outbox must already exist via `tide.outbox_create(...)` when `cdc_bridge_enabled = on`.

---

### `pg_ripple.trickle_integration`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.52.0 |

Legacy master switch for relay bridge integration features. When `off`, CDC bridge code paths are disabled even when pg_tide is installed. Use `pg_ripple.pg_trickle_available()` separately for IVM-backed live views.

---

### `pg_ripple.replication_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.54.0 |

Enable the RDF logical replication consumer worker. When `on`, the worker subscribes to a logical replication slot and applies triple changes from a primary server.

---

### `pg_ripple.prov_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.58.0 |

When `on`, emits PROV-O provenance triples for all ingest operations, linking each triple to its `prov:Activity`, `prov:Agent`, and timestamp.

---

### `pg_ripple.citus_sharding_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `postmaster` |
| Since | v0.58.0 |

Enable Citus horizontal sharding of VP tables. When `on`, new VP tables are created with `REPLICA IDENTITY FULL` and distributed via `create_distributed_table(s)`.

---

### `pg_ripple.citus_trickle_compat`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.58.0 |

When `on`, `create_distributed_table` uses `colocate_with = 'none'` for legacy CDC/IVM compatibility. Prevents cross-shard tombstone deletes.

---

### `pg_ripple.approx_distinct`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.68.0 |

When `on`, routes `COUNT(DISTINCT …)` aggregates through Citus HLL (`hll_add_agg`) when the `hll` extension is available. Provides approximate but highly scalable distinct counts on distributed VP tables.

---

### `pg_ripple.citus_service_pruning`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.68.0 |

When `on`, the SPARQL federation translator rewrites SERVICE subqueries targeting Citus workers to include shard-constraint annotations, pruning irrelevant shards.

---

### `pg_ripple.columnar_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `-1` (disabled) |
| Context | `sighup` |
| Since | v0.57.0 |

Triple count threshold above which the HTAP merge converts `vp_{id}_main` from heap to columnar storage (via `pg_columnar`). `-1` disables columnar conversion.

---

### `pg_ripple.adaptive_indexing_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.57.0 |

Enable automatic adaptive index creation based on query access patterns. When `on`, the planner creates additional indexes on VP tables for frequently-queried predicate combinations.

---

### `pg_ripple.arrow_flight_secret`

| | |
|---|---|
| Type | String |
| Default | *(none — tickets unsigned)* |
| Context | `sighup` |
| Access | Superuser only |
| Since | v0.66.0 |

HMAC-SHA256 secret for signing Arrow Flight export tickets. In production, set to a long random value. Empty string = unsigned tickets (rejected by default in `pg_ripple_http`).

```sql
ALTER SYSTEM SET pg_ripple.arrow_flight_secret = 'your-long-random-secret-here';
SELECT pg_reload_conf();
```

---

### `pg_ripple.arrow_flight_expiry_secs`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `3600` |
| Context | `sighup` |
| Since | v0.66.0 |

Arrow Flight ticket validity period. Tickets older than this many seconds are rejected.

---

### `pg_ripple.arrow_unsigned_tickets_allowed`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Access | Superuser only |
| Since | v0.67.0 |

When `on`, unsigned Arrow Flight tickets are accepted. For local development only — never enable in production.

---

### `pg_ripple.merge_lock_timeout_ms`

| | |
|---|---|
| Type | Integer (ms) |
| Default | `5000` |
| Context | `sighup` |
| Since | v0.82.0 |

Fence lock timeout for the merge worker. If the lock is not acquired within this interval, the merge cycle is skipped.

---

### `pg_ripple.merge_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `1000000` |
| Range | `100`–`100000000` |
| Context | `sighup` |
| Since | v0.82.0 |

Maximum rows processed in a single merge `INSERT…SELECT` batch. Tune downward to reduce transaction duration at the cost of more merge passes.

---

### `pg_ripple.describe_max_depth`

| | |
|---|---|
| Type | Integer |
| Default | `16` |
| Range | `1`–`256` |
| Context | `userset` |
| Since | v0.85.0 |

Maximum recursion depth for `DESCRIBE` CBD traversal. Prevents runaway recursion on cyclic or deeply nested graphs.

---

### `pg_ripple.bulk_load_use_copy`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.94.0 |

When `on`, bulk loaders use `COPY ... FROM STDIN BINARY` for dictionary-encoded triple stream insertion instead of batched INSERTs. May improve throughput for very large loads.

---

### `pg_ripple.cdc_watermark_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `sighup` |
| Since | v0.91.0 |

Number of CDC events to accumulate before flushing the LSN watermark. Reduces per-event write amplification.

---

### `pg_ripple.bidi_relay_max_inflight`

| | |
|---|---|
| Type | Integer |
| Default | `1000` |
| Range | `1`–`100000` |
| Context | `userset` |
| Since | v0.94.0 |

Maximum concurrent in-flight bidirectional relay operations per process. When the limit is reached, new relay dispatch calls are dropped (drop-oldest) and `pg_ripple_bidi_relay_dropped_total` is incremented.

---

## Federation Parameters

### `pg_ripple.federation_timeout`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `30` |
| Context | `userset` |

Per-SERVICE-call wall-clock timeout. Endpoints that do not respond within this interval return an empty result (with PT214 WARNING).

---

### `pg_ripple.federation_max_results`

| | |
|---|---|
| Type | Integer |
| Default | `10000` |
| Context | `userset` |

Maximum rows accepted from a single remote SERVICE call.

---

### `pg_ripple.federation_on_error`

| | |
|---|---|
| Type | Enum string |
| Default | `'empty'` |
| Valid values | `empty`, `error` |
| Context | `userset` |

Behaviour when a SERVICE call fails completely. `empty` returns an empty result set; `error` raises an exception.

---

### `pg_ripple.federation_pool_size`

| | |
|---|---|
| Type | Integer |
| Default | `4` |
| Context | `sighup` |
| Since | v0.19.0 |

Number of idle HTTP connections to keep per remote federation endpoint (connection pooling).

---

### `pg_ripple.federation_cache_ttl`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `0` (disabled) |
| Context | `userset` |
| Since | v0.19.0 |

TTL for cached SERVICE results. When `0`, federation results are not cached. Set to a positive value to enable result caching for idempotent queries.

---

### `pg_ripple.federation_adaptive_timeout`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.19.0 |

When `on`, derives the effective per-endpoint timeout from observed P95 latency, adapting to endpoint responsiveness.

---

### `pg_ripple.federation_endpoint_policy`

| | |
|---|---|
| Type | Enum string |
| Default | `'default-deny'` |
| Valid values | `default-deny`, `allowlist`, `open` |
| Context | `sighup` |
| Since | v0.55.0 |

Network security policy for federation endpoints.

| Value | Behaviour |
|---|---|
| `default-deny` | Blocks RFC-1918 private, loopback, and link-local addresses (SSRF protection) |
| `allowlist` | Only endpoints listed in `federation_allowed_endpoints` are permitted |
| `open` | All endpoints allowed — development only, never use in production |

---

### `pg_ripple.federation_allowed_endpoints`

| | |
|---|---|
| Type | String (comma-separated URLs) |
| Default | *(none)* |
| Context | `sighup` |
| Since | v0.55.0 |

Comma-separated list of allowed federation endpoint URLs. Consulted only when `federation_endpoint_policy = 'allowlist'`.

---

### `pg_ripple.federation_circuit_breaker_threshold`

| | |
|---|---|
| Type | Integer |
| Default | `5` |
| Context | `userset` |
| Since | v0.56.0 |

Consecutive failures before opening the circuit breaker for a remote endpoint. Once tripped, the endpoint is bypassed for `federation_circuit_breaker_reset_seconds`.

---

### `pg_ripple.federation_circuit_breaker_reset_seconds`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `60` |
| Context | `userset` |
| Since | v0.56.0 |

Seconds until a tripped circuit breaker half-opens and allows a retry probe.

---

### `pg_ripple.federation_connect_timeout_secs`

| | |
|---|---|
| Type | Integer (seconds) |
| Default | `10` |
| Context | `userset` |
| Since | v0.96.0 |

TCP/TLS connection timeout for federation SERVICE endpoints. Separate from `federation_timeout` (which covers the query body). If the endpoint does not accept the connection within this window, the request is rejected immediately.

---

### `pg_ripple.federation_allow_unregistered_service_endpoints`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.98.0 |

When `off` (default), executing a SERVICE clause against an endpoint not registered in `pg_ripple.federation_endpoints` raises PT-SSRF-01. Set to `on` only for development or testing.

---

## Observability Parameters

### `pg_ripple.tracing_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.40.0 |

Master switch for OpenTelemetry distributed tracing. When `on`, SPARQL and Datalog query executions emit spans to the configured exporter.

---

### `pg_ripple.tracing_exporter`

| | |
|---|---|
| Type | Enum string |
| Default | `'stdout'` |
| Valid values | `stdout`, `otlp` |
| Context | `sighup` |
| Since | v0.40.0 |

OpenTelemetry exporter backend. Use `otlp` with `tracing_otlp_endpoint` to send spans to an OTLP collector (Jaeger, Tempo, etc.).

---

### `pg_ripple.tracing_otlp_endpoint`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `sighup` |
| Since | v0.51.0 |

OTLP collector endpoint URL for span export. Example: `http://localhost:4317`.

---

### `pg_ripple.audit_log_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.56.0 |

Enable SPARQL write-operation audit logging into `_pg_ripple.audit_log`. Records all INSERT/DELETE/UPDATE operations with timestamp, query text, and role.

---

### `pg_ripple.audit_retention_days`

| | |
|---|---|
| Type | Integer (days) |
| Default | `90` |
| Context | `sighup` |
| Since | v0.78.0 |

Retention period for `_pg_ripple.event_audit` rows. A background worker sweep prunes rows older than this many days once per hour. Set to `0` to disable automatic pruning.

---

### `pg_ripple.export_max_rows`

| | |
|---|---|
| Type | Integer |
| Default | `0` (unlimited) |
| Context | `userset` |
| Since | v0.40.0 |

Maximum rows returned by export functions (`export_turtle()`, `export_ntriples()`, `export_jsonld()`). Truncated exports emit PT642.

---

## PageRank Parameters

### `pg_ripple.pagerank_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.88.0 |

Master switch for the Datalog-native PageRank engine. Must be `on` to use `pagerank_run()`, `centrality_run()`, and related functions.

---

### `pg_ripple.pagerank_damping`

| | |
|---|---|
| Type | Float |
| Default | `0.85` |
| Range | `0.0`–`1.0` |
| Context | `userset` |
| Since | v0.88.0 |

PageRank damping factor (teleportation probability = 1 − damping). Standard value is 0.85 (matching Google's original paper).

---

### `pg_ripple.pagerank_max_iterations`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `userset` |
| Since | v0.88.0 |

Maximum PageRank iteration count. Stops early when convergence is reached.

---

### `pg_ripple.pagerank_convergence_delta`

| | |
|---|---|
| Type | Float |
| Default | `0.0001` |
| Context | `userset` |
| Since | v0.88.0 |

Convergence threshold for PageRank. Iteration stops when the maximum score change across all nodes falls below this value.

---

### `pg_ripple.pagerank_convergence_norm`

| | |
|---|---|
| Type | Enum string |
| Default | `'l1'` |
| Valid values | `l1`, `l2`, `linf` |
| Context | `userset` |
| Since | v0.90.0 |

Convergence norm for PageRank iteration. `l1` matches NetworkX; `l2` matches igraph; `linf` is most conservative.

---

### `pg_ripple.pagerank_partition`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.88.0 |

Enable graph-partitioned parallel PageRank computation. When `on`, the number of partitions is auto-tuned to `min(num_cpus, count(named_graphs))`. Set to `off` for single-partition (debugging) mode.

---

### `pg_ripple.pagerank_incremental`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.88.0 |

Enable pg-trickle incremental K-hop refresh. When `on`, only the K-hop neighborhood of changed nodes is re-evaluated after each write, rather than a full recompute.

---

### `pg_ripple.pagerank_khop_limit`

| | |
|---|---|
| Type | Integer |
| Default | `30` |
| Context | `userset` |
| Since | v0.88.0 |

Maximum K-hop propagation depth for incremental PageRank updates.

---

### `pg_ripple.pagerank_rules`

| | |
|---|---|
| Type | String (comma-separated IRIs) |
| Default | *(empty — all object-valued predicates)* |
| Context | `userset` |
| Since | v0.88.0 |

Comma-separated IRI list of edge predicates to include in the PageRank graph. Empty string means all object-valued predicates.

---

### `pg_ripple.pagerank_confidence_weighted`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.88.0 |

When `on`, multiplies edge weights by confidence scores from `_pg_ripple.confidence` during PageRank computation. Higher-confidence edges contribute more to PageRank propagation.

---

### `pg_ripple.pagerank_full_recompute_threshold`

| | |
|---|---|
| Type | Float |
| Default | `0.01` |
| Range | `0.0`–`1.0` |
| Context | `userset` |
| Since | v0.90.0 |

Fraction of stale `pagerank_scores` rows that triggers a full recompute instead of incremental refresh.

---

### `pg_ripple.pagerank_max_seeds`

| | |
|---|---|
| Type | Integer |
| Default | `1024` |
| Range | `1`–`1048576` |
| Context | `userset` |
| Since | v0.89.0 |

Maximum number of seed IRIs accepted by `pagerank_run(..., seed_iris)`. Arrays longer than this raise PT0411.

---

### `pg_ripple.pagerank_katz_alpha`

| | |
|---|---|
| Type | Float |
| Default | `0.01` |
| Context | `userset` |
| Since | v0.89.0 |

Attenuation factor for Katz centrality computation via `centrality_run()`.

---

## LLM / AI Parameters

### `pg_ripple.llm_endpoint`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `userset` |
| Since | v0.49.0 |

LLM API base URL for natural-language → SPARQL generation via `sparql_from_nl()`. Must be an OpenAI-compatible chat completions endpoint.

```sql
ALTER SYSTEM SET pg_ripple.llm_endpoint = 'https://api.openai.com/v1';
-- Or for Ollama:
ALTER SYSTEM SET pg_ripple.llm_endpoint = 'http://localhost:11434/v1';
```

---

### `pg_ripple.llm_model`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `userset` |
| Since | v0.49.0 |

LLM model identifier used for NL → SPARQL generation. Example: `'gpt-4o'`, `'llama3'`.

---

### `pg_ripple.llm_api_key_env`

| | |
|---|---|
| Type | String |
| Default | *(none)* |
| Context | `sighup` |
| Since | v0.49.0 |

Name of the OS environment variable that holds the LLM API key. The extension reads the key from this environment variable at query time.

---

### `pg_ripple.llm_include_shapes`

| | |
|---|---|
| Type | Boolean |
| Default | `on` |
| Context | `userset` |
| Since | v0.49.0 |

When `on`, active SHACL shapes are included as semantic context in the prompt sent to the LLM for NL → SPARQL generation.

---

### `pg_ripple.kge_enabled`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.57.0 |

Enable the knowledge-graph embedding (KGE) background worker. When `on`, the worker trains TransE/RotatE embeddings on the graph structure.

---

### `pg_ripple.kge_model`

| | |
|---|---|
| Type | Enum string |
| Default | `'transe'` |
| Valid values | `transe`, `rotate` |
| Context | `sighup` |
| Since | v0.57.0 |

Knowledge-graph embedding model. `transe` is faster; `rotate` handles relation composition better.

---

### `pg_ripple.auto_embed`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `sighup` |
| Since | v0.28.0 |

Master switch for trigger-based auto-embedding of new dictionary entries. When `on`, newly loaded entities are queued for background embedding.

---

### `pg_ripple.embedding_batch_size`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `sighup` |
| Since | v0.28.0 |

Number of entities dequeued and embedded per background worker batch.

---

### `pg_ripple.use_graph_context`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.28.0 |

When `on`, serializes each entity's RDF neighborhood before passing it to the embedding model. Produces graph-contextual embeddings at the cost of longer input strings.

---

### `pg_ripple.vector_federation_timeout_ms`

| | |
|---|---|
| Type | Integer (ms) |
| Default | `5000` |
| Context | `userset` |
| Since | v0.28.0 |

HTTP timeout for calls to external vector service endpoints.

---

## SHACL Parameters

### `pg_ripple.shacl_mode`

| | |
|---|---|
| Type | Enum string |
| Default | `'off'` |
| Valid values | `off`, `sync`, `async` |
| Context | `userset` |

SHACL validation mode.

| Value | Behaviour |
|---|---|
| `off` | SHACL enforcement disabled — `validate()` still works on demand |
| `sync` | Violations are caught inline during triple insert; violating triples are rejected with PT301 |
| `async` | Violations are queued for background validation; inserts are not blocked |

---

### `pg_ripple.shacl_rule_max_iterations`

| | |
|---|---|
| Type | Integer |
| Default | `100` |
| Context | `userset` |
| Since | v0.79.0 |

Maximum fixpoint iterations for `sh:SPARQLRule` evaluation per validation cycle. Prevents infinite loops when rules fire each other.

---

### `pg_ripple.shacl_rule_cwb`

| | |
|---|---|
| Type | Boolean |
| Default | `off` |
| Context | `userset` |
| Since | v0.79.0 |

When `on`, `sh:SPARQLRule` rules whose target graph matches an existing CONSTRUCT writeback pipeline are registered as CWB rules rather than standalone SPARQL evaluations.

---

## Quick-Reference Table

For convenience, here is a summary of all parameters grouped by the most common tuning need:

### Performance Tuning

| Parameter | Default | Purpose |
|---|---|---|
| `dictionary_cache_size` | `65536` | In-memory encode/decode LRU cache size |
| `merge_threshold` | `10000` | Rows before merge worker fires |
| `merge_workers` | `1` | Parallel merge background workers |
| `export_batch_size` | `10000` | SPARQL cursor batch size |
| `wcoj_enabled` | `on` | Worst-case optimal joins for cyclic patterns |
| `bgp_reorder` | `on` | Join reordering by selectivity |
| `topn_pushdown` | `on` | `ORDER BY … LIMIT N` optimization |
| `arrow_batch_size` | `1000` | Arrow IPC record batch size |
| `auto_analyze` | `on` | Post-merge `ANALYZE` |

### Security

| Parameter | Default | Purpose |
|---|---|---|
| `federation_endpoint_policy` | `default-deny` | SSRF protection for SERVICE clauses |
| `federation_allow_private` | `off` | Block private-IP federation targets |
| `rls_bypass` | `off` | Superuser graph-RLS bypass |
| `arrow_flight_secret` | *(none)* | Arrow Flight ticket signing |
| `arrow_unsigned_tickets_allowed` | `off` | Development-only unsigned tickets |

### Inference Tuning

| Parameter | Default | Purpose |
|---|---|---|
| `inference_mode` | `on_demand` | When inference runs |
| `magic_sets` | `on` | Goal-directed demand evaluation |
| `dred_enabled` | `on` | Incremental deletion re-derivation |
| `datalog_parallel_workers` | `4` | Parallel stratum evaluation |
| `owl_profile` | `RL` | OWL 2 rule set selection |
| `wfs_max_iterations` | `100` | Well-founded semantics cap |

### JSON Mapping Writeback (v0.128.0)

| Parameter | Default | Purpose |
|---|---|---|
| `json_writeback_batch_size` | `100` | Queue rows drained per background merge-worker tick |

---

## `pg_ripple.json_writeback_batch_size`

Controls how many `_pg_ripple.json_writeback_queue` rows the background merge
worker processes per tick.

| Attribute | Value |
|---|---|
| Type | Integer |
| Default | `100` |
| Range | `0`–`10000` |
| Context | `suset` |
| Since | v0.128.0 |

Set to `0` to disable automatic background draining (rows will accumulate until
processed manually or via a direct `pg_ripple.writeback_json_row()` call).

Higher values increase writeback throughput at the cost of longer merge-worker
transactions. The default of 100 is suitable for most workloads.
