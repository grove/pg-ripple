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
