# Monitoring and Observability

pg_ripple provides built-in monitoring through SQL functions, PostgreSQL's standard statistics infrastructure, and Prometheus-compatible metrics via `pg_ripple_http`. This page explains what to monitor, how to collect the data, and what thresholds indicate a healthy system.

---

## pg_ripple.stats()

The primary monitoring function. Returns a JSONB object with key metrics:

```sql
SELECT pg_ripple.stats();
```

### Output Fields

| Field | Type | Description |
|---|---|---|
| `total_triples` | int | Total triple count across all graphs (including delta rows not yet merged) |
| `dedicated_predicates` | int | Number of predicates with their own VP table |
| `htap_predicates` | int | Number of predicates using the HTAP delta/main split |
| `rare_triples` | int | Triples stored in the consolidated `vp_rare` table |
| `unmerged_delta_rows` | int | Total rows across all delta tables (from shared memory counter). `-1` if shared memory is not available |
| `merge_worker_pid` | int | PID of the background merge worker. `0` if not running |
| `live_statistics_enabled` | bool | Whether pg_trickle live statistics are active |
| `encode_cache_capacity` | int | Total entries the shared encode cache can hold |
| `encode_cache_utilization_pct` | int | Percentage of cache slots currently in use |
| `encode_cache_hits` | int | Cumulative cache hit count since server start |
| `encode_cache_misses` | int | Cumulative cache miss count since server start |
| `encode_cache_evictions` | int | Cumulative eviction count |

### Example Output

```json
{
  "total_triples": 4523891,
  "dedicated_predicates": 127,
  "htap_predicates": 127,
  "rare_triples": 2341,
  "unmerged_delta_rows": 8432,
  "merge_worker_pid": 12345,
  "live_statistics_enabled": false,
  "encode_cache_capacity": 65536,
  "encode_cache_utilization_pct": 72,
  "encode_cache_hits": 18934521,
  "encode_cache_misses": 234012,
  "encode_cache_evictions": 45123
}
```

### Computing the Cache Hit Rate

```sql
SELECT
    (s->>'encode_cache_hits')::bigint AS hits,
    (s->>'encode_cache_misses')::bigint AS misses,
    ROUND(
        (s->>'encode_cache_hits')::numeric /
        NULLIF((s->>'encode_cache_hits')::numeric + (s->>'encode_cache_misses')::numeric, 0),
        4
    ) AS hit_rate
FROM pg_ripple.stats() s;
```

```admonish warning title="Cache hit rate threshold"
A healthy system should maintain a cache hit rate above **95%** (0.95). If it drops below 90%, increase `pg_ripple.dictionary_cache_size` and restart PostgreSQL. Sustained rates below 80% indicate the working set significantly exceeds cache capacity.
```

---

## pg_ripple.canary()

A health check function that returns a JSONB object with pass/fail indicators:

```sql
SELECT pg_ripple.canary();
```

### Output Fields

| Field | Type | Healthy Value | Description |
|---|---|---|---|
| `merge_worker` | text | `"ok"` | `"ok"` if merge worker PID is in shared memory; `"stalled"` otherwise |
| `cache_hit_rate` | float | > 0.95 | Dictionary encode cache hit rate (0.0–1.0) |
| `catalog_consistent` | bool | `true` | VP table count in `pg_class` matches promoted predicates |
| `orphaned_rare_rows` | int | `0` | `vp_rare` rows for predicates that already have dedicated VP tables |

### Interpreting Results

```sql
SELECT
    c->>'merge_worker' AS worker,
    (c->>'cache_hit_rate')::float AS hit_rate,
    (c->>'catalog_consistent')::bool AS catalog_ok,
    (c->>'orphaned_rare_rows')::int AS orphaned
FROM pg_ripple.canary() c;
```

```admonish tip title="Use canary() for automated health checks"
`canary()` is designed for load balancer health checks and monitoring systems. Call it periodically and alert when `merge_worker = 'stalled'`, `cache_hit_rate < 0.90`, or `catalog_consistent = false`.
```

