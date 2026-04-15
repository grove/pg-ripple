# pg_ripple — Implementation Plan

## 1. Project Overview

**pg_ripple** is a PostgreSQL 18 extension written in Rust using pgrx 0.17 that implements a high-performance, scalable RDF triple store. It brings native SPARQL query capability, dictionary-encoded storage with vertical partitioning, HTAP architecture, SHACL validation, and optional distributed execution—all within PostgreSQL.

### Design Principles

- **Performance first**: Dictionary-encoded integers, vertical partitioning, zero-copy Rust data paths
- **PostgreSQL-native**: Leverage the optimizer, MVCC, WAL, parallel query, AIO (PG18), and skip scan
- **Safe Rust**: Use pgrx 0.17's safe abstractions; `unsafe` only at FFI boundaries where required
- **Incremental adoption**: Usable from the first release; advanced features layered progressively
- **Standards compliance**: W3C RDF 1.1, SPARQL 1.1, SHACL Core

---

## 2. Technology Stack

| Layer | Technology |
|---|---|
| Language | Rust (Edition 2024) |
| PG binding | `pgrx` 0.17 (`pg18` feature flag) |
| PostgreSQL | 18.x |
| SPARQL parser | `spargebra` crate (W3C-compliant SPARQL 1.1 algebra) |
| SPARQL optimizer | `sparopt` crate (Apache-2.0/MIT; first-pass algebra optimizer fed from `spargebra` output; adds filter pushdown, constant folding, empty-pattern elimination before pg_ripple's own pass; v0.3.0+) — **verify crates.io availability and API stability before v0.3.0 begins; fallback: inline these optimizations into `src/sparql/algebra.rs`** |
| RDF parsers | `rio_turtle`, `rio_xml` crates (Turtle, N-Triples, RDF/XML); `oxttl` / `oxrdf` added at v0.4.0 for RDF-star |
| Hashing | `xxhash-rust` (XXH3-128 for dictionary collision resistance) |
| Serialization | `serde` + `serde_json` (SHACL reports, SPARQL results, config) |
| HTTP server | `axum` (built on tokio) — SPARQL Protocol HTTP endpoint (`pg_ripple_http` binary) |
| PG client (HTTP service) | `tokio-postgres` + `deadpool-postgres` — async connection pool from HTTP service to PostgreSQL |
| HTTP client (federation) | `reqwest` — outbound calls to remote SPARQL endpoints (SERVICE keyword) |
| Testing | pgrx `#[pg_test]`, `cargo pgrx regress`, pgbench via `pgrx-bench`, `proptest`, `cargo-fuzz` |
| IVM (optional) | `pg_trickle` — stream tables, incremental view maintenance ([analysis](ecosystem/pg_trickle.md)) |
| Datalog (optional) | Built-in reasoning engine — RDFS/OWL RL entailment + user-defined rules ([design](ecosystem/datalog.md)) |

---

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                     Client Layer                        │
│  SPARQL endpoint (SQL function)  │  SQL/SPI interface   │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│               Query Translation Engine                   │
│  SPARQL Parser → Algebra IR → SQL Generator              │
│  Join minimization · Filter pushdown · CTE compilation   │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│                 Storage Engine                            │
│  Dictionary Encoder ←→ VP Tables (per-predicate)         │
│  Delta partition (OLTP) │ Main partition (OLAP)          │
│  BRIN + B-tree indices  │ Bloom filters                  │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│              Validation & Governance                      │
│  SHACL → DDL constraints  │  Async CDC validation        │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│              Reasoning Layer (src/datalog/)               │
│  Datalog parser · Stratifier · SQL compiler              │
│  Built-in: RDFS (13 rules) · OWL RL (~80 rules)         │
│  Modes: on-demand (inline CTEs) │ materialized (↓)       │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│        Reactivity Layer (optional — pg_trickle)          │
│  Stream tables: ExtVP │ Inference │ Stats │ SPARQL Views │
│  IVM engine · DAG scheduler · CDC triggers               │
└─────────────────────────────────────────────────────────┘
```

---

## 4. Module Breakdown

### 4.1 Extension Bootstrap (`src/lib.rs`)

- pgrx `#[pg_extern]` entry points
- `_PG_init()` hook for background worker startup (v0.6.0+: also shared memory registration)
- GUC parameters: `pg_ripple.default_graph`, `pg_ripple.dictionary_cache_size`, `pg_ripple.merge_threshold`, `pg_ripple.shacl_mode`, `pg_ripple.inference_mode`, `pg_ripple.named_graph_optimized` (adds G-leading index on each VP table; off by default) — see §4.11 for the full canonical GUC reference
- **GUC-gated lazy initialization**: the merge worker, SHACL validator, and reasoning engine are only started when their respective GUCs (`pg_ripple.merge_threshold > 0`, `pg_ripple.shacl_mode != 'off'`, `pg_ripple.inference_mode != 'off'`) are active. `_PG_init` never starts subsystems the user has not enabled. See §4.11 for the full canonical GUC reference.
- **Error taxonomy module** (`src/error.rs`, v0.1.0): `thiserror`-based error types with PT error code constants. Initial ranges: dictionary errors (PT001–PT099) and storage errors (PT100–PT199). PostgreSQL-style formatting: lowercase first word, no trailing period. Extended in subsequent milestones as new subsystems are added (see §13.6 for the complete range table).
- **Dictionary cache phasing**: v0.1.0–v0.5.1 use a **backend-local** `lru::LruCache` for the dictionary cache — no `shared_preload_libraries` required. The shared-memory dictionary cache, bloom filters, slot versioning, and `pg_ripple.shared_memory_size` startup GUC are all introduced in v0.6.0 when the HTAP architecture requires cross-backend coordination. This significantly simplifies the first 6 releases and defers the most complex pgrx API surface to when it is actually needed.
- **Shared-memory slot versioning** (v0.6.0+): the first 16 bytes of every `PgSharedMem` slot are a fixed magic number followed by a 4-byte layout version integer. On startup the extension checks both; a mismatch (e.g. after an in-place upgrade) triggers a controlled re-initialization rather than a silent crash.
- **pgrx 0.17 shared memory API** (v0.6.0+): the shared memory surface in pgrx 0.17 uses the `PgSharedObject` trait and `PgSharedMem::new_array` / `PgSharedMem::new_object` constructors — a substantial redesign from the `PgSharedMem` API used in pgrx ≤0.14. The implementation must follow the [pgrx 0.17 shared memory examples](https://github.com/pgcentralfoundation/pgrx/tree/develop/pgrx-examples/shmem) and declare all allocation sizes at `_PG_init` time via the `pg_shmem_init!` macro. Shared memory block size is determined at postmaster start by the `pg_ripple.shared_memory_size` GUC (a startup GUC in `postgresql.conf`); it cannot be grown at runtime. The `pg_ripple.cache_budget` GUC is a utilization cap enforced in Rust, not a re-allocation signal.
- Extension SQL: `CREATE EXTENSION pg_ripple` creates core schema and catalog tables

### 4.2 Dictionary Encoder (`src/dictionary/`)

**Purpose**: Map every IRI, blank node, and literal to a compact `i64` identifier.

#### 4.2.1 Schema

A single unified dictionary table holds all term types (IRIs, blank nodes, and literals). Using one table with one IDENTITY sequence guarantees that every dictionary ID is globally unique — the decode path can always resolve an `i64` to exactly one term without ambiguity. This is the approach used by RDF4J, Blazegraph, and Oxigraph.

```sql
-- Unified dictionary (IRIs, blank nodes, and literals)
CREATE TABLE _pg_ripple.dictionary (
    id        BIGINT   GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash      BYTEA    NOT NULL,  -- full 16-byte XXH3-128 of (kind_le_bytes || term_utf8)
    value     TEXT     NOT NULL,
    kind      SMALLINT NOT NULL,  -- 0=IRI, 1=blank node, 2=literal, 3=typed-literal, 4=lang-literal
    datatype  TEXT,               -- for literals: xsd datatype IRI (kind=3)
    lang      TEXT                -- for language-tagged literals (kind=4)
);
CREATE UNIQUE INDEX ON _pg_ripple.dictionary (hash);
```

> **Design decision — Hash-Backed Sequence (Route 2)**: The XXH3-128 hash is stored in full (16 bytes, `BYTEA`) and used as the *collision-detection key* via `ON CONFLICT (hash) DO NOTHING`. The dense `i64` join key used in every VP table is the IDENTITY-generated `id` — a sequential integer independent of the hash. This avoids the birthday-problem collision risk of schemes that truncate the 128-bit hash to 64 bits (collision expected around 4 billion terms in the 64-bit variant). The `kind` discriminant is mixed into the hash input as two little-endian bytes — so the same string encoded as an IRI and as a blank node always maps to distinct rows. For the encode path, this yields an `ENCODE_CACHE` keyed on the full `u128` hash.

> **Earlier alternative rejected**: Separate `resource_dict` and `literal_dict` tables, each with their own IDENTITY sequence, create an ID space collision — `resource_dict.id = 42` and `literal_dict.id = 42` can both exist, making the decode path ambiguous. A unified table eliminates this class of bugs at zero performance cost (lookups are by `id` primary key or `hash` unique index — both $O(1)$ regardless of table size).

#### 4.2.2 Implementation

- **Encoding path** (`encode(term, kind)`): Compute XXH3-128 over `kind_le_bytes || term_utf8` → check `ENCODE_CACHE (u128 → i64)` for a cache hit → `INSERT … ON CONFLICT (hash) DO NOTHING RETURNING id` → if no row returned (conflict), `SELECT id … WHERE hash = $1` → populate both caches → return the IDENTITY `i64`
- **Decoding path** (`decode(id)`): `i64` → `DECODE_CACHE (i64 → String)` → `SELECT value … WHERE id = $1` → populate cache → return string
- **Batch decoding** (`decode_batch()`): Collect all output `i64` IDs from a result set, resolve in a single `WHERE id = ANY(...)` query, build an in-memory `HashMap<i64, String>`, then emit decoded rows. Avoids per-row dictionary round-trips — critical for large result sets
- **Batch encoding** (`encode_batch()`): Bulk insert with `ON CONFLICT DO NOTHING` + `RETURNING`, minimising round-trips during data load
- **Blank node document-scoping** (`src/dictionary/bnode.rs`): Each bulk load call (and each `INSERT DATA` statement) is assigned a monotonically-increasing `load_generation BIGINT` from another shared sequence. Blank node labels are hashed as `"{generation}:{label}"` rather than `"{label}"` — so `_:b0` from load call #5 hashes as `"5:b0"` and `_:b0` from load call #6 hashes as `"6:b0"`, yielding distinct dictionary IDs. This isolation is mandatory for correct multi-file RDF loading and is in effect from v0.2.0. The `load_generation` is stored in a thread-local / SPI-session context and advanced at the start of each top-level load operation.
- **Per-query `EncodingCache`** (`src/dictionary/query_cache.rs`): A short-lived `HashMap<&str, i64>` allocated at the start of each SPARQL query and discarded when the query exits. Constants appearing multiple times in a pattern (e.g. the same IRI in multiple BGPs) are encoded once and reused within the same query without hitting the shared-memory LRU or the database. Distinct from `encode_batch()` which is used during data load.
- **In-memory cache** (phased implementation):
  - **v0.1.0–v0.5.1**: backend-local `lru::LruCache<u128, i64>` — simple, no shared memory required, no `shared_preload_libraries` dependency. Each backend has its own cache; cache misses hit the dictionary tables via SPI.
  - **v0.6.0+**: `HashMap<u128, i64>` in shared memory via pgrx `PgSharedMem`, **sharded into N buckets** (default: 64) with per-shard lightweight locks to eliminate global lock contention under concurrent workloads. Sized by GUC.
- **Shared-memory budget** (v0.6.0+): `pg_ripple.cache_budget` GUC governs the *utilization cap* of the pre-allocated shared memory block — it is enforced in Rust and does not cause PostgreSQL to allocate additional memory. The complementary startup GUC `pg_ripple.shared_memory_size` (set in `postgresql.conf`) declares the actual block size to PostgreSQL in `_PG_init`; it must be ≥ `cache_budget` and cannot be changed without a postmaster restart. Automatic eviction priority: bloom filters first, then oldest LRU dictionary entries. Back-pressure on bulk loads when utilisation exceeds 90% of `cache_budget`.
- **Prefix compression**: Common IRI prefixes (registered via `pg_ripple.register_prefix()`) are stripped from the `value` column before storage and the expansion is held separately in the prefix registry. The stored `value` contains only the local part (e.g. `"Person"` rather than `"http://xmlns.com/foaf/0.1/Person"`). The XXH3-128 **hash is computed over the full expanded IRI** to maintain globally unique collision-resistant IDs — not over the compressed form. The benefit is storage compression (~40% reduction in `value` column size for typical prefix-heavy RDF datasets), not hash-space compression. Decoding reconstructs the full IRI by joining the local part with the registered expansion.
- **Inline value encoding** (`src/dictionary/inline.rs`, v0.5.1): Type-tagged i64 values for `xsd:integer`, `xsd:boolean`, `xsd:dateTime`, `xsd:date`. Deferred from v0.3.0 to keep the initial SPARQL engine focused on a single ID space; the dual-space model is introduced once the query engine is stable (v0.5.1, after v0.5.0 completes query-engine work). Bit 63 set signals an inline value; bits 56–62 hold a 7-bit type code; bits 0–55 hold the encoded value. FILTER comparisons on these types require zero dictionary round-trips — the SPARQL→SQL translator encodes constants at translation time and emits a plain B-tree range condition on the VP column.

  **Assigned inline type codes**:

  | Code (7-bit) | xsd datatype | Encoding of bits 0–55 |
  |---|---|---|
  | `0` | `xsd:integer` | Two's complement signed 56-bit integer |
  | `1` | `xsd:boolean` | `0` = false, `1` = true |
  | `2` | `xsd:dateTime` | Microseconds since Unix epoch (UTC) |
  | `3` | `xsd:date` | Days since Unix epoch |
  | `4`–`127` | Reserved | For future typed literals (decimal, double, duration, etc.) |

  > **Why no `xsd:double`?** Truncating IEEE 754 binary64 to 56 bits (removing 8 exponent/mantissa bits) produces undefined precision and range limits. Since range comparisons on doubles are uncommon in SPARQL, `xsd:double` values are stored in the dictionary table where lossless encoding is guaranteed. If inline double support is needed in the future, code `4` could use IEEE 754 binary32 (float) in 32 of the 56 available bits, giving well-understood 7-digit precision.

  IRI-based dictionary IDs always have bit 63 = 0, so the inline and non-inline ranges are disjoint.
- **ID ordering** (v0.5.1): Typed-literal IDs are allocated in monotonically increasing semantic order within each type (integers by numeric value, dates chronologically). This enables FILTER range conditions to compile to `BETWEEN $lo AND $hi` scans on the raw i64 column without decoding. The integer and date ranges are disjoint from IRI ranges via the type-tag bits.
- **Tiered dictionary** (`src/dictionary/hot.rs`, v0.10.0): `_pg_ripple.dictionary_hot` (UNLOGGED, stays in `shared_buffers`) holds IRIs ≤512 bytes, all prefix-registry IRIs, and all predicate IRIs. `_pg_ripple.dictionary` (heap) remains the full table; the encoder checks the hot table first. `pg_prewarm` warms `dictionary_hot` at server start via `_PG_init`. At Wikidata scale (3B vocabulary entries, 190 GB uncompressed), this keeps the hot lookup path I/O-free for the overwhelming majority of query-time decodes.

### 4.3 Storage Engine (`src/storage/`)

**Purpose**: Physically store triples as integer tuples in vertically partitioned tables.

#### 4.3.1 Vertical Partitioning

Each unique predicate `p` gets its own table:

```sql
-- Created once at extension bootstrap (v0.1.0+):
CREATE SEQUENCE _pg_ripple.statement_id_seq;

CREATE TABLE _pg_ripple.vp_{predicate_id} (
    s       BIGINT NOT NULL,  -- subject dictionary ID
    o       BIGINT NOT NULL,  -- object dictionary ID
    g       BIGINT NOT NULL DEFAULT 0,  -- named graph ID (0 = default)
    i       BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq')  -- globally-unique statement identifier (SID)
    -- source SMALLINT NOT NULL DEFAULT 0 added by v0.10.0 migration
);
CREATE INDEX ON _pg_ripple.vp_{predicate_id} (s, o);
CREATE INDEX ON _pg_ripple.vp_{predicate_id} (o, s);
-- Created only when pg_ripple.named_graph_optimized = true:
-- CREATE INDEX ON _pg_ripple.vp_{predicate_id} (g, s, o);
```

> **Why a shared sequence?** Using `GENERATED ALWAYS AS IDENTITY` gives each VP table its own private sequence, meaning two different VP tables can both produce `i = 1`. RDF-star (v0.4.0) requires SIDs to be globally unique — they must appear as subjects or objects in *other* VP tables and be unambiguously resolved via `_pg_ripple.statements`. A single shared `statement_id_seq` sequence guarantees global uniqueness across all VP tables and `vp_rare`.

- Tables are created dynamically on first encounter of a new predicate during data ingestion
- A catalog table `_pg_ripple.predicates` maps predicate dictionary IDs to table OIDs for fast lookup
- PG18's **skip scan** on the composite B-tree indices enables efficient lookups even when only the second column (`o`) is bound
- **`i` column (Statement Identifier)** (v0.1.0): Every statement gets a globally-unique `BIGINT` drawn from the shared `_pg_ripple.statement_id_seq` sequence. Using a shared sequence (rather than per-table `GENERATED ALWAYS AS IDENTITY`) guarantees that no two rows across any VP table or `vp_rare` share the same SID — a prerequisite for RDF-star (v0.4.0), where a SID can appear in the `s` or `o` column of any other VP table and must be unambiguously resolvable. This makes the storage schema SPOI-compatible (inspired by the OneGraph 1G model).

  **`_pg_ripple.statements` catalog** (v0.2.0): A lightweight **range-mapping table** — not a view unioning all VP table rows — is maintained by the merge worker to support SID lookups:

  ```sql
  CREATE TABLE _pg_ripple.statements (
      sid_min     BIGINT NOT NULL,
      sid_max     BIGINT NOT NULL,
      predicate_id BIGINT NOT NULL,
      table_oid   OID NOT NULL,
      PRIMARY KEY (sid_min)
  );
  ```

  After each merge cycle the worker inserts one range row per VP table covering the SIDs allocated since the last merge. Because SIDs are drawn from a monotonically increasing sequence, ranges are non-overlapping and a binary search on `sid_min` resolves any SID to its owning VP table in $O(\log n)$ with no full-table scans. Rows in `vp_rare` are also covered: since `vp_rare` does not split, its SIDs span multiple ranges but the predicate is stored inline per `vp_rare` row, so a fallback `SELECT predicate_id FROM _pg_ripple.vp_rare WHERE i = $1` is used for unmatched ranges (rare, as `vp_rare` rows are eventually promoted).
- **`source` column** (v0.10.0): `SMALLINT DEFAULT 0` — `0` = explicit triple asserted by the user; `1` = derived triple produced by the Datalog/RDFS/OWL RL reasoning engine. Added to every dedicated VP table **and to `_pg_ripple.vp_rare`** via `ALTER TABLE … ADD COLUMN source SMALLINT NOT NULL DEFAULT 0` in the v0.10.0 migration script. This is a zero-downtime fast-path column addition in PostgreSQL — no table rewrite. Queries can pass `include_derived := false` to filter to `WHERE source = 0` only. Because the column is added as part of the v0.10.0 migration script, it has zero cost before reasoning is enabled.
- **Named-graph index** (`pg_ripple.named_graph_optimized = true`): when enabled, each VP table gains an additional `(g, s, o)` index supporting `GRAPH ?g { ... }` patterns without a full-table scan. Off by default to avoid index bloat for single-graph users.

**Rare-Predicate Consolidation**:
- Predicates with fewer than `pg_ripple.vp_promotion_threshold` triples (default: 1,000) are stored in a shared `_pg_ripple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT, i BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'))` table with three secondary indices:
  - `(p, s, o)` — primary access pattern: all triples for a given predicate
  - `(s, p)` — DESCRIBE queries: enumerate all predicates for a given subject without a full-table scan
  - `(g, p, s, o)` — graph-drop: enumerate and bulk-delete all triples in a named graph
- Once a predicate crosses the threshold, its rows are auto-migrated to a dedicated VP table and the catalog updated — transparent to callers
- Promotion is **deferred to end-of-statement**: during bulk loads, triples accumulate in `vp_rare`; after the load completes (or during the next merge worker cycle), predicates exceeding the threshold are promoted in a single `INSERT … SELECT` + `DELETE` transaction
- `pg_ripple.promote_rare_predicates()` can also be called manually
- Prevents catalog bloat for predicate-rich datasets (DBpedia ≈60K predicates, Wikidata ≈10K predicates) — avoids hundreds of thousands of PostgreSQL objects, reduces planner overhead, and cuts VACUUM cost
- **`vp_rare` is exempt from the HTAP delta/main split** (v0.6.0+): rare predicates see few writes by definition, so a dual-partition layout adds overhead for negligible benefit. `vp_rare` remains a single flat table throughout the HTAP migration and after. Concurrent read/write safety relies on PostgreSQL row-level locking (the table is accessed under standard heap locking — no extra row locks required). The bloom filter for delta existence checks treats `vp_rare` as always "in-delta" (i.e., no cached main-only shortcut applies), which is conservative and correct.

#### 4.3.2 HTAP Dual-Partition Architecture

> **Version note**: The delta/main split is introduced in **v0.6.0** via a schema migration (see ROADMAP v0.6.0). Versions v0.1.0–v0.5.1 use a single flat VP table per predicate — all reads and writes target that same table. The architecture described below is the v0.6.0+ steady-state. The `UNION ALL delta + main` query path, bloom filter, and background merge worker are all v0.6.0 deliverables.

**Delta Partition** (write-optimized):
- Standard heap tables with B-tree indices
- All INSERTs and DELETEs target the delta partition
- Small enough to remain in `shared_buffers`

**Main Partition** (read-optimized):
- BRIN-indexed, physically sorted by subject via `INSERT ... ORDER BY` during generation merge
- Populated by the background merge worker
- Uses PG18 async I/O for faster sequential scans

**Tombstone Table** (v0.6.0+):
- When deleting a triple that may exist in `_main`, the delete is recorded in `_pg_ripple.vp_{id}_tombstones (s BIGINT, o BIGINT, g BIGINT)`
- Query path becomes: `(main EXCEPT tombstones) UNION ALL delta`
- The merge worker applies tombstones against main during each generation merge, then truncates the tombstone table
- Necessary because `_main` is read-only between merges — a DELETE targeting a main-resident triple cannot modify `_main` directly

**Merge Worker** (background worker via pgrx `BackgroundWorker`):
- Periodically merges delta into main when delta exceeds `pg_ripple.merge_threshold` rows
- Runs as a pgrx background worker with `BGWORKER_SHMEM_ACCESS`
- **Concurrency & Locking** (v0.6.0+): The rename/truncate step requires an `AccessExclusiveLock`. To prevent stalling the database if queries are actively holding shared read locks on the delta table, the merge worker uses a low `lock_timeout` and retry logic for the `ALTER TABLE ... RENAME` statement, ensuring concurrent `INSERT` and `SELECT` operations are not blocked entirely by a queued exclusive lock.
- **Fresh-table generation merge** (v0.6.0): each merge cycle creates a *new* `vp_{id}_main_new` table rather than inserting incrementally into the existing one (incremental inserts degrade BRIN effectiveness because BRIN requires physically sorted data):
  1. `CREATE TABLE _pg_ripple.vp_{id}_main_new` (heap)
  2. `INSERT … SELECT … ORDER BY s FROM (SELECT * FROM vp_{id}_main EXCEPT SELECT * FROM vp_{id}_tombstones UNION ALL SELECT * FROM vp_{id}_delta)` — combines existing main (minus tombstones) with new delta rows; `ORDER BY s` on a freshly created heap produces physically sorted pages so BRIN works correctly without a separate `CLUSTER` step
  3. `ALTER TABLE … RENAME` to atomically replace the old main (catalog-only, zero query downtime since queries read `UNION ALL delta + main`)
  4. `TRUNCATE vp_{id}_delta, vp_{id}_tombstones` — clear delta and tombstones
  5. Old main retained for `pg_ripple.merge_retention_seconds` (GUC, default 60s) then `DROP TABLE`
- `pg_ripple.compact(keep_old BOOL DEFAULT false)` triggers an immediate full merge across all VP tables; `keep_old := false` drops previous generations immediately
- Updates BRIN summaries post-merge
- Runs `ANALYZE` on merged VP tables so the PostgreSQL planner has fresh selectivity estimates
- Triggers `pg_ripple.promote_rare_predicates()` for any rare predicates that crossed the promotion threshold
- Signals completion via shared-memory latch
- **Commit-hook early trigger**: a PostgreSQL `ProcessUtility` hook (or `ExecutorEnd` hook) detects when a write transaction commits more than `pg_ripple.latch_trigger_threshold` rows (default: 10,000) and pokes the merge worker's shared-memory latch immediately — avoiding the full polling interval wait for bursty workloads. Implemented as an `ExecutorEnd_hook` in `src/storage/merge.rs`.

**Query Path**:
- `(main EXCEPT tombstones) UNION ALL delta`, with bloom filter for fast existence checks
- When tombstones table is empty (common case between merges for insert-heavy workloads), simplifies to `UNION ALL` of main + delta
- For queries touching only historical data, the delta scan is skipped

#### 4.3.3 Bulk Loading

- Inline TEXT variants: `pg_ripple.load_turtle(data TEXT)`, `pg_ripple.load_ntriples(data TEXT)`, `pg_ripple.load_nquads(data TEXT)` (v0.2.0), `pg_ripple.load_trig(data TEXT)` (v0.2.0)
- File-path variants: `pg_ripple.load_turtle_file(path TEXT)`, `pg_ripple.load_ntriples_file(path TEXT)`, `pg_ripple.load_nquads_file(path TEXT)`, `pg_ripple.load_trig_file(path TEXT)` (v0.2.0) — read via `pg_read_file()` with superuser privilege check; essential for datasets exceeding the ~1 GB TEXT parameter limit
- Parses via `rio_turtle` / `rio_api` crates in streaming fashion; `oxttl` / `oxrdf` for RDF-star variants (v0.4.0+)
- Batches of 10,000 triples: dictionary-encode → `COPY` into VP tables (delta partition from v0.6.0+)
- Disables index updates during load; rebuilds at end
- Malformed RDF is caught in the `rio_turtle` / `rio_api` streaming parser layer before data reaches `COPY`; no PostgreSQL-level fault tolerance needed (note: `COPY ... REJECT_LIMIT` is a Greenplum/Cloudberry feature, not stock PostgreSQL; PG17+ offers `ON_ERROR ignore` but it is unnecessary here since parsing happens in Rust)
- Runs `ANALYZE` on affected VP tables after load completes (from v0.2.0; ensures PostgreSQL planner has accurate selectivity estimates)

#### 4.3.4 Subject Patterns (`_pg_ripple.subject_patterns`, v0.6.0)

Precomputed index mapping each subject to the sorted array of all its predicate IDs:

```sql
CREATE TABLE _pg_ripple.subject_patterns (
    s        BIGINT NOT NULL,
    pattern  BIGINT[] NOT NULL,  -- sorted array of predicate IDs for this subject
    PRIMARY KEY (s)
);
CREATE INDEX ON _pg_ripple.subject_patterns USING GIN (pattern);
```

- **DESCRIBE queries**: look up `pattern` for the subject in one index seek, then query only the N VP tables in the array — O(N) instead of scanning all VP tables
- **Statistics**: `SELECT unnest(pattern), count(*) FROM subject_patterns GROUP BY 1` gives predicate-popularity counts without touching VP tables
- **GIN index**: enables "subjects that have both predicate P1 and P2" queries (`pattern @> ARRAY[$1, $2]`) efficiently
- Maintained by the merge worker after each generation merge, not on individual INSERTs

#### 4.3.5 Object Patterns (`_pg_ripple.object_patterns`, v0.6.0)

Precomputed index mapping each object to the sorted array of all its predicate IDs. This solves the "unbound object problem" for reverse-edge exploration.

```sql
CREATE TABLE _pg_ripple.object_patterns (
    o        BIGINT NOT NULL,
    pattern  BIGINT[] NOT NULL,  -- sorted array of predicate IDs for this object
    PRIMARY KEY (o)
);
CREATE INDEX ON _pg_ripple.object_patterns USING GIN (pattern);
```

- **Reverse DESCRIBE / Incoming Arcs**: Similar to `subject_patterns`, `object_patterns` intercepts scattergun reverse queries (`?s ?p <Object>`) in O(N) rather than forcing a catastrophic `UNION ALL` across all thousands of VP tables.
- Maintained by the merge worker after each generation merge.

### 4.4 SPARQL Query Engine (`src/sparql/`)

**Purpose**: Parse SPARQL, translate to optimized SQL, execute, decode results.

#### 4.4.1 Pipeline

```
SPARQL text
    │
    ▼
spargebra::parse()  →  SPARQL Algebra tree
    │
    ▼
sparopt::Optimizer::optimize()  (v0.3.0+)
    (upstream algebra pass: filter pushdown, constant folding, empty-pattern elimination)
    │
    ▼
Algebrizer (src/sparql/algebra.rs)
    - Reads loaded SHACL shapes + predicate catalog BEFORE building join tree
      (sh:minCount, sh:maxCount, sh:class available at plan time → used in optimizer below)
    - Per-query EncodingCache: encode all constant IRIs/literals once, reuse across BGPs
    │
    ▼
Algebra Optimizer (Rust)  — pg_ripple-specific second pass
    - Self-join elimination
    - Optional-to-inner downgrade only when proven semantics-preserving for the query domain
    - Filter pushdown (pre-decode)
    - UNION folding → WHERE IN
    - BGP join reordering: uses `pg_stats.n_distinct` + `pg_class.reltuples` for each
      VP table to estimate selectivity; reorders BGPs cheapest-first
    │
    ▼
SQL Generator
    - Map BGPs to VP table joins (integer columns)
    - Property paths → WITH RECURSIVE + CYCLE detection
    - OPTIONAL → LEFT JOIN
    - LIMIT/OFFSET pushdown
    - DISTINCT projection pushing
    - `ORDER BY` on join-variable CTEs when the variable matches the VP table primary index sort key — enables PostgreSQL merge-join planning for large intermediate results
    - `SERVICE <local:view-name>` → reference to a PostgreSQL `MATERIALIZED VIEW` of the same name (zero extension code; automatic query-planner reuse)
    - Join-order hints: `<http://pg-ripple.io/hints/join-order>` in query prologue
      emits `SET LOCAL join_collapse_limit = 1` around the generated SQL
    - `no-inference` hint: appends `AND source = 0` on all VP table scans
    │
    ▼
SPI::connect() → execute SQL → result set of i64 tuples
    │
    ▼
Batch Dictionary Decoder → collect all i64 IDs → single WHERE id = ANY(...)
    → build decode map → human-readable result set
    │
    ▼
Projector (src/sparql/projector.rs)
    - Maps decoded row columns to named SPARQL variables
    - Applies SELECT expressions, BIND, computed values
    - Emits SETOF RECORD / JSON / TABLE
    │
    ▼
Return as SETOF RECORD / JSON / TABLE
```

#### 4.4.2 SQL Functions

```sql
-- Primary query interface
pg_ripple.sparql(query TEXT, include_derived BOOL DEFAULT true) RETURNS SETOF JSONB
pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false) RETURNS TEXT
  -- analyze := true wraps the generated SQL in EXPLAIN (ANALYZE, BUFFERS) and returns the plan

-- Basic querying (v0.1.0, SQL-level, no SPARQL)
pg_ripple.find_triples(s TEXT, p TEXT, o TEXT) RETURNS TABLE (s TEXT, p TEXT, o TEXT, g TEXT)
  -- any param can be NULL for wildcard; returns decoded string values

-- Data manipulation
pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL) RETURNS BIGINT  -- returns SID from v0.4.0
pg_ripple.delete_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL)
pg_ripple.load_turtle(data TEXT) RETURNS BIGINT  -- returns count
pg_ripple.load_ntriples(data TEXT) RETURNS BIGINT

