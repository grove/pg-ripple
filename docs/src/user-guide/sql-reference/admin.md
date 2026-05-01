# Administration Functions

pg_ripple v0.6.0 introduced a set of administration and monitoring functions in the `pg_ripple` schema for HTAP maintenance, change data capture, and statistics.

---

## compact()

```sql
pg_ripple.compact() → bigint
```

Triggers a synchronous merge of all HTAP delta tables into their corresponding main tables. Blocks until the merge is complete.

**Returns**: the total number of rows now in all main tables (after merge).

**Use cases**:
- After a large bulk load, call `compact()` to flush delta to main before starting read-heavy queries
- In maintenance windows to pre-emptively reduce delta size
- In tests to simulate a completed merge cycle

```sql
SELECT pg_ripple.compact();
-- 1500000
```

> **Note**: For background (non-blocking) merges, rely on the merge worker instead. `compact()` is a foreground operation and holds an exclusive lock during the table swap.

---

## stats()

```sql
pg_ripple.stats() → jsonb
```

Returns a JSONB object with extension-wide statistics. Fields:

| Field | Type | Description |
|---|---|---|
| `total_triples` | integer | Total triples across all VP tables and `vp_rare` |
| `dedicated_predicates` | integer | Number of predicates with their own VP table |
| `htap_predicates` | integer | Number of predicates using the delta/main split |
| `rare_triples` | integer | Triples stored in the shared `vp_rare` table |
| `unmerged_delta_rows` | integer | Rows in all delta tables not yet merged — `-1` if `shared_preload_libraries` is not set |
| `merge_worker_pid` | integer | PID of the background merge worker — `0` if not running |

```sql
SELECT pg_ripple.stats();
-- {
--   "total_triples": 1500000,
--   "dedicated_predicates": 42,
--   "htap_predicates": 42,
--   "rare_triples": 1234,
--   "unmerged_delta_rows": 8742,
--   "merge_worker_pid": 12345
-- }
```

Monitor `unmerged_delta_rows` over time. If it grows without bound, the merge worker may be blocked or misconfigured.

---

## htap_migrate_predicate(pred_id)

```sql
pg_ripple.htap_migrate_predicate(pred_id bigint) → void
```

Migrates an existing flat VP table (created before v0.6.0) to the delta/main partition split. Called automatically by the `pg_ripple--0.5.1--0.6.0.sql` migration script.

**Parameters**: `pred_id` — the dictionary integer ID of the predicate.

```sql
-- Find the predicate ID first
SELECT id FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
WHERE d.value = 'https://schema.org/name';

-- Then migrate
SELECT pg_ripple.htap_migrate_predicate(12345678);
```

---

## subscribe(pattern, channel)

```sql
pg_ripple.subscribe(pattern text, channel text) → bigint
```

Registers a CDC (Change Data Capture) subscription. Fires a `pg_notify` on `channel` whenever a triple matching `pattern` is inserted or deleted in a VP delta table.

**Parameters**:
- `pattern` — predicate IRI (e.g. `'<https://schema.org/name>'`) or `'*'` to subscribe to all predicates
- `channel` — name of the PostgreSQL NOTIFY channel to send notifications to

**Returns**: the subscription ID (integer).

```sql
-- Subscribe to all changes on schema:name predicate
SELECT pg_ripple.subscribe('<https://schema.org/name>', 'name_changes');

-- In another session, listen for notifications
LISTEN name_changes;

-- Insert a triple to trigger the notification
SELECT pg_ripple.insert_triple(
    '<https://example.org/Alice>',
    '<https://schema.org/name>',
    '"Alice"'
);
-- NOTIFY name_changes, '{"op":"INSERT","s":...,"p":...,"o":...}'
```

Notification payload is a JSON object with fields `op` (`"INSERT"` or `"DELETE"`), `s`, `p`, `o` (N-Triples encoded), and `g` (graph ID).

---

## unsubscribe(channel)

```sql
pg_ripple.unsubscribe(channel text) → bigint
```

Removes all CDC subscriptions for a given channel.

**Returns**: the number of subscriptions removed.

```sql
SELECT pg_ripple.unsubscribe('name_changes');
-- 1
```

---

## subject_predicates(subject_id) / object_predicates(object_id)

```sql
pg_ripple.subject_predicates(subject_id bigint) → bigint[]
pg_ripple.object_predicates(object_id  bigint) → bigint[]
```

Return the sorted array of predicate IDs for which the given subject (or object) has at least one triple. Backed by the `_pg_ripple.subject_patterns` and `_pg_ripple.object_patterns` indexes populated by the merge worker.

Returns `NULL` if the subject/object has not been indexed yet (before the first merge).

```sql
-- Find all predicates used by Alice
SELECT pg_ripple.subject_predicates(
    pg_ripple.encode_term('https://example.org/Alice', 0)
);
```

---

## predicate_stats (view)

```sql
SELECT * FROM pg_ripple.predicate_stats;
```

