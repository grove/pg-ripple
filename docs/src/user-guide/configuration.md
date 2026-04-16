# Configuration

All GUC (Grand Unified Configuration) parameters for pg_ripple. Set them in `postgresql.conf`, via `ALTER SYSTEM SET`, or per-session with `SET pg_ripple.parameter_name = value`.

## vp_promotion_threshold

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `1000` |
| Min / Max | `1 / 2147483647` |
| Restart required | No |

Predicates with fewer distinct triples than this threshold are stored in the shared `_pg_ripple.vp_rare` table. Once a predicate's triple count crosses this threshold it is automatically promoted to a dedicated `_pg_ripple.vp_{id}` table.

Lower values create more VP tables (more specific index scans, larger schema). Higher values keep more data in `vp_rare` (simpler schema, potentially slower queries on large datasets).

---

## named_graph_optimized

| Property | Value |
|---|---|
| Type | `boolean` |
| Default | `false` |
| Restart required | No |

When `true`, adds a `(g, s, o)` composite index to each VP table. This speeds up queries that filter by named graph but increases storage and index maintenance overhead. Enable for workloads that frequently use `GRAPH <…> { … }` patterns.

---

## plan_cache_size

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `128` |
| Min / Max | `1 / 4096` |
| Restart required | No |

Maximum number of compiled SPARQL→SQL plans to cache per backend. Each entry stores the generated SQL string and the list of bound parameter values. Larger values reduce recompilation overhead for workloads with many distinct query shapes.

---

## max_path_depth

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `100` |
| Min / Max | `1 / 2147483647` |
| Restart required | No |

Maximum recursion depth for property path queries (`+`, `*`). The generated `WITH RECURSIVE` CTE includes a `WHERE _depth < max_path_depth` guard clause that stops the recursion when this limit is reached.

Set a lower value to protect against runaway queries on dense graphs:

```sql
SET pg_ripple.max_path_depth = 10;
```

The plan cache key includes this value, so changing `max_path_depth` automatically invalidates cached path query plans for the current session.

---

## dictionary_cache_size

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `65536` (64 K entries) |
| Min / Max | `256 / 1048576` |
| Restart required | **Yes** (shared memory) |

Size of the in-process LRU cache for recently used dictionary entries. Larger values reduce dictionary table lookups for workloads with high term reuse. Requires a PostgreSQL restart because shared memory is allocated at startup.

---

## HTAP Parameters (v0.6.0)

The following parameters control the HTAP delta/main split introduced in v0.6.0. They are all `SIGHUP`-reloadable — no PostgreSQL restart is required. They take effect only when `pg_ripple` is loaded via `shared_preload_libraries`.

### merge_threshold

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `10000` |
| Min / Max | `1 / 2147483647` |
| Restart required | No (SIGHUP) |

Minimum number of rows in a delta table before the background merge worker considers that predicate ready to merge. Lower values trigger more frequent merges (tighter read freshness, more I/O). Higher values batch more writes before merging (fewer merges, larger delta scans between merges).

---

### merge_interval_secs

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `60` |
| Min / Max | `1 / 3600` |
| Restart required | No (SIGHUP) |

Maximum number of seconds between merge worker poll cycles. The worker wakes up at least this often even if no latch poke has been received. Decrease for more responsive merges; increase to reduce background I/O on quiet deployments.

---

### merge_retention_seconds

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `60` |
| Min / Max | `0 / 86400` |
| Restart required | No (SIGHUP) |

Seconds to keep the previous `vp_{id}_main` table after a successful merge before dropping it. This provides a safety window for long-running read transactions that were started before the merge completed. After the retention period the old main table is dropped.

Set to `0` to drop immediately (not recommended for production — transactions in flight may fail).

---

### latch_trigger_threshold

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `10000` |
| Min / Max | `1 / 2147483647` |
| Restart required | No (SIGHUP) |

When the shared-memory `TOTAL_DELTA_ROWS` counter reaches this value, the `ExecutorEnd` hook pokes the merge worker's latch to trigger an early merge without waiting for the next `merge_interval_secs` cycle. Set lower than `merge_threshold` for low-latency deployments; set higher to reduce poke frequency on bulk-insert workloads.

---

### worker_database

| Property | Value |
|---|---|
| Type | `string` |
| Default | `postgres` |
| Restart required | No (SIGHUP) |

Name of the database the background merge worker connects to. Must match the database where `CREATE EXTENSION pg_ripple` was executed. Change this if the extension lives in a non-default database.

---

### merge_watchdog_timeout

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `300` |
| Min / Max | `10 / 86400` |
| Restart required | No (SIGHUP) |

Seconds of merge worker inactivity before a `WARNING` is logged. This helps diagnose situations where the worker is alive but blocked (e.g. lock contention, out-of-disk). Normal operation resets the watchdog on every successful merge poll.

---

## SHACL & Data Quality Parameters (v0.7.0+)

### shacl_mode

| Property | Value |
|---|---|
| Type | `enum` |
| Default | `'off'` |
| Values | `'off'`, `'sync'`, `'async'` |
| Restart required | No |

Controls when SHACL validation runs:

