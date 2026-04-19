# Performance Tuning

pg_ripple performance depends on three interacting subsystems: the query engine, the write path, and the dictionary cache. This page provides diagnostic steps and tuning recipes for each bottleneck area, with realistic numbers from BSBM benchmarks and internal testing.

---

## The Three Bottleneck Areas

```
┌──────────────────────────────────────────────────────┐
│                   Performance                         │
│                                                      │
│   ┌──────────┐   ┌──────────┐   ┌──────────────┐    │
│   │  Query    │   │  Write   │   │  Cache       │    │
│   │  Engine   │   │  Path    │   │  Pressure    │    │
│   │          │   │          │   │              │    │
│   │ Slow     │   │ Merge    │   │ Dictionary   │    │
│   │ SPARQL   │   │ worker   │   │ misses →     │    │
│   │ queries  │   │ lag,     │   │ table        │    │
│   │          │   │ delta    │   │ lookups      │    │
│   │          │   │ bloat    │   │              │    │
│   └──────────┘   └──────────┘   └──────────────┘    │
└──────────────────────────────────────────────────────┘
```

---

## Diagnostic Workflow

Before tuning, identify which subsystem is the bottleneck:

```sql
-- Step 1: Overall health
SELECT pg_ripple.canary();

-- Step 2: Cache hit rate
SELECT
    (s->>'encode_cache_hits')::bigint AS hits,
    (s->>'encode_cache_misses')::bigint AS misses,
    ROUND(
        (s->>'encode_cache_hits')::numeric /
        NULLIF((s->>'encode_cache_hits')::numeric + (s->>'encode_cache_misses')::numeric, 0),
        4
    ) AS hit_rate
FROM pg_ripple.stats() s;

-- Step 3: Delta accumulation
SELECT (pg_ripple.stats()->>'unmerged_delta_rows')::int AS delta_rows;

-- Step 4: Slowest queries
SELECT calls, mean_exec_time::numeric(10,2) AS avg_ms, LEFT(query, 100)
FROM pg_stat_statements
WHERE query LIKE '%_pg_ripple.vp_%'
ORDER BY mean_exec_time DESC
LIMIT 10;
```