-- SPARQL DESCRIBE strategy (v0.5.1)
pg_ripple.describe_strategy GUC  -- 'cbd' (default), 'scbd', 'simple'
  -- CBD: Concise Bounded Description (follow outgoing arcs + blank node closures)
  -- SCBD: Symmetric CBD (follow both incoming and outgoing arcs)
  -- simple: one-hop subject/object expansion only

-- Maintenance
pg_ripple.vacuum_dictionary() RETURNS BIGINT  -- removes unreferenced dictionary entries; safe to run any time
pg_ripple.compact(keep_old BOOL DEFAULT false) RETURNS VOID  -- trigger immediate full generation merge
```

#### 4.4.3 Join Optimization Strategies

Optimizations fall into two categories: **structural rewrites** that are applied by the algebra optimizer during SPARQL→SQL translation (low overhead, no statistics required, active from v0.3.0) and **statistics-driven rewrites** that read PostgreSQL planner statistics at plan time (introduced in v0.13.0 — Performance Hardening). Benchmarking infrastructure (BSBM, SP2Bench, fuzz testing) is built in v0.5.0+ and exercised in v0.6.0 (HTAP concurrent workload) and v0.13.0 to measure optimization effectiveness and validate performance targets.

**Structural rewrites (v0.3.0+)**:
1. **Self-join elimination**: Star patterns on the same subject collapse into a single scan of the subject across multiple VP tables, joined by subject ID equality
2. **Optional-self-join elimination**: OPTIONAL → INNER JOIN only when the optimizer can prove the rewrite preserves query semantics for the actual focus-node domain; SHACL metadata can contribute evidence but is not sufficient on its own
3. **Self-union elimination**: Multiple triple patterns binding the same variable to different predicates are rewritten to `WHERE predicate_id IN (...)`
4. **Projection pushing**: `SELECT DISTINCT ?p` queries enumerate the `_pg_ripple.predicates` catalog instead of scanning all VP tables
5. **Filter pushdown**: SPARQL `FILTER` clauses operating on bound IRIs are resolved to integer IDs *before* generating SQL, ensuring B-tree index usage. From v0.5.1, typed numeric/date literals use the inline-encoded i64 range (see §4.2.2) to enable `BETWEEN $lo AND $hi` range scans with no decode step. Prior to v0.5.1, FILTER comparisons on typed literals use a dictionary-join decode approach.
6. **Merge-join enablement**: When the join variable matches the `s` sort key of a VP table's `(s, o, g)` primary index, the emitter wraps the CTE in `ORDER BY s`. The PostgreSQL planner then considers a merge join rather than a hash join, reducing memory pressure for large intermediate results.

**Plan caching (v0.3.0+)**:

SPI re-parses and re-plans the generated SQL string on every query invocation. The SPARQL-layer plan cache avoids this overhead for structurally identical queries by caching the SPARQL→SQL translation result keyed on a structural hash of the normalized algebra tree (variable names replaced with position indices). Controlled by `pg_ripple.plan_cache_size` GUC (default: 256; 0 = disabled). This is introduced in v0.3.0 — before the performance milestone — because re-translation overhead is observable from the first SPARQL-capable release. Additional optimization work (prepared statements, PostgreSQL plan caching instrumentation) remains v0.13.0 work. Benchmarked via BSBM from v0.5.0 onward.

**Statistics-driven rewrites (v0.13.0+)**:
7. **BGP join reordering**: The algebra optimizer reads `pg_stats.n_distinct` and `pg_class.reltuples` for each VP table involved in the query and reorders BGPs cheapest-first (most selective predicate scanned first). Only activated when statistics are available; falls back to source order otherwise. When active, emits `SET LOCAL join_collapse_limit = 1` before the generated SQL to lock the PostgreSQL planner into the computed join order, preventing it from re-ordering the already-optimized sequence.
8. **Optimizer Robustness / Fallback**: Because deriving perfect selectivity from `pg_stats.n_distinct` is fragile over multi-way self-joins, the Rust-based optimizer implements dynamic sampling or uses fallback heuristic costs (e.g. reverting to native PostgreSQL planning) if `pg_stats` suggests high cardinality uncertainty. This prevents forcing PostgreSQL into highly suboptimal plans.
9. **Join-order hints**: A `<http://pg-ripple.io/hints/join-order>` pragma in the SPARQL prologue overrides statistics-driven ordering by emitting `SET LOCAL join_collapse_limit = 1` with the user-specified BGP order.
10. **`no-inference` hint**: Adding `hint:no-inference true` to the query prologue appends `AND source = 0` on every VP table scan, restricting results to explicitly asserted triples only (v0.10.0+).

