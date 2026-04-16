# Scaling & HTAP Architecture

pg_ripple v0.6.0 introduces an **HTAP** (Hybrid Transactional/Analytical) storage layout that decouples write throughput from query freshness. This page explains the architecture, tuneable parameters, and operational guidance for production deployments.

## Overview

Prior to v0.6.0, each VP table was a single flat heap table. Writes and reads competed for the same B-tree indexes; heavy insertion workloads caused index bloat and degraded read latency.

From v0.6.0 onward, each VP table is split into two partitions:

| Partition | Type | Purpose |
|---|---|---|
| `vp_{id}_delta` | Heap + B-tree | All new writes land here first |
| `vp_{id}_main` | BRIN-indexed heap | Read-optimised; written only by the merge worker |
| `vp_{id}_tombstones` | Small heap | Records main-resident row deletions |

A **background merge worker** periodically promotes delta rows into main, removing tombstoned rows in the process. During the merge, reads continue to serve `(main EXCEPT tombstones) UNION ALL delta` — no locking of user sessions is required.

```
      INSERT / DELETE
           │
           ▼
    ┌──────────────┐         ┌──────────────────────┐
    │  vp_{id}_delta│◄────── │ vp_{id}_tombstones   │
    └──────────────┘  delete │ (main-resident deletes)│
           │          main   └──────────────────────┘
           │ merge              ▲
           ▼                    │ read tombstone list
    ┌──────────────┐            │
    │  vp_{id}_main │◄──────────┘
    │  (BRIN index) │
    └──────────────┘
           │
           ▼
    UNION ALL → query path
```

## Merge Worker Lifecycle

1. **Startup**: The merge worker is registered during `_PG_init` (requires `shared_preload_libraries`). It connects to `pg_ripple.worker_database` and stores its PID in shared memory.
2. **Poll loop**: The worker wakes every `merge_interval_secs` seconds (default: 60). It can also be woken early by a latch poke from the `ExecutorEnd` hook (fires when `TOTAL_DELTA_ROWS >= latch_trigger_threshold`).
3. **Per-predicate merge**: For each HTAP-enabled predicate whose delta row count exceeds `merge_threshold`, the worker:
   a. Creates a new main table from `(current_main EXCEPT tombstones) UNION ALL delta`
   b. Rebuilds the BRIN index
   c. Atomically swaps the new table into place
   d. Waits `merge_retention_seconds` before dropping the old main table
4. **Compact on demand**: Call `pg_ripple.compact()` to run a merge cycle immediately (blocks until the merge finishes).

## Shared Memory

When loaded via `shared_preload_libraries`, pg_ripple allocates a small fixed-size region in PostgreSQL shared memory:

| Atomic | Type | Description |
|---|---|---|
| `SHMEM_READY` | bool | True once shmem is initialised |
| `MERGE_WORKER_PID` | i32 | PID of the merge background worker |
| `TOTAL_DELTA_ROWS` | i64 | Running count of unmerged rows across all delta tables |

These counters are updated without locks using `Relaxed` atomic operations; the latch-poke uses `Acquire`/`Release` memory ordering.

## Tuning for Write-Heavy Workloads

For bulk-insert scenarios (e.g. loading a large knowledge graph):

```sql
-- Lower the trigger threshold so the worker starts merging early
ALTER SYSTEM SET pg_ripple.latch_trigger_threshold = 50000;
-- Allow larger delta accumulation before forcing each merge
ALTER SYSTEM SET pg_ripple.merge_threshold = 100000;
-- Shorten the retention window to free disk space faster
ALTER SYSTEM SET pg_ripple.merge_retention_seconds = 10;
SELECT pg_reload_conf();
```

For real-time write workloads with freshness requirements:

```sql
-- Trigger early merges more aggressively
ALTER SYSTEM SET pg_ripple.latch_trigger_threshold = 1000;
ALTER SYSTEM SET pg_ripple.merge_threshold = 5000;
ALTER SYSTEM SET pg_ripple.merge_interval_secs = 10;
SELECT pg_reload_conf();
```

## Monitoring Merge State

```sql
-- Check current HTAP state
SELECT pg_ripple.stats();

-- Example output:
-- {
--   "total_triples": 1500000,
--   "dedicated_predicates": 42,
--   "htap_predicates": 42,
--   "rare_triples": 1234,
--   "unmerged_delta_rows": 8742,
--   "merge_worker_pid": 12345
-- }

-- Check how many rows are in each delta table
SELECT
    relname,
    n_live_tup
FROM pg_stat_user_tables
WHERE relname LIKE '%_delta'
ORDER BY n_live_tup DESC;
```

If `unmerged_delta_rows` stays high for a long time, the merge worker may be falling behind. See the [Troubleshooting](../reference/troubleshooting.md) page.