---

## SPARQL Query Analysis with sparql_explain()

Analyze SPARQL query performance using the explain functions:

### Basic SQL Generation

```sql
-- See the generated SQL without executing
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?s <http://schema.org/name> ?name }',
    false
);
```

### Full EXPLAIN ANALYZE

```sql
-- Execute and show timing + row counts
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?s <http://schema.org/name> ?name }',
    true
);
```

### explain_sparql() with Format Options

The `explain_sparql()` function (v0.23.0) provides more output formats:

```sql
-- Generated SQL only
SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?o WHERE { ?s <http://xmlns.com/foaf/0.1/knows> ?o }',
    'sql'
);

-- EXPLAIN ANALYZE as text (default)
SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?o WHERE { ?s <http://xmlns.com/foaf/0.1/knows> ?o }',
    'text'
);

-- EXPLAIN ANALYZE as JSON (for programmatic consumption)
SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?o WHERE { ?s <http://xmlns.com/foaf/0.1/knows> ?o }',
    'json'
);

-- SPARQL algebra tree (for debugging the optimizer)
SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?o WHERE { ?s <http://xmlns.com/foaf/0.1/knows> ?o }',
    'sparql_algebra'
);
```

```admonish info title="What to look for in EXPLAIN output"
- **Seq Scan on vp_rare** — A predicate is not promoted yet. Consider lowering `vp_promotion_threshold` or loading more data.
- **Nested Loop with high row estimates** — BGP reordering may not be optimal. Check `bgp_reorder` is on.
- **Recursive CTE with high loop count** — Property path is deep. Check `max_path_depth` setting.
- **Sort + Unique** — A `DISTINCT` that might be avoidable with SHACL `sh:maxCount 1` hints.
```

---

## pg_stat_statements Integration

pg_ripple generates standard SQL that is tracked by `pg_stat_statements`. This gives you deep visibility into the actual SQL performance:

```sql
-- Enable pg_stat_statements (if not already)
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Find the slowest SPARQL-generated queries
SELECT
    calls,
    mean_exec_time::numeric(10,2) AS avg_ms,
    total_exec_time::numeric(10,2) AS total_ms,
    rows,
    LEFT(query, 120) AS query_prefix
FROM pg_stat_statements
WHERE query LIKE '%_pg_ripple.vp_%'
ORDER BY mean_exec_time DESC
LIMIT 20;
```

### Identifying Hot VP Tables

```sql
-- Which VP tables are scanned most?
SELECT
    regexp_matches(query, '_pg_ripple\.(vp_\d+)', 'g') AS vp_table,
    sum(calls) AS total_calls,
    sum(total_exec_time)::numeric(10,2) AS total_ms
FROM pg_stat_statements
WHERE query LIKE '%_pg_ripple.vp_%'
GROUP BY 1
ORDER BY total_ms DESC
LIMIT 10;
```

---

## Prometheus Metrics (pg_ripple_http)

The `pg_ripple_http` companion service exposes Prometheus-compatible metrics at the `/metrics` endpoint:

```bash
curl http://localhost:7878/metrics
```

### Available Metrics

| Metric | Type | Description |
|---|---|---|
| `pg_ripple_http_queries_total` | counter | Total SPARQL queries processed |
| `pg_ripple_http_errors_total` | counter | Total query errors |
| `pg_ripple_http_query_duration_seconds_total` | counter | Cumulative query execution time |

### Prometheus Scrape Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'pg_ripple_http'
    scrape_interval: 15s
    static_configs:
      - targets: ['pg-ripple-http:7878']
    metrics_path: /metrics
```

### Derived Metrics for Dashboards

Use PromQL to compute useful rates:

```promql
# Queries per second
rate(pg_ripple_http_queries_total[5m])

# Error rate
rate(pg_ripple_http_errors_total[5m]) / rate(pg_ripple_http_queries_total[5m])