#### 4.4.4 Property Path Compilation

SPARQL property paths (`+`, `*`, `?`) compile to `WITH RECURSIVE` CTEs with PG18's `CYCLE` clause for hash-based cycle detection:

```sql
WITH RECURSIVE path(s, o, depth) AS (
    -- Anchor: direct one-hop
    SELECT s, o, 1
    FROM _pg_ripple.vp_{predicate_id}
    WHERE s = $1
  UNION ALL
    -- Recursive: extend by one hop
    SELECT p.s, vp.o, p.depth + 1
    FROM path p
    JOIN _pg_ripple.vp_{predicate_id} vp ON p.o = vp.s
    WHERE p.depth < pg_ripple.max_path_depth
)
CYCLE o SET is_cycle USING cycle_path
SELECT DISTINCT s, o FROM path WHERE NOT is_cycle;
```

- Configurable `pg_ripple.max_path_depth` GUC (default: 100)
- PG18 `CYCLE` clause for hash-based cycle detection (replaces array-based visited tracking — $O(1)$ membership checks instead of $O(n)$ array scans)
- PG18's improved CTE performance benefits recursive path queries
- **Performance constraint**: PostgreSQL materializes each level of a `WITH RECURSIVE` CTE into a work-table before proceeding to the next. For deep traversals (depth > ~5-10 hops) or wide fan-out on large graphs, the per-level materialization cost dominates and can cause execution times to degrade exponentially. The <100 ms benchmark target (§13) applies to bounded-depth paths (depth ≤ 10) on typical RDF datasets; unbounded paths on dense graphs will inherently bottleneck the PG execution planner. Mitigation: `max_path_depth` GUC, `statement_timeout`, and the resource-exhaustion test suite in v0.5.0.