## Pre-v0.6.0 Migration

Existing flat VP tables (created before v0.6.0) are automatically migrated to the delta/main split by the `pg_ripple--0.5.1--0.6.0.sql` migration script. You can also migrate individual predicates manually:

```sql
-- Migrate a specific predicate by its dictionary ID
SELECT pg_ripple.htap_migrate_predicate(
    (SELECT id FROM _pg_ripple.predicates
     JOIN _pg_ripple.dictionary ON id = id
     WHERE value = 'https://schema.org/name')
);
```

## Limitations

- The merge worker requires `shared_preload_libraries = 'pg_ripple'` in `postgresql.conf`. Without it, all writes go into delta and no background merges occur.
- `compact()` can be called manually but it blocks the calling session until the merge finishes.
- Very large predicates (>100M triples) may cause the merge to hold an exclusive lock briefly during the table swap. Schedule maintenance windows for extremely large merges.

---

## Performance Hardening (v0.13.0)

### BGP Join Reordering

By default (`pg_ripple.bgp_reorder = on`), triple patterns within a Basic Graph Pattern are reordered by estimated selectivity before SQL generation.

**How it works:**
1. At translation time, pg_ripple queries `pg_class.reltuples` and `pg_stats.n_distinct` for each VP table.
2. Patterns are sorted cheapest-first using a greedy left-deep algorithm.
3. `SET LOCAL join_collapse_limit = 1` is emitted before each query so the PostgreSQL planner follows the computed order.
4. `SET LOCAL enable_mergejoin = on` is also set to exploit merge-join when join columns are sorted.

**To disable** (e.g. for debugging, or when the planner already has good statistics):
```sql
SET pg_ripple.bgp_reorder = off;
```

### Parallel Query Exploitation

Queries joining 3 or more VP tables automatically enable PostgreSQL parallel workers:
```sql
SET pg_ripple.parallel_query_min_joins = 3;  -- default
```

When the threshold is met, before query execution:
```sql
SET LOCAL max_parallel_workers_per_gather = 4;
SET LOCAL enable_parallel_hash = on;
SET LOCAL parallel_setup_cost = 10;
```

Verify with EXPLAIN:
```sql
SELECT pg_ripple.sparql_explain($$
  SELECT ?s ?name ?age ?email WHERE {
    ?s <https://schema.org/name>  ?name  .
    ?s <https://schema.org/age>   ?age   .
    ?s <https://schema.org/email> ?email .
  }
$$, true);
-- Look for "Parallel Hash Join" in the EXPLAIN output
```

### Extended Statistics

When a predicate is promoted from `vp_rare` to a dedicated VP table, pg_ripple automatically creates:
```sql
CREATE STATISTICS _pg_ripple.ext_stats_vp_{id}
  (ndistinct, dependencies) ON s, o
  FROM _pg_ripple.vp_{id}_delta;
```

This gives the PostgreSQL planner correlation data for `(s, o)` pairs, enabling more accurate cardinality estimates for multi-predicate star queries.

### Plan Cache Monitoring

Monitor cache efficiency with:
```sql
SELECT pg_ripple.plan_cache_stats();
-- {"hits": 1234, "misses": 56, "size": 48, "capacity": 256, "hit_rate": 0.9567}
```

Tune the cache size:
```sql
SET pg_ripple.plan_cache_size = 512;  -- default: 256 (max: 65536, 0 = disabled)
```

Reset counters:
```sql
SELECT pg_ripple.plan_cache_reset();
```

### GUC Tuning Reference

| Deployment size | `plan_cache_size` | `bgp_reorder` | `parallel_query_min_joins` | `merge_threshold` |
|---|---|---|---|---|
| Small (<1M triples) | 128 | on | 2 | 5,000 |
| Medium (1M–100M triples) | 256 | on | 3 | 10,000 |
| Large (>100M triples) | 512 | on | 3 | 50,000 |
| Analytics (read-heavy) | 1024 | on | 2 | 100,000 |
| OLTP (write-heavy) | 64 | on | 5 | 5,000 |

### Index Strategy Per Workload

| Pattern | Recommended strategy |
|---|---|
| Star patterns (same subject, many predicates) | Ensure ANALYZE has run; `bgp_reorder = on` reorders to start with bound subject |
| Object lookups (find all subjects with object=X) | BRIN on `o` column; `(o, s)` B-tree index already present |
| Named-graph scoped queries | `SET pg_ripple.named_graph_optimized = on` adds `(g, s, o)` index |
| Time-series (monotonic SIDs) | BRIN on `main` partition already covers this |
| Full-text search on literals | `pg_ripple.fts_index('<predicate_iri>')` creates GIN tsvector index |
