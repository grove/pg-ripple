# pg_triple — Implementation Plan

## 1. Project Overview

**pg_triple** is a PostgreSQL 18 extension written in Rust using pgrx 0.17 that implements a high-performance, scalable RDF triple store. It brings native SPARQL query capability, dictionary-encoded storage with vertical partitioning, HTAP architecture, SHACL validation, and optional distributed execution—all within PostgreSQL.

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
| SPARQL optimizer | `sparopt` crate (Apache-2.0/MIT; first-pass algebra optimizer fed from `spargebra` output; adds filter pushdown, constant folding, empty-pattern elimination before pg_triple's own pass; v0.3.0+) |
| RDF parsers | `rio_turtle`, `rio_xml` crates (Turtle, N-Triples, RDF/XML); `oxttl` / `oxrdf` added at v0.4.0 for RDF-star |
| Hashing | `xxhash-rust` (XXH3-128 for dictionary collision resistance) |
| Serialization | `serde` + `serde_json` (SHACL reports, SPARQL results, config) |
| HTTP server | `axum` (built on tokio) — SPARQL Protocol HTTP endpoint (`pg_triple_http` binary) |
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
- `_PG_init()` hook for shared memory registration and background worker startup
- GUC parameters: `pg_triple.default_graph`, `pg_triple.dictionary_cache_size`, `pg_triple.merge_threshold`, `pg_triple.enable_shacl`, `pg_triple.named_graph_optimized` (adds G-leading index on each VP table; off by default)
- **GUC-gated lazy initialization**: the merge worker, SHACL validator, and reasoning engine are only started when their respective GUCs (`pg_triple.merge_threshold`, `pg_triple.enable_shacl`, `pg_triple.enable_reasoning`) are non-zero/true. `_PG_init` never starts subsystems the user has not enabled.
- **Shared-memory slot versioning**: the first 16 bytes of every `PgSharedMem` slot are a fixed magic number (`0x70675f74726970_00` = `p g _ t r i p l e \0`) followed by a 4-byte layout version integer. On startup the extension checks both; a mismatch (e.g. after an in-place upgrade) triggers a controlled re-initialization rather than a silent crash.
- Extension SQL: `CREATE EXTENSION pg_triple` creates core schema and catalog tables

### 4.2 Dictionary Encoder (`src/dictionary/`)

**Purpose**: Map every IRI, blank node, and literal to a compact `i64` identifier.

#### 4.2.1 Schema

```sql
-- Resource dictionary (IRIs and blank nodes)
CREATE TABLE _pg_triple.resource_dict (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash      BYTEA NOT NULL,          -- XXH3-128 of the IRI/bnode
    value     TEXT NOT NULL,
    kind      SMALLINT NOT NULL         -- 0=IRI, 1=blank node
);
CREATE UNIQUE INDEX ON _pg_triple.resource_dict (hash);

-- Literal dictionary (typed values)
CREATE TABLE _pg_triple.literal_dict (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash      BYTEA NOT NULL,
    value     TEXT NOT NULL,
    datatype  BIGINT REFERENCES _pg_triple.resource_dict(id),
    lang      TEXT
);
CREATE UNIQUE INDEX ON _pg_triple.literal_dict (hash);
```

#### 4.2.2 Implementation

- **Encoding path** (`encode()`): Hash → check in-memory cache (LRU, configurable via GUC) → check PG table → INSERT if miss → return `i64`
- **Decoding path** (`decode()`): `i64` → LRU cache → PG lookup → return string
- **Batch decoding** (`decode_batch()`): Collect all output `i64` IDs from a result set, resolve in a single `WHERE id = ANY(...)` query, build an in-memory `HashMap<i64, String>`, then emit decoded rows. Avoids per-row dictionary round-trips — critical for large result sets
- **Batch encoding** (`encode_batch()`): Bulk insert with `ON CONFLICT DO NOTHING` + `RETURNING`, minimising round-trips during data load
- **Per-query `EncodingCache`** (`src/dictionary/query_cache.rs`): A short-lived `HashMap<&str, i64>` allocated at the start of each SPARQL query and discarded when the query exits. Constants appearing multiple times in a pattern (e.g. the same IRI in multiple BGPs) are encoded once and reused within the same query without hitting the shared-memory LRU or the database. Distinct from `encode_batch()` which is used during data load.
- **In-memory cache**: `HashMap<u128, i64>` in shared memory via pgrx `PgSharedMem`, **sharded into N buckets** (default: 64) with per-shard lightweight locks to eliminate global lock contention under concurrent workloads. Sized by GUC
- **Shared-memory budget**: `pg_triple.shared_memory_limit` GUC governs total allocation across dictionary cache, bloom filters, and merge worker buffers. Automatic eviction priority: bloom filters first, then oldest LRU dictionary entries. Back-pressure on bulk loads when utilisation exceeds 90%
- **Prefix compression**: Common IRI prefixes (registered via `pg_triple.register_prefix()`) are stripped before hashing and stored separately, reducing storage by ~40% for typical RDF datasets
- **Inline value encoding** (`src/dictionary/inline.rs`, v0.3.0): Type-tagged i64 values for `xsd:integer`, `xsd:boolean`, `xsd:dateTime`, `xsd:date`, `xsd:double`. Bit 63 set signals an inline value; bits 56–62 hold a 7-bit type code; bits 0–55 hold the encoded value. FILTER comparisons on these types require zero dictionary round-trips — the SPARQL→SQL translator encodes constants at translation time and emits a plain B-tree range condition on the VP column.
- **ID ordering** (v0.3.0): Typed-literal IDs are allocated in monotonically increasing semantic order within each type (integers by numeric value, dates chronologically). This enables FILTER range conditions to compile to `BETWEEN $lo AND $hi` scans on the raw i64 column without decoding. The integer and date ranges are disjoint from IRI ranges via the type-tag bits.
- **Tiered dictionary** (`src/dictionary/hot.rs`, v0.10.0): `_pg_triple.resources_hot` (UNLOGGED, stays in `shared_buffers`) holds IRIs ≤512 bytes, all prefix-registry IRIs, and all predicate IRIs. `_pg_triple.resources_cold` (heap) holds long literals and infrequently-accessed IRIs. The encoder checks hot first; `pg_prewarm` warms `resources_hot` at server start via `_PG_init`. At Wikidata scale (3B vocabulary entries, 190 GB uncompressed), this keeps the hot lookup path I/O-free for the overwhelming majority of query-time decodes.

### 4.3 Storage Engine (`src/storage/`)

**Purpose**: Physically store triples as integer tuples in vertically partitioned tables.

#### 4.3.1 Vertical Partitioning

Each unique predicate `p` gets its own table:

```sql
CREATE TABLE _pg_triple.vp_{predicate_id} (
    s       BIGINT NOT NULL,  -- subject dictionary ID
    o       BIGINT NOT NULL,  -- object dictionary ID
    g       BIGINT NOT NULL DEFAULT 0,  -- named graph ID (0 = default)
    i       BIGINT GENERATED ALWAYS AS IDENTITY,  -- statement identifier (SID); LPG-ready from v0.2.0
    source  SMALLINT NOT NULL DEFAULT 0  -- 0 = explicit triple; 1 = rule-derived (v0.10.0)
);
CREATE INDEX ON _pg_triple.vp_{predicate_id} (s, o);
CREATE INDEX ON _pg_triple.vp_{predicate_id} (o, s);
-- Created only when pg_triple.named_graph_optimized = true:
-- CREATE INDEX ON _pg_triple.vp_{predicate_id} (g, s, o);
```

- Tables are created dynamically on first encounter of a new predicate during data ingestion
- A catalog table `_pg_triple.predicates` maps predicate dictionary IDs to table OIDs for fast lookup
- PG18's **skip scan** on the composite B-tree indices enables efficient lookups even when only the second column (`o`) is bound
- **`i` column (Statement Identifier)** (v0.2.0): Every statement gets a unique `BIGINT` identity via `GENERATED ALWAYS AS IDENTITY`. This makes the storage schema SPOI-compatible (inspired by the OneGraph 1G model). SIDs are used by RDF-star (v0.4.0) for edge properties and meta-statements: the SID of a statement can appear in the `s` or `o` position of another VP table row, enabling statements about statements without structural changes. A `_pg_triple.statements` catalog view maps SIDs to their containing VP table for cross-table SID lookups.
- **`source` column** (v0.10.0): `SMALLINT DEFAULT 0` — `0` = explicit triple asserted by the user; `1` = derived triple produced by the Datalog/RDFS/OWL RL reasoning engine. Queries can pass `include_derived := false` to filter to `WHERE source = 0` only. Because the column is added as part of the v0.10.0 migration script, it has zero cost before reasoning is enabled.
- **Named-graph index** (`pg_triple.named_graph_optimized = true`): when enabled, each VP table gains an additional `(g, s, o)` index supporting `GRAPH ?g { ... }` patterns without a full-table scan. Off by default to avoid index bloat for single-graph users.

**Rare-Predicate Consolidation**:
- Predicates with fewer than `pg_triple.vp_promotion_threshold` triples (default: 1,000) are stored in a shared `_pg_triple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT, i BIGINT GENERATED ALWAYS AS IDENTITY)` table with three secondary indices:
  - `(p, s, o)` — primary access pattern: all triples for a given predicate
  - `(s, p)` — DESCRIBE queries: enumerate all predicates for a given subject without a full-table scan
  - `(g, p, s, o)` — graph-drop: enumerate and bulk-delete all triples in a named graph
- Once a predicate crosses the threshold, its rows are auto-migrated to a dedicated VP table and the catalog updated — transparent to callers
- Promotion is **deferred to end-of-statement**: during bulk loads, triples accumulate in `vp_rare`; after the load completes (or during the next merge worker cycle), predicates exceeding the threshold are promoted in a single `INSERT … SELECT` + `DELETE` transaction
- `pg_triple.promote_rare_predicates()` can also be called manually
- Prevents catalog bloat for predicate-rich datasets (DBpedia ≈60K predicates, Wikidata ≈10K predicates) — avoids hundreds of thousands of PostgreSQL objects, reduces planner overhead, and cuts VACUUM cost

#### 4.3.2 HTAP Dual-Partition Architecture

**Delta Partition** (write-optimized):
- Standard heap tables with B-tree indices
- All INSERTs and DELETEs target the delta partition
- Small enough to remain in `shared_buffers`

**Main Partition** (read-optimized):
- BRIN-indexed, `CLUSTER`-ed by subject for sequential access
- Populated by the background merge worker
- Uses PG18 async I/O for faster sequential scans

**Merge Worker** (background worker via pgrx `BackgroundWorker`):
- Periodically merges delta into main when delta exceeds `pg_triple.merge_threshold` rows
- Runs as a pgrx background worker with `BGWORKER_SHMEM_ACCESS`
- **Fresh-table generation merge** (v0.6.0): each merge cycle creates a *new* `vp_{id}_main_new` table rather than inserting incrementally into the existing one (incremental inserts degrade BRIN effectiveness because BRIN requires physically sorted data):
  1. `CREATE TABLE _pg_triple.vp_{id}_main_new` (heap)
  2. `INSERT … SELECT … ORDER BY s` from delta into the new table
  3. `CLUSTER vp_{id}_main_new USING (s, o, g)` — physically sorts rows for BRIN
  4. `ALTER TABLE … RENAME` to atomically replace the old main (catalog-only, zero query downtime since queries read `UNION ALL delta + main`)
  5. Old main retained for `pg_triple.merge_retention_seconds` (GUC, default 60s) then `DROP TABLE`
- `pg_triple.compact(keep_old BOOL DEFAULT false)` triggers an immediate full merge across all VP tables; `keep_old := false` drops previous generations immediately
- Updates BRIN summaries post-merge
- Runs `ANALYZE` on merged VP tables so the PostgreSQL planner has fresh selectivity estimates
- Triggers `pg_triple.promote_rare_predicates()` for any rare predicates that crossed the promotion threshold
- Signals completion via shared-memory latch
- **Commit-hook early trigger**: a PostgreSQL `ProcessUtility` hook (or `ExecutorEnd` hook) detects when a write transaction commits more than `pg_triple.latch_trigger_threshold` rows (default: 10,000) and pokes the merge worker's shared-memory latch immediately — avoiding the full polling interval wait for bursty workloads. Implemented as an `ExecutorEnd_hook` in `src/storage/merge.rs`.

**Query Path**:
- `UNION ALL` of main + delta, with bloom filter for fast existence checks
- For queries touching only historical data, the delta scan is skipped

#### 4.3.3 Bulk Loading

- `pg_triple.load_turtle(path TEXT)` / `pg_triple.load_ntriples(path TEXT)`
- Parses via `rio_turtle` / `rio_api` crates in streaming fashion
- Batches of 10,000 triples: dictionary-encode → `COPY` into delta VP tables
- Disables index updates during load; rebuilds at end
- Uses PG18 `COPY ... REJECT_LIMIT` for fault tolerance

#### 4.3.4 Subject Patterns (`_pg_triple.subject_patterns`, v0.5.0)

Precomputed index mapping each subject to the sorted array of all its predicate IDs:

```sql
CREATE TABLE _pg_triple.subject_patterns (
    s        BIGINT NOT NULL,
    pattern  BIGINT[] NOT NULL,  -- sorted array of predicate IDs for this subject
    PRIMARY KEY (s)
);
CREATE INDEX ON _pg_triple.subject_patterns USING GIN (pattern);
```

- **DESCRIBE queries**: look up `pattern` for the subject in one index seek, then query only the N VP tables in the array — O(N) instead of scanning all VP tables
- **Statistics**: `SELECT unnest(pattern), count(*) FROM subject_patterns GROUP BY 1` gives predicate-popularity counts without touching VP tables
- **GIN index**: enables "subjects that have both predicate P1 and P2" queries (`pattern @> ARRAY[$1, $2]`) efficiently
- Maintained by the merge worker after each generation merge, not on individual INSERTs

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
Algebra Optimizer (Rust)  — pg_triple-specific second pass
    - Self-join elimination
    - Optional-to-inner downgrade (with SHACL hints)
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
    - Join-order hints: `<http://pg-triple.io/hints/join-order>` in query prologue
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
pg_triple.sparql(query TEXT, include_derived BOOL DEFAULT true) RETURNS SETOF JSONB
pg_triple.sparql_explain(query TEXT, analyze BOOL DEFAULT false) RETURNS TEXT
  -- analyze := true wraps the generated SQL in EXPLAIN (ANALYZE, BUFFERS) and returns the plan

-- Data manipulation
pg_triple.insert_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL) RETURNS BIGINT  -- returns SID from v0.4.0
pg_triple.delete_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL)
pg_triple.load_turtle(data TEXT) RETURNS BIGINT  -- returns count
pg_triple.load_ntriples(data TEXT) RETURNS BIGINT

