[← Back to Blog Index](README.md)

# HTAP for Triples: Reads and Writes at the Same Time

## The delta/main split that lets pg_ripple handle mixed workloads without locking

---

Most triple stores make you choose: fast writes or fast reads. Bulk-loading 10 million triples into a B-tree-indexed table is slow because every insert updates the index. Rebuilding the index after a bulk load is fast for writes but locks readers for the duration.

pg_ripple doesn't make you choose. It uses an HTAP (Hybrid Transactional/Analytical Processing) architecture that separates the write path from the read path, merging them in the background without blocking either.

---

## The Problem: B-Trees vs. Bulk Writes

A standard VP table with B-tree indexes on `(s, o)` and `(o, s)` handles point lookups beautifully. Find all objects for subject 12345 on predicate `foaf:name`? That's a single B-tree descent — sub-millisecond.

But inserting 10 million triples into that table means 10 million B-tree insertions across two indexes. Each insertion potentially splits pages, updates parent pointers, and dirties cache lines. At scale, the write throughput drops to 10–50K triples/second, bottlenecked by random I/O into the B-tree.

You could batch the inserts and rebuild the indexes afterward. That's fast — 200K+ triples/second. But rebuilding an index takes an `ACCESS EXCLUSIVE` lock. No reads while it's happening. For a table with 50 million triples, the rebuild can take minutes. Minutes of downtime for a production system.

This is the fundamental tension: B-trees are optimized for read-heavy workloads with occasional writes. Knowledge graphs often need both — continuous writes (data ingestion, inference, CDC) and continuous reads (SPARQL queries, API serving).

---

## The HTAP Split

Since v0.6.0, each VP table is split into three physical components:

```
vp_{id}_main         -- BRIN-indexed, read-optimized, bulk-loaded
vp_{id}_delta        -- B-tree-indexed, write-optimized, small
vp_{id}_tombstones   -- tracks deletes from main
```

### Writes Go to Delta

All `INSERT` operations append to the `_delta` table. The delta table has B-tree indexes (needed for deduplication and point lookups), but it's small — typically a few thousand to a few hundred thousand rows between merges. Inserting into a small B-tree is fast.

### Deletes Create Tombstones

When a triple is deleted and it exists in `_main`, a row is written to `_tombstones` to mark it as deleted. The main table itself is not modified. This avoids the cost of finding and removing a row from a BRIN-indexed table (which would require a sequential scan).

### Reads Union Both

The query path for any VP table is:

```sql
(SELECT s, o, g FROM vp_{id}_main
 WHERE (s, o, g) NOT IN (SELECT s, o, g FROM vp_{id}_tombstones))
UNION ALL
(SELECT s, o, g FROM vp_{id}_delta)
```

This is logically equivalent to a single table. The SPARQL-to-SQL translator generates this union transparently — the SPARQL query writer doesn't know or care about the split.

The UNION ALL is efficient because:
- The `_main` scan uses BRIN indexes (block range indexes), which are tiny and cache-resident.
- The tombstone anti-join is typically small (few deletes between merges).
- The `_delta` scan is small by definition.

---

## The Background Merge Worker

Periodically, a background worker merges delta into main:

1. Create a new `_main` table from `(old_main EXCEPT tombstones) UNION ALL delta`.
2. Build BRIN indexes on the new main.
3. Atomically swap the old main for the new one (rename).
4. Truncate delta and tombstones.

Steps 1–2 happen without holding any lock on the read path. Queries continue to read from the old main + delta while the new main is being built.

Step 3 is a brief metadata-only operation — renaming a table is a catalog update, not a data operation. The lock is held for microseconds.

After the swap, new queries read from the fresh main (with BRIN indexes, no tombstones, no delta). The old main is dropped.

### BRIN vs. B-Tree for Main

The main table uses BRIN (Block Range Index) instead of B-tree. BRIN indexes are dramatically smaller — typically 0.1% the size of the equivalent B-tree — because they store summary information per block range rather than per row.

For sequential or nearly-sequential data (which the merge produces, since it sorts during the merge), BRIN indexes are nearly as effective as B-trees for range scans and much cheaper to build and maintain.

The trade-off: BRIN indexes don't support point lookups as efficiently as B-trees. But point lookups against main are rare — they're handled by the delta table's B-tree indexes. Main is scanned for analytical queries, where BRIN excels.

---

## Concurrency

The HTAP split means:

- **Writers never block readers.** Writes go to delta. Reads union main and delta. There's no exclusive lock.
- **Readers never block writers.** The main table is read-only between merges. MVCC handles concurrent access to delta.
- **Merges don't block queries.** The merge builds a new main in the background. The swap is atomic and near-instant.

The only contention point is the delta table's B-tree indexes during concurrent writes. For most workloads — even 10K triples/second sustained — this isn't a bottleneck because the delta table is small and its indexes are cache-resident.

---

## Merge Scheduling

The merge worker runs based on configurable thresholds:

- **Row count.** When delta exceeds a threshold (default: 100,000 rows), a merge is triggered.
- **Time.** A merge runs at least every N seconds (default: 300), even if the row threshold hasn't been reached, to keep tombstones from accumulating.

You can also trigger a merge manually:

```sql
SELECT pg_ripple.vacuum();
```

This runs an immediate merge for all VP tables, useful after a bulk load.

---

## Multi-Worker Parallel Merge

Since v0.42.0, pg_ripple can run multiple merge workers in parallel — one per VP table. For a dataset with 200 active predicates, this means 200 merges can run concurrently (limited by `pg_ripple.merge_workers`).

This matters for write-heavy workloads where multiple predicates accumulate large deltas simultaneously. A single merge worker processing 200 tables sequentially might take 10 minutes. Eight parallel workers finish in ~90 seconds.

---

## What This Looks Like in Practice

Loading 10 million triples into a clean pg_ripple instance:

```sql
-- Bulk load a large Turtle file
SELECT pg_ripple.load_turtle_file('/data/dbpedia-2024.ttl');
-- Completes in ~45 seconds (writes to delta tables)

-- Queries work immediately against delta
SELECT * FROM pg_ripple.sparql('
  SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }
');
-- Returns 10,000,000 in ~200ms (scanning deltas)

-- Trigger merge to optimize for reads
SELECT pg_ripple.vacuum();
-- Completes in ~60 seconds (builds BRIN-indexed main tables)

-- Same query, now against main
SELECT * FROM pg_ripple.sparql('
  SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }
');
-- Returns 10,000,000 in ~50ms (BRIN scan)
```

The key insight: queries work at every stage. Before merge, they're a bit slower (B-tree scan on delta). After merge, they're fast (BRIN scan on main). But they always work, and they never block writes.

---

## When Single-Table Storage Is Fine

The HTAP split adds complexity. For small datasets (under 1 million triples) or read-only workloads (load once, query forever), the overhead of maintaining delta/main/tombstones isn't worth it.

pg_ripple used single flat VP tables from v0.1.0 through v0.5.1. The HTAP split was introduced in v0.6.0 specifically for workloads that need concurrent reads and writes. If your graph is loaded in a batch and rarely changes, the simpler storage model is sufficient — but the HTAP split doesn't hurt (the delta is simply empty and the union degenerates to a main-only scan).
