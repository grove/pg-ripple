# Configuration and Tuning

pg_ripple exposes its configuration through PostgreSQL GUC (Grand Unified Configuration) parameters. All parameters use the `pg_ripple.` prefix and can be set in `postgresql.conf`, via `ALTER SYSTEM`, or per-session with `SET`.

```admonish info title="Restart requirements"
Parameters marked **Postmaster** require a PostgreSQL restart. Parameters marked **SIGHUP** can be reloaded with `SELECT pg_reload_conf()`. All others can be changed per-session with `SET`.
```

---

## Storage Parameters

Control how triples are stored in VP tables and the rare-predicate consolidation table.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `vp_promotion_threshold` | int | `1000` | 10 – 10,000,000 | Userset | Minimum triples before a predicate gets a dedicated VP table. Below this, triples go to `vp_rare`. |
| `named_graph_optimized` | bool | `off` | — | Userset | Adds a `(g, s, o)` index per VP table. Speeds up `GRAPH` queries but increases write overhead. |
| `default_graph` | text | `''` | Any IRI | Userset | IRI used as the default graph when `g` is not specified on insert. |
| `dedup_on_merge` | bool | `off` | — | Userset | When on, the merge worker deduplicates `(s, o, g)` rows, keeping the lowest SID. |

---

## HTAP / Merge Worker Parameters

Control the delta/main split and background merge behavior. These take effect only when pg_ripple is loaded via `shared_preload_libraries`.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `merge_threshold` | int | `10000` | 1 – 2,147,483,647 | SIGHUP | Delta row count that triggers a merge for a predicate. Lower = fresher reads, more I/O. |
| `merge_interval_secs` | int | `60` | 1 – 3600 | SIGHUP | Maximum seconds between merge worker poll cycles. |
| `merge_retention_seconds` | int | `60` | 0 – 86,400 | SIGHUP | Seconds to keep the old main table after a merge before dropping it. |
| `latch_trigger_threshold` | int | `10000` | 1 – 2,147,483,647 | SIGHUP | Rows written in a batch before poking the merge worker latch immediately. |
| `merge_watchdog_timeout` | int | `300` | 10 – 86,400 | SIGHUP | Seconds of merge worker inactivity before logging a WARNING. |
| `worker_database` | text | `'postgres'` | — | SIGHUP | Database the background merge worker connects to. |
| `auto_analyze` | bool | `on` | — | SIGHUP | Run ANALYZE on VP main tables after each merge cycle. |

---

## Query Engine Parameters

Tune SPARQL-to-SQL translation and execution.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `plan_cache_size` | int | `256` | 0 – 65,536 | Userset | Cached SPARQL→SQL translations per backend. 0 disables caching. |
| `max_path_depth` | int | `100` | 0 – 10,000 | Userset | Maximum recursion depth for property path queries (`+`, `*`). 0 = unlimited. |
| `property_path_max_depth` | int | `64` | 1 – 100,000 | Userset | Alternative property path depth limit (v0.24.0). |
| `describe_strategy` | text | `'cbd'` | `'cbd'`, `'scbd'`, `'simple'` | Userset | DESCRIBE algorithm: Concise Bounded Description, Symmetric CBD, or simple one-hop. |
| `bgp_reorder` | bool | `on` | — | Userset | Reorder BGP triple patterns by estimated selectivity before SQL generation. |
| `parallel_query_min_joins` | int | `3` | 1 – 100 | Userset | Minimum VP-table joins before enabling parallel query workers. |
| `sparql_strict` | bool | `on` | — | Userset | When on, unsupported FILTER functions raise an error; when off, they are silently dropped. |
| `export_batch_size` | int | `10000` | 100 – 1,000,000 | Userset | Triples per cursor batch during streaming export. |

---

## Inference / Datalog Parameters

