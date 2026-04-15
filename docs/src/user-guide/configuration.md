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
