# Parallel Merge Worker Pool

> Added in v0.42.0

## Overview

pg_ripple uses a **Vertical Partitioning (VP) architecture** where each unique predicate gets its own storage table. The merge worker pool keeps the read-optimised `_main` partitions in sync with the write-optimised `_delta` tables.

By default, a single background worker handles all predicates sequentially. For workloads with many distinct predicates — such as rich ontologies with 50+ property types — a pool of parallel workers can significantly improve write throughput.

## Configuration

### `pg_ripple.merge_workers` (startup only)

Controls the number of parallel merge worker processes. Must be set in `postgresql.conf` or before the server starts; it cannot be changed with `SET` at session level.

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'
pg_ripple.merge_workers = 4
```

- **Default**: `1` (single worker, original behaviour)
- **Range**: `1` to `16`
- **Type**: `integer`, `PGC_POSTMASTER` (startup-only)

### `pg_ripple.merge_threshold`

Minimum rows in a VP delta table before a merge is triggered. Increasing this reduces merge frequency but increases per-merge cost.

```sql
SET pg_ripple.merge_threshold = 50000;  -- default: 10000
```

### `pg_ripple.merge_interval_secs`

Maximum seconds between merge worker polling cycles.

```sql
SET pg_ripple.merge_interval_secs = 30;  -- default: 60
```

## How It Works

With `merge_workers = N`, pg_ripple spawns N background worker processes. Each worker owns a disjoint round-robin subset of VP predicates:

- **Worker 0** handles predicates where `pred_id % N == 0`
- **Worker 1** handles predicates where `pred_id % N == 1`
- … and so on

**Advisory locking** prevents races: before merging a predicate, a worker calls `pg_try_advisory_lock(pred_id)`. If another worker already holds the lock, it skips that predicate.

**Work-stealing**: after processing its assigned predicates, an idle worker checks whether any "foreign" predicate (not in its round-robin slice) has a delta table above the merge threshold and no lock held. If so, it steals that work. This prevents a single overloaded predicate from delaying the merge cycle.

## Monitoring

Use `pg_ripple.diagnostic_report()` to check merge worker activity:

```sql
SELECT value FROM pg_ripple.diagnostic_report()
WHERE key LIKE 'merge_%';
```

Or query the background worker state:

```sql
SELECT pid, application_name, state
FROM pg_stat_activity
WHERE application_name LIKE 'pg_ripple merge%';
```

## Choosing the Right Worker Count

| Predicate count | Recommended workers |
|---|---|
| < 20 | 1 (default) |
| 20–100 | 2–4 |
| 100–500 | 4–8 |
| > 500 | 8–16 |

For most workloads, the bottleneck is not the worker count but the merge threshold and interval. Tune those first before scaling workers.

## Restart Requirement

Because `merge_workers` is a `PGC_POSTMASTER` GUC, changes take effect only after a PostgreSQL restart:

```bash
# After updating postgresql.conf:
pg_ctl restart -D $PGDATA
```
