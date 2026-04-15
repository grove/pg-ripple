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
