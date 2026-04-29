# PostgreSQL Native Partitioning for pg_ripple VP Tables

> **Status:** Research report — speculative, not a roadmap commitment.
> **Relates to:** [future-directions.md § D.2 "Beyond Citus"](future-directions.md)
> **Date:** 2026-04-29

---

## 1. Executive summary

pg_ripple stores RDF triples in **Vertical Partitioning (VP) tables** — one
table per unique predicate. Each VP table is split into an HTAP delta/main/
tombstones trio. As knowledge graphs grow into the billions of triples, the
largest VP tables (common predicates like `rdf:type`, `rdfs:label`,
`owl:sameAs`) can contain hundreds of millions of rows. PostgreSQL's native
declarative partitioning — specifically **hash partitioning on the subject
column** — can transparently split these large VP tables into smaller physical
partitions, unlocking partition pruning, partition-wise joins, reduced index
bloat, and better CPU parallelism on multi-core hardware. This requires no
changes to the SPARQL query engine's SQL generation because the translator
already references VP tables through views, and PostgreSQL's planner handles
partition routing internally.

This report analyses the opportunity in depth: what partitioning strategies
apply, how to modify the storage DDL, what the SPARQL query engine gains,
what the merge worker must adapt to, and what the risks are.

---

## 2. Current VP storage architecture

### 2.1 Physical layout (v0.6.0+)

Each dedicated predicate `p` is stored as four objects in the `_pg_ripple`
schema:

```
_pg_ripple.vp_{p}_delta       — heap table, B-tree on (s,o) and (o,s), UNIQUE(s,o,g)
_pg_ripple.vp_{p}_main        — heap table, BRIN on (i)
_pg_ripple.vp_{p}_tombstones  — heap table, B-tree on (s,o,g)
_pg_ripple.vp_{p}             — view: (main LEFT JOIN tombstones) UNION ALL delta
```

Columns are uniformly `(s BIGINT, o BIGINT, g BIGINT, i BIGINT, source SMALLINT)`.
All values are dictionary-encoded integers (XXH3-128 hashes). No raw strings
exist in VP tables.

### 2.2 How the SPARQL translator references VP tables

The SPARQL→SQL translator (`src/sparql/sqlgen.rs`) resolves each predicate
atom to a `VpSource`:

```rust
match predicate_cache.resolve(pred_id) {
    Some(desc) if desc.dedicated => VpSource::Dedicated(format!("_pg_ripple.vp_{pred_id}")),
    Some(_)                      => VpSource::Rare(pred_id),
    None                         => VpSource::Empty,
}
```

For dedicated predicates, the generated SQL references the **view name**
`_pg_ripple.vp_{id}`. PostgreSQL expands the view to the underlying
`(main − tombstones) UNION ALL delta` automatically. The translator does not
generate table-specific SQL; it trusts the view abstraction.

**This is the key enabler for partitioning.** Because the translator references
a view — not the physical tables — we can replace the physical tables with
partitioned tables without any changes to `sqlgen.rs`, `bgp.rs`, or
`optimizer.rs`. The PostgreSQL planner will see the partitioned tables when
it expands the view and apply partition pruning, partition-wise joins, and
parallel append automatically.

### 2.3 Star-pattern join structure

Star patterns (same subject, multiple predicates) generate equi-joins on the
subject column:

```sql
-- SPARQL: ?person rdf:type foaf:Person . ?person foaf:name ?name .
SELECT t0.s AS person, t1.o AS name_id
FROM _pg_ripple.vp_291 t0          -- rdf:type
JOIN _pg_ripple.vp_42  t1 ON t1.s = t0.s   -- foaf:name
WHERE t0.o = 1847293               -- foaf:Person
```

The join condition `t1.s = t0.s` is exactly the pattern that **partition-wise
joins** are designed to accelerate: if both tables are hash-partitioned on `s`
with the same modulus, PostgreSQL can join corresponding partitions
independently, enabling parallelism and reduced memory pressure.

