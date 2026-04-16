# SPARQL Federation

SPARQL federation lets a single query combine data from pg_ripple with data stored at external SPARQL endpoints. Use the `SERVICE` keyword to delegate part of your query to a remote endpoint.

## Quick start

```sql
-- 1. Register a remote endpoint (required for SSRF protection)
SELECT pg_ripple.register_endpoint('https://query.wikidata.org/sparql');

-- 2. Query across local and remote data
SELECT result->>'local_s' AS local_subject,
       result->>'remote_o' AS remote_label
FROM pg_ripple.sparql($$
  SELECT ?local_s ?remote_o WHERE {
    ?local_s <https://example.org/sameAs> ?wikidata_item .
    SERVICE <https://query.wikidata.org/sparql> {
      ?wikidata_item <http://www.w3.org/2000/01/rdf-schema#label> ?remote_o .
      FILTER(LANG(?remote_o) = "en")
    }
  }
$$);
```

## SERVICE clause syntax

```sparql
SERVICE <endpoint-url> { ... graph pattern ... }
SERVICE SILENT <endpoint-url> { ... }
SERVICE ?var { ... }  -- variable endpoint (requires VALUES binding)
```

- **`SERVICE <url> { … }`** — execute the inner pattern at the remote SPARQL endpoint. Raises an ERROR if the call fails (unless `federation_on_error = 'empty'`).
- **`SERVICE SILENT <url> { … }`** — same, but silently returns empty results on failure. A WARNING is still logged.
- **`SERVICE ?var { … }` with `VALUES`** — bind the endpoint URL to a variable, allowing dynamic dispatch.

## Endpoint registration

Only allowlisted endpoints can be contacted. Calling an unregistered URL raises an error — this prevents Server-Side Request Forgery (SSRF) attacks.

### `pg_ripple.register_endpoint(url, local_view_name)`

Register a remote SPARQL endpoint.

| Parameter | Type | Default | Description |
|---|---|---|---|
| `url` | `TEXT` | — | Full URL of the endpoint (e.g. `https://dbpedia.org/sparql`) |
| `local_view_name` | `TEXT` | `NULL` | Optional name of a local SPARQL view stream table that pre-materialises the data from this endpoint. When set, `SERVICE` calls targeting this URL are rewritten to scan the local table instead of making HTTP calls. |

```sql
-- Register a plain remote endpoint
SELECT pg_ripple.register_endpoint('https://dbpedia.org/sparql');

-- Register with a local view override (SERVICE becomes a local scan)
SELECT pg_ripple.register_endpoint(
    'https://internal-kb.example.com/sparql',
    'my_local_view_stream'
);
```

### `pg_ripple.remove_endpoint(url)`

Permanently remove an endpoint from the allowlist.

```sql
SELECT pg_ripple.remove_endpoint('https://dbpedia.org/sparql');
```

### `pg_ripple.disable_endpoint(url)`

Temporarily disable an endpoint without removing it. Re-enable by calling `register_endpoint()` again.

```sql
SELECT pg_ripple.disable_endpoint('https://slow-endpoint.example.com/sparql');
-- Later:
SELECT pg_ripple.register_endpoint('https://slow-endpoint.example.com/sparql');
```

### `pg_ripple.list_endpoints()`

List all registered endpoints.

```sql
SELECT * FROM pg_ripple.list_endpoints();
```

Returns: `(url TEXT, enabled BOOLEAN, local_view_name TEXT)`.

## Configuration GUCs

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.federation_timeout` | `30` | Per-SERVICE call wall-clock timeout in seconds. |
| `pg_ripple.federation_max_results` | `10000` | Maximum rows accepted from a single remote call. Extra rows are silently dropped. |
| `pg_ripple.federation_on_error` | `'warning'` | Behaviour on failure: `'warning'` (emit WARNING, return empty), `'error'` (raise ERROR), `'empty'` (silent empty result). |

```sql
-- Tighten timeout for latency-sensitive queries
SET pg_ripple.federation_timeout = 5;

