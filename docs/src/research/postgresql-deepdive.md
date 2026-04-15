# PostgreSQL as a High-Performance Triple Store

> *This page mirrors [`plans/postgresql-triplestore-deep-dive.md`](https://github.com/grove/pg-ripple/blob/main/plans/postgresql-triplestore-deep-dive.md) — the architectural blueprint written during the design phase of pg_ripple. It explains *why* each major design decision was made.*

---

## Introduction

Building a world-class triple store on top of a relational database requires bridging a fundamental gap: graph patterns (nodes and directed edges) vs. relational tables (rows and columns). The core challenge is to bridge this *structural impedance mismatch* without introducing unacceptable query latency.

Historically, RDF triple stores and relational databases have occupied competing ecosystems. Native triple stores excel at arbitrary graph traversal and schema flexibility; relational databases dominate in OLTP, strict concurrency control, ACID guarantees, and massively parallel analytical aggregation. PostgreSQL 18 provides the right foundation because it offers *both*: a mature cost-based optimizer, parallel query execution, robust MVCC, and an extension API powerful enough to add native SPARQL support.

The design decisions documented here are the result of evaluating several alternative approaches and selecting the one with the best combination of query performance, write throughput, and operational simplicity.

---

## Relational Storage Layouts for RDF Data

Three main layouts have been studied for storing triples in a relational database:

### Triple Table

The simplest approach: one table with three columns `(subject, predicate, object)`. Every SPARQL query touching N triple patterns requires an N-way self-join on this single table. At enterprise scale (billions of triples), even with comprehensive composite B-tree indexing across all column permutations (SPO, POS, OSP, PSO), complex queries exhaust `work_mem` and spill to disk — response times degrade exponentially.

**Verdict**: unsuitable for high-performance workloads.

### Property Tables

Groups subjects by ontological type into wide, property-per-column tables. Reduces self-joins for uniform entity types, but RDF data is inherently sparse and heterogeneous. Wide tables saturate with NULL values, waste disk space, and degrade sequential scan performance. Schema evolution requires blocking `ALTER TABLE` operations, negating the schema-agility advantage of RDF.

**Verdict**: not viable for open-world, schema-flexible graphs.

### Vertical Partitioning (VP) — pg_ripple's choice

A separate two-column table `(subject, object)` per unique predicate. This is the approach pioneered by Abadi et al. (VLDB 2007) and used by pg_ripple from day one.

**Why VP wins:**

| Property | Triple Table | Property Table | **VP** |
|---|---|---|---|
| Self-joins | N-way self-join | Low | **Per-predicate, targeted** |
| Sparsity | None | Heavy NULLs | **None** |
| Schema changes | None | Blocking DDL | **Dynamic per predicate** |
| I/O for bound predicate | Full scan | Full row | **Single predicate table** |
| Storage overhead | 33% for predicate col | Wide row | **Dense, no NULLs** |

The predicate column is *implicit* in the table name — eliminating 33% of storage per triple before any compression. Historical benchmarks (Barton library catalog dataset) show an order-of-magnitude improvement in query resolution time vs. Triple Table, dropping from minutes to seconds.

Each VP table has dual B-tree indices on `(s, o)` and `(o, s)`, enabling efficient merge joins regardless of traversal direction.

### Extended VP (ExtVP) — future direction

ExtVP builds on standard VP by pre-computing semi-joins between frequently co-joined predicates. When query profiling reveals that predicate A is frequently joined with predicate B on the subject column, a materialized view holds the subset of A where the subject also exists in B. The SPARQL→SQL translator rewrites queries to target these materialized views, bypassing billions of CPU cycles.

This is implemented in pg_ripple as a post-1.0 workload-driven optimization (see v0.11.0).

---

## Dictionary Encoding

Raw IRI strings (often 50–200 bytes) stored natively in VP tables would make integer joins infeasible. pg_ripple dictionary-encodes every IRI, blank node, and literal to a `BIGINT` before writing to any VP table.

### Design

- **Hash function**: XXH3-128 over `kind_le_bytes ‖ term_utf8` — the kind byte (IRI/blank-node/literal) is mixed into the hash so the same string with different term types gets distinct IDs. No 64-bit truncation: the full 16-byte hash is stored as a `BYTEA` collision-detection key; a PostgreSQL `GENERATED ALWAYS AS IDENTITY` sequence generates the dense, sequential `i64` join key.
- **Why not truncate to 64 bits?** The birthday problem: collisions expected at ~4 billion terms in a 64-bit space. With the full 128-bit hash stored separately, collision detection is a table lookup, not a hash comparison.
- **Inline encoding** (v0.5.1+): Common typed literals (`xsd:integer`, `xsd:boolean`, `xsd:dateTime`, `xsd:date`) are encoded inline with bit 63 set as a type tag. FILTER comparisons on these types require zero dictionary round-trips.

### Caching (v0.1.0–v0.5.1 vs. v0.6.0+)

- **v0.1.0–v0.5.1**: Backend-local `lru::LruCache<u128, i64>` — simple, no `shared_preload_libraries` dependency.
- **v0.6.0+**: Sharded `HashMap<u128, i64>` in shared memory via pgrx `PgSharedMem`, partitioned into 64 shards each with a per-shard lightweight lock. Eliminates global lock contention under concurrent encode/decode workloads. Sized by `pg_ripple.dictionary_cache_size` GUC.

### Query decoding

All output IDs are collected from a result set and decoded in a single `WHERE id = ANY(...)` query — never per-row. This "batch decode" pattern is critical for large result sets.

---

## HTAP Dual-Partition Architecture (v0.6.0)

Modern production systems need heavy reads and writes simultaneously. Without special care, writes block reads and vice versa.

### Delta / Main split

All INSERTs and DELETEs target the `_delta` partition (standard heap + B-tree). The `_main` partition is read-only, BRIN-indexed, and physically sorted by subject. Between them sits a tombstone table for cross-partition deletes.

**Query path**: `(main EXCEPT tombstones) UNION ALL delta`

When the tombstone table is empty (common between merges for insert-heavy workloads), this simplifies to a `UNION ALL` of main and delta.

### Background merge worker

A pgrx `BackgroundWorker` merges delta into main when delta exceeds `pg_ripple.merge_threshold` rows:

1. Create `vp_{id}_main_new` (empty heap)
2. `INSERT … SELECT … ORDER BY s FROM (main − tombstones) UNION ALL delta` — physically sorted, making BRIN maximally effective
3. `ALTER TABLE … RENAME` atomically replaces the old main
4. `TRUNCATE vp_{id}_delta, vp_{id}_tombstones`
5. `ANALYZE` on merged tables

The `ORDER BY s` at table-creation time is critical: BRIN requires physically sorted data for its summary ranges to be accurate. Inserting in random order into an existing main table degrades BRIN to near-uselessness.

### Bloom filters

Each VP table has a per-shard bloom filter in shared memory. Queries against data known to be only in main can skip the delta scan entirely.

### `vp_rare` exemption

Predicates with fewer than `pg_ripple.vp_promotion_threshold` triples are stored in a single flat `vp_rare` table instead of a dedicated VP table + delta/main split. Rare predicates see few writes by definition — delta/main overhead would exceed the benefit. Standard PostgreSQL row-level locking handles concurrent access safely.

### Why not partition on write?

PostgreSQL's declarative table partitioning is designed for stable partition keys (dates, ranges). RDF writes are effectively random across predicate space — using declarative partitioning here would require custom routing logic with no benefit over the explicit delta/main design.

---

## SPARQL-to-SQL Translation

Naive translation from SPARQL algebra to SQL produces verbose nested subqueries that confuse the PostgreSQL planner. pg_ripple implements several structural rewrites:

### Self-join elimination

Star patterns (same subject, N predicates) collapse into a single scan of the subject across N VP tables joined by subject ID equality. Eliminates redundant round-trips through SPI.

### Optional-to-inner downgrade

OPTIONAL → LEFT JOIN in SQL. When the optimizer can prove that the required property is always present (e.g., via a loaded SHACL shape with `sh:minCount 1`), it downgrades to INNER JOIN. Applied conservatively — only when semantics-preserving for the query domain.

### Filter pushdown

SPARQL FILTER clauses on bound IRIs are resolved to integer IDs *before* generating SQL. This ensures B-tree index usage. Typed numeric/date literals use the inline-encoded i64 range (v0.5.1+) to emit `BETWEEN $lo AND $hi` range scans with no decode step.

### Property path compilation

SPARQL `+`, `*`, `?` paths compile to `WITH RECURSIVE` CTEs with PG18's `CYCLE` clause for hash-based cycle detection:

```sql
WITH RECURSIVE path(s, o, depth) AS (
    SELECT s, o, 1 FROM _pg_ripple.vp_{id} WHERE s = $1
  UNION ALL
    SELECT p.s, vp.o, p.depth + 1
    FROM path p
    JOIN _pg_ripple.vp_{id} vp ON p.o = vp.s
    WHERE p.depth < pg_ripple.max_path_depth
)
CYCLE o SET is_cycle USING cycle_path
SELECT DISTINCT s, o FROM path WHERE NOT is_cycle;
```

PG18's `CYCLE` clause uses hash-based cycle detection ($O(1)$ membership checks vs. $O(n)$ array scans in the older array-based approach).

---

## SHACL Validation

SHACL is a W3C standard for defining data quality rules over RDF graphs. pg_ripple implements SHACL validation with *spec-first* semantics: loaded shapes are parsed into a shape IR that preserves W3C SHACL semantics. PostgreSQL constraints, triggers, or stream tables may be used as *internal accelerators* when they are proven semantics-preserving for the specific shape pattern — they are never the normative definition of constraint behavior.

### Sync vs. async validation

- **`pg_ripple.shacl_mode = 'sync'`** (v0.7.0): validation runs inline on INSERT; bad triples are rejected immediately. Suitable for low-volume transactional writes.
- **`pg_ripple.shacl_mode = 'async'`**: a lightweight trigger queues the inserted triple IDs into `_pg_ripple.validation_queue`; a background worker validates against loaded shapes. Invalid triples are moved to `_pg_ripple.dead_letter_queue` with a JSONB violation report.

### Query optimization via SHACL

The SPARQL→SQL translator reads loaded SHACL shapes at plan time:
- `sh:maxCount 1` may enable cardinality-sensitive optimizations when the query is restricted to the same focus-node population as the validated shape.
- `sh:minCount 1` may downgrade OPTIONAL → INNER JOIN when semantically safe.

---

## Performance Targets

| Metric | Target | Strategy |
|---|---|---|
| Bulk insert | >100K triples/sec | Batch COPY, deferred indexing, HTAP delta |
| Transactional insert | >10K triples/sec | Delta partition, async SHACL |
| Simple BGP query | <5 ms (10M triples) | Integer joins, B-tree on VP tables |
| Star query (5 patterns) | <20 ms (10M triples) | Self-join elimination, PG parallel hash joins |
| Property path (depth 10) | <100 ms (10M triples) | Recursive CTE + `CYCLE` clause |
| Dictionary encode (cache hit) | <1 μs | Sharded LRU in shared memory |
| Dictionary encode (miss) | <50 μs | B-tree index on hash |
| Batch decode (1,000 IDs) | <1 ms | Single `WHERE id = ANY(...)` query |

Calibration reference: QLever (C++, Apache-2.0) on DBLP (390M triples) loads at 1.7M triples/s and answers benchmark queries in 0.7s average. QLever's flat pre-sorted permutation files make every SPARQL join a merge join with zero random I/O. pg_ripple's B-tree/heap design pays ~5× overhead on bulk sequential scans in exchange for transactional concurrent writes, MVCC, and the full PostgreSQL ecosystem.

---

## Why PostgreSQL 18?

PostgreSQL 18 brings several features that materially improve pg_ripple's performance:

- **Async I/O** (`io_method = io_uring` on Linux): reduces sequential scan latency for the BRIN-indexed main partition
- **`CYCLE` clause in `WITH RECURSIVE`**: hash-based cycle detection in property path queries
- **Improved parallel query**: better parallel hash join plans for star-pattern BGPs
- **Skip scan on B-tree**: enables efficient lookups on composite `(s, o)` indices when only `o` is bound (unbound subject queries)

---

## Further Reading

- [plans/implementation_plan.md](https://github.com/grove/pg-ripple/blob/main/plans/implementation_plan.md) — detailed API signatures, algorithms, and crate choices
- [plans/postgresql-triplestore-deep-dive.md](https://github.com/grove/pg-ripple/blob/main/plans/postgresql-triplestore-deep-dive.md) — extended research survey with academic references
- [Abadi et al. 2007](https://dsf.berkeley.edu/cs286/papers/rdf-vldb2007.pdf) — original VP paper
- [qEndpoint](https://www.semantic-web-journal.net/content/qendpoint-novel-triple-store-architecture-large-rdf-graphs) — HTAP inspiration for the delta/main split
- [Berlin SPARQL Benchmark (BSBM)](http://wifo5-03.informatik.uni-mannheim.de/bizer/berlinsparqlbenchmark/) — query mix used in pg_ripple benchmarks