-- Maintenance
pg_triple.vacuum_dictionary() RETURNS BIGINT  -- removes unreferenced dictionary entries; safe to run any time
pg_triple.compact(keep_old BOOL DEFAULT false) RETURNS VOID  -- trigger immediate full generation merge
```

#### 4.4.3 Join Optimization Strategies

1. **Self-join elimination**: Star patterns on the same subject collapse into a single scan of the subject across multiple VP tables, joined by subject ID equality
2. **Optional-self-join elimination**: When SHACL declares `sh:minCount 1`, OPTIONAL → INNER JOIN
3. **Self-union elimination**: Multiple triple patterns binding the same variable to different predicates are rewritten to `WHERE predicate_id IN (...)`
4. **Projection pushing**: `SELECT DISTINCT ?p` queries enumerate the `_pg_triple.predicates` catalog instead of scanning all VP tables
5. **Filter pushdown**: SPARQL `FILTER` clauses operating on bound IRIs are resolved to integer IDs *before* generating SQL, ensuring B-tree index usage. For typed numeric/date literals, the inline-encoded i64 range (see §4.2.2) enables `BETWEEN $lo AND $hi` range scans with no decode step.
6. **Merge-join enablement**: When the join variable matches the `s` sort key of a VP table's `(s, o, g)` primary index, the emitter wraps the CTE in `ORDER BY s`. The PostgreSQL planner then considers a merge join rather than a hash join, reducing memory pressure for large intermediate results.
7. **BGP join reordering** (v0.13.0): The algebra optimizer reads `pg_stats.n_distinct` and `pg_class.reltuples` for each VP table involved in the query and reorders BGPs cheapest-first (most selective predicate scanned first). This provides a statistics-driven complement to `sparopt`'s structural rewrites.
8. **Join-order hints**: A `<http://pg-triple.io/hints/join-order>` pragma in the SPARQL prologue (e.g. `PREFIX hint: <http://pg-triple.io/hints/>`) causes the SQL generator to emit `SET LOCAL join_collapse_limit = 1` around the generated SQL, locking in the BGP-reordered join sequence and preventing the PG planner from re-ordering it.
9. **`no-inference` hint**: Adding `hint:no-inference true` to the query prologue appends `AND source = 0` on every VP table scan, restricting results to explicitly asserted triples only (v0.10.0+).