---

## 3. Partitioning strategy analysis

### 3.1 Why hash partitioning on subject

Three partitioning methods are available in PG18: range, list, and hash.
For VP tables:

| Method | Key | Fit | Rationale |
|--------|-----|-----|-----------|
| **Hash** | `s` (subject) | **Excellent** | Uniform distribution of dictionary-encoded BIGINT subjects. Star patterns join on `s`, enabling partition-wise joins across all VP tables. |
| Range | `i` (SID) | Moderate | Time-ordered partitioning for data lifecycle (cold eviction). Conflicts with the `s`-based access pattern of SPARQL queries. |
| Range | `s` | Poor | Subject IDs are hash-derived (XXH3-128 truncated to i64), not naturally ordered. Range boundaries would be arbitrary. |
| List | `g` (graph) | Niche | Useful for multi-tenant isolation but creates a variable number of partitions and breaks partition-wise joins on `s`. |

**Recommendation: Hash partitioning on `s` with a configurable modulus.**

The subject column is ideal because:
1. Dictionary encoding via XXH3-128 already distributes subjects uniformly.
2. The dominant access pattern — entity-centric SPARQL queries — filters on `s`.
3. Star-pattern joins across multiple VP tables all share the same join key (`s`).
4. Hash partitioning requires no maintenance (no rebalancing, no boundary management).

### 3.2 Partition count selection

PostgreSQL hash partitioning uses a fixed modulus at table creation time.
The number of partitions affects:

| Factor | Fewer partitions (4–8) | More partitions (16–64) |
|--------|------------------------|-------------------------|
| Planning overhead | Lower | Higher (each partition adds planner work) |
| Partition-wise join benefit | Lower parallelism ceiling | More parallel workers can be utilised |
| Per-partition index size | Larger | Smaller (better cache hit ratio) |
| Open file descriptors | Lower | Higher (3× files per partition: heap + TOAST + index) |
| Lock contention | Lower | Lower (writes spread across more partitions) |

**Recommendation:** Default to **16 partitions** (`MODULUS 16, REMAINDER 0..15`).
This is a sweet spot for knowledge graphs in the 100M–10B triple range:
- 16 partitions × ~100 predicates = ~1,600 physical tables, well within PG18's
  comfortable range (best practices say up to a few thousand partitions).
- 16 is the default `max_parallel_workers_per_gather` on many systems, matching
  the parallelism ceiling.
- Expose as a GUC: `pg_ripple.vp_partition_count` (default: 16, minimum: 1,
  maximum: 256). Value of 1 means no partitioning (backward-compatible default).

### 3.3 Sub-partitioning: HTAP split × hash partitioning

Today the HTAP split creates three physical tables per predicate. With hash
partitioning, two design options exist:

**Option A: Partition the main table only (recommended)**

```
vp_{p}_delta         — unpartitioned heap (small, recently written)
vp_{p}_main          — PARTITION BY HASH (s), 16 partitions
vp_{p}_tombstones    — unpartitioned heap (small, pending deletes)
vp_{p}               — view: (main − tombstones) UNION ALL delta
```

Rationale:
- Delta is intentionally small (flushed by the merge worker). Partitioning it
  adds overhead for no benefit.
- Main is the large, read-heavy table. Partitioning it delivers the scan and
  join benefits.
- Tombstones are small and short-lived (cleared each merge cycle).

**Option B: Partition all three tables**

```
vp_{p}_delta         — PARTITION BY HASH (s), 16 partitions
vp_{p}_main          — PARTITION BY HASH (s), 16 partitions
vp_{p}_tombstones    — PARTITION BY HASH (s), 16 partitions
vp_{p}               — view: (main − tombstones) UNION ALL delta
```

Rationale:
- Full partition-wise join between main and tombstones during the merge view
  evaluation.
- Delta inserts are spread across partitions, reducing per-partition lock
  contention for concurrent writes.

