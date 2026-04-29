# Plan Cache

pg_ripple maintains a backend-local SPARQL plan cache to avoid redundant SPARQL → SQL translation for repeated queries. This document describes the cache contract, key construction, and isolation properties.

## Cache key construction

The cache key is derived from the SPARQL query text alone:

1. The query text is parsed by `spargebra` into a canonical algebra tree.
2. The algebra tree is printed with `spargebra`'s `Display` implementation to produce a canonical string.
3. Two GUC values are appended: `pg_ripple.max_path_depth` and `pg_ripple.bgp_reorder`.

The resulting canonical string is used as the hash map key.

### What is **not** in the cache key

| Omitted item | Rationale |
|---|---|
| Current role / session user | PostgreSQL enforces Row Level Security at executor level, not at plan level. The cached SQL string is safe to execute under any role; the RLS policy filters rows after the plan runs. |
| Named-graph binding (`GRAPH <iri>`) | Named graphs are encoded as integer IDs at translation time. The graph IRI is embedded in the SQL `WHERE g = <id>` clause, so the query text already includes graph identity. |
| RLS-relevant session variables | Session-level GUCs that affect RLS policies (e.g. `app.current_tenant`) are evaluated by the PostgreSQL executor, not by pg_ripple's query planner. |

### Why this is safe

PostgreSQL's RLS mechanism applies row filters at execution time, after the query plan is resolved. A cached plan that omits the role from its key produces correct (RLS-filtered) results when executed under any role. The plan may not be *optimal* for a role-specific workload (a role-specialized plan could use tighter index scans), but it cannot return rows the caller should not see.

## SERVICE clause exclusion

Queries containing `SERVICE` clauses (federated SPARQL) are **never cached**. Remote endpoint availability and result shapes may vary between invocations; caching such plans would produce stale or incorrect results.

## Cache capacity and eviction

The plan cache is a `LruCache` with a fixed capacity set by `pg_ripple.plan_cache_capacity` (default: 256 plans). When the cache is full, the least-recently-used plan is evicted.

## Manual invalidation

```sql
-- Evict all cached plans and reset hit/miss counters.
SELECT pg_ripple.plan_cache_reset();

-- Inspect cache statistics.
SELECT pg_ripple.plan_cache_stats();
```

`plan_cache_stats()` returns a JSONB object with keys:
- `hits` — number of cache hits since last reset.
- `misses` — number of cache misses (translation performed) since last reset.
- `size` — current number of cached plans.
- `capacity` — maximum number of plans the cache can hold.

## Role isolation regression test

The pg_regress file `tests/pg_regress/sql/plan_cache_rls.sql` verifies that two
roles with different RLS grants see only their own data when running the same
SPARQL query text:

- Role `ripple_alice` has access to `<https://example.org/graph_a>`.
- Role `ripple_bob` has access to `<https://example.org/graph_b>` only.
- The same SPARQL query is executed as each role.
- `ripple_alice` receives triples from graph A; `ripple_bob` receives triples from graph B.

This test confirms that plan cache sharing between roles does not bypass
PostgreSQL's RLS enforcement.