| Symptom | Likely Bottleneck | Section |
|---|---|---|
| High `mean_exec_time` on VP queries | Query engine | [Query Performance](#query-performance) |
| `delta_rows` growing unbounded | Write path / merge | [Write Throughput](#write-throughput) |
| Cache hit rate < 95% | Dictionary cache | [Cache Pressure](#cache-pressure) |
| `merge_worker_pid = 0` | Merge worker not running | [Write Throughput](#write-throughput) |

---

## Query Performance

### Typical Performance Numbers

Based on BSBM benchmarks and internal testing with 10M triples on a 4-core/16GB instance:

| Query Pattern | Typical Latency | Notes |
|---|---|---|
| Simple triple pattern (1 BGP) | 0.5–2ms | Single VP table scan with B-tree |
| Star pattern (3–5 joins, same subject) | 2–10ms | Self-join elimination reduces to 1 scan + joins |
| Path query (3 hops) | 5–20ms | WITH RECURSIVE, bounded depth |
| Complex BGP (5–8 patterns) | 10–50ms | Benefits from `bgp_reorder` |
| Aggregation (COUNT/SUM over 100K rows) | 20–80ms | PostgreSQL native aggregation |
| DESCRIBE (CBD, 50 outgoing arcs) | 5–15ms | Depends on `describe_strategy` |
| Federation (1 SERVICE call) | 50–500ms | Network-dominated |

### Tuning: Slow Single Queries

**Step 1: Get the EXPLAIN output**

```sql
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE {
        ?person <http://schema.org/knows> ?friend .
        ?friend <http://schema.org/name> ?name
    }',
    'text'
);
```

**Step 2: Check for common issues**

| EXPLAIN Pattern | Problem | Fix |
|---|---|---|
| `Seq Scan on vp_rare` | Predicate below promotion threshold | Lower `vp_promotion_threshold` or load more data |
| `Nested Loop` with millions of rows | Poor join order | Verify `bgp_reorder = on`; run `ANALYZE` on VP tables |
| `Sort + Unique` on large result | Unnecessary DISTINCT | Add SHACL `sh:maxCount 1` for functional predicates |
| `CTE Scan` with high loops | Unbounded property path | Lower `max_path_depth`; add FILTER bounds |
| `Hash Join` with large build side | Join on a high-cardinality predicate | Rewrite query to filter the large predicate first |

**Step 3: Enable the plan cache**

```sql
-- Cache compiled SQL for repeated queries
SET pg_ripple.plan_cache_size = 512;
```

The plan cache eliminates parse/optimize/generate overhead for repeated SPARQL patterns. With BSBM's mix of 12 query templates, a cache size of 256 achieves ~98% hit rate.

### Tuning: Overall Query Throughput

For workloads with many concurrent queries:

```ini
# Enable parallel query for complex joins
pg_ripple.parallel_query_min_joins = 2

# PostgreSQL parallel execution
max_parallel_workers_per_gather = 4
max_parallel_workers = 8

# Larger work_mem for complex joins
work_mem = '128MB'
```

```admonish tip title="BGP reordering impact"
On a 10M triple dataset with 5-pattern BGPs, enabling `bgp_reorder` reduces median query time from 45ms to 12ms — a 3.7x improvement. Always keep this on unless you have a specific reason to disable it.
```

---

## Write Throughput

### Typical Write Performance

| Operation | Throughput | Notes |
|---|---|---|
| `insert_triple()` (single) | 5,000–15,000 triples/sec | Per-backend, includes dictionary encoding |
| `load_turtle()` (bulk, inline) | 30,000–80,000 triples/sec | Batch dictionary encoding |
| `load_turtle_file()` (bulk, file) | 50,000–120,000 triples/sec | Streaming from disk, larger batches |
| `sparql_update()` INSERT DATA | 10,000–30,000 triples/sec | SPARQL parse overhead |

### Tuning: Merge Worker Lag

If `unmerged_delta_rows` grows continuously, the merge worker cannot keep up with the write rate.

**Diagnosis:**

```sql
-- Check delta accumulation
SELECT (pg_ripple.stats()->>'unmerged_delta_rows')::int AS delta;
-- Run again 60 seconds later — if delta is growing, merges are lagging
```

**Solutions (in order of impact):**

1. **Lower merge_threshold** — Merge smaller batches more frequently:
   ```sql
   ALTER SYSTEM SET pg_ripple.merge_threshold = 5000;
   SELECT pg_reload_conf();
   ```

2. **Increase merge frequency** — Reduce polling interval:
   ```sql
   ALTER SYSTEM SET pg_ripple.merge_interval_secs = 15;
   SELECT pg_reload_conf();
   ```

3. **Manual compaction** — Force an immediate merge:
   ```sql
   SELECT pg_ripple.compact();
   ```

4. **Separate write windows** — Batch writes during off-peak hours, then compact.

### Tuning: Bulk Load Performance

For large initial data loads:

```sql
-- Temporarily disable SHACL validation
SET pg_ripple.shacl_mode = 'off';

-- Use file-based loading for best throughput
SELECT pg_ripple.load_turtle_file('/data/large_dataset.ttl');

-- Re-enable validation
SET pg_ripple.shacl_mode = 'async';

-- Force merge to move data to main tables
SELECT pg_ripple.compact();
```

```admonish info title="Cache back-pressure"
During bulk loads, pg_ripple monitors cache utilization against `cache_budget`. When utilization exceeds 90%, batch sizes are automatically reduced to prevent out-of-memory conditions. If you see slower-than-expected bulk loads, check `encode_cache_utilization_pct` in `stats()`.
```

---

## Cache Pressure

### Diagnosis

```sql
SELECT
    (s->>'encode_cache_capacity')::int AS capacity,
    (s->>'encode_cache_utilization_pct')::int AS util_pct,
    (s->>'encode_cache_hits')::bigint AS hits,
    (s->>'encode_cache_misses')::bigint AS misses,
    (s->>'encode_cache_evictions')::bigint AS evictions
FROM pg_ripple.stats() s;
```

| Metric | Healthy | Action Needed |
|---|---|---|
| Hit rate > 95% | Normal operation | None |
| Hit rate 90–95% | Marginal | Consider increasing cache |
| Hit rate < 90% | Cache thrashing | Increase `dictionary_cache_size` |
| Utilization > 90% | Near-full | Increase `cache_budget` |
| Evictions > 10% of hits | High churn | Working set exceeds cache |

### Sizing the Dictionary Cache

Rule of thumb: the cache should hold at least 80% of your unique terms.

```sql
-- Count unique terms
SELECT count(*) AS unique_terms FROM _pg_ripple.dictionary;
```

| Unique Terms | Recommended `dictionary_cache_size` | Memory (approx.) |
|---|---|---|
| < 50K | 8,192 | ~2 MB |
| 50K – 500K | 65,536 | ~13 MB |
| 500K – 5M | 262,144 | ~50 MB |
| 5M – 50M | 500,000 | ~100 MB |
| > 50M | 1,000,000 (max) | ~200 MB |

```admonish warning title="Restart required"
Changing `dictionary_cache_size` requires a PostgreSQL restart because shared memory is allocated at postmaster start. Plan your cache sizing during initial deployment.
```

---

## Workload-Specific Recipes

### Read-Heavy Analytics

Optimized for complex SPARQL queries with rare writes:

```ini
# Large plan cache for diverse query shapes
pg_ripple.plan_cache_size = 2048

# BGP optimization
pg_ripple.bgp_reorder = on
pg_ripple.parallel_query_min_joins = 2

# Large dictionary cache
pg_ripple.dictionary_cache_size = 262144
pg_ripple.cache_budget = 256

# Infrequent merges (writes are rare)
pg_ripple.merge_threshold = 100000
pg_ripple.merge_interval_secs = 300

# PostgreSQL
shared_buffers = '4GB'
effective_cache_size = '12GB'
work_mem = '256MB'
random_page_cost = 1.1
```

Expected: P95 query latency < 50ms for 5-pattern BGPs on 10M triples.

### Write-Heavy Ingestion

Optimized for continuous data ingestion with periodic queries:

```ini
# Smaller plan cache (fewer distinct queries)
pg_ripple.plan_cache_size = 64

# Aggressive merging to keep delta small
pg_ripple.merge_threshold = 5000
pg_ripple.merge_interval_secs = 10
pg_ripple.latch_trigger_threshold = 2000
pg_ripple.auto_analyze = on

# Large cache to handle encoding pressure
pg_ripple.dictionary_cache_size = 500000
pg_ripple.cache_budget = 512

# Disable SHACL during ingestion
pg_ripple.shacl_mode = 'off'

# PostgreSQL — optimize for writes
shared_buffers = '2GB'
wal_buffers = '64MB'
checkpoint_completion_target = 0.9
max_wal_size = '4GB'
```

Expected: Sustained ingestion at 50K+ triples/sec with merge lag < 30 seconds.

### Mixed HTAP (Read + Write)

Balanced for concurrent queries and writes:

```ini
# Moderate plan cache
pg_ripple.plan_cache_size = 512

# Balanced merge — not too frequent, not too lazy
pg_ripple.merge_threshold = 25000
pg_ripple.merge_interval_secs = 30
pg_ripple.latch_trigger_threshold = 10000
pg_ripple.auto_analyze = on

# Good cache coverage
pg_ripple.dictionary_cache_size = 131072
pg_ripple.cache_budget = 128

# Async SHACL so writes are not blocked
pg_ripple.shacl_mode = 'async'

# BGP optimization for read queries
pg_ripple.bgp_reorder = on

# PostgreSQL
shared_buffers = '4GB'
effective_cache_size = '12GB'
work_mem = '128MB'
max_parallel_workers_per_gather = 2
```

Expected: Read P95 < 30ms, write throughput > 20K triples/sec, merge lag < 60 seconds.

---

## Benchmarking Your Deployment

Use the built-in `compact()` function and `pg_stat_statements` to establish baselines:

```sql
-- Reset statistics
SELECT pg_stat_statements_reset();

-- Run your workload (queries, inserts, etc.)

-- Collect results
SELECT
    calls,
    mean_exec_time::numeric(10,2) AS avg_ms,
    stddev_exec_time::numeric(10,2) AS stddev_ms,
    min_exec_time::numeric(10,2) AS min_ms,
    max_exec_time::numeric(10,2) AS max_ms,
    rows,
    LEFT(query, 80) AS query_prefix
FROM pg_stat_statements
WHERE query LIKE '%_pg_ripple%'
ORDER BY total_exec_time DESC
LIMIT 20;
```

```admonish tip title="Iterative tuning"
Change one parameter at a time, re-run your benchmark, and compare. The most impactful parameters in order are:
1. `dictionary_cache_size` (cache hit rate)
2. `bgp_reorder` (query planning)
3. `merge_threshold` (read freshness vs. write throughput)
4. `plan_cache_size` (repeated query overhead)
5. PostgreSQL `work_mem` (complex join performance)
```