### 4.5 Named Graph Support (`src/graph/`)

- The `g` column in VP tables stores the named graph dictionary ID
- `g = 0` represents the default graph
- SPARQL `GRAPH ?g { ... }` and `FROM NAMED <uri>` map to `WHERE g = encode(uri)` filters
- Graph management functions:
  - `pg_ripple.create_graph(uri TEXT)`
  - `pg_ripple.drop_graph(uri TEXT)`
  - `pg_ripple.list_graphs() RETURNS SETOF TEXT`

> **Post-1.0 consideration — graph-partitioned VP tables**: For workloads where `DROP GRAPH` is frequent and graphs are large, consider adding optional PostgreSQL range- or list-partitioning on the `g` column of individual VP tables. A graph drop would then become a DDL `DETACH PARTITION; DROP TABLE` operation, completely bypassing the `DELETE + VACUUM` overhead that the current `(g, p, s, o)` index-driven bulk delete incurs. This adds schema complexity and is only worthwhile when graphs are large and short-lived (e.g., named-graph-per-document load patterns). Revisit during v0.12.0 planning.

### 4.6 SHACL Validation Engine (`src/shacl/`)

**Purpose**: Enforce data integrity constraints defined in SHACL shapes.

#### 4.6.1 Exact SHACL Semantics

SHACL support is **spec-first**: loaded shapes are parsed into a shape IR that preserves W3C SHACL semantics, and validation is executed against that IR. PostgreSQL constraints, triggers, helper tables, or stream tables may be used as **internal accelerators only when they are proven semantics-preserving for the specific shape pattern**; they are never the normative definition of constraint behavior.

Validation is compiled to per-shape validator plans over focus nodes and value nodes:

| SHACL Constraint | Validation Strategy |
|---|---|
| `sh:minCount` | Count matching value nodes per focus node and compare against the required minimum |
| `sh:maxCount` | Count matching value nodes per focus node and compare against the allowed maximum |
| `sh:datatype` | Validate RDF term kind and datatype IRI exactly against dictionary metadata / inline encoding |
| `sh:in (...)` | Validate RDF-term equality against the allowed value set |
| `sh:pattern` | Apply SHACL regex semantics to the lexical form of the value node |
| `sh:class` | Evaluate required `rdf:type` membership for each value node |
| `sh:node` / `sh:property` | Recurse into nested shapes using the same validator pipeline |
| `sh:or` / `sh:and` / `sh:not` / qualified constraints | Evaluate via composed validator plans (v0.8.0+) |

Examples of **allowed internal accelerators** when exactness is preserved:
- An index on `(s, g)` to speed up `sh:maxCount 1` counting
- A cached target-node set for `sh:targetClass`
- A trigger that short-circuits obvious violations but still falls back to the full validator when needed

Examples of **disallowed semantic shortcuts**:
- Treating `sh:minCount 1` as a bare `NOT NULL` on a VP row
- Treating `sh:maxCount 1` as a table-wide `UNIQUE` index without proving the same focus-node and path semantics as the SHACL shape

