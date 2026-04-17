# Operations

This page covers production operations for pg_ripple: monitoring, merge management, rollback safety, and routine maintenance.

---

## Monitoring

### Cache statistics

`pg_ripple.cache_stats()` returns hit/miss/eviction statistics for the shared-memory encode cache (introduced in v0.22.0). A healthy deployment should show a hit rate of 90% or higher for typical workloads.

```sql
SELECT * FROM pg_ripple.cache_stats();
```

| Column | Type | Description |
|---|---|---|
| `hits` | `bigint` | Number of encode cache hits since last PostgreSQL restart |
| `misses` | `bigint` | Number of encode cache misses |
| `evictions` | `bigint` | Number of LRU evictions from the 4-way set-associative cache |
| `utilisation` | `float` | Fraction of cache slots currently occupied (0.0 – 1.0) |

If the hit rate drops below 90% (`hits / (hits + misses) < 0.9`), consider increasing `pg_ripple.dictionary_cache_size` (requires PostgreSQL restart).

### Triple count

```sql
SELECT pg_ripple.triple_count();
```

Returns the total number of triples across all graphs, including triples currently in the delta partition (not yet merged to main).

### Predicate inventory

```sql
SELECT id, triple_count, table_oid IS NOT NULL AS has_dedicated_table
FROM _pg_ripple.predicates
ORDER BY triple_count DESC
LIMIT 20;
```

Predicates with `has_dedicated_table = false` are stored in `_pg_ripple.vp_rare`. Once `triple_count` crosses `pg_ripple.vp_promotion_threshold`, they are automatically promoted.

---

## Rollback safety guarantee (v0.22.0)

Starting with v0.22.0, pg_ripple guarantees that a rolled-back `insert_triple` call cannot plant a phantom term ID into the triple store.

**How it works:**

- Every backend registers a transaction-end callback (`RegisterXactCallback`) during `_PG_init`.
- On `XACT_EVENT_ABORT`, both the thread-local encode LRU cache and the shared-memory encode cache are flushed for that backend.
- A per-backend epoch counter is incremented on rollback. Shared-memory cache entries from an earlier epoch are rejected as stale.

**What this means in practice:**

```sql
BEGIN;
SELECT pg_ripple.insert_triple(
    '<https://example.org/new-entity>',
    '<https://example.org/name>',
    '"Rolled Back Name"'
);
ROLLBACK;

-- The term ID assigned to "Rolled Back Name" has been discarded.
-- Inserting it again will get a fresh, consistent ID.
SELECT pg_ripple.insert_triple(
    '<https://example.org/new-entity>',
    '<https://example.org/name>',
    '"Rolled Back Name"'
);
-- Succeeds cleanly; no phantom references.
```

**Recommendation:** There is no configuration required. Rollback safety is always active from v0.22.0 onwards.

---

## Merge correctness guarantees (v0.22.0)

The background merge worker (which compacts delta partitions into main partitions) has two important correctness guarantees from v0.22.0.

### Tombstone epoch fence

When a `delete_triple` call is issued while a merge is in progress, the tombstone record is now protected by an epoch fence. At merge start, the worker records `max_sid_at_snapshot = currval('_pg_ripple.statement_id_seq')`. At merge end, only tombstones with `i ≤ max_sid_at_snapshot` are cleaned up. Tombstones for deletes that committed *after* the snapshot survive to the next merge cycle.

**Effect:** Deleted triples can no longer "resurrect" after a concurrent merge completes.

### View-rename atomicity

The merge worker no longer performs a `CREATE OR REPLACE VIEW` step after renaming `_main`. The VP view's `FROM` clause always references `vp_{id}_main` directly; PostgreSQL re-resolves the table name after the atomic rename. This closes a window where a concurrent query could fail with `relation does not exist` while the view was being rebuilt.

---

## Routine maintenance

### Analyze after bulk loads

After large bulk loads, run ANALYZE to update planner statistics:

```sql
SELECT pg_ripple.bulk_load_turtle('/path/to/data.ttl');
ANALYZE;
```

The bulk loader calls `ANALYZE` on VP tables automatically, but running `ANALYZE` again after multiple loads ensures fresh statistics.

### Force merge

To compact all delta partitions immediately (e.g., before a backup or during a maintenance window):

```sql
SELECT pg_ripple.force_merge();
```

### Vacuum

```sql
SELECT pg_ripple.vacuum_extension();
```

Runs `VACUUM` on all VP tables to reclaim space from deleted triples.

### Reindex

```sql
SELECT pg_ripple.reindex_extension();
```

Rebuilds all indexes on VP tables. Use after large bulk deletes or if index bloat is detected.

---

## Delta growth monitoring

The merge worker automatically fires when the number of pending delta inserts exceeds `pg_ripple.merge_threshold`. Monitor delta growth with:

```sql
SELECT
    relname,
    n_live_tup AS live_rows,
    n_dead_tup AS dead_rows
FROM pg_stat_user_tables
WHERE relname LIKE 'vp_%_delta'
  AND schemaname = '_pg_ripple'
ORDER BY n_live_tup DESC
LIMIT 10;
```

If delta tables are growing faster than the merge worker can consume them, consider lowering `pg_ripple.merge_threshold` or increasing the merge worker's frequency via `pg_ripple.merge_interval_ms`.