- **`'off'`** — no automatic validation; call `validate()` on demand
- **`'sync'`** — `insert_triple()` immediately rejects triples that violate `sh:maxCount`, `sh:datatype`, `sh:in`, or `sh:pattern` constraints
- **`'async'`** — inserts complete immediately; violations are queued in `_pg_ripple.validation_queue` for background processing

```sql
SET pg_ripple.shacl_mode = 'sync';
```

---

### dedup_on_merge

| Property | Value |
|---|---|
| Type | `boolean` |
| Default | `false` |
| Restart required | No |

When `true`, the HTAP merge worker deduplicates `(s, o, g)` rows during compaction, keeping the row with the lowest SID. Eliminates duplicates automatically without a separate `deduplicate_all()` call.

```sql
SET pg_ripple.dedup_on_merge = true;
```

---

## SPARQL Parameters (v0.5.1+)

### describe_strategy

| Property | Value |
|---|---|
| Type | `enum` |
| Default | `'cbd'` |
| Values | `'cbd'`, `'scbd'`, `'simple'` |
| Restart required | No |

Default DESCRIBE expansion algorithm:

- **`'cbd'`** — Concise Bounded Description: all outgoing triples from the resource
- **`'scbd'`** — Symmetric CBD: outgoing and incoming triples
- **`'simple'`** — subject triples only (fastest)

```sql
SET pg_ripple.describe_strategy = 'scbd';
```

---

## Datalog Parameters (v0.10.0)

### inference_mode

| Property | Value |
|---|---|
| Type | `enum` |
| Default | `'on_demand'` |
| Values | `'off'`, `'on_demand'`, `'materialized'` |
| Restart required | No |

Controls how the Datalog reasoning engine operates:

- **`'off'`** — engine disabled; `infer()` is a no-op
- **`'on_demand'`** — inference runs via CTEs when `infer()` is called
- **`'materialized'`** — uses pg_trickle stream tables for automatic refresh

```sql
SET pg_ripple.inference_mode = 'on_demand';
```

---

### enforce_constraints

| Property | Value |
|---|---|
| Type | `enum` |
| Default | `'warn'` |
| Values | `'off'`, `'warn'`, `'error'` |
| Restart required | No |

Controls how Datalog constraint violations (rules with empty heads) are handled:

- **`'off'`** — violations are silenced
- **`'warn'`** — violations are logged as warnings
- **`'error'`** — violations raise an exception

```sql
SET pg_ripple.enforce_constraints = 'error';
```

---

### rule_graph_scope

| Property | Value |
|---|---|
| Type | `enum` |
| Default | `'default'` |
| Values | `'default'`, `'all'` |
| Restart required | No |

Controls which graphs Datalog rules apply to:

- **`'default'`** — rules only apply to the default graph
- **`'all'`** — rules apply across all named graphs

```sql
SET pg_ripple.rule_graph_scope = 'all';
```

---

## Performance Parameters (v0.13.0)

### bgp_reorder

| Property | Value |
|---|---|
| Type | `boolean` |
| Default | `true` |
| Restart required | No |

When enabled, the SPARQL engine reorders triple patterns within a Basic Graph Pattern (BGP) by estimated selectivity — most restrictive first. Uses `pg_class.reltuples` and `pg_stats.n_distinct` to estimate row counts at translation time.

Disable if you need deterministic query plan ordering for debugging:

```sql
SET pg_ripple.bgp_reorder = off;
```

---

### parallel_query_min_joins

| Property | Value |
|---|---|
| Type | `integer` |
| Default | `3` |
| Min / Max | `1 / 100` |
| Restart required | No |

Minimum number of VP table joins in a SPARQL query before parallel query hints are emitted. When the threshold is met, the engine sets `max_parallel_workers_per_gather = 4` and `enable_parallel_hash = on` for that query.

Lower the threshold for queries with few but large VP tables. Raise it if parallel overhead hurts small queries:

```sql
SET pg_ripple.parallel_query_min_joins = 2;
```

---

## Security Parameters (v0.14.0)

### rls_bypass

| Property | Value |
|---|---|
| Type | `boolean` |
| Default | `false` |
| Context | Superuser only (`GUC_SUSET`) |
| Restart required | No |

When `true`, skips Row-Level Security checks on VP tables. Only a superuser can set this. Used for administrative operations that need to read/write all graphs regardless of RLS policies.

```sql
-- Superuser only
SET pg_ripple.rls_bypass = on;
```

---

## Quick tuning reference

| Workload | Key parameters |
|----------|----------------|
| **Small (< 1M triples)** | Defaults work well |
| **Medium (1–50M triples)** | `dictionary_cache_size = 131072`, `merge_threshold = 50000` |
| **Large (50M+)** | `dictionary_cache_size = 262144`, `bgp_reorder = on`, `parallel_query_min_joins = 2` |
| **Write-heavy OLTP** | `merge_interval_secs = 10`, `latch_trigger_threshold = 5000`, `shacl_mode = 'async'` |
| **Analytics / OLAP** | `plan_cache_size = 512`, `bgp_reorder = on`, `parallel_query_min_joins = 2` |
| **Reasoning workload** | `inference_mode = 'on_demand'`, `dictionary_cache_size = 262144` |