-- Raise an error on any SERVICE failure
SET pg_ripple.federation_on_error = 'error';
```

## Variable endpoints with VALUES

```sparql
SELECT ?s ?label WHERE {
  VALUES ?endpoint {
    <https://query.wikidata.org/sparql>
    <https://dbpedia.org/sparql>
  }
  SERVICE ?endpoint {
    ?s <http://www.w3.org/2000/01/rdf-schema#label> ?label
    FILTER(LANG(?label) = "en")
  }
}
```

Both endpoints must be registered. Results from both are combined and deduplicated via `SELECT DISTINCT`.

## Local view rewrite

When a `SERVICE` endpoint has a `local_view_name` set, pg_ripple rewrites the `SERVICE` clause to scan the pre-materialised stream table directly:

- **No HTTP call**: zero network latency.
- **PostgreSQL planner optimises**: the local scan participates in the full query plan.
- **Accurate statistics**: `ANALYZE` on the stream table gives the planner cardinality information.

Set this up using `create_sparql_view()` (see [Views](views.md)) and then register the endpoint with the view name:

```sql
-- Create a SPARQL view backed by a stream table
SELECT pg_ripple.create_sparql_view(
    'eu_companies',
    'SELECT ?company ?name WHERE { ?company <https://eu.example.org/name> ?name }',
    'manual'
);

-- Register the remote endpoint with the local view as override
SELECT pg_ripple.register_endpoint(
    'https://eu-kb.example.com/sparql',
    '_pg_ripple.eu_companies'  -- stream table name
);
```

## Health-based endpoint skipping

pg_ripple tracks the success/failure of each SERVICE call in `_pg_ripple.federation_health`. If a registered endpoint has a success rate below 10% in the last 5 minutes, the executor skips it automatically (emits a WARNING) rather than waiting for a full timeout. This prevents a single slow endpoint from blocking the entire query.

```sql
-- Check recent health
SELECT url,
       COUNT(*) AS total_probes,
       AVG(CASE WHEN success THEN 1.0 ELSE 0.0 END) AS success_rate,
       AVG(latency_ms) AS avg_latency_ms
FROM _pg_ripple.federation_health
WHERE probed_at >= now() - INTERVAL '5 minutes'
GROUP BY url;
```

## SSRF protection

pg_ripple enforces a strict allowlist: only endpoints registered with `register_endpoint()` can be contacted. Any `SERVICE` clause targeting an unregistered URL raises:

```
ERROR: federation endpoint not registered: http://internal-host/sparql;
       use pg_ripple.register_endpoint() to allow it
```

This prevents queries from being used as a vector to probe internal network services.

## Parallelism

Within a PostgreSQL session (SPI context), multiple `SERVICE` clauses in a single query execute **sequentially** to avoid conflict between HTTP I/O and SPI transactions. The pg_ripple_http sidecar process can execute federation calls in parallel via its async runtime; performance-critical federation workloads should use the HTTP interface.

---

## v0.19.0: Performance improvements

### Connection pooling

A per-backend thread-local `ureq::Agent` reuses TCP and TLS sessions across `SERVICE` calls within a session. Previously each call opened and discarded a new TCP connection.

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.federation_pool_size` | 4 | Idle connections kept per endpoint in the pool (1–32) |

```sql
-- Use a larger pool for latency-sensitive workloads with many endpoints
SET pg_ripple.federation_pool_size = 16;
```

### Result caching with TTL

When `pg_ripple.federation_cache_ttl > 0`, successful remote results are stored in `_pg_ripple.federation_cache`. Repeat calls with the same endpoint URL and SPARQL text within the TTL window skip the HTTP call entirely.

The cache key is `(url, XXH3-64(sparql_text))`. Expired rows are cleaned up by the merge background worker on each polling cycle.

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.federation_cache_ttl` | 0 | Cache TTL in seconds; 0 = disabled (0–86400) |

```sql
-- Cache Wikidata label results for 10 minutes
SET pg_ripple.federation_cache_ttl = 600;

-- Inspect the cache
SELECT url, query_hash, cached_at, expires_at
FROM _pg_ripple.federation_cache
ORDER BY cached_at DESC;