**Option A is recommended** for the initial implementation because delta and
tombstones are designed to be small, and the merge worker already processes
them in a single transaction. Option B can be evaluated later if write
contention on delta becomes a bottleneck.

---

## 4. DDL changes

### 4.1 Modified `ensure_htap_tables()` for partitioned main

The core change is in `src/storage/merge.rs::ensure_htap_tables()`. Today it
creates `vp_{id}_main` as a flat heap table. With partitioning enabled, it
creates a partitioned table and attaches child partitions:

```sql
-- Only when pg_ripple.vp_partition_count > 1:

CREATE TABLE _pg_ripple.vp_{id}_main (
    s      BIGINT   NOT NULL,
    o      BIGINT   NOT NULL,
    g      BIGINT   NOT NULL DEFAULT 0,
    i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'),
    source SMALLINT NOT NULL DEFAULT 0
) PARTITION BY HASH (s);

-- Create child partitions:
CREATE TABLE _pg_ripple.vp_{id}_main_p0
    PARTITION OF _pg_ripple.vp_{id}_main
    FOR VALUES WITH (MODULUS 16, REMAINDER 0);
CREATE TABLE _pg_ripple.vp_{id}_main_p1
    PARTITION OF _pg_ripple.vp_{id}_main
    FOR VALUES WITH (MODULUS 16, REMAINDER 1);
-- ... through p15

-- BRIN index on the partitioned table (auto-created on each partition):
CREATE INDEX idx_vp_{id}_main_i_brin
    ON _pg_ripple.vp_{id}_main USING BRIN (i);

-- B-tree indexes for subject and object lookups (also auto-created):
CREATE INDEX idx_vp_{id}_main_s ON _pg_ripple.vp_{id}_main (s);
CREATE INDEX idx_vp_{id}_main_o ON _pg_ripple.vp_{id}_main (o);
```

### 4.2 Index strategy for partitioned main

On unpartitioned tables, BRIN on `i` is effective because the merge worker
writes rows sorted by `s`, and `i` (SID) is monotonically increasing. With
hash partitioning, each partition receives a subset of subjects, and the
physical row order within a partition depends on the insert order from the
merge worker.

**Index recommendations per partition:**

| Index | Type | Columns | Purpose |
|-------|------|---------|---------|
| Primary scan | B-tree | `(s, o)` | Subject-bound lookups (star patterns) |
| Reverse scan | B-tree | `(o, s)` | Object-bound lookups (inverse patterns) |
| SID BRIN | BRIN | `(i)` | Merge-cycle identification, cold eviction |