# Average query latency
rate(pg_ripple_http_query_duration_seconds_total[5m]) / rate(pg_ripple_http_queries_total[5m])
```

---

## Monitoring the Merge Worker

The background merge worker is critical for HTAP performance. Monitor it through multiple channels:

### Shared Memory Status

```sql
-- Is the merge worker running?
SELECT (pg_ripple.stats()->>'merge_worker_pid')::int AS pid;
-- Returns 0 if not running
```

### Delta Table Sizes

```sql
-- Check delta accumulation per predicate
SELECT
    p.id AS predicate_id,
    d.value AS predicate_iri,
    p.triple_count,
    (SELECT count(*) FROM format('_pg_ripple.vp_%s_delta', p.id)::regclass) AS delta_rows
FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
WHERE p.htap = true
ORDER BY p.triple_count DESC
LIMIT 10;
```

### Merge Worker Logs

The merge worker logs to PostgreSQL's standard log:

```
LOG:  pg_ripple merge worker: merge cycle complete
LOG:  pg_ripple merge worker: processed 3 async validation item(s)
WARNING:  pg_ripple merge worker: watchdog timeout (300s)
```

```admonish warning title="Watchdog timeout"
If you see `watchdog timeout` warnings in the PostgreSQL log, the merge worker has stalled. Common causes:
- Long-running transactions holding locks on VP tables
- `worker_database` pointing to the wrong database
- Insufficient `max_worker_processes` in `postgresql.conf`
```

---

## Health Check Thresholds

Use these thresholds for alerting:

| Metric | Green | Yellow | Red |
|---|---|---|---|
| Cache hit rate | > 95% | 90–95% | < 90% |
| Merge worker PID | > 0 | — | = 0 |
| Delta rows (total) | < 2× merge_threshold | 2–5× | > 5× |
| Catalog consistent | true | — | false |
| Orphaned rare rows | 0 | 1–100 | > 100 |
| Query error rate | < 1% | 1–5% | > 5% |
| Avg query latency | < 100ms | 100–500ms | > 500ms |

### Automated Monitoring Query

Run this periodically from your monitoring system:

```sql
SELECT
    CASE
        WHEN (c->>'merge_worker') = 'ok'
             AND (c->>'cache_hit_rate')::float > 0.90
             AND (c->>'catalog_consistent')::bool
             AND (c->>'orphaned_rare_rows')::int = 0
        THEN 'healthy'
        WHEN (c->>'merge_worker') = 'stalled'
             OR (c->>'cache_hit_rate')::float < 0.80
        THEN 'critical'
        ELSE 'warning'
    END AS status,
    c->>'merge_worker' AS worker,
    c->>'cache_hit_rate' AS hit_rate,
    c->>'catalog_consistent' AS catalog,
    c->>'orphaned_rare_rows' AS orphaned
FROM pg_ripple.canary() c;
```

---

## Predicate Inventory

Monitor predicate distribution to catch imbalances:

```sql
SELECT
    p.id,
    d.value AS predicate_iri,
    p.triple_count,
    p.table_oid IS NOT NULL AS has_vp_table,
    CASE WHEN p.htap THEN 'htap' ELSE 'flat' END AS storage_mode
FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
ORDER BY p.triple_count DESC
LIMIT 20;
```

```admonish tip title="Skewed predicates"
If one predicate has 10x more triples than the next, its VP table dominates storage and merge time. Consider partitioning the data by named graph or filtering queries to avoid full scans of that predicate.
```

---

## Log-Based Monitoring

Configure PostgreSQL logging for SPARQL workload visibility:

```ini
# postgresql.conf
log_min_duration_statement = 500    # Log queries slower than 500ms
log_statement = 'none'              # Don't log every statement
log_line_prefix = '%t [%p] %d '     # Timestamp, PID, database
```

SPARQL-generated SQL appears in the PostgreSQL log with VP table references, making it easy to correlate slow log entries with specific SPARQL patterns.