-- Clear the cache manually
DELETE FROM _pg_ripple.federation_cache;
```

**When to use caching:**
- Reference datasets that update infrequently (Wikidata labels, DBpedia categories, controlled vocabularies).
- Queries where the same sub-pattern is evaluated many times (e.g. inside a loop or repeated SPARQL calls from an application).

**When not to use caching:**
- Live event streams, sensor data, or any endpoint where freshness matters.
- Endpoints that return large variable result sets (high cache miss rate, high storage cost).

### Endpoint complexity hints

Register an endpoint with a performance hint to guide multi-endpoint query ordering. Fast endpoints execute first, enabling earlier failure detection and lower total wall-clock time.

```sql
-- Register with a hint
SELECT pg_ripple.register_endpoint(
    'https://fast-kb.example.com/sparql',
    NULL,        -- local_view_name
    'fast'       -- complexity: 'fast', 'normal', or 'slow'
);

-- Update after registration
SELECT pg_ripple.set_endpoint_complexity('https://slow-kb.example.com/sparql', 'slow');

-- View all endpoints with complexity
SELECT url, enabled, complexity FROM pg_ripple.list_endpoints();
```

### Variable projection rewrite

Instead of sending `SELECT * WHERE { … }` to the remote endpoint, pg_ripple now sends an explicit `SELECT ?v1 ?v2 … WHERE { … }` listing the variables that appear in the inner pattern. This:

- Reduces data transfer when the remote supports projection pushdown.
- Produces a stable, deterministic query text for cache key matching.
- Makes it easier to inspect the SPARQL sent (visible in WARNING messages on failure).

### Partial result handling

When `pg_ripple.federation_on_partial = 'use'`, a connection drop mid-response uses however many rows were received rather than discarding them entirely. A WARNING names the endpoint, the row count received, and the error.

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.federation_on_partial` | `'empty'` | `'empty'` = discard all, `'use'` = keep partial rows |

```sql
SET pg_ripple.federation_on_partial = 'use';
```

### Adaptive timeout

When `pg_ripple.federation_adaptive_timeout = on`, the effective per-endpoint timeout is derived from `max(1s, p95_latency_ms × 3 / 1000)` observed in `_pg_ripple.federation_health`. Fast endpoints get a tighter timeout; slow endpoints get more room. Falls back to `pg_ripple.federation_timeout` when no health data is available.

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.federation_adaptive_timeout` | `off` | Derive timeout from P95 health data |

```sql
SET pg_ripple.federation_adaptive_timeout = on;
```

### Batch SERVICE calls

When a single query contains two or more `SERVICE` clauses targeting the **same** registered endpoint with **independent** inner patterns (no shared variables), pg_ripple combines them into a single HTTP request:

```sparql
SELECT * WHERE {
  SERVICE <https://kb.example.com/sparql> { ?s <ex:label> ?label }
  SERVICE <https://kb.example.com/sparql> { ?s <ex:type>  ?type  }
  # ^ One HTTP request: SELECT * WHERE { { ?s <ex:label> ?label } UNION { ?s <ex:type> ?type } }
}
```

This halves the HTTP round trips for queries that pull multiple independent properties from the same endpoint.

### GUC reference (v0.19.0 additions)

| GUC | Type | Default | Range | Description |
|---|---|---|---|---|
| `pg_ripple.federation_pool_size` | INT | 4 | 1–32 | Idle connections per endpoint in the thread-local pool |
| `pg_ripple.federation_cache_ttl` | INT | 0 | 0–86400 | Result cache TTL in seconds (0 = disabled) |
| `pg_ripple.federation_on_partial` | STRING | `'empty'` | `'empty'`, `'use'` | Behaviour when SERVICE delivers rows then fails |
| `pg_ripple.federation_adaptive_timeout` | BOOL | off | — | Derive timeout from P95 health latency |

## Limitations

- **No bind-join pushdown** at runtime: the full inner pattern is sent to the remote endpoint without pre-binding known variables.
- **SPARQL results+JSON only**: XML response format is not yet supported for the direct SPI path.
- **No streaming**: remote results are fully buffered in memory before being dictionary-encoded. Large result sets should use `federation_max_results` to cap memory usage.