#### 4.6.2 Asynchronous Validation Pipeline

For bulk loads where synchronous validation is too expensive:

1. Lightweight trigger captures inserted triple IDs into `_pg_ripple.validation_queue`
2. Background worker (pgrx `BackgroundWorker`) processes queued triples against loaded SHACL shapes
3. Invalid triples moved to `_pg_ripple.dead_letter_queue` with violation report (as JSONB)
4. Valid triples remain in the VP tables

#### 4.6.3 Query Optimization via SHACL

The SPARQL→SQL translator reads loaded SHACL shapes:
- Shape metadata may inform **costing** and may enable **semantics-preserving rewrites** when the proof obligation is satisfied for the exact query domain
- `sh:maxCount 1` may enable cardinality-sensitive optimizations only when the query is provably restricted to the same focus-node population as the validated shape
- `sh:class` / `sh:targetClass` may support type-based pruning only when the query's variable domain is provably identical to the shape target set
- The presence of a shape alone is never sufficient to rewrite query semantics; unsafe rewrites are disabled by default

### 4.7 Serialization & Export (`src/export/`)

- `pg_ripple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
- `pg_ripple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
- `pg_ripple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
- Streaming output via `RETURNS SETOF TEXT` for large graphs

### 4.8 Statistics & Monitoring (`src/stats/`)

- `pg_ripple.stats() RETURNS JSONB` — triple count, predicate distribution, dictionary size, cache hit ratio, delta/main partition sizes
- Integration with `pg_stat_statements` for SPARQL query tracking
- Custom `EXPLAIN` option (PG18 feature) to annotate SPARQL→SQL translations
- **When pg_trickle is available**: `stats()` reads from `_pg_ripple.predicate_stats` and `_pg_ripple.graph_stats` stream tables (instant, no full scan) instead of re-scanning VP tables on every call. See §4.10.

### 4.9 Administrative Functions (`src/admin/`)

- `pg_ripple.vacuum()` — force delta→main merge
- `pg_ripple.compact(keep_old BOOL DEFAULT false)` — immediate full generation merge across all VP tables; `keep_old := false` drops previous main-table generations immediately
- `pg_ripple.vacuum_dictionary() RETURNS BIGINT` — removes dictionary entries not referenced by any VP table column; returns count of removed entries
- `pg_ripple.reindex()` — rebuild VP table indices
- `pg_ripple.dictionary_stats()` — cache hit ratio, dictionary sizes
- `pg_ripple.register_prefix(prefix TEXT, expansion TEXT)` — IRI prefix registration
- `pg_ripple.prefixes() RETURNS TABLE(prefix TEXT, expansion TEXT)`

### 4.10 Ecosystem: pg_trickle Integration (`src/ecosystem/`)

**Purpose**: Optional reactivity layer powered by [pg_trickle](https://github.com/grove/pg-trickle) stream tables. All features in this module require pg_trickle to be installed; core pg_ripple functionality works without it. See [full analysis](ecosystem/pg_trickle.md).

#### 4.10.1 Runtime Detection

```rust
fn has_pg_trickle() -> bool {
    Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle')"
    ).unwrap_or(Some(false)).unwrap_or(false)
}
```

All stream-table features gate on this check. Functions that require pg_trickle return a clear error with install instructions when it is absent.

#### 4.10.2 Live Statistics (Stream Tables)

When pg_trickle is detected, `pg_ripple.enable_live_statistics()` creates stream tables:

- `_pg_ripple.predicate_stats` — per-predicate triple count, distinct subjects/objects (refreshed every 5s)
- `_pg_ripple.graph_stats` — per-graph triple count (refreshed every 10s)

`pg_ripple.stats()` reads from these stream tables instead of full-scanning VP tables — 100–1000× faster.

#### 4.10.3 SHACL Violation Monitors

Simple SHACL constraints (cardinality, datatype, class) can be modeled as stream tables with `IMMEDIATE` refresh mode, validating within the same transaction as the DML:

- `sh:minCount` violations → `NOT EXISTS` stream table
- `sh:datatype` violations → filtered join stream table
- Multiple shapes → pg_trickle's DAG scheduler handles refresh ordering

Complex shapes (`sh:or`, `sh:and`, multi-hop) still use the procedural validation pipeline from §4.6.

#### 4.10.4 Inference Materialization (→ Datalog Engine)

> **Note**: This section is superseded by the general Datalog reasoning engine. See [plans/ecosystem/datalog.md](plans/ecosystem/datalog.md) for the full design.

The original plan — `pg_ripple.enable_inference_materialization()` creating hard-coded `WITH RECURSIVE` stream tables for `rdfs:subClassOf` and `rdfs:subPropertyOf` — is replaced by a general-purpose Datalog engine that:

- Parses user-defined and built-in rules (RDFS, OWL RL) in a Turtle-flavoured Datalog syntax
- Stratifies rules to handle negation-as-failure correctly
- Compiles each stratum to SQL: non-recursive → `INSERT … SELECT`, recursive → `WITH RECURSIVE … CYCLE`, negation → `NOT EXISTS`
- Materializes derived predicates as pg_trickle stream tables (recommended) or inlines them as CTEs at query time (on-demand, no pg_trickle needed)
- Registers derived VP tables in `_pg_ripple.predicates` so the SPARQL engine treats them identically to base VP tables
- Multi-head rules: each head atom may target a different predicate and carry an optional named graph ID
- **Incremental materialization phases** (inspired by RDFox): each materialization cycle runs three phases in order:
  1. *Addition* — derive and insert new triples produced by rules applied to newly asserted facts; write with `source = 1`
  2. *Deletion* — identify derived triples whose support has been retracted; remove them from VP tables
  3. *BwdChain* — re-derive any derived triple that was deleted but is still entailed by surviving facts (avoids over-deletion)
- **Rule set catalog**: `_pg_ripple.rule_sets (name TEXT, graph_ids BIGINT[], rule_hash BIGINT)` stores named rule sets. `rule_hash` is the XXH3-64 hash of the canonicalized rule text; the materialization worker skips re-computation when the hash is unchanged. Rule set caches are keyed on this hash so a re-activated rule set resumes from its previous derived state.
- **Named rule sets**: `pg_ripple.load_rules(name TEXT, rules TEXT)` registers a rule set; `pg_ripple.enable_rule_set(name TEXT)` activates it for a given set of named graphs.

#### 4.10.5 SPARQL Views

```sql
pg_ripple.create_sparql_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT DEFAULT '5s'
) RETURNS VOID
```

Parses SPARQL → generates SQL → creates a pg_trickle stream table. The result is an always-fresh materialized SPARQL query: multi-join VP table scans + dictionary decoding happen once during materialization, and queries become simple table scans.

#### 4.10.5.1 Datalog Views

```sql
pg_ripple.create_datalog_view(
    name     TEXT,
    rules    TEXT DEFAULT NULL,     -- inline Datalog rules (NULL when using rule_set)
    rule_set TEXT DEFAULT NULL,     -- reference a loaded rule set by name
    goal     TEXT,                  -- goal pattern: '?x ex:indirectManager ex:Alice .'
    schedule TEXT DEFAULT '10s',
    decode   BOOLEAN DEFAULT FALSE
) RETURNS VOID
```

Bundles a Datalog rule set with a goal pattern into a single pg_trickle stream table. The existing rule parser, stratifier, and SQL compiler (§4.10.4) produce the recursive CTE; the goal pattern's bound constants are dictionary-encoded and pushed into a `WHERE` clause on the outermost `SELECT`. Unbound goal variables become named columns in the stream table. See [plans/ecosystem/datalog.md § 15](plans/ecosystem/datalog.md) for the full design.

Constraint rules (empty-head) work as a special case: the body variables become projected columns and any row in the stream table represents a violation. `IMMEDIATE` mode catches violations within the same transaction.

#### 4.10.6 ExtVP (Extended Vertical Partitioning)

Pre-computed semi-joins between frequently co-joined predicates, implemented as stream tables. The SPARQL→SQL translator rewrites queries to target ExtVP tables when available. Initially manual via `create_sparql_view()`; automated workload-driven creation is a post-1.0 goal.

---

## 4.11 Canonical GUC Reference

All GUC parameters exposed by pg_ripple, listed alphabetically. GUCs marked **startup** must be set in `postgresql.conf` and take effect only at postmaster start; all others can be changed per-session with `SET`.

| GUC Name | Type | Default | Valid Values / Range | Introduced | Notes |
|---|---|---|---|---|---|
| `pg_ripple.default_graph` | `TEXT` | `''` | Any IRI string | v0.1.0 | Graph ID used when `g` is not specified on insert |
| `pg_ripple.describe_strategy` | `ENUM` | `'cbd'` | `'cbd'`, `'scbd'`, `'simple'` | v0.5.1 | DESCRIBE algorithm: `'cbd'` = Concise Bounded Description (outgoing arcs + blank node closure); `'scbd'` = Symmetric CBD (incoming + outgoing arcs); `'simple'` = one-hop s/o expansion |
| `pg_ripple.dictionary_cache_size` | `INT` | `65536` | 1 – 1,000,000 | v0.1.0 | Number of entries in the LRU dictionary cache. v0.1.0–v0.5.1: per-backend local cache. v0.6.0+: per-shard in shared memory (64 shards) |
| `pg_ripple.enforce_constraints` | `ENUM` | `'warn'` | `'error'`, `'warn'`, `'off'` | v0.10.0 | Controls behavior when Datalog constraint rules (empty-head rules) detect violations |
| `pg_ripple.federation_max_results` | `INT` | `10000` | 1 – 1,000,000 | v0.16.0 | Maximum rows accepted from a single remote `SERVICE` call |
| `pg_ripple.federation_on_error` | `ENUM` | `'warn'` | `'warn'`, `'error'`, `'ignore'` | v0.16.0 | How to handle a failed remote `SERVICE` call |
| `pg_ripple.federation_timeout` | `INT` | `30` | 1 – 3600 (seconds) | v0.16.0 | Per-`SERVICE` HTTP timeout |
| `pg_ripple.inference_mode` | `ENUM` | `'off'` | `'off'`, `'on_demand'`, `'materialized'` | v0.10.0 | Controls the Datalog reasoning engine; `'materialized'` requires pg_trickle |
| `pg_ripple.latch_trigger_threshold` | `INT` | `10000` | 0 – 10,000,000 | v0.6.0 | Row count at which a committing write transaction pokes the merge worker latch immediately |
| `pg_ripple.max_path_depth` | `INT` | `100` | 1 – 10,000 | v0.5.0 | Maximum recursion depth for property path (`+`, `*`) queries |
| `pg_ripple.merge_retention_seconds` | `INT` | `60` | 0 – 3600 | v0.6.0 | Seconds to keep the previous `_main` table generation after an atomic rename before dropping it |
| `pg_ripple.merge_threshold` | `INT` | `100000` | 0 – 1,000,000,000 | v0.6.0 | Delta row count that triggers a background merge; `0` disables the merge worker entirely |
| `pg_ripple.merge_watchdog_timeout` | `INT` | `300` | 60 – 3600 (seconds) | v0.6.0 | If the merge worker heartbeat stalls for longer than this, `_PG_init` on the next backend connection logs a WARNING and attempts restart |
| `pg_ripple.named_graph_optimized` | `BOOL` | `off` | `on`, `off` | v0.2.0 | When `on`, adds a `(g, s, o)` index per VP table; increases write overhead; useful for heavy named-graph workloads |
| `pg_ripple.plan_cache_size` | `INT` | `256` | 0 – 100,000 | v0.3.0 | Number of SPARQL→SQL translation results cached per session; `0` disables. v0.13.0 extends cache instrumentation and prepared-statement work, but the translation cache itself lands in v0.3.0. Benchmarked via BSBM from v0.5.0 |
| `pg_ripple.rls_bypass` | `BOOL` | `off` | `on`, `off` | v0.14.0 | Superuser override to bypass graph-level Row-Level Security policies |
| `pg_ripple.rule_graph_scope` | `ENUM` | `'default'` | `'default'`, `'all'` | v0.10.0 | Controls whether unscoped Datalog rule atoms operate on the default graph only or all graphs |
| `pg_ripple.shacl_mode` | `ENUM` | `'off'` | `'off'`, `'sync'`, `'async'` | v0.7.0 | Controls SHACL validation; `'sync'` rejects bad triples inline; `'async'` queues for background validation |
| `pg_ripple.cache_budget` | `INT` | `134217728` | 1 MB – system limit (bytes) | v0.6.0 | Utilization cap for the pre-allocated shared memory block (dictionary cache + bloom filters + merge worker buffers); back-pressure activates at 90%. Renamed from `shared_memory_limit` to avoid confusion with `shared_memory_size` |
| `pg_ripple.shared_memory_size` | `INT` | `268435456` | 1 MB – system limit (bytes) | v0.6.0 | **Startup.** Size of the shared memory block declared to PostgreSQL in `_PG_init`. Must be ≥ `cache_budget`. Cannot be changed at runtime — set in `postgresql.conf`. Not needed before v0.6.0 (backend-local cache is used in v0.1.0–v0.5.1) |
| `pg_ripple.vp_promotion_threshold` | `INT` | `1000` | 1 – 1,000,000 | v0.2.0 | Triples per predicate below which rows are stored in `vp_rare` instead of a dedicated VP table |

> **`shared_memory_size` vs `cache_budget`**: `shared_memory_size` is a *startup* GUC that declares the total shared memory block to PostgreSQL at postmaster start — it cannot be changed without a restart. `cache_budget` is a *runtime* cap that controls how much of that pre-allocated block pg_ripple is allowed to use. Setting `cache_budget > shared_memory_size` is an error caught at `_PG_init`.

---

## 5. Data Flow: Insert Path

> **Target-state note**: The flow below is the **v0.6.0+ target architecture** after the HTAP split lands. In v0.1.0–v0.5.1, inserts write directly to the flat `_pg_ripple.vp_{predicate_id}` table.

```
1. pg_ripple.insert_triple('http://ex.org/Alice', 'http://ex.org/knows', 'http://ex.org/Bob')
2. Dictionary encode: s=42, p=7, o=43
3. Look up predicate p=7 → vp_7 table
4. INSERT INTO _pg_ripple.vp_7_delta (s, o, g) VALUES (42, 43, 0)
5. If SHACL enabled: queue validation (async) or validate inline (sync)
6. Background worker periodically merges vp_7_delta → vp_7_main
```

## 6. Data Flow: Query Path

> **Target-state note**: The flow below is the **v0.6.0+ target architecture**. In v0.1.0–v0.5.1, queries read from the flat `_pg_ripple.vp_{predicate_id}` tables only.

```
1. pg_ripple.sparql('SELECT ?name WHERE { ?person foaf:knows ex:Bob . ?person foaf:name ?name }')
2. Parse → Algebra: Join(BGP(person, foaf:knows, ex:Bob), BGP(person, foaf:name, name))
3. Encode bound terms: ex:Bob → 43, foaf:knows → 7, foaf:name → 12
4. Generate SQL:
     SELECT d.o AS name
     FROM (SELECT s FROM _pg_ripple.vp_7 WHERE o = 43
           UNION ALL
           SELECT s FROM _pg_ripple.vp_7_delta WHERE o = 43) AS knows
     JOIN (SELECT s, o FROM _pg_ripple.vp_12
           UNION ALL
           SELECT s, o FROM _pg_ripple.vp_12_delta) AS name_tbl
       ON knows.s = name_tbl.s