Control the Datalog reasoning engine, magic sets, and rule caching.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `inference_mode` | text | `'off'` | `'off'`, `'on_demand'`, `'materialized'` | Userset | Datalog reasoning mode. `'materialized'` requires pg_trickle. |
| `enforce_constraints` | text | `'off'` | `'off'`, `'warn'`, `'error'` | Userset | Behavior when Datalog constraint rules detect violations. |
| `rule_graph_scope` | text | `'default'` | `'default'`, `'all'` | Userset | Whether unscoped rule atoms operate on the default graph only or all graphs. |
| `magic_sets` | bool | `on` | — | Userset | Use magic sets for goal-directed inference in `infer_goal()`. |
| `datalog_cost_reorder` | bool | `on` | — | Userset | Sort rule body atoms by ascending VP-table cardinality before SQL compilation. |
| `datalog_antijoin_threshold` | int | `1000` | 0 – 10,000,000 | Userset | Minimum VP rows for NOT atoms to use LEFT JOIN anti-join form. |
| `delta_index_threshold` | int | `500` | 0 – 10,000,000 | Userset | Minimum semi-naive delta rows before creating a B-tree index. |
| `demand_transform` | bool | `on` | — | Userset | Auto-apply demand transformation when multiple goal patterns are specified. |
| `sameas_reasoning` | bool | `on` | — | Userset | Apply `owl:sameAs` canonicalization pre-pass during inference. |
| `rule_plan_cache` | bool | `on` | — | Userset | Cache compiled SQL for each rule set. Invalidated by `drop_rules()` and `load_rules()`. |
| `rule_plan_cache_size` | int | `64` | 1 – 4,096 | Userset | Maximum rule sets in the plan cache. |

---

## Well-Founded Semantics / Tabling Parameters

Control WFS evaluation and tabling cache (v0.32.0).

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `wfs_max_iterations` | int | `100` | 1 – 10,000 | Userset | Safety cap on alternating fixpoint rounds per WFS pass. Emits PT520 WARNING if not converged. |
| `tabling` | bool | `on` | — | Userset | Cache `infer_wfs()` and SPARQL results in `_pg_ripple.tabling_cache`. |
| `tabling_ttl` | int | `300` | 0 – 86,400 | Userset | TTL in seconds for tabling cache entries. 0 disables TTL-based expiry. |

---

## SHACL Validation Parameters

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `shacl_mode` | text | `'off'` | `'off'`, `'sync'`, `'async'` | Userset | `'sync'` rejects violations inline; `'async'` queues for background validation. |

---

## Federation Parameters

Control remote SPARQL endpoint calls via the `SERVICE` keyword.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `federation_timeout` | int | `30` | 1 – 3,600 | Userset | Per-SERVICE call wall-clock timeout in seconds. |
| `federation_max_results` | int | `10000` | 1 – 1,000,000 | Userset | Maximum rows accepted from a single remote call. |
| `federation_on_error` | text | `'warning'` | `'warning'`, `'error'`, `'empty'` | Userset | Behavior on SERVICE call failure. |
| `federation_pool_size` | int | `4` | 1 – 32 | Userset | Idle HTTP connections per endpoint host. |
| `federation_cache_ttl` | int | `0` | 0 – 86,400 | Userset | Remote result cache TTL in seconds. 0 disables caching. |
| `federation_on_partial` | text | `'empty'` | `'empty'`, `'use'` | Userset | Behavior on mid-stream SERVICE failure. |
| `federation_adaptive_timeout` | bool | `off` | — | Userset | Derive per-endpoint timeout from P95 latency. |

---

## Shared Memory Parameters (Startup Only)

These must be set in `postgresql.conf` before PostgreSQL starts. They cannot be changed at runtime.

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `dictionary_cache_size` | int | `4096` | 0 – 1,000,000 | Postmaster | Shared-memory encode cache capacity in entries. |
| `cache_budget` | int | `64` | 0 – 65,536 | Postmaster | Shared-memory budget cap in MB. Bulk loads throttle at 90% utilization. |

```admonish warning title="Startup GUCs require restart"
Changes to `dictionary_cache_size` and `cache_budget` require a full PostgreSQL restart. Plan your cache sizing before deploying to production.
```

---

## Security Parameters

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `rls_bypass` | bool | `off` | — | Suset | Superuser override to bypass graph-level Row-Level Security. |

---

## Vector / Embedding Parameters

| Parameter | Type | Default | Range | Context | Description |
|---|---|---|---|---|---|
| `embedding_model` | text | `''` | — | Userset | Model name tag stored in `_pg_ripple.embeddings`. |
| `embedding_dimensions` | int | `1536` | 1 – 16,000 | Userset | Vector dimension count. Must match model output. |
| `embedding_api_url` | text | `''` | — | Userset | Base URL for OpenAI-compatible embedding API. |
| `embedding_api_key` | text | `''` | — | Suset | API key (superuser-only, masked in `pg_settings`). |
| `pgvector_enabled` | bool | `on` | — | Userset | Disable pgvector code paths without uninstalling. |
| `embedding_index_type` | text | `'hnsw'` | `'hnsw'`, `'ivfflat'` | Userset | Index type on embeddings table. |
| `embedding_precision` | text | `'single'` | `'single'`, `'half'`, `'binary'` | Userset | Storage precision for embedding vectors. |
| `auto_embed` | bool | `off` | — | Userset | Auto-embed new entities via background worker. |
| `embedding_batch_size` | int | `100` | 1 – 10,000 | Userset | Entities dequeued per background worker batch. |

