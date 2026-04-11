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
| PG binding | pgrx 0.17 (`pg18` feature flag) |
| PostgreSQL | 18.x |
| SPARQL parser | `spargebra` crate (W3C-compliant SPARQL 1.1 algebra) |
| RDF parser | `rio_turtle`, `rio_xml` crates (Turtle, N-Triples, RDF/XML) |
| Hashing | `xxhash-rust` (XXH3-128 for dictionary collision resistance) |
| Serialization | `serde` + `serde_json` (for SHACL reports, config) |
| Testing | pgrx `#[pg_test]`, `cargo pgrx regress`, pgbench via `pgrx-bench` |

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
└─────────────────────────────────────────────────────────┘
```

---

## 4. Module Breakdown

### 4.1 Extension Bootstrap (`src/lib.rs`)

- pgrx `#[pg_extern]` entry points
- `_PG_init()` hook for shared memory registration and background worker startup
- GUC parameters: `pg_triple.default_graph`, `pg_triple.dictionary_cache_size`, `pg_triple.merge_threshold`, `pg_triple.enable_shacl`
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
- **Batch encoding** (`encode_batch()`): Bulk insert with `ON CONFLICT DO NOTHING` + `RETURNING`, minimising round-trips during data load
- **In-memory cache**: `HashMap<u128, i64>` in shared memory via pgrx `PgSharedMem`, sized by GUC
- **Prefix compression**: Common IRI prefixes (registered via `pg_triple.register_prefix()`) are stripped before hashing and stored separately, reducing storage by ~40% for typical RDF datasets

### 4.3 Storage Engine (`src/storage/`)

**Purpose**: Physically store triples as integer tuples in vertically partitioned tables.

#### 4.3.1 Vertical Partitioning

Each unique predicate `p` gets its own table:

```sql
CREATE TABLE _pg_triple.vp_{predicate_id} (
    s   BIGINT NOT NULL,  -- subject dictionary ID
    o   BIGINT NOT NULL,  -- object dictionary ID
    g   BIGINT NOT NULL DEFAULT 0  -- named graph ID (0 = default)
);
CREATE INDEX ON _pg_triple.vp_{predicate_id} (s, o);
CREATE INDEX ON _pg_triple.vp_{predicate_id} (o, s);
```

- Tables are created dynamically on first encounter of a new predicate during data ingestion
- A catalog table `_pg_triple.predicates` maps predicate dictionary IDs to table OIDs for fast lookup
- PG18's **skip scan** on the composite B-tree indices enables efficient lookups even when only the second column (`o`) is bound

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
- Uses SPI to execute the merge as a single transaction
- Updates BRIN summaries post-merge
- Signals completion via shared-memory latch

**Query Path**:
- `UNION ALL` of main + delta, with bloom filter for fast existence checks
- For queries touching only historical data, the delta scan is skipped

#### 4.3.3 Bulk Loading

- `pg_triple.load_turtle(path TEXT)` / `pg_triple.load_ntriples(path TEXT)`
- Parses via `rio_turtle` / `rio_api` crates in streaming fashion
- Batches of 10,000 triples: dictionary-encode → `COPY` into delta VP tables
- Disables index updates during load; rebuilds at end
- Uses PG18 `COPY ... REJECT_LIMIT` for fault tolerance

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
Algebra Optimizer (Rust)
    - Self-join elimination
    - Optional-to-inner downgrade (with SHACL hints)
    - Filter pushdown (pre-decode)
    - UNION folding → WHERE IN
    │
    ▼
SQL Generator
    - Map BGPs to VP table joins (integer columns)
    - Property paths → WITH RECURSIVE + CYCLE detection
    - OPTIONAL → LEFT JOIN
    - LIMIT/OFFSET pushdown
    - DISTINCT projection pushing
    │
    ▼
SPI::connect() → execute SQL → result set of i64 tuples
    │
    ▼
Dictionary Decoder → human-readable result set
    │
    ▼
Return as SETOF RECORD / JSON / TABLE
```

#### 4.4.2 SQL Functions

```sql
-- Primary query interface
pg_triple.sparql(query TEXT) RETURNS SETOF JSONB
pg_triple.sparql_explain(query TEXT) RETURNS TEXT