5. Execute via SPI
6. Batch decode: collect all i64 IDs from result → single `WHERE id = ANY(...)` → build decode map
7. Emit decoded rows as SETOF JSONB: [{"name": "Alice"}, ...]
```

---

## 7. Performance Targets

> **Calibration reference**: QLever (C++, Apache-2.0) on DBLP (390M triples) loads at 1.7M triples/s, produces an 8 GB index, and answers benchmark queries in 0.7s average. QLever's flat pre-sorted permutation files make every SPARQL join a merge join with zero random I/O. pg_ripple's B-tree/heap design pays ~5× overhead on bulk sequential scans in exchange for transactional concurrent writes, MVCC, and the full PostgreSQL ecosystem. The targets below reflect this accepted trade-off.

> **Pre-HTAP baseline (v0.1.0–v0.5.1)**: Before the HTAP split lands in v0.6.0, all reads and writes target a single flat VP table. The CI performance gate during these releases uses a lower baseline (>30K triples/sec bulk insert) which improves to >100K after the delta/main split and BRIN indexing are in place.

| Metric | Target | Approach |
|---|---|---|
| Insert throughput | >100K triples/sec (bulk load) | Batch COPY, deferred indexing |
| Insert throughput | >10K triples/sec (transactional) | Delta partition, async validation |
| Simple BGP query | <5ms (10M triples) | Integer joins, B-tree on VP tables |
| Star query (5 patterns) | <20ms (10M triples) | Self-join elimination, co-located VP scans, PG parallel hash joins |
| Property path (depth 10) | <100ms (10M triples) | Recursive CTE + `CYCLE` clause (hash-based) |
| Dictionary encode | <1μs (cache hit) | Sharded LRU in shared memory |
| Dictionary encode | <50μs (cache miss) | B-tree index on hash |
| Dictionary batch decode | <1ms per 1,000 IDs | Single `WHERE id = ANY(...)` query |
| Unbound-predicate scan | <500ms (10M triples, ≤60K predicates) | Rare-predicate consolidation table avoids scanning thousands of empty VP tables |

> **Unbound-predicate query verification (v0.3.0+)**: `find_triples(s, NULL, NULL)` and the SPARQL equivalent (`?s ?p ?o` without predicate binding) must efficiently enumerate the predicate catalog and query each relevant VP table before falling back to `vp_rare`. The implementation should use a single `UNION ALL` plan across matched VP tables rather than per-predicate round-trips through SPI. Benchmark this pattern explicitly at v0.3.0 against the <500 ms target with a 60K-predicate dataset (DBpedia scale). Vertical Partitioning inherently penalises exploratory/unbound queries relative to monolithic layouts; the rare-predicate table partially mitigates this but the mitigation should be verified empirically.

---

## 8. Testing Strategy

### 8.1 Unit Tests

- pgrx `#[pg_test]` for every SQL-exposed function
- Rust unit tests for pure logic (dictionary hashing, SPARQL algebra transforms, SQL generation)
- Property-based tests (`proptest`) for dictionary encode/decode round-trips

### 8.2 Integration Tests

- `cargo pgrx regress` with pg_regress test suites:
  - `sql/dictionary.sql` — encode/decode, prefix expansion, hash collision behaviour
  - `sql/basic_crud.sql` — insert, delete, find_triples, triple_count
  - `sql/triple_crud.sql` — insert, delete, query basics (VP storage)
  - `sql/sparql_queries.sql` — comprehensive SPARQL coverage
  - `sql/sparql_injection.sql` — adversarial inputs (SQL metacharacters in IRIs/literals)
  - `sql/bulk_load.sql` — Turtle/N-Triples ingestion
  - `sql/shacl_validation.sql` — constraint enforcement
  - `sql/shacl_malformed.sql` — invalid shape definitions, actionable errors
  - `sql/named_graphs.sql` — GRAPH patterns
  - `sql/property_paths.sql` — recursive traversal
  - `sql/resource_limits.sql` — Cartesian products, unbounded paths, memory limits
  - `sql/concurrent_write_merge.sql` — merge during concurrent writes (no data loss)
  - `sql/admin_functions.sql` — vacuum, reindex, stats
  - `sql/graph_rls.sql` — RLS policy enforcement, cross-role isolation
  - `sql/upgrade_path.sql` — sequential version upgrades with data integrity checks
  - `sql/datalog_malformed.sql` — syntax errors, unstratifiable programs

### 8.3 Adversarial & Security Testing

- **SQL injection prevention**: SPARQL queries with crafted IRIs containing SQL metacharacters (`'; DROP TABLE --`, Unicode escapes, null bytes) must be safely dictionary-encoded; generated SQL must never contain raw user strings
- **Malformed input resilience**: invalid Turtle, truncated N-Triples, malformed SPARQL, broken SHACL shapes, invalid Datalog rules — verify clean error messages, no panics, no partial state corruption
- **Resource exhaustion defence**: Cartesian-product queries, unbounded property paths, deeply nested subqueries — verify that `max_path_depth`, `statement_timeout`, and memory limits prevent runaway consumption

### 8.4 Fuzz Testing

**Phase 1 (v0.13.0 — Performance Hardening)**: Fuzz testing infrastructure is built and integrated into CI in v0.13.0 as part of the performance hardening and production-readiness work. This ensures the SPARQL→SQL pipeline, Turtle parser, and Datalog rule parser are robust against adversarial or malformed inputs before the system is declared production-ready.

