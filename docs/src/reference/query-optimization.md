# Query Optimization Reference

This page is the reference for pg_ripple's SPARQL query optimizer.

## Overview

pg_ripple applies multiple optimization passes before generating SQL from
SPARQL algebra:

1. **sparopt algebra optimizer**: first-pass algebra rewriting (variable
   substitution, filter pushdown, DISTINCT elimination with SHACL hints).
2. **Self-join elimination**: star patterns on the same subject are collapsed
   into single-scan plans with multiple joins.
3. **Filter pushdown**: FILTER constants are encoded to `BIGINT` at
   translation time so comparisons happen on integer values, not strings.
4. **SHACL hints**: `sh:maxCount 1` predicates omit DISTINCT; `sh:minCount 1`
   predicates use INNER JOIN instead of LEFT JOIN.
5. **Plan cache**: compiled SQL is stored in `_pg_ripple.plan_cache` and
   reused across identical queries (keyed by query text + current role + relevant GUCs).
6. **TopN push-down**: `ORDER BY ... LIMIT N` patterns are pushed into
   subqueries to avoid sorting full result sets.
7. **Leapfrog TrieJoin (WCOJ)**: worst-case optimal join planning for
   cyclic SPARQL graph patterns.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE '%plan%' OR feature_name LIKE '%cache%' OR feature_name LIKE '%wcoj%';
```

## Plan Cache

The plan cache avoids re-compiling SPARQL→SQL for repeated queries. Key details:

- Cache key: SHA-256 of (query text, current_role, GUC snapshot)
- Cache invalidated on: VP promotion (schema change), extension upgrade, `plan_cache_reset()`
- Maximum entries: `pg_ripple.plan_cache_size` (default: 512)
- Eviction policy: LRU

```sql
-- Inspect cache
SELECT * FROM _pg_ripple.plan_cache;

-- Manual invalidation
SELECT pg_ripple.plan_cache_reset();
```

## Property Path Optimization

Property paths compile to `WITH RECURSIVE … CYCLE` queries using PostgreSQL 18's
native `CYCLE` clause for hash-based cycle detection. Bounded depth paths
(`{n,m}`) use iterative CTEs limited to `pg_ripple.max_property_path_depth`
hops (default: 20). Early fixpoint termination avoids iterating past convergence
for bounded hierarchies.

## Magic Sets (Goal-Directed Inference)

When using `pg_ripple.query_goal()` for Datalog queries, magic sets
transformation rewrites rules to focus inference on the bindings needed to
answer the goal, avoiding full forward-chaining materialization.

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.plan_cache_reset() → void` | Invalidate all cached query plans |
| `pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN) → TEXT` | Inspect query plan with optional live stats |

## Related Pages

- [Plan Cache](plan-cache.md)
- [SPARQL Reference](sparql.md)
- [Architecture Internals](architecture.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