-- Data manipulation
pg_triple.insert_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL)
pg_triple.delete_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL)
pg_triple.load_turtle(data TEXT) RETURNS BIGINT  -- returns count
pg_triple.load_ntriples(data TEXT) RETURNS BIGINT
```

#### 4.4.3 Join Optimization Strategies

1. **Self-join elimination**: Star patterns on the same subject collapse into a single scan of the subject across multiple VP tables, joined by subject ID equality
2. **Optional-self-join elimination**: When SHACL declares `sh:minCount 1`, OPTIONAL → INNER JOIN
3. **Self-union elimination**: Multiple triple patterns binding the same variable to different predicates are rewritten to `WHERE predicate_id IN (...)`
4. **Projection pushing**: `SELECT DISTINCT ?p` queries enumerate the `_pg_triple.predicates` catalog instead of scanning all VP tables
5. **Filter pushdown**: SPARQL `FILTER` clauses operating on bound IRIs are resolved to integer IDs *before* generating SQL, ensuring B-tree index usage

#### 4.4.4 Property Path Compilation

SPARQL property paths (`+`, `*`, `?`) compile to `WITH RECURSIVE` CTEs:

```sql
WITH RECURSIVE path(s, o, depth, visited) AS (
    -- Anchor: direct one-hop
    SELECT s, o, 1, ARRAY[s]
    FROM _pg_triple.vp_{predicate_id}
    WHERE s = $1
  UNION ALL
    -- Recursive: extend by one hop
    SELECT p.s, vp.o, p.depth + 1, p.visited || vp.s
    FROM path p
    JOIN _pg_triple.vp_{predicate_id} vp ON p.o = vp.s
    WHERE NOT vp.s = ANY(p.visited)  -- cycle detection
      AND p.depth < pg_triple.max_path_depth
)
SELECT DISTINCT s, o FROM path;
```

- Configurable `pg_triple.max_path_depth` GUC (default: 100)
- Array-based visited tracking for cycle prevention
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

### 4.9 Administrative Functions (`src/admin/`)

- `pg_triple.vacuum()` — force delta→main merge
- `pg_triple.reindex()` — rebuild VP table indices
- `pg_triple.dictionary_stats()` — cache hit ratio, dictionary sizes
- `pg_triple.register_prefix(prefix TEXT, expansion TEXT)` — IRI prefix registration
- `pg_triple.prefixes() RETURNS TABLE(prefix TEXT, expansion TEXT)`

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
6. Decode name_tbl.o → dictionary lookup → human-readable strings
7. Return as SETOF JSONB: [{"name": "Alice"}, ...]
```

---

## 7. Performance Targets

| Metric | Target | Approach |
|---|---|---|
| Insert throughput | >100K triples/sec (bulk load) | Batch COPY, deferred indexing |
| Insert throughput | >10K triples/sec (transactional) | Delta partition, async validation |
| Simple BGP query | <5ms (10M triples) | Integer joins, B-tree on VP tables |
| Star query (5 patterns) | <20ms (10M triples) | Self-join elimination, co-located VP scans |
| Property path (depth 10) | <100ms (10M triples) | Recursive CTE + cycle detection |
| Dictionary encode | <1μs (cache hit) | LRU in shared memory |
| Dictionary encode | <50μs (cache miss) | B-tree index on hash |

---

## 8. Testing Strategy

### 8.1 Unit Tests

- pgrx `#[pg_test]` for every SQL-exposed function
- Rust unit tests for pure logic (dictionary hashing, SPARQL algebra transforms, SQL generation)
- Property-based tests (`proptest`) for dictionary encode/decode round-trips

### 8.2 Integration Tests

- `cargo pgrx regress` with pg_regress test suites:
  - `sql/triple_crud.sql` — insert, delete, query basics
  - `sql/sparql_queries.sql` — comprehensive SPARQL coverage
  - `sql/bulk_load.sql` — Turtle/N-Triples ingestion
  - `sql/shacl_validation.sql` — constraint enforcement
  - `sql/named_graphs.sql` — GRAPH patterns
  - `sql/property_paths.sql` — recursive traversal

### 8.3 Benchmarks

- pgrx-bench integration for in-process pgbench
- Berlin SPARQL Benchmark (BSBM) adapted to SQL function calls
- SP2Bench for academic comparison points
- Custom benchmarks:
  - Bulk load: 1M, 10M, 100M triples
  - Point queries vs star queries vs path queries
  - Concurrent read/write under HTAP workload

### 8.4 Conformance

- W3C SPARQL 1.1 Query test suite (subset applicable to our supported features)
- SHACL Core test suite

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
│   │   └── prefix.rs                  # IRI prefix compression
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── vp_table.rs                # VP table DDL management
│   │   ├── delta.rs                   # Delta partition operations
│   │   ├── merge.rs                   # Delta→Main merge logic
│   │   └── bulk_load.rs               # Streaming parsers + COPY
│   ├── sparql/
│   │   ├── mod.rs
│   │   ├── parser.rs                  # spargebra integration
│   │   ├── algebra.rs                 # IR and optimizations
│   │   ├── sql_gen.rs                 # Algebra → SQL text
│   │   ├── property_path.rs           # Recursive CTE generation
│   │   └── executor.rs               # SPI execution + decoding
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
│   └── admin/
│       ├── mod.rs
│       └── maintenance.rs             # Vacuum, reindex, config
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
- **Extended VP (ExtVP)**: Pre-computed semi-join materialized views for high-frequency query patterns
- **Temporal versioning**: Bitstring validity columns for versioned graph snapshots
- **TimescaleDB integration**: Hypertable-backed temporal graph management
- **Apache AGE interop**: Bidirectional projection between RDF and LPG models
- **SPARQL Update (SPARQL 1.1 Update)**: Full INSERT DATA / DELETE DATA / DELETE WHERE support
- **SPARQL Federation**: SERVICE keyword for federated queries across remote SPARQL endpoints
- **GraphQL-to-SPARQL bridge**: Auto-generate GraphQL schema from SHACL shapes