BRIN remains useful for `i`-based range scans (e.g., "all triples inserted
after SID X"), but B-tree on `(s, o)` becomes the primary access path for
SPARQL queries. This is a tradeoff: more index space per partition, but each
index is 1/16th the size and more likely to fit in `shared_buffers`.

### 4.3 View definition — no change required

The VP view definition remains unchanged:

```sql
CREATE OR REPLACE VIEW _pg_ripple.vp_{id} AS
SELECT DISTINCT ON (s, o, g) s, o, g, i, source
FROM (
    SELECT m.s, m.o, m.g, m.i, m.source
    FROM _pg_ripple.vp_{id}_main m
    LEFT JOIN _pg_ripple.vp_{id}_tombstones t
        ON m.s = t.s AND m.o = t.o AND m.g = t.g
    WHERE t.s IS NULL
    UNION ALL
    SELECT d.s, d.o, d.g, d.i, d.source
    FROM _pg_ripple.vp_{id}_delta d
) merged
ORDER BY s, o, g, i ASC;
```

PostgreSQL treats `_pg_ripple.vp_{id}_main` as a partitioned table and
expands it into an `Append` node over child partitions automatically. When
the SPARQL query binds `s` (e.g., `WHERE t0.s = 12345`), the planner prunes
all partitions except the one matching `12345 % 16`.

---

## 5. Query engine benefits

### 5.1 Partition pruning for subject-bound queries

The most common SPARQL access pattern is entity-centric: "give me all facts
about subject X". After SPARQL→SQL translation, this becomes:

```sql
SELECT t0.o FROM _pg_ripple.vp_{p} t0 WHERE t0.s = 12345;
```

With hash partitioning on `s`, PostgreSQL prunes 15 of 16 partitions before
scanning. The effective table size for this query is 1/16th of the full VP
table. Combined with a B-tree index on `(s, o)` per partition, this is an
index lookup into a partition that is likely entirely in `shared_buffers`.

**Expected speedup:** For large VP tables (>10M rows), subject-bound lookups
should see 5–15× improvement from pruning + smaller indexes.

### 5.2 Partition-wise joins for star patterns

Consider a 3-predicate star pattern:

```sql
SELECT t0.s, t1.o, t2.o
FROM _pg_ripple.vp_100 t0
JOIN _pg_ripple.vp_200 t1 ON t1.s = t0.s
JOIN _pg_ripple.vp_300 t2 ON t2.s = t0.s
WHERE t0.o = 9876;
```

If all three VP tables are hash-partitioned on `s` with the same modulus,
and `enable_partitionwise_join = on`, PostgreSQL will decompose this into
16 independent joins — one per partition — that can execute in parallel.

Without partition-wise joins, PostgreSQL must build a single hash table
spanning all rows of `vp_200` and `vp_300`. With partition-wise joins, each
parallel worker builds a hash table covering only 1/16th of the data.

**Expected speedup:** Proportional to `min(partition_count, max_parallel_workers_per_gather)`.
On a 16-core machine with 16 partitions, a full-scan star-pattern query can
use all cores, achieving up to 8–16× speedup for analytical queries.

### 5.3 Partition-wise aggregation

SPARQL aggregates (`COUNT`, `SUM`, `AVG`, `GROUP BY`) that operate per-subject
benefit from `enable_partitionwise_aggregate = on`. Each partition computes
partial aggregates independently, and PostgreSQL merges them in a finalize
step. This reduces memory pressure and improves cache locality.

### 5.4 Parallel append for unbound scans

Queries that scan an entire VP table (e.g., `SELECT ?s ?o WHERE { ?s rdf:type ?o }`)
benefit from `enable_parallel_append = on`. PostgreSQL assigns different
partitions to different parallel workers, achieving near-linear speedup for
sequential scans.

### 5.5 No changes needed in `sqlgen.rs`

The SPARQL→SQL translator does not need to know about partitioning. It
generates SQL that references the view `_pg_ripple.vp_{id}`. PostgreSQL's
planner handles partition expansion, pruning, partition-wise joins, and
parallel execution transparently. This is the single strongest argument for
using native partitioning over application-level sharding.

---

## 6. Merge worker adaptations

### 6.1 Fresh-table generation merge with partitions

Today the merge worker creates a new flat table `vp_{id}_main_new`, populates
it with `(main − tombstones) UNION ALL delta ORDER BY s`, then atomically
renames it to `vp_{id}_main`. With a partitioned main table, this approach
needs modification.

**Option M1: Partition-aware CTAS (recommended)**

```sql
-- Step 1: Create partitioned replacement table
CREATE TABLE _pg_ripple.vp_{id}_main_new (
    s BIGINT NOT NULL, o BIGINT NOT NULL,
    g BIGINT NOT NULL DEFAULT 0, i BIGINT NOT NULL,
    source SMALLINT NOT NULL DEFAULT 0
) PARTITION BY HASH (s);

-- Step 2: Create child partitions
CREATE TABLE _pg_ripple.vp_{id}_main_new_p0
    PARTITION OF _pg_ripple.vp_{id}_main_new
    FOR VALUES WITH (MODULUS 16, REMAINDER 0);
-- ... through p15

-- Step 3: Populate (INSERT INTO automatically routes to correct partition)
INSERT INTO _pg_ripple.vp_{id}_main_new (s, o, g, i, source)
SELECT m.s, m.o, m.g, m.i, m.source
FROM _pg_ripple.vp_{id}_main m
LEFT JOIN _pg_ripple.vp_{id}_tombstones t
    ON m.s = t.s AND m.o = t.o AND m.g = t.g
WHERE t.s IS NULL
UNION ALL
SELECT d.s, d.o, d.g, d.i, d.source
FROM _pg_ripple.vp_{id}_delta d;

-- Step 4: Create indexes on partitioned table
CREATE INDEX idx_vp_{id}_main_new_s_o ON _pg_ripple.vp_{id}_main_new (s, o);
CREATE INDEX idx_vp_{id}_main_new_o_s ON _pg_ripple.vp_{id}_main_new (o, s);
CREATE INDEX idx_vp_{id}_main_new_brin ON _pg_ripple.vp_{id}_main_new USING BRIN (i);

-- Step 5: Atomic swap
DROP TABLE _pg_ripple.vp_{id}_main;
ALTER TABLE _pg_ripple.vp_{id}_main_new RENAME TO vp_{id}_main;
-- Rename partitions too:
ALTER TABLE _pg_ripple.vp_{id}_main_new_p0 RENAME TO vp_{id}_main_p0;
-- ... through p15

-- Step 6: Recreate view (references new table)
-- Step 7: TRUNCATE delta and tombstones
-- Step 8: ANALYZE
```

**Partition renames:** PostgreSQL does not automatically rename child
partitions when the parent is renamed. The merge worker must rename each
child partition individually. This adds O(partition_count) DDL statements
to each merge cycle but each is a metadata-only operation (instant).

**Option M2: In-place merge (no table swap)**

Instead of creating a replacement table, merge in-place:

1. DELETE rows from main that appear in tombstones.
2. INSERT INTO main SELECT * FROM delta.
3. TRUNCATE tombstones and delta.

This is simpler with partitioned tables (no need to recreate the partition
structure), but it does not preserve BRIN effectiveness (rows are not sorted),
and DELETE + INSERT on partitioned tables is slower than a fresh CTAS because
each row must be routed. Not recommended for the initial implementation.

**Recommendation:** Option M1 with the full partition-aware table swap.

### 6.2 Parallel merge per partition

A future optimization is to merge each partition independently:

1. For each partition `p0..p15`, create `vp_{id}_main_new_pN` from the
   corresponding partition of main + matching delta rows.
2. Detach the old partition, attach the new one.

This enables **parallel merge execution** — multiple workers each handling
one partition — and is the natural evolution once the basic partitioned merge
is proven. It also enables **incremental merge**: only partitions with delta
activity need to be merged, skipping idle partitions entirely.

---

## 7. Predicate promotion with partitioning

When a predicate exceeds `pg_ripple.vp_promotion_threshold` (default: 1,000
triples) in `vp_rare`, it is promoted to a dedicated VP table. With
partitioning enabled, the promotion path in `src/storage/promote.rs` must:

1. Create the partitioned main table structure (same as `ensure_htap_tables`).
2. Copy rows from `vp_rare` into the partitioned main (PostgreSQL routes each
   row to the correct partition automatically via `INSERT INTO ... SELECT`).
3. Create delta, tombstones, and view as today.

The existing two-phase promotion (shadow copy + atomic rename) remains
valid. The only change is that the shadow copy target is now a partitioned
table.

---

## 8. Migration path for existing installations

Existing pg_ripple deployments have unpartitioned VP tables. A migration
strategy must convert them to partitioned tables without downtime.

### 8.1 Online migration via merge worker

The simplest migration leverages the existing merge cycle:

1. Set `pg_ripple.vp_partition_count = 16` in `postgresql.conf` and reload.
2. The next merge cycle for each predicate creates `vp_{id}_main_new` as a
   **partitioned** table, populates it via INSERT ... SELECT (which routes
   rows to partitions), and atomically swaps it in.
3. After one full merge cycle, all VP tables are partitioned.

This is a zero-downtime migration. The merge worker already performs a full
table swap each cycle; the only difference is that the replacement table
is partitioned.

**Estimated migration time:** Proportional to the total VP data size.
For 1B triples across 100 predicates, with each merge cycle processing
one predicate at ~500K rows/sec, full migration takes approximately
1B / 500K = 2,000 seconds ≈ 33 minutes. Each predicate is migrated
independently, so the system remains queryable throughout.

### 8.2 Reverting to unpartitioned

If partitioning is disabled (`pg_ripple.vp_partition_count = 1`), the next
merge cycle creates unpartitioned replacement tables, effectively reverting.
This makes the feature fully reversible.

---

## 9. Configuration

### 9.1 New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.vp_partition_count` | integer | 1 | Number of hash partitions for VP main tables. 1 = no partitioning (backward-compatible). Must be a power of 2 between 1 and 256. |
| `pg_ripple.partitionwise_join` | boolean | true | Whether to set `enable_partitionwise_join = on` in sessions that execute SPARQL queries. Only effective when `vp_partition_count > 1`. |
| `pg_ripple.partitionwise_aggregate` | boolean | true | Whether to set `enable_partitionwise_aggregate = on` in sessions that execute SPARQL queries. |

### 9.2 Recommended `postgresql.conf` settings

When VP partitioning is enabled, users should also tune:

```ini
# Enable partition-wise joins (OFF by default in PG18 due to planning overhead)
enable_partitionwise_join = on

# Enable partition-wise aggregation
enable_partitionwise_aggregate = on

# Partition pruning (already ON by default)
enable_partition_pruning = on

# Increase parallel workers for large graphs
max_parallel_workers_per_gather = 8   # Up from default 2
max_parallel_workers = 16             # Match partition count

# Increase work_mem for partition-wise joins
# (each partition join gets its own work_mem allocation)
work_mem = '256MB'                    # Up from default 4MB
```

**Warning:** `enable_partitionwise_join` and `enable_partitionwise_aggregate`
are OFF by default in PostgreSQL because they increase planning time and per-
query memory usage proportional to partition count. For pg_ripple's workload
this tradeoff is almost always worthwhile, but operators of memory-constrained
systems should test before enabling globally.

---

## 10. What partitioning does NOT help

### 10.1 Object-bound queries

Queries that filter on `o` (object) without binding `s` cannot benefit from
`s`-based hash partitioning. The planner must scan all partitions:

```sql
-- "Who has email alice@example.com?"
SELECT t0.s FROM _pg_ripple.vp_87 t0 WHERE t0.o = 54321;
-- All 16 partitions scanned (no pruning on o)
```

**Mitigation:** The B-tree index on `(o, s)` per partition still provides
fast index lookups. The parallel append across partitions compensates for the
lack of pruning. For workloads dominated by object-bound queries, dual hash
partitioning (on both `s` and `o`) is theoretically possible via sub-
partitioning, but dramatically increases the number of physical tables.

### 10.2 Cross-predicate joins with different partition counts

Partition-wise joins require **identical partition key, type, and modulus**
on both sides. If VP tables have different partition counts (e.g., one has
16, another has 32), partition-wise joins are not used. pg_ripple must
enforce a uniform partition count across all VP tables.

### 10.3 Small VP tables

VP tables with fewer than ~100K rows gain nothing from partitioning. The
overhead of 16 partitions (metadata, planning time, file descriptors) exceeds
the benefit. pg_ripple should only partition VP tables above a configurable
threshold:

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.vp_partition_min_rows` | 100000 | Minimum row count in main before the merge worker creates a partitioned replacement. Below this threshold, main remains unpartitioned. |

This means a mixed estate: some VP tables partitioned, some not.
Partition-wise joins only apply when both sides are partitioned with the same
modulus. The SPARQL planner does not need to know — PostgreSQL handles this.

### 10.4 vp_rare

The consolidated rare-predicate table `_pg_ripple.vp_rare` is shared by many
predicates. Partitioning it by `s` would not help because queries filter by
`p` first. Partitioning by `p` (list partitioning) is possible but defeats
the purpose of consolidation. **vp_rare should remain unpartitioned.**

---

## 11. Interaction with other features

### 11.1 HTAP cold tier (storage tiering)

Hash partitioning on `s` for the main table is orthogonal to the cold-tier
work described in [storage-tiering-slatedb-duckdb.md](storage-tiering-slatedb-duckdb.md).
Cold eviction operates on the main table's `i` (SID) column for age-based
eviction. With a partitioned main, the merge worker can evict old rows from
each partition independently, or detach entire partitions and archive them.

A future enhancement: **range sub-partitioning on `i`** within each hash
partition, enabling `DETACH PARTITION` for instant cold eviction of time-
bounded data:

```sql
CREATE TABLE _pg_ripple.vp_{id}_main (s, o, g, i, source)
    PARTITION BY HASH (s);

CREATE TABLE _pg_ripple.vp_{id}_main_p0
    PARTITION OF _pg_ripple.vp_{id}_main
    FOR VALUES WITH (MODULUS 16, REMAINDER 0)
    PARTITION BY RANGE (i);

CREATE TABLE _pg_ripple.vp_{id}_main_p0_current
    PARTITION OF _pg_ripple.vp_{id}_main_p0
    FOR VALUES FROM (MINVALUE) TO (MAXVALUE);
```

When the cold tier is ready, the merge worker splits `_p0_current` into a
`_p0_hot` and `_p0_cold` range partition, then detaches `_p0_cold` for
archival. This is a post-v2.0 consideration.

### 11.2 Citus integration

pg_ripple's Citus integration (v0.59+) distributes VP tables across Citus
worker nodes. Citus uses its own sharding (hash distribution on a chosen
column). **Native PG18 partitioning and Citus distribution are orthogonal:**

- Citus distributes across nodes (horizontal scaling).
- PG18 partitioning divides within a node (vertical scaling).
- A VP table can be both Citus-distributed and locally partitioned.

However, the interaction is complex: Citus creates distributed partitioned
tables with its own metadata, and the merge worker must account for both
layers. **Recommendation:** Do not enable PG18 partitioning on Citus-
distributed VP tables in the first release. Support the combination in a
follow-up.

### 11.3 RLS (Row-Level Security)

pg_ripple applies graph-based RLS to VP tables
(`src/security_api::apply_rls_to_vp_table`). RLS policies on a partitioned
table are automatically inherited by all child partitions. No code changes
needed.

### 11.4 Datalog inference

The Datalog reasoner (`src/datalog/`) generates recursive CTEs that read from
and write to VP tables. Partitioned VP tables are transparent to CTEs — they
work through the same view abstraction. No changes needed.

### 11.5 CONSTRUCT rules and IVM

CONSTRUCT writeback rules (`src/construct_rules/`) insert into VP delta
tables. Since delta remains unpartitioned (Option A), no changes needed.

---

## 12. Benchmarking plan

Before committing to partitioned VP tables, the following benchmarks should
be run on a representative dataset (BSBM 1B triples or WatDiv 100M triples):

| Benchmark | Metric | Without partitioning | With 16 partitions |
|-----------|--------|---------------------|-------------------|
| Single-entity lookup | Latency (p99) | Baseline | Expected: 5–15× faster |
| Star-pattern join (3 predicates) | Latency (p50, p99) | Baseline | Expected: 4–8× faster (partition-wise join) |
| Full predicate scan (unbound) | Throughput (rows/s) | Baseline | Expected: 2–4× faster (parallel append) |
| SPARQL aggregate (COUNT GROUP BY) | Latency | Baseline | Expected: 2–4× faster (partition-wise aggregate) |
| Object-bound lookup | Latency (p99) | Baseline | Expected: ≤1.2× (no pruning, parallel append only) |
| Merge cycle time | Duration | Baseline | Expected: 1.5–2× slower (partition DDL overhead) |
| Bulk load (1M triples) | Throughput (triples/s) | Baseline | Expected: ~equal (inserts go to unpartitioned delta) |
| Planning time | Latency (planning only) | Baseline | Expected: 1.2–1.5× slower |
| Memory usage | RSS per backend | Baseline | Expected: 1.1–1.3× higher (partition metadata) |

**Gate:** Partition-wise join speedup must exceed 3× on the star-pattern
benchmark for the feature to ship as a default. If planning-time regression
exceeds 2× on simple queries, the feature ships as opt-in only.

---

## 13. Implementation roadmap

### Phase 1: Infrastructure (post-v1.0.0)

1. Add `pg_ripple.vp_partition_count` GUC (default: 1 = no partitioning).
2. Modify `ensure_htap_tables()` to create partitioned main when count > 1.
3. Modify `merge_predicate()` to create partitioned replacement tables.
4. Add partition rename logic to the merge worker.
5. Add `pg_ripple.vp_partition_min_rows` GUC for the promotion threshold.
6. Run BSBM and WatDiv benchmarks.

### Phase 2: Optimization (v1.2+)

7. Auto-set `enable_partitionwise_join` and `enable_partitionwise_aggregate`
   in SPI sessions used by the SPARQL engine.
8. Implement per-partition parallel merge.
9. Add `pg_ripple.partition_stats()` admin function reporting per-partition
   row counts, sizes, and index sizes.
10. Add migration guide to documentation.

### Phase 3: Advanced (v2.0+)

11. Range sub-partitioning on `i` for instant cold eviction.
12. Evaluate Citus + local partitioning combination.
13. Investigate partition-level RLS for per-tenant isolation.

---

## 14. Risks and mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Planning-time regression** for queries touching many VP tables | Medium | Gate behind GUC; benchmark on realistic workloads; ensure `pg_ripple.vp_partition_min_rows` keeps small tables unpartitioned |
| **Merge worker slowdown** from partition DDL | Medium | Partition DDL is metadata-only; INSERT routing overhead is ~5% per benchmarks in PG18 release notes |
| **File descriptor exhaustion** with many predicates × many partitions | Medium | 100 predicates × 16 partitions × 3 files = 4,800 FDs; well within default `ulimit -n 65536` but operators must be aware |
| **Mixed partition counts** across VP tables break partition-wise joins | Low | Enforce uniform partition count; fail-safe: PostgreSQL falls back to standard joins gracefully |
| **Backward incompatibility** with tools expecting flat tables | Low | View abstraction hides partitioning; tools that query views are unaffected |
| **VACUUM and ANALYZE overhead** increases linearly with partition count | Low | PG18 auto-vacuum handles partitions individually; each partition is smaller so VACUUM is faster per-partition |

---

## 15. Conclusion

PostgreSQL 18's declarative hash partitioning is a natural fit for pg_ripple's
VP storage model. Hash partitioning on the subject column (`s`) aligns with
the dominant SPARQL access pattern (entity-centric queries and star-pattern
joins), enables partition pruning and partition-wise joins without changes to
the SPARQL→SQL translator, and provides a single-node scale-up path that does
not require Citus.

The implementation is a storage-layer change localized to `ensure_htap_tables`
and `merge_predicate`, gated behind a GUC, fully reversible via the merge
worker, and transparent to all higher layers (SPARQL engine, Datalog reasoner,
SHACL validator, CONSTRUCT rules, CDC, HTTP service).

**Recommended posture: Invest post-v1.0.0.** Implement Phase 1 as an opt-in
feature, benchmark on BSBM/WatDiv, and promote to default-on if the star-
pattern join speedup exceeds 3×.
