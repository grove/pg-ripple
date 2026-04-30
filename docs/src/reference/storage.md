# Storage Reference

This page is the reference for pg_ripple's HTAP storage layer.

## Overview

pg_ripple uses Vertical Partitioning (VP) storage: one table per unique
predicate, with subjects and objects stored as `BIGINT` dictionary IDs. An
HTAP (Hybrid Transactional/Analytical Processing) design splits writes to a
delta table (heap + B-tree) and reads from a main table (BRIN-indexed).
A background merge worker periodically combines main + delta (minus tombstones)
into a fresh main table.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE '%htap%' OR feature_name LIKE '%storage%';
```

## Table Layout

Each VP predicate gets three physical tables in `_pg_ripple`:

| Table | Description |
|---|---|
| `vp_{id}_delta` | Recent writes (heap, B-tree indices on `(s,o)` and `(o,s)`) |
| `vp_{id}_main` | Historical data (BRIN-indexed, read-optimized) |
| `vp_{id}_tombstones` | Deleted rows from `main` (cleared after merge) |

The query path is: `(main EXCEPT tombstones) UNION ALL delta`.

Rare predicates (below `pg_ripple.vp_promotion_threshold`, default 1,000 triples)
are stored in `_pg_ripple.vp_rare (p, s, o, g, i, source)`.

## Dictionary Encoding

Every IRI, blank node, and literal is mapped to `BIGINT` (i64) via XXH3-128
hash before storage. The `_pg_ripple.dictionary` table stores the full
round-trip mapping. A shared-memory LRU cache (size controlled by
`pg_ripple.dictionary_cache_size`) avoids repeated database lookups.

## VP Table Columns

| Column | Type | Description |
|---|---|---|
| `s` | `BIGINT` | Subject dictionary ID |
| `o` | `BIGINT` | Object dictionary ID |
| `g` | `BIGINT` | Named graph dictionary ID (0 = default graph) |
| `i` | `BIGINT` | Statement ID (SID) from shared sequence |
| `source` | `SMALLINT` | 0 = explicit, 1 = inferred |

## Merge Worker

The background merge worker runs continuously and:
1. Detects VP tables whose delta exceeds `pg_ripple.merge_threshold`.
2. Acquires an advisory lock on the predicate ID.
3. Builds a new `vp_{id}_main` table (main EXCEPT tombstones UNION ALL delta).
4. Atomically renames tables (swap) using DDL lock.
5. Truncates delta and tombstones.

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.vacuum_triples(graph_iri TEXT) → BIGINT` | Deduplicate triples in a graph |
| `pg_ripple.reindex_vp(predicate_iri TEXT) → void` | Rebuild indices on a VP table |
| `pg_ripple.force_merge(predicate_iri TEXT) → void` | Trigger immediate merge for a predicate |
| `pg_ripple.promote_predicate(predicate_iri TEXT) → void` | Promote from vp_rare to dedicated table |
| `pg_ripple.recover_interrupted_promotions() → BIGINT` | Recover VP tables stuck in 'promoting' state |

## Related Pages

- [Architecture Internals](architecture.md)
- [Scalability](scalability.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