#### 4.4.4 Property Path Compilation

SPARQL property paths (`+`, `*`, `?`) compile to `WITH RECURSIVE` CTEs with PG18's `CYCLE` clause for hash-based cycle detection:

```sql
WITH RECURSIVE path(s, o, depth) AS (
    -- Anchor: direct one-hop
    SELECT s, o, 1
    FROM _pg_triple.vp_{predicate_id}
    WHERE s = $1
  UNION ALL
    -- Recursive: extend by one hop
    SELECT p.s, vp.o, p.depth + 1
    FROM path p
    JOIN _pg_triple.vp_{predicate_id} vp ON p.o = vp.s
    WHERE p.depth < pg_triple.max_path_depth
)
CYCLE o SET is_cycle USING cycle_path
SELECT DISTINCT s, o FROM path WHERE NOT is_cycle;
```

- Configurable `pg_triple.max_path_depth` GUC (default: 100)
- PG18 `CYCLE` clause for hash-based cycle detection (replaces array-based visited tracking — $O(1)$ membership checks instead of $O(n)$ array scans)
- PG18's improved CTE performance benefits recursive path queries

### 4.5 Named Graph Support (`src/graph/`)

- The `g` column in VP tables stores the named graph dictionary ID
- `g = 0` represents the default graph
- SPARQL `GRAPH ?g { ... }` and `FROM NAMED <uri>` map to `WHERE g = encode(uri)` filters
- Graph management functions:
  - `pg_triple.create_graph(uri TEXT)`
  - `pg_triple.drop_graph(uri TEXT)`
  - `pg_triple.list_graphs() RETURNS SETOF TEXT`