- `cargo-fuzz` with libFuzzer on the SPARQL→SQL pipeline: feed random/mutated SPARQL strings through parser and SQL generator; verify no panics, no invalid SQL emitted, no memory safety violations
- Fuzz targets for Turtle parser integration (complement `rio_turtle`'s own fuzz testing with pg_ripple's error propagation layer)
- Fuzz targets for Datalog rule parser
- Run in CI nightly (time-limited: 10 minutes per target)
- Fuzz testing runs without panics or unsafe SQL generation across all parser surfaces

### 8.5 Concurrency Testing

- Concurrent dictionary encode: two backends encoding the same IRI must return the same i64 (verifies shard lock correctness)
- Dictionary cache eviction: verify decode correctness after cache entries are evicted under memory pressure
- Concurrent merge + write: bulk insert and merge worker running simultaneously with no data loss
- Merge worker edge cases: empty delta (no-op), crash during merge (recovery), near-capacity shared memory (back-pressure)

### 8.6 Performance Regression

- **CI benchmark gate** (from v0.2.0): record insert throughput and point-query latency as baselines; fail CI if a commit regresses throughput by >10%
- Baselines extended at each milestone:
  - v0.3.0: star queries
  - v0.5.0: BSBM full mix + property paths, resource exhaustion test suite validation
  - v0.6.0: concurrent read/write (HTAP workload)
  - v0.13.0: join reordering effectiveness, prepared statement reuse, fuzz testing coverage
- Performance regression suite maintained as pgbench custom scripts in `sql/bench/`
- **v0.13.0 consolidation**: All prior-version benchmarks are re-run and baseline values finalized; performance targets confirmed (<10ms simple BGP at 10M triples, >100K triples/sec bulk load, <5ms cached repeat queries)

### 8.7 Benchmarks

- **v0.5.0–v0.6.0 (Integration phase)**:
  - pgrx-bench integration for in-process pgbench
  - Berlin SPARQL Benchmark (BSBM) data generator adapted for bulk load
  - SP2Bench benchmark subset
  - Initial timing collection and baseline documentation
  
- **v0.13.0 (Finalization & Hardening)**:
  - BSBM results published in release notes
  - Benchmark workloads expanded: star patterns, property paths, aggregates, concurrent OLTP/OLAP
  - Baseline comparisons with v0.5.0 (pre-HTAP) and other systems (QLever, RDF4J, Oxigraph)
  - Each optimization in v0.13.0 validated against benchmarks for regression/improvement
  - Docker benchmark container with pre-loaded datasets for easy reproduction

### 8.8 Conformance

- **W3C SPARQL 1.1 Query conformance gate**: run applicable manifest tests from v0.3.0 onward; extend at each SPARQL milestone (v0.4.0, v0.5.0, v0.9.0, v0.12.0, v0.16.0) until full conformance at v1.0.0
- W3C SPARQL 1.1 Update test suite (from v0.12.0)
- W3C SHACL Core test suite (from v0.7.0)
- SPARQL 1.1 Protocol conformance tests via `curl` (from v0.15.0)

---

## 9. Project Structure

> **Target architecture**: This section describes the **intended end-state repository layout and module structure**, not a claim that every milestone or the current checkout already matches it exactly. The implementation plan is authoritative for the eventual architecture; intermediate releases may temporarily use a simpler layout while converging toward this structure.

> **Cargo workspace**: The target repository is a Cargo workspace with two members: `pg_ripple/` (the PostgreSQL extension) and `pg_ripple_http/` (the companion HTTP binary). The HTTP binary is an empty placeholder (`fn main() {}`) until v0.15.0. Establishing the workspace early in the target architecture avoids a structural disruption later that would break CI, dependency caches, and tooling.

```
pg_ripple/                             # Cargo workspace root
├── Cargo.toml                         # [workspace] manifest listing members = ["pg_ripple", "pg_ripple_http"]
├── pg_ripple/                         # Extension crate (Cargo workspace member)
│   ├── Cargo.toml
│   ├── pg_ripple.control
│   ├── sql/
│   │   ├── pg_ripple--0.1.0.sql              # Initial extension SQL
│   │   ├── pg_ripple--0.1.0--0.2.0.sql       # Upgrade: flat triples table → VP tables (see §4.3 upgrade notes)
│   │   └── pg_ripple--0.N.0--0.N+1.0.sql     # One upgrade script per version transition
│   └── src/
│       ├── lib.rs                         # Extension entry, GUCs, _PG_init
│       ├── error.rs                       # All PT### error types (thiserror); SQLSTATE codes for extension-visible errors
│       ├── dictionary/
│       │   ├── mod.rs
│       │   ├── encoder.rs                 # Encode/decode logic
│       │   ├── cache.rs                   # LRU shared-memory cache (sharded)
│       │   ├── query_cache.rs             # Per-query EncodingCache (short-lived HashMap, discarded after each query)
│       │   ├── bnode.rs                   # Blank node document-scoping (load_generation counter, label namespacing)
│       │   ├── inline.rs                  # Type-tagged inline i64 encoding for numerics, dates, booleans (v0.5.1)
│       │   ├── hot.rs                     # Tiered hot/cold dictionary tables (v0.10.0)
│       │   └── prefix.rs                  # IRI prefix compression
│       ├── storage/
│       │   ├── mod.rs
│       │   ├── vp_table.rs                # VP table DDL management
│       │   ├── delta.rs                   # Delta partition operations (v0.6.0+)
│       │   ├── merge.rs                   # Delta→Main generation merge logic (v0.6.0+)
│       │   ├── subject_patterns.rs        # Subject→predicate-set index (v0.6.0)
│       │   └── bulk_load.rs               # Streaming parsers + COPY
│       ├── sparql/
│       │   ├── mod.rs
│       │   ├── parser.rs                  # spargebra + sparopt integration
│       │   ├── algebra.rs                 # IR and pg_ripple-specific optimizations; reads SHACL catalog before join-tree construction
│       │   ├── sql_gen.rs                 # Algebra → SQL text
│       │   ├── property_path.rs           # Recursive CTE generation
│       │   ├── projector.rs               # Maps decoded i64 rows → named SPARQL variables; applies SELECT expressions, BIND, computed values
│       │   ├── executor.rs                # SPI execution + decoding
│       │   ├── update.rs                  # SPARQL 1.1 Update parsing + execution (INSERT DATA/DELETE DATA v0.5.1; advanced v0.12.0)
│       │   └── federation.rs              # SERVICE keyword: remote endpoint execution + result injection (v0.16.0)
│       ├── datalog/
│       │   ├── mod.rs                     # Public API (#[pg_extern] functions)
│       │   ├── parser.rs                  # Rule text → Rule IR
│       │   ├── stratify.rs                # Dependency graph, stratification, cycle detection
│       │   ├── compiler.rs                # Rule IR → SQL (per stratum)
│       │   ├── builtins.rs                # Built-in rule sets (RDFS, OWL RL)
│       │   └── catalog.rs                 # _pg_ripple.rules table CRUD
│       ├── graph/
│       │   ├── mod.rs
│       │   └── named_graph.rs             # Named graph CRUD
│       ├── shacl/
│       │   ├── mod.rs
│       │   ├── parser.rs                  # SHACL Turtle → shape IR
│       │   ├── compiler.rs                # Shape IR → DDL/triggers
│       │   ├── validator.rs               # Async validation worker
│       │   └── optimizer.rs               # SHACL hints for query planner
│       ├── export/
│       │   ├── mod.rs
│       │   └── serializer.rs              # Turtle/N-Triples/JSON-LD output
│       ├── stats/
│       │   ├── mod.rs
│       │   └── monitoring.rs              # Statistics collection
│       ├── ecosystem/
│       │   ├── mod.rs
│       │   └── trickle.rs                 # pg_trickle integration (optional)
│       └── admin/
│           ├── mod.rs
│           └── maintenance.rs             # Vacuum, reindex, compact, config
├── pg_ripple_http/                    # HTTP companion binary (Cargo workspace member; placeholder until v0.15.0)
│   ├── Cargo.toml                     # axum, tokio, tokio-postgres, deadpool-postgres, reqwest
│   └── src/
│       └── main.rs                    # Placeholder fn main() {}; full axum server at v0.15.0
├── tests/
│   ├── integration_tests.rs
│   └── sparql_conformance.rs
├── sql/
│   ├── regress/
│   │   ├── sql/                       # pg_regress input SQL
│   │   └── expected/                  # Expected output
│   └── bench/
│       └── bsbm.sql                   # Benchmark queries
├── plans/
│   ├── postgresql-triplestore-deep-dive.md
│   └── implementation_plan.md         # This document
├── ROADMAP.md
├── README.md
└── LICENSE
```

---

## 10. Build & Development Setup

```bash
# Prerequisites
rustup update stable        # Rust 1.88+ required for pgrx 0.17
cargo install cargo-pgrx --version 0.17.0 --locked
cargo pgrx init --pg18 download  # Download and compile PG18

# Create extension (inside the pg_ripple/ workspace member folder)
cargo pgrx new pg_ripple --pg18

# Development cycle (run from workspace root or pg_ripple/ member)
cargo pgrx run pg18          # Run in psql
cargo pgrx test pg18         # Run #[pg_test] tests
cargo pgrx regress pg18      # Run pg_regress tests
cargo pgrx package --pg18    # Build installable package

# Benchmarking
cargo pgrx bench pg18        # Run in-process pgbench
```

### Workspace `Cargo.toml` (root)

```toml
[workspace]
members = ["pg_ripple", "pg_ripple_http"]
resolver = "3"
```

### `pg_ripple/Cargo.toml` (extension crate)

```toml
[package]
name = "pg_ripple"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "lib"]

[features]
default = ["pg18"]
pg18 = ["pgrx/pg18"]

[dependencies]
pgrx = "0.17"
spargebra = "0.3"           # SPARQL 1.1 algebra parser
sparopt = "0.1"             # SPARQL algebra optimizer (filter pushdown, constant folding; first pass before pg_ripple optimizer)
rio_turtle = "0.9"          # Turtle/N-Triples parser
rio_api = "0.9"             # RDF API traits
rio_xml = "0.9"             # RDF/XML parser (v0.9.0+)
oxttl = "0.1"               # RDF-star Turtle/N-Triples-star parser (added at v0.4.0)
oxrdf = "0.2"               # RDF-star term types (added at v0.4.0)
xxhash-rust = { version = "0.8", features = ["xxh3"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
lru = "0.12"                # LRU cache
thiserror = "2"             # Error types (PT### error taxonomy in src/error.rs)

[dev-dependencies]
pgrx-tests = "0.17"
proptest = "1"
```

### `pg_ripple_http/Cargo.toml` (HTTP companion binary)

