# Federation Performance

This page covers practical strategies for getting the best performance from SPARQL `SERVICE` queries in pg_ripple.

## Choosing a cache TTL

`pg_ripple.federation_cache_ttl` controls how long remote results are reused before the endpoint is re-queried. The right value depends on how quickly the source data changes.

| Data type | Suggested TTL |
|---|---|
| Slowly-changing reference data (Wikidata labels, DBpedia categories) | 3600–86400 seconds (1 hour to 1 day) |
| Daily batch data (published reports, snapshots) | 3600 seconds (1 hour) |
| Near-real-time data (news, stock prices) | 0 (disabled) |
| Highly dynamic data (sensor streams) | 0 (disabled) |

```sql
-- Cache Wikidata labels for 1 hour
SET pg_ripple.federation_cache_ttl = 3600;

-- Inspect cache hit rate (requires logging extension)
SELECT url,
       COUNT(*) AS rows_cached,
       MIN(cached_at) AS oldest_entry,
       MAX(expires_at) AS latest_expiry
FROM _pg_ripple.federation_cache
GROUP BY url;
```

**Tip:** Set `federation_cache_ttl` at the session level before running batch federation jobs. Reset it to 0 for interactive queries where freshness matters.

## Setting complexity hints

When a single query contacts multiple endpoints, set complexity hints so fast endpoints run first. This reduces total wall-clock time because:

1. Fast endpoints resolve early, binding variables that may prune later patterns.
2. Failures at slow endpoints are detected sooner.

```sql
SELECT pg_ripple.register_endpoint('https://fast-mirror.example.com/sparql', NULL, 'fast');
SELECT pg_ripple.register_endpoint('https://main-kb.example.com/sparql', NULL, 'normal');
SELECT pg_ripple.register_endpoint('https://archive.example.com/sparql', NULL, 'slow');
```

Or update after registration:

```sql
SELECT pg_ripple.set_endpoint_complexity('https://archive.example.com/sparql', 'slow');
```

View current hints:

```sql
SELECT url, complexity, enabled
FROM pg_ripple.list_endpoints()
ORDER BY complexity, url;
```

## Designing queries for variable projection

pg_ripple automatically sends `SELECT ?v1 ?v2 … WHERE { … }` instead of `SELECT *` to remote endpoints. For maximum data reduction, write patterns that bind only the variables your outer query needs:

```sparql
-- Less efficient: inner pattern binds ?s, ?p, ?o, ?label, ?type, ?comment
-- but outer query only needs ?label
SERVICE <https://kb.example.com/sparql> {
  ?s ?p ?o .
  ?s <rdfs:label> ?label .
  ?s <rdf:type> ?type .
  ?s <rdfs:comment> ?comment .
}

-- More efficient: inner pattern binds only ?label
SERVICE <https://kb.example.com/sparql> {
  ?s <rdfs:label> ?label .
}
```

Even if the remote endpoint does not honour projection (returning all columns anyway), the explicit projection reduces the size of the inline VALUES clause injected into the local SQL query.

## Monitoring with federation_health

The `_pg_ripple.federation_health` table records every SERVICE call outcome. Use it to identify slow or flaky endpoints:

```sql
-- Latency percentiles per endpoint over the last hour
SELECT url,
       PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY latency_ms) AS p50_ms,
       PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95_ms,
       PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY latency_ms) AS p99_ms,
       COUNT(*) AS total_calls,
       ROUND(100.0 * AVG(CASE WHEN success THEN 1.0 ELSE 0.0 END), 1) AS success_pct
FROM _pg_ripple.federation_health
WHERE probed_at >= now() - INTERVAL '1 hour'
GROUP BY url
ORDER BY p95_ms DESC;
```

Use `pg_ripple.federation_adaptive_timeout = on` to automatically tighten timeouts for fast endpoints and give slow ones more headroom:

```sql
SET pg_ripple.federation_adaptive_timeout = on;
-- Effective timeout = max(1s, p95_latency * 3).
-- A 200ms p95 endpoint gets a 0.6s timeout (floored to 1s).
-- A 5000ms p95 endpoint gets a 15s timeout.
```

## Monitoring with federation_cache

```sql
-- See which queries are being cached
SELECT url,
       query_hash,
       pg_size_pretty(octet_length(result_jsonb::text)) AS result_size,
       cached_at,
       expires_at,
       CASE WHEN expires_at > now() THEN 'active' ELSE 'expired' END AS status
FROM _pg_ripple.federation_cache
ORDER BY cached_at DESC;
```

Expired rows are cleaned up automatically by the merge background worker. To evict immediately:

```sql
DELETE FROM _pg_ripple.federation_cache WHERE expires_at <= now();
```

## Sidecar vs in-process tradeoffs

The pg_ripple_http sidecar (`pg_ripple_http/`) executes federation requests in an async Tokio runtime, enabling true parallel HTTP within a single query. The in-process SPI path (this page) is sequential.

| Approach | Latency | Concurrency | Setup |
|---|---|---|---|
| In-process SPI (default) | +1–5ms per call overhead | Sequential | None |
| pg_ripple_http sidecar | ~0 overhead, async | Parallel | Deploy sidecar binary |

For workloads with 3+ independent SERVICE clauses, the sidecar provides significant speedup. For 1–2 clauses or when the batch detection optimisation applies (same endpoint), the in-process path is sufficient.

## Connection pooling tips

The thread-local connection pool (`federation_pool_size`) reuses TCP and TLS connections across multiple SERVICE calls in the same backend session. Each PostgreSQL backend has its own pool.

```sql
-- Increase pool size for sessions that query many endpoints
SET pg_ripple.federation_pool_size = 16;

-- Keep at 1 for single-use batch jobs to reduce memory usage
SET pg_ripple.federation_pool_size = 1;
```

**Note:** The pool is created on first use in a session and not recreated when `federation_pool_size` changes. For the new setting to take effect, start a new session.
