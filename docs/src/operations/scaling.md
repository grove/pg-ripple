# Scaling

pg_ripple scales vertically within a single PostgreSQL instance and horizontally for read traffic via streaming replication. This page covers how to allocate resources, tune the merge worker, set up read replicas, and understand current limitations.

```admonish info title="Current scaling model"
pg_ripple runs entirely within PostgreSQL. It inherits PostgreSQL's single-writer architecture: one primary handles all writes, and read replicas serve read-only SPARQL queries. Horizontal sharding across multiple workers is available via the [Citus integration](citus-sharding.md) (v0.58.0+).
```

---

## Vertical Scaling

The most impactful scaling lever is giving your single PostgreSQL instance more resources.

### Memory

Memory affects three key areas:

| Resource | Controlled By | Impact |
|---|---|---|
| Dictionary LRU cache | `pg_ripple.dictionary_cache_size` | Reduces disk I/O for IRI/literal lookups. Every SPARQL query touches the dictionary on decode. |
| PostgreSQL shared buffers | `shared_buffers` | Caches VP table pages. Larger = fewer disk reads for joins. |
| Work memory | `work_mem` | Memory for sorts, hash joins, and hash aggregates in SPARQL-generated SQL. |

#### Dictionary Cache Sizing

The dictionary cache is allocated in shared memory at server startup. Each entry consumes approximately 200 bytes.

```sql
-- Check current utilization
SELECT
  s->>'encode_cache_capacity' AS capacity,
  s->>'encode_cache_utilization_pct' AS utilization_pct,
  ROUND(
    (s->>'encode_cache_hits')::numeric /
    NULLIF((s->>'encode_cache_hits')::numeric + (s->>'encode_cache_misses')::numeric, 0),
    4
  ) AS hit_rate
FROM pg_ripple.stats() s;
```

| Hit Rate | Action |
|---|---|
| > 95% | Healthy — no change needed |
| 90–95% | Consider increasing `dictionary_cache_size` |
| < 90% | Double `dictionary_cache_size` and restart |

```admonish tip title="Rule of thumb"
Set `dictionary_cache_size` to at least 10% of your total unique IRIs + literals. For a dataset with 5M unique terms, start with 500K entries (~100 MB of shared memory).
```

#### PostgreSQL Memory Settings

```ini
# postgresql.conf — for a 64 GB server with pg_ripple as the primary workload
shared_buffers = 16GB
effective_cache_size = 48GB
work_mem = 256MB
maintenance_work_mem = 2GB
```

```admonish warning title="work_mem and SPARQL"
Complex SPARQL queries with multiple joins, UNIONs, or aggregates can spawn many hash operations. PostgreSQL allocates `work_mem` **per operation per query**. Start conservative (64MB–256MB) and increase if you see "temporary file" entries in the logs.
```

### CPU

| Workload | CPU Benefit |
|---|---|
| SPARQL query execution | More cores → more parallel workers for large joins |
| Merge worker | Single-threaded per predicate, but merges run concurrently across predicates |
| Bulk loading | `load_turtle` / `load_ntriples` are I/O-bound; CPU helps with dictionary encoding |
| Datalog inference | Semi-naive fixpoint is CPU-intensive; benefits from faster cores |

Set `max_parallel_workers_per_gather` to allow PostgreSQL to parallelize large VP table scans:

```ini
max_parallel_workers_per_gather = 4
max_parallel_workers = 8
parallel_setup_cost = 100
parallel_tuple_cost = 0.001
```

pg_ripple's `parallel_query_min_joins` GUC controls when the SPARQL engine enables parallel hints in generated SQL (default: 3 joins).

### Storage

| Tier | Recommendation |
|---|---|
| NVMe SSD | Best for all workloads. Random I/O for dictionary lookups and VP table joins. |
| SATA SSD | Acceptable for medium datasets. |
| HDD | Not recommended. Dictionary lookups and VP joins are random-I/O heavy. |

```admonish tip title="Separate WAL and data"
Place `pg_wal` on a separate NVMe device from the main data directory. pg_ripple's bulk load and merge operations generate significant WAL traffic.
```

---

## Merge Worker Tuning

The HTAP merge worker is the most important pg_ripple-specific scaling knob. It controls how quickly delta rows (recent writes) are consolidated into the main BRIN-indexed partition.

### How the Merge Worker Operates

1. The worker polls every `merge_interval_secs` (default: 60s)
2. For each predicate, it checks if `delta row count >= merge_threshold`
3. If yes, it creates a new main table: `(old main − tombstones) UNION ALL delta`
4. It swaps the view to point at the new main, then drops the old main after `merge_retention_seconds`
5. If `auto_analyze` is on, it runs `ANALYZE` on the new main

### Tuning for Write-Heavy Workloads

Lower the merge threshold and interval to keep the delta tables small:

```ini
pg_ripple.merge_threshold = 5000
pg_ripple.merge_interval_secs = 30
pg_ripple.latch_trigger_threshold = 5000
```

This gives fresher reads but increases I/O from more frequent merges.

### Tuning for Read-Heavy Workloads

Raise the threshold to batch more writes before merging:

```ini
pg_ripple.merge_threshold = 50000
pg_ripple.merge_interval_secs = 120
```