---

## Quick-Start Configurations

### Small Dataset (< 1M triples)

Suitable for development, prototyping, or small knowledge graphs:

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'

# Dictionary cache — small footprint
pg_ripple.dictionary_cache_size = 8192
pg_ripple.cache_budget = 16

# Merge worker — merge early for fresh reads
pg_ripple.merge_threshold = 5000
pg_ripple.merge_interval_secs = 30

# Query engine
pg_ripple.plan_cache_size = 64
pg_ripple.max_path_depth = 50
```

### Medium Dataset (1M – 100M triples)

Production workloads with moderate query complexity:

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'

# Dictionary cache — larger cache for better hit rates
pg_ripple.dictionary_cache_size = 131072
pg_ripple.cache_budget = 128

# Merge worker — balance freshness and I/O
pg_ripple.merge_threshold = 50000
pg_ripple.merge_interval_secs = 60
pg_ripple.latch_trigger_threshold = 20000
pg_ripple.auto_analyze = on

# Query engine — larger plan cache for diverse queries
pg_ripple.plan_cache_size = 512
pg_ripple.max_path_depth = 100
pg_ripple.bgp_reorder = on

# Inference (if used)
pg_ripple.inference_mode = 'on_demand'
pg_ripple.magic_sets = on
```

### Large Dataset (> 100M triples)

High-throughput production with heavy query loads:

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'

# Dictionary cache — maximize cache coverage
pg_ripple.dictionary_cache_size = 500000
pg_ripple.cache_budget = 512

# Merge worker — batch larger merges, reduce churn
pg_ripple.merge_threshold = 200000
pg_ripple.merge_interval_secs = 120
pg_ripple.latch_trigger_threshold = 100000
pg_ripple.merge_retention_seconds = 120
pg_ripple.auto_analyze = on

# Query engine — large plan cache, parallel queries
pg_ripple.plan_cache_size = 2048
pg_ripple.max_path_depth = 200
pg_ripple.bgp_reorder = on
pg_ripple.parallel_query_min_joins = 2

# Named graph optimization (if heavy GRAPH usage)
pg_ripple.named_graph_optimized = on

# Inference
pg_ripple.inference_mode = 'on_demand'
pg_ripple.magic_sets = on
pg_ripple.rule_plan_cache = on
pg_ripple.rule_plan_cache_size = 256

# Tabling cache for repeated inference patterns
pg_ripple.tabling = on
pg_ripple.tabling_ttl = 600

# Federation (if used)
pg_ripple.federation_timeout = 60
pg_ripple.federation_pool_size = 8
pg_ripple.federation_cache_ttl = 300
```

```admonish tip title="PostgreSQL tuning"
Don't forget to tune PostgreSQL itself alongside pg_ripple. Key PostgreSQL parameters for triple store workloads:
- `shared_buffers` = 25% of RAM
- `effective_cache_size` = 75% of RAM
- `work_mem` = 64MB–256MB (for complex joins)
- `maintenance_work_mem` = 512MB–1GB (for merge ANALYZE)
- `random_page_cost` = 1.1 (if using SSDs)
- `max_parallel_workers_per_gather` = 4
```

## Uncertain Knowledge Engine GUCs (v0.87.0)

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.probabilistic_datalog` | bool | `off` | Enable @weight rule annotations |
| `pg_ripple.prob_datalog_cyclic` | bool | `off` | Allow approximate evaluation on cyclic rule sets |
| `pg_ripple.prob_datalog_max_iterations` | int | `100` | Maximum semi-naive iterations |
| `pg_ripple.prob_datalog_convergence_delta` | float8 | `0.001` | Early-exit convergence threshold |
| `pg_ripple.prob_datalog_cyclic_strict` | bool | `off` | Promote non-convergence to ERROR (PT0307) |
| `pg_ripple.default_fuzzy_threshold` | float8 | `0.7` | Default fuzzy match threshold |
| `pg_ripple.prov_confidence` | bool | `off` | Enable PROV-O pg:sourceTrust propagation |
| `pg_ripple.export_confidence` | bool | `off` | Include RDF-star annotations in Turtle export |
| `pg_ripple.cwb_confidence_propagation` | string | (empty) | CONSTRUCT rule name for CWB trust propagation |