```toml
[package]
name = "pg_ripple_http"
version = "0.1.0"
edition = "2024"

# Empty binary until v0.15.0; only the dependencies below are added at v0.15.0.

[dependencies]
axum = "0.8"                             # HTTP server framework
tokio = { version = "1", features = ["full"] }
tokio-postgres = "0.7"                   # Async PostgreSQL client
deadpool-postgres = "0.14"              # Connection pool
reqwest = { version = "0.12", features = ["json"] }  # Outbound HTTP for federation (v0.16.0)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### `pg_ripple/pg_ripple.control`

```
default_version = '0.1.0'
module_pathname = '$libdir/pg_ripple'
comment = 'High-performance RDF triple store with native SPARQL query support'
schema = 'pg_ripple'
relocatable = false
superuser = false
trusted = true
```

Key fields:
- `schema = 'pg_ripple'` — all user-visible objects are created in the `pg_ripple` schema; internal tables go in `_pg_ripple` (created explicitly in the SQL scripts, not governed by this field)
- `relocatable = false` — VP tables use schema-qualified names that cannot be relocated
- `trusted = true` — v0.1.0–v0.5.1 use no shared memory, background workers, or hooks, so any user with `CREATE` privilege can install the extension. **Changed to `trusted = false` in v0.6.0** when the HTAP architecture introduces `PgSharedMem` and background workers.

---

## 11. Security Considerations

- **SQL Injection**: All SQL generated by the SPARQL→SQL translator uses parameterized queries via SPI's `$N` parameter binding; no string interpolation of user data into SQL
- **Input validation**: RDF parsers (`rio_*`, `oxttl`) are well-tested and handle malformed input gracefully; all external input is validated before dictionary encoding
- **Privilege model**: Extension functions default to `SECURITY INVOKER`; schema `_pg_ripple` is only accessible by the extension owner
- **Resource limits**: `pg_ripple.max_path_depth` prevents unbounded recursive CTEs; `statement_timeout` respected for all SPI calls
- **Memory safety**: Rust's ownership system prevents buffer overflows; pgrx handles Postgres memory context integration
- **SSRF prevention** (v0.16.0 federation): the `SERVICE <url>` keyword can only contact endpoints explicitly registered in `_pg_ripple.federation_endpoints`. Any `SERVICE` clause referencing an unregistered IRI is rejected with a `PT610` error before any network connection is attempted. This prevents Server-Side Request Forgery — an attacker who can craft a SPARQL query cannot use it to probe internal network services or cloud metadata endpoints. The allowlist is managed via `pg_ripple.register_endpoint()` / `pg_ripple.remove_endpoint()` and is restricted to superusers by default.

---

## 12. Future Architecture (Post-1.0)

These items are documented for architectural awareness but are not in the 0.1–1.0 scope:

- **Distributed execution via Citus**: Subject-based sharding of VP tables across worker nodes
- **pgvector integration**: Store embeddings alongside graph nodes for hybrid semantic + vector search
- **Automated ExtVP**: Workload-driven analysis to automatically decide which semi-join stream tables to create (manual ExtVP via `create_sparql_view()` is in-scope for 0.x when pg_trickle is present)
- **Temporal versioning**: Bitstring validity columns for versioned graph snapshots
- **TimescaleDB integration**: Hypertable-backed temporal graph management
- **Cypher / GQL**: Query and write data using industry-standard graph query languages via a standalone `cypher-algebra` crate (see ROADMAP v1.6)
- **GraphQL-to-SPARQL bridge**: Auto-generate GraphQL schema from SHACL shapes
- **GeoSPARQL + PostGIS**: `geo:asWKT` literal type backed by PostGIS `geometry`, spatial FILTER functions, R-tree index on spatial VP tables (see ROADMAP v1.7)
- **OTTR template expansion**: `pg_ripple.expand_template(iri TEXT, query TEXT)` for OTTR-style DataFrame→RDF bulk load (see [prior_art_commercial.md](ecosystem/prior_art_commercial.md))
- **Ontology change propagation DAG**: When pg_trickle is present, model derived structures (ExtVP, inference, SHACL, stats) as a DAG of stream tables with automatic topological refresh on ontology changes

---

## 13. Operational Considerations

### 13.1 Merge Worker Health

- The merge worker registers a heartbeat timestamp in shared memory, updated on each cycle
- If the heartbeat stalls for longer than `pg_ripple.merge_watchdog_timeout` (default: 5 minutes), `_PG_init` on the next backend connection logs a `WARNING` and attempts to restart the worker
- `pg_ripple.stats()` includes `merge_worker_status` (`running` / `stalled` / `disabled`) and `merge_worker_last_heartbeat`

### 13.2 Shared-Memory Cache Lifecycle

- The dictionary LRU cache resides in `PgSharedMem` and survives individual backend crashes
- The cache is cleared on postmaster restart (standard PostgreSQL shared-memory lifecycle)
- Slot versioning (§4.1) detects layout mismatches after an in-place extension upgrade and re-initializes gracefully

### 13.3 `pg_upgrade` Behaviour

- Extension tables (`_pg_ripple.*`) migrate with standard `pg_upgrade` — no special handling required
- Shared-memory state (dictionary cache, bloom filters) is rebuilt from on-disk tables at the first `_PG_init` after the upgrade
- The slot versioning mechanism (§4.1) ensures safe re-initialization if the shared-memory layout changed between versions

### 13.4 Extension Downgrades

- Downgrades are **not supported** (standard for PostgreSQL extensions)
- Users should test upgrades on a staging instance and rely on `pg_dump`/`pg_restore` for rollback

### 13.5 Dictionary Vacuum Concurrency

- `pg_ripple.vacuum_dictionary()` acquires an `ADVISORY LOCK` to prevent concurrent runs
- Concurrent inserts are safe: the vacuum only deletes dictionary entries with zero references across all VP tables, checked via `NOT EXISTS` subqueries within a single snapshot
- Running `vacuum_dictionary()` during heavy bulk loads is discouraged but safe — it may miss newly-orphaned entries which will be cleaned on the next run

### 13.6 Error Code Taxonomy

Extension error messages use PostgreSQL-style formatting (lowercase first word, no trailing period). Error codes use the `PT` prefix:

| Range | Category |
|---|---|
| `PT001`–`PT099` | Dictionary errors (encoding failures, hash collisions, cache overflow) |
| `PT100`–`PT199` | Storage errors (VP table creation, merge failures, bulk load errors) |
| `PT200`–`PT299` | SPARQL errors (parse failures, unsupported features, query timeout) |
| `PT300`–`PT399` | SHACL errors (shape parse failures, validation violations) |
| `PT400`–`PT499` | Datalog errors (rule parse failures, stratification errors, constraint violations) |
| `PT500`–`PT599` | Admin errors (vacuum, reindex, upgrade) |
| `PT600`–`PT699` | Federation / HTTP errors (endpoint unreachable, SSRF rejection, timeout) |
| `PT700`–`PT799` | Serialization / export errors (format errors, encoding failures) |

---

## 14. Documentation

> **Authoritative source**: [plans/documentation.md](documentation.md) is the definitive specification for the documentation site — page list, section structure, tooling setup, and milestone delivery schedule. This section is a summary for implementers; always read `plans/documentation.md` alongside this plan when working on documentation deliverables.

### 14.1 Tooling

| Concern | Technology |
|---|---|
| Site generator | [mdBook](https://rust-lang.github.io/mdBook/) |
| Hosting | GitHub Pages, published via `.github/workflows/docs.yml` |
| Diagrams | `mdbook-mermaid` preprocessor (added at v0.6.0) |
| Link validation | `mdbook-linkcheck` preprocessor (from day one) |
| Mirrored files | CHANGELOG.md, ROADMAP.md, RELEASE.md, and all `plans/` research documents copied at CI build time — not symlinked |
| Local development | `cargo install mdbook && cd docs && mdbook serve` |

### 14.2 Site Structure

Three top-level sections, each a directory under `docs/src/`:

```
docs/src/
  user-guide/            — Task-oriented: install, query, configure, scale
    introduction.md
    installation.md
    getting-started.md
    playground.md        — Docker sandbox (⭐ highest adoption leverage)
    sql-reference/       — One page per SQL API surface area
    best-practices/      — Pattern guides (bulk loading, SPARQL, SHACL, Datalog)
    configuration.md     — All GUC parameters
    scaling.md
    pre-deployment.md
    upgrading.md
    backup-restore.md
    contributing.md
  reference/             — Look-up: errors, FAQ, changelog, runbook
    faq.md
    troubleshooting.md
    error-reference.md   — PT001–PT799 table
    changelog.md         — (mirrored)
    roadmap.md           — (mirrored)
    release-process.md   — (mirrored)
    security.md
  research/              — Architecture rationale, prior art, design decisions
    index.md
    prior-art.md
    postgresql-deepdive.md
    pg-trickle.md
    … (mirrors of plans/ documents)
```

### 14.3 Writing Conventions

- **Lead with what the user can do**, not implementation detail — "Load a Turtle file in one SQL call" before any mention of VP tables.
- **One working copy-paste example per function** in the SQL Reference.
- Implementation detail goes in a clearly marked "Internal / Advanced" subsection or a `<details>` block.
- Add `> **Available since vX.Y.Z**` callouts at the top of sections describing features not present from v0.1.0.
- Every SQL Reference page must link back to the roadmap milestone where the feature was introduced.

### 14.4 Milestone Delivery

Each ROADMAP.md version section includes a `### Documentation` subsection listing the specific `docs/src/` pages that must be created or expanded as part of that version's deliverables. Documentation checkboxes in those sections are treated the same as code deliverables — they must be ticked (`- [x]`) before a version is considered done and the exit criteria are satisfied.

**v0.5.0 special case**: Because v0.1.0–v0.4.0 shipped without any documentation, v0.5.0 carries the full catch-up backlog (site scaffold, installation, getting started, playground, SQL reference for all released APIs, best practices, FAQ, configuration, troubleshooting) in addition to its own new pages.

### 14.5 Error Reference Generation

The `reference/error-reference.md` page documents every PT error code (PT001–PT799, see §13.6). The target workflow is:
1. Error codes and their message templates are defined in `src/error.rs` and its subsystem-specific modules.
2. A build-time step or a manual curation pass extracts them into `docs/src/reference/error-reference.md`.
3. Each entry includes: code, subsystem, message template, and a resolution note written for operators, not developers.

The full PT001–PT799 table is due at v0.14.0 (see ROADMAP.md `### Documentation` for that version).