### 4.6 SHACL Validation Engine (`src/shacl/`)

**Purpose**: Enforce data integrity constraints defined in SHACL shapes.

#### 4.6.1 Static Constraint Compilation

SHACL shapes loaded via `pg_triple.load_shacl(data TEXT)` are transpiled to:

| SHACL Constraint | PostgreSQL Implementation |
|---|---|
| `sh:minCount 1` | `NOT NULL` on VP table (or CHECK trigger) |
| `sh:maxCount 1` | `UNIQUE` index on `(s, g)` in the VP table |
| `sh:datatype xsd:integer` | `CHECK` constraint on literal dictionary entry's datatype |
| `sh:in (...)` | `CHECK (o IN (...))` on VP table |
| `sh:pattern` | `CHECK` constraint with regex on decoded value |
| `sh:class` | Trigger verifying `rdf:type` triple exists |
| `sh:node` / `sh:property` (complex) | PL/pgSQL validation trigger |

#### 4.6.2 Asynchronous Validation Pipeline

For bulk loads where synchronous validation is too expensive:

1. Lightweight trigger captures inserted triple IDs into `_pg_triple.validation_queue`
2. Background worker (pgrx `BackgroundWorker`) processes queued triples against loaded SHACL shapes
3. Invalid triples moved to `_pg_triple.dead_letter_queue` with violation report (as JSONB)
4. Valid triples remain in the VP tables