A convenience view over `_pg_ripple.predicates` and `_pg_ripple.dictionary`:

| Column | Description |
|---|---|
| `predicate_iri` | Full IRI of the predicate |
| `triple_count` | Total triples (across delta + main) |
| `storage` | `'dedicated'` (own VP table) or `'rare'` (`vp_rare`) |

```sql
-- Top 10 predicates by triple count
SELECT predicate_iri, triple_count, storage
FROM pg_ripple.predicate_stats
ORDER BY triple_count DESC
LIMIT 10;
```

---

## deduplicate_predicate(p_iri TEXT) → BIGINT (v0.7.0)

Remove duplicate `(s, o, g)` rows for a single predicate, keeping the row with the lowest SID (oldest assertion). Returns the count of rows removed.

- **Delta tables** (`vp_{id}_delta`): duplicate rows are physically deleted — the minimum-SID row per group is kept.
- **Main tables** (`vp_{id}_main`): tombstone rows are inserted for all but the minimum-SID duplicate, masking duplicates from queries immediately; they are physically removed on the next merge cycle.
- **vp_rare**: duplicate rows are physically deleted (minimum SID kept).
- ANALYZE is run on all modified tables after deduplication.

```sql
-- Remove duplicates for a specific predicate
SELECT pg_ripple.deduplicate_predicate('<https://schema.org/name>');

-- Returns: number of rows removed
```

**Typical usage**: call once after a bulk load that may contain duplicate triples.

---

## vacuum() → bigint (v0.14.0)

```sql
pg_ripple.vacuum() → bigint
```

Forces a full delta→main merge on all HTAP VP tables, then runs `ANALYZE` on every VP table (delta, main, tombstones) and `vp_rare`.

**Returns**: the number of VP table groups analyzed.

```sql
SELECT pg_ripple.vacuum();
-- 42
```

> **Note**: `ANALYZE` updates planner statistics. PostgreSQL's `VACUUM` itself cannot run inside a transaction block; call it separately if you need dead-tuple reclamation.

**Lock levels acquired (ADMIN-LOCK-01, v0.82.0):**
- `ANALYZE` acquires a brief `ShareUpdateExclusiveLock` on each VP table. Concurrent reads and writes are not blocked.
- The delta→main merge acquires a `SET LOCAL lock_timeout` (configurable via `pg_ripple.merge_lock_timeout_ms`, default 5 s) before taking a `ShareRowExclusiveLock` on the VP table during the final swap.

---

## reindex() → bigint (v0.14.0)

```sql
pg_ripple.reindex() → bigint
```

Rebuilds all indices on every VP table (delta and main) and `vp_rare` using `REINDEX TABLE`. Run this after large bulk deletes or to recover from index corruption.

**Returns**: the number of VP table groups reindexed.

```sql
SELECT pg_ripple.reindex();
-- 42
```

**Lock levels acquired (ADMIN-LOCK-01, v0.82.0):**
- `REINDEX TABLE` acquires an `AccessExclusiveLock` on each VP table for the duration of the rebuild. All concurrent reads and writes on that table are blocked until the reindex completes.
- To minimise impact, `reindex()` processes one VP table at a time. On databases with many predicates, consider running during a maintenance window.

---

## vacuum_dictionary() → bigint (v0.14.0)

```sql
pg_ripple.vacuum_dictionary() → bigint
```

Removes dictionary entries that are no longer referenced by any VP table. Orphaned entries accumulate after bulk deletes.

Uses an advisory transaction lock (key `0x7269706c`) to prevent concurrent runs. Safe to run during normal operation — may miss very recently orphaned entries, which are cleaned on the next run.

**Returns**: the number of dictionary entries removed.

```sql
SELECT pg_ripple.vacuum_dictionary();
-- 128
```

**Typical usage**: run periodically after bulk deletes, or after `drop_graph()`.

---

## dictionary_stats() → jsonb (v0.14.0)

```sql
pg_ripple.dictionary_stats() → jsonb
```

Returns detailed metrics about the dictionary and cache configuration.

| Field | Description |
|---|---|
| `total_entries` | Total rows in the dictionary |
| `hot_entries` | Rows in the unlogged hot dictionary cache |
| `cache_capacity` | Shared-memory encode cache capacity (entries) |
| `cache_budget_mb` | Configured cache budget cap in MB |
| `shmem_ready` | Whether shared memory is initialized |

```sql
SELECT pg_ripple.dictionary_stats();
-- {
--   "total_entries":   450000,
--   "hot_entries":     1024,
--   "cache_capacity":  4096,
--   "cache_budget_mb": 64,
--   "shmem_ready":     true
-- }
```

---

## enable_graph_rls() → boolean (v0.14.0)

```sql
pg_ripple.enable_graph_rls() → boolean
```

Activates Row-Level Security policies on `_pg_ripple.vp_rare` using the `g` column and the `_pg_ripple.graph_access` mapping table. Default graph (g = 0) is always accessible. Named graphs require an explicit grant.

Returns `true` on success.

