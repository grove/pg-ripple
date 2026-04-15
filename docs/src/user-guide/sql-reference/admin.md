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


--
pical usage**: call once after a bulk load that may contain duplicate triples.
imum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimuen imum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimuen imum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimum-SID d andimum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimum-SID duplicaten `ipg_rimuen imum-SID duplicaten `ipg_rimum-SIple.dedup_on_merge` | BOOL | `off` | When `on`, the HTAP generation merge deduplicates `(s, o, g)` rows using `DISTINCT ON`, keeping the lowest-SID row. |

When enabled, the merge worker's fresh-table geWhen enabled, the merge worker's fresh-table geWhen enabled, the merguplicating projection:

```sql
-- Enable merge-time dedup
SET pg_rSET p.dSET pg_rSET p.dSET pg_rSET p.dSET pg_rSET pdeduplication happens atomically during compaction)
SELECT pg_ripple.compSELECT pg_ripple.compSELECT pg_r_meSge;
SELECT pg_ripple.compSELECT pg_ripple.compSELECT pg_r_meSge;
uplication happens a. Bupweenumerges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe shouplication happens a. Bupweenumerges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe shouplication happens a. Bupweenumerges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe shouplication happens a. Bupweenumerges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe shouplication happens a. Bupweenumerges, the `(main EXCEPT tombstones) UNION ALL delta` query view may observe shouplication happens a. Bupweenumerges, the `(main EXCEPT tomb annotation workloads.