#### 4.6.3 Query Optimization via SHACL

The SPARQL→SQL translator reads loaded SHACL shapes:
- `sh:minCount 1` → downgrade LEFT JOIN to INNER JOIN for that predicate
- `sh:maxCount 1` → enables single-row optimizations (no need for DISTINCT)
- `sh:class` / `sh:targetClass` → enables type-based pruning of VP tables to scan

### 4.7 Serialization & Export (`src/export/`)

- `pg_triple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
- `pg_triple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
- `pg_triple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
- Streaming output via `RETURNS SETOF TEXT` for large graphs

### 4.8 Statistics & Monitoring (`src/stats/`)

- `pg_triple.stats() RETURNS JSONB` — triple count, predicate distribution, dictionary size, cache hit ratio, delta/main partition sizes
- Integration with `pg_stat_statements` for SPARQL query tracking
- Custom `EXPLAIN` option (PG18 feature) to annotate SPARQL→SQL translations
- **When pg_trickle is available**: `stats()` reads from `_pg_triple.predicate_stats` and `_pg_triple.graph_stats` stream tables (instant, no full scan) instead of re-scanning VP tables on every call. See §4.10.

### 4.9 Administrative Functions (`src/admin/`)

- `pg_triple.vacuum()` — force delta→main merge
- `pg_triple.compact(keep_old BOOL DEFAULT false)` — immediate full generation merge across all VP tables; `keep_old := false` drops previous main-table generations immediately
- `pg_triple.vacuum_dictionary() RETURNS BIGINT` — removes dictionary entries not referenced by any VP table column; returns count of removed entries
- `pg_triple.reindex()` — rebuild VP table indices
- `pg_triple.dictionary_stats()` — cache hit ratio, dictionary sizes
- `pg_triple.register_prefix(prefix TEXT, expansion TEXT)` — IRI prefix registration
- `pg_triple.prefixes() RETURNS TABLE(prefix TEXT, expansion TEXT)`

### 4.10 Ecosystem: pg_trickle Integration (`src/ecosystem/`)

**Purpose**: Optional reactivity layer powered by [pg_trickle](https://github.com/grove/pg-trickle) stream tables. All features in this module require pg_trickle to be installed; core pg_triple functionality works without it. See [full analysis](ecosystem/pg_trickle.md).

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

When pg_trickle is detected, `pg_triple.enable_live_statistics()` creates stream tables:

- `_pg_triple.predicate_stats` — per-predicate triple count, distinct subjects/objects (refreshed every 5s)
- `_pg_triple.graph_stats` — per-graph triple count (refreshed every 10s)

`pg_triple.stats()` reads from these stream tables instead of full-scanning VP tables — 100–1000× faster.

#### 4.10.3 SHACL Violation Monitors

Simple SHACL constraints (cardinality, datatype, class) can be modeled as stream tables with `IMMEDIATE` refresh mode, validating within the same transaction as the DML:

- `sh:minCount` violations → `NOT EXISTS` stream table
- `sh:datatype` violations → filtered join stream table
- Multiple shapes → pg_trickle's DAG scheduler handles refresh ordering

Complex shapes (`sh:or`, `sh:and`, multi-hop) still use the procedural validation pipeline from §4.6.

#### 4.10.4 Inference Materialization (→ Datalog Engine)

> **Note**: This section is superseded by the general Datalog reasoning engine. See [plans/ecosystem/datalog.md](plans/ecosystem/datalog.md) for the full design.

The original plan — `pg_triple.enable_inference_materialization()` creating hard-coded `WITH RECURSIVE` stream tables for `rdfs:subClassOf` and `rdfs:subPropertyOf` — is replaced by a general-purpose Datalog engine that:

- Parses user-defined and built-in rules (RDFS, OWL RL) in a Turtle-flavoured Datalog syntax
- Stratifies rules to handle negation-as-failure correctly
- Compiles each stratum to SQL: non-recursive → `INSERT … SELECT`, recursive → `WITH RECURSIVE … CYCLE`, negation → `NOT EXISTS`
- Materializes derived predicates as pg_trickle stream tables (recommended) or inlines them as CTEs at query time (on-demand, no pg_trickle needed)
- Registers derived VP tables in `_pg_triple.predicates` so the SPARQL engine treats them identically to base VP tables
- Multi-head rules: each head atom may target a different predicate and carry an optional named graph ID
- **Incremental materialization phases** (inspired by RDFox): each materialization cycle runs three phases in order:
  1. *Addition* — derive and insert new triples produced by rules applied to newly asserted facts; write with `source = 1`
  2. *Deletion* — identify derived triples whose support has been retracted; remove them from VP tables
  3. *BwdChain* — re-derive any derived triple that was deleted but is still entailed by surviving facts (avoids over-deletion)
- **Rule set catalog**: `_pg_triple.rule_sets (name TEXT, graph_ids BIGINT[], rule_hash BIGINT)` stores named rule sets. `rule_hash` is the XXH3-64 hash of the canonicalized rule text; the materialization worker skips re-computation when the hash is unchanged. Rule set caches are keyed on this hash so a re-activated rule set resumes from its previous derived state.
- **Named rule sets**: `pg_triple.load_rules(name TEXT, rules TEXT)` registers a rule set; `pg_triple.enable_rule_set(name TEXT)` activates it for a given set of named graphs.

#### 4.10.5 SPARQL Views

```sql
pg_triple.create_sparql_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT DEFAULT '5s'
) RETURNS VOID
```

Parses SPARQL → generates SQL → creates a pg_trickle stream table. The result is an always-fresh materialized SPARQL query: multi-join VP table scans + dictionary decoding happen once during materialization, and queries become simple table scans.

#### 4.10.5.1 Datalog Views

```sql
pg_triple.create_datalog_view(
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

## 5. Data Flow: Insert Path

```
1. pg_triple.insert_triple('http://ex.org/Alice', 'http://ex.org/knows', 'http://ex.org/Bob')
2. Dictionary encode: s=42, p=7, o=43
3. Look up predicate p=7 → vp_7 table
4. INSERT INTO _pg_triple.vp_7_delta (s, o, g) VALUES (42, 43, 0)
5. If SHACL enabled: queue validation (async) or validate inline (sync)
6. Background worker periodically merges vp_7_delta → vp_7_main
```

## 6. Data Flow: Query Path

```
1. pg_triple.sparql('SELECT ?name WHERE { ?person foaf:knows ex:Bob . ?person foaf:name ?name }')
2. Parse → Algebra: Join(BGP(person, foaf:knows, ex:Bob), BGP(person, foaf:name, name))
3. Encode bound terms: ex:Bob → 43, foaf:knows → 7, foaf:name → 12
4. Generate SQL:
     SELECT d.o AS name
     FROM (SELECT s FROM _pg_triple.vp_7 WHERE o = 43
           UNION ALL
           SELECT s FROM _pg_triple.vp_7_delta WHERE o = 43) AS knows
     JOIN (SELECT s, o FROM _pg_triple.vp_12
           UNION ALL
           SELECT s, o FROM _pg_triple.vp_12_delta) AS name_tbl
       ON knows.s = name_tbl.s
5. Execute via SPI
6. Batch decode: collect all i64 IDs from result → single `WHERE id = ANY(...)` → build decode map
7. Emit decoded rows as SETOF JSONB: [{"name": "Alice"}, ...]
```

---

## 7. Performance Targets

> **Calibration reference**: QLever (C++, Apache-2.0) on DBLP (390M triples) loads at 1.7M triples/s, produces an 8 GB index, and answers benchmark queries in 0.7s average. QLever's flat pre-sorted permutation files make every SPARQL join a merge join with zero random I/O. pg_triple's B-tree/heap design pays ~5× overhead on bulk sequential scans in exchange for transactional concurrent writes, MVCC, and the full PostgreSQL ecosystem. The targets below reflect this accepted trade-off.

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

- `cargo-fuzz` with libFuzzer on the SPARQL→SQL pipeline: feed random/mutated SPARQL strings through parser and SQL generator; verify no panics, no invalid SQL emitted, no memory safety violations
- Fuzz targets for Turtle parser integration (complement `rio_turtle`'s own fuzz testing with pg_triple's error propagation layer)
- Fuzz targets for Datalog rule parser
- Run in CI nightly (time-limited: 10 minutes per target)

### 8.5 Concurrency Testing

- Concurrent dictionary encode: two backends encoding the same IRI must return the same i64 (verifies shard lock correctness)
- Dictionary cache eviction: verify decode correctness after cache entries are evicted under memory pressure
- Concurrent merge + write: bulk insert and merge worker running simultaneously with no data loss
- Merge worker edge cases: empty delta (no-op), crash during merge (recovery), near-capacity shared memory (back-pressure)

### 8.6 Performance Regression

- **CI benchmark gate** (from v0.2.0): record insert throughput and point-query latency as baselines; fail CI if a commit regresses throughput by >10%
- Baselines extended at each milestone: star queries (v0.3.0), property paths (v0.5.0), concurrent read/write (v0.6.0), BSBM full mix (v0.13.0)
- Performance regression suite maintained as pgbench custom scripts in `sql/bench/`

### 8.7 Benchmarks

- pgrx-bench integration for in-process pgbench
- Berlin SPARQL Benchmark (BSBM) adapted to SQL function calls
- SP2Bench for academic comparison points
- Custom benchmarks:
  - Bulk load: 1M, 10M, 100M triples
  - Point queries vs star queries vs path queries
  - Concurrent read/write under HTAP workload

### 8.8 Conformance

- **W3C SPARQL 1.1 Query conformance gate**: run applicable manifest tests from v0.3.0 onward; extend at each SPARQL milestone (v0.4.0, v0.5.0, v0.9.0, v0.12.0, v0.16.0) until full conformance at v1.0.0
- W3C SPARQL 1.1 Update test suite (from v0.12.0)
- W3C SHACL Core test suite (from v0.7.0)
- SPARQL 1.1 Protocol conformance tests via `curl` (from v0.15.0)

---

## 9. Project Structure

```
pg_triple/
├── Cargo.toml
├── pg_triple.control
├── sql/
│   ├── pg_triple--0.1.0.sql          # Initial extension SQL
│   └── pg_triple--0.1.0--0.2.0.sql   # Upgrade scripts
├── src/
│   ├── lib.rs                         # Extension entry, GUCs, _PG_init
│   ├── dictionary/
│   │   ├── mod.rs
│   │   ├── encoder.rs                 # Encode/decode logic
│   │   ├── cache.rs                   # LRU shared-memory cache
│   │   ├── query_cache.rs             # Per-query EncodingCache (short-lived HashMap, discarded after each query)
│   │   ├── inline.rs                  # Type-tagged inline i64 encoding for numerics, dates, booleans (v0.3.0)
│   │   ├── hot.rs                     # Tiered hot/cold dictionary tables (v0.10.0)
│   │   └── prefix.rs                  # IRI prefix compression
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── vp_table.rs                # VP table DDL management
│   │   ├── delta.rs                   # Delta partition operations
│   │   ├── merge.rs                   # Delta→Main generation merge logic
│   │   ├── subject_patterns.rs        # Subject→predicate-set index (v0.5.0)
│   │   └── bulk_load.rs               # Streaming parsers + COPY
│   ├── sparql/
│   │   ├── mod.rs
│   │   ├── parser.rs                  # spargebra + sparopt integration
│   │   ├── algebra.rs                 # IR and pg_triple-specific optimizations; reads SHACL catalog before join-tree construction
│   │   ├── sql_gen.rs                 # Algebra → SQL text
│   │   ├── property_path.rs           # Recursive CTE generation
│   │   ├── projector.rs               # Maps decoded i64 rows → named SPARQL variables; applies SELECT expressions, BIND, computed values
│   │   ├── executor.rs               # SPI execution + decoding
│   │   ├── update.rs                  # SPARQL 1.1 Update parsing + execution (v0.12.0)
│   │   └── federation.rs              # SERVICE keyword: remote endpoint execution + result injection (v0.16.0)
│   ├── datalog/
│   │   ├── mod.rs                     # Public API (#[pg_extern] functions)
│   │   ├── parser.rs                  # Rule text → Rule IR
│   │   ├── stratify.rs                # Dependency graph, stratification, cycle detection
│   │   ├── compiler.rs                # Rule IR → SQL (per stratum)
│   │   ├── builtins.rs                # Built-in rule sets (RDFS, OWL RL)
│   │   └── catalog.rs                 # _pg_triple.rules table CRUD
│   ├── graph/
│   │   ├── mod.rs
│   │   └── named_graph.rs             # Named graph CRUD
│   ├── shacl/
│   │   ├── mod.rs
│   │   ├── parser.rs                  # SHACL Turtle → shape IR
│   │   ├── compiler.rs                # Shape IR → DDL/triggers
│   │   ├── validator.rs               # Async validation worker
│   │   └── optimizer.rs               # SHACL hints for query planner
│   ├── export/
│   │   ├── mod.rs
│   │   └── serializer.rs              # Turtle/N-Triples/JSON-LD output
│   ├── stats/
│   │   ├── mod.rs
│   │   └── monitoring.rs              # Statistics collection
│   ├── ecosystem/
│   │   ├── mod.rs
│   │   └── trickle.rs                 # pg_trickle integration (optional)
│   └── admin/
│       ├── mod.rs
│       └── maintenance.rs             # Vacuum, reindex, compact, config
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
├── pg_triple_http/                    # Companion HTTP binary (Cargo workspace member, v0.15.0)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                    # axum HTTP server, connection pool, content negotiation
├── ROADMAP.md
├── README.md
└── LICENSE
```

> **Note**: At v0.15.0, the project becomes a Cargo workspace with two members: the extension crate (`pg_triple/`) and the HTTP binary (`pg_triple_http/`). Planning for this workspace structure from the start avoids a disruptive restructuring later.

---

## 10. Build & Development Setup

```bash
# Prerequisites
rustup update stable        # Rust 1.88+ required for pgrx 0.17
cargo install cargo-pgrx --version 0.17.0 --locked
cargo pgrx init --pg18 download  # Download and compile PG18

# Create extension
cargo pgrx new pg_triple --pg18
cd pg_triple

# Development cycle
cargo pgrx run pg18          # Run in psql
cargo pgrx test pg18         # Run #[pg_test] tests
cargo pgrx regress pg18      # Run pg_regress tests
cargo pgrx package --pg18    # Build installable package

# Benchmarking
cargo pgrx bench pg18        # Run in-process pgbench
```

### Cargo.toml Dependencies

```toml
[package]
name = "pg_triple"
version = "0.1.0"
edition = "2024"
resolver = "3"

[lib]
crate-type = ["cdylib", "lib"]

[features]
default = ["pg18"]
pg18 = ["pgrx/pg18"]

[dependencies]
pgrx = "0.17"
spargebra = "0.3"           # SPARQL 1.1 algebra parser
sparopt = "0.1"             # SPARQL algebra optimizer (filter pushdown, constant folding; first pass before pg_triple optimizer)
rio_turtle = "0.9"          # Turtle/N-Triples parser
rio_api = "0.9"             # RDF API traits
rio_xml = "0.9"             # RDF/XML parser
xxhash-rust = { version = "0.8", features = ["xxh3"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
lru = "0.12"                # LRU cache
thiserror = "2"             # Error types

[dev-dependencies]
pgrx-tests = "0.17"
proptest = "1"
```

---

## 11. Security Considerations

- **SQL Injection**: All SQL generated by the SPARQL→SQL translator uses parameterized queries via SPI's `$N` parameter binding; no string interpolation of user data into SQL
- **Input validation**: RDF parsers (`rio_*`) are well-tested and handle malformed input gracefully; all external input is validated before dictionary encoding
- **Privilege model**: Extension functions default to `SECURITY INVOKER`; schema `_pg_triple` is only accessible by the extension owner
- **Resource limits**: `pg_triple.max_path_depth` prevents unbounded recursive CTEs; `statement_timeout` respected for all SPI calls
- **Memory safety**: Rust's ownership system prevents buffer overflows; pgrx handles Postgres memory context integration

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
- **OTTR template expansion**: `pg_triple.expand_template(iri TEXT, query TEXT)` for OTTR-style DataFrame→RDF bulk load (see [prior_art_commercial.md](ecosystem/prior_art_commercial.md))
- **Ontology change propagation DAG**: When pg_trickle is present, model derived structures (ExtVP, inference, SHACL, stats) as a DAG of stream tables with automatic topological refresh on ontology changes

---

## 13. Operational Considerations

### 13.1 Merge Worker Health

- The merge worker registers a heartbeat timestamp in shared memory, updated on each cycle
- If the heartbeat stalls for longer than `pg_triple.merge_watchdog_timeout` (default: 5 minutes), `_PG_init` on the next backend connection logs a `WARNING` and attempts to restart the worker
- `pg_triple.stats()` includes `merge_worker_status` (`running` / `stalled` / `disabled`) and `merge_worker_last_heartbeat`

### 13.2 Shared-Memory Cache Lifecycle

- The dictionary LRU cache resides in `PgSharedMem` and survives individual backend crashes
- The cache is cleared on postmaster restart (standard PostgreSQL shared-memory lifecycle)
- Slot versioning (§4.1) detects layout mismatches after an in-place extension upgrade and re-initializes gracefully

### 13.3 `pg_upgrade` Behaviour

- Extension tables (`_pg_triple.*`) migrate with standard `pg_upgrade` — no special handling required
- Shared-memory state (dictionary cache, bloom filters) is rebuilt from on-disk tables at the first `_PG_init` after the upgrade
- The slot versioning mechanism (§4.1) ensures safe re-initialization if the shared-memory layout changed between versions

### 13.4 Extension Downgrades

- Downgrades are **not supported** (standard for PostgreSQL extensions)
- Users should test upgrades on a staging instance and rely on `pg_dump`/`pg_restore` for rollback

### 13.5 Dictionary Vacuum Concurrency

- `pg_triple.vacuum_dictionary()` acquires an `ADVISORY LOCK` to prevent concurrent runs
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