```sql
SELECT pg_ripple.enable_graph_rls();
-- true
```

---

## grant_graph(role, graph, permission) (v0.14.0)

```sql
pg_ripple.grant_graph(role text, graph text, permission text) → void
```

Grants `permission` (`'read'`, `'write'`, or `'admin'`) on a named graph to a PostgreSQL role.

```sql
SELECT pg_ripple.grant_graph('app_user', '<https://example.org/graph1>', 'read');
SELECT pg_ripple.grant_graph('admin_user', '<https://example.org/graph1>', 'admin');
```

---

## revoke_graph(role, graph [, permission]) (v0.14.0)

```sql
pg_ripple.revoke_graph(role text, graph text, permission text DEFAULT NULL) → void
```

Revokes a permission on a named graph from a role. Pass `NULL` (or omit) for `permission` to revoke all permissions for that role on that graph.

```sql
-- Revoke specific permission
SELECT pg_ripple.revoke_graph('app_user', '<https://example.org/graph1>', 'read');

-- Revoke all permissions
SELECT pg_ripple.revoke_graph('app_user', '<https://example.org/graph1>');
```

---

## list_graph_access() → jsonb (v0.14.0)

```sql
pg_ripple.list_graph_access() → jsonb
```

Returns all graph access control entries as a JSONB array. Each element has `role`, `graph` (decoded IRI), and `permission`.

```sql
SELECT * FROM jsonb_array_elements(pg_ripple.list_graph_access());
```

---

## schema_summary() → jsonb (v0.14.0)

```sql
pg_ripple.schema_summary() → jsonb
```

Returns a live class→property→cardinality summary as a JSONB array. When `enable_schema_summary()` has been called (requires pg_trickle), reads from the materialized `_pg_ripple.inferred_schema` stream table.

Each element: `{"class": "...", "property": "...", "cardinality": N}`.

```sql
SELECT * FROM jsonb_array_elements(pg_ripple.schema_summary());
```

---

## enable_schema_summary() → boolean (v0.14.0)

```sql
pg_ripple.enable_schema_summary() → boolean
```

Creates `_pg_ripple.inferred_schema` as a pg_trickle stream table (refreshed every 30 s) for SPARQL IDE auto-completion. Requires pg_trickle. Returns `false` with a warning if pg_trickle is not installed.

```sql
SELECT pg_ripple.enable_schema_summary();
-- true (or false with warning if pg_trickle missing)
```

---

## deduplicate_all() → bigint (v0.7.0)

```sql
pg_ripple.deduplicate_all() → bigint
```

Removes duplicate `(s, o, g)` rows across all predicates, keeping the row with the lowest SID. Returns the total number of duplicate rows removed.

```sql
SELECT pg_ripple.deduplicate_all();
```

---

## dedup_on_merge (GUC)

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.dedup_on_merge` | `boolean` | `off` | When `on`, the HTAP merge worker deduplicates `(s, o, g)` rows using `DISTINCT ON` during compaction, keeping the lowest-SID row |

```sql
-- Enable merge-time dedup
SET pg_ripple.dedup_on_merge = true;

-- Trigger a merge (deduplication happens atomically during compaction)
SELECT pg_ripple.compact();
```

Between merges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe short-lived duplicates. This is harmless for most workloads.

---

## plan_cache_stats() → jsonb (v0.13.0)

```sql
pg_ripple.plan_cache_stats() → jsonb
```

Returns statistics about the SPARQL plan cache as a JSONB object. Use this to monitor cache effectiveness and tune `pg_ripple.plan_cache_size`.

| Field | Description |
|-------|-------------|
| `hits` | Number of cache hits since startup |
| `misses` | Number of cache misses (recompilations) |
| `size` | Current number of cached plans |
| `capacity` | Maximum cache capacity |

```sql
SELECT pg_ripple.plan_cache_stats();
-- {"hits": 1523, "misses": 42, "size": 38, "capacity": 128}
```

A high miss rate (> 50%) suggests either too many distinct query shapes or too small a cache. Try increasing `pg_ripple.plan_cache_size` or parameterizing queries with `VALUES` blocks.

---

## plan_cache_reset() → void (v0.13.0)

```sql
pg_ripple.plan_cache_reset() → void
```

Evicts all cached SPARQL→SQL plans and resets the hit/miss counters. Useful after schema changes, VP promotions, or when switching `pg_ripple.bgp_reorder` on/off.

```sql
SELECT pg_ripple.plan_cache_reset();
```

---

## promote_rare_predicates() → bigint (v0.2.0)

```sql
pg_ripple.promote_rare_predicates() → bigint
```

Scans `_pg_ripple.vp_rare` for predicates whose triple count has exceeded `pg_ripple.vp_promotion_threshold` and promotes each to a dedicated VP table. Returns the number of predicates promoted.

```sql
SELECT pg_ripple.promote_rare_predicates();
-- 3
```

Promotion is normally automatic during inserts. Use this after changing the threshold or after a bulk load where auto-promotion was deferred.