This reduces merge I/O overhead but means queries scan larger delta tables.

### Monitoring Merge Activity

```sql
-- Is the merge worker running?
SELECT (pg_ripple.stats()->>'merge_worker_pid')::int AS pid;

-- How many unmerged delta rows?
SELECT (pg_ripple.stats()->>'unmerged_delta_rows')::int AS delta_rows;
```

```admonish warning title="Merge worker stalls"
If `unmerged_delta_rows` grows continuously while `merge_worker_pid` is non-zero, the worker may be stuck. Check `pg_stat_activity` for long-running merge transactions and look for lock contention. The `merge_watchdog_timeout` GUC (default: 300s) logs a WARNING if the worker is idle too long.
```

---

## Read Replicas

PostgreSQL streaming replication provides horizontal read scaling for SPARQL queries.

### Architecture

```
┌────────────┐     WAL stream     ┌────────────┐
│  Primary   │ ──────────────────→ │  Replica 1 │ ← SPARQL reads
│  (writes)  │                     └────────────┘
│            │     WAL stream     ┌────────────┐
│            │ ──────────────────→ │  Replica 2 │ ← SPARQL reads
└────────────┘                     └────────────┘
```

### Setting Up a Read Replica

On the primary:

```ini
# postgresql.conf
wal_level = replica
max_wal_senders = 5
wal_keep_size = 1GB
```

Create a replication slot:

```sql
SELECT pg_create_physical_replication_slot('replica1');
```

On the replica:

```bash
pg_basebackup -h primary-host -D /var/lib/postgresql/18/main -R -S replica1 -P
```

Start the replica — it will begin streaming WAL and replaying changes, including all VP table mutations.

### Replica Considerations

```admonish note title="Merge worker does not run on replicas"
The background merge worker only runs on the primary. Replicas receive already-merged state through WAL replay. This means replicas always have a consistent view of the data without any additional overhead.
```

- **SPARQL queries work identically** on replicas — the query engine reads VP tables the same way
- **Dictionary cache** is independent per instance — each replica maintains its own LRU cache
- **Replication lag**: monitor with `pg_stat_replication` on the primary. Under normal load, lag should be sub-second
- **Hot standby conflicts**: long-running SPARQL queries on replicas may conflict with WAL replay. Set `max_standby_streaming_delay` appropriately:

```ini
# On the replica
max_standby_streaming_delay = 30s
hot_standby_feedback = on
```

---

## Connection Pooling

For workloads with many concurrent SPARQL clients, use a connection pooler:

```admonish tip title="PgBouncer with pg_ripple"
pg_ripple uses session-level GUC parameters (e.g., `pg_ripple.inference_mode`). If you use PgBouncer, configure it in **session** pooling mode, not transaction mode. Transaction-mode pooling resets GUCs between transactions, which can cause unexpected behavior.
```

```ini
# pgbouncer.ini
[databases]
mydb = host=127.0.0.1 port=5432 dbname=mydb

[pgbouncer]
pool_mode = session
max_client_conn = 200
default_pool_size = 20
```

---

## Scaling Limits and Honest Boundaries

| Dimension | Current Capability | Limitation |
|---|---|---|
| Triples per instance | Tested to 1B+ | Bound by disk and memory |
| Concurrent SPARQL queries | Hundreds (with pooler) | Bound by `max_connections` and CPU |
| Write throughput | ~50K–200K triples/sec (bulk load) | Single-writer architecture |
| Read replicas | Unlimited | Standard PG replication |
| Cross-node sharding | **Supported** (Citus 12+, v0.58.0+) | Subject-hash distribution; see [Citus sharding](citus-sharding.md) |
| Multi-primary writes | **Not supported** | PostgreSQL limitation |
| Federation | Supported (SERVICE clause) | Remote endpoints add latency |

```admonish tip title="Horizontal sharding with Citus"
pg_ripple v0.58.0+ supports distributing VP tables across Citus worker nodes. Triples are hash-sharded by subject ID so star-pattern queries co-locate on a single worker. v0.59.0 adds SPARQL shard-pruning (10–100× speedup for bound-subject queries). See [Citus Horizontal Sharding](citus-sharding.md) for setup instructions.
```

---

## Capacity Planning

### Storage Estimates

| Component | Per Triple (approx.) |
|---|---|
| VP table row (s, o, g, i, source) | ~40 bytes |
| VP indexes (dual B-tree) | ~80 bytes |
| Dictionary entry (per unique term) | ~120 bytes |
| HTAP overhead (delta + tombstone tables) | ~20% of VP size during active writes |

**Example**: 100M triples with 20M unique terms ≈ 12 GB (VP) + 2.4 GB (dictionary) + overhead ≈ **~20 GB** total.

### Memory Estimates

| Component | Sizing |
|---|---|
| `shared_buffers` | 25% of RAM |
| `dictionary_cache_size` | 10% of unique terms |
| `work_mem` | 64MB–512MB depending on query complexity |
| OS page cache | Remaining RAM |

```admonish tip title="Start small, measure, scale"
Deploy with conservative settings, load your data, and run representative queries. Use `pg_ripple.stats()` and PostgreSQL's `pg_stat_user_tables` to identify bottlenecks before adding hardware.
```
