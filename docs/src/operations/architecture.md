# Architecture Overview

pg_ripple is a PostgreSQL 18 extension that implements a high-performance RDF triple store with native SPARQL query execution. This page describes the internal architecture: how data is stored, how queries are executed, and how the subsystems interact.

---

## System Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Client Applications                           │
│   psql / JDBC / SPARQL Protocol (pg_ripple_http) / REST / ODBC      │
└────────────────────────────┬─────────────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────────────┐
│                     PostgreSQL 18 Backend                             │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │                    pg_ripple Extension                          │  │
│  │                                                                │  │
│  │  ┌──────────────┐  ┌───────────────┐  ┌────────────────────┐  │  │
│  │  │  SPARQL       │  │  Datalog       │  │  SHACL             │  │  │
│  │  │  Engine       │  │  Reasoner      │  │  Validator         │  │  │
│  │  │              │  │               │  │                    │  │  │
│  │  │  parse →     │  │  stratify →   │  │  shapes → DDL     │  │  │
│  │  │  optimize → │  │  compile →   │  │  constraints +    │  │  │
│  │  │  SQL gen →  │  │  semi-naive   │  │  async pipeline   │  │  │
│  │  │  SPI exec → │  │  fixpoint     │  │                    │  │  │
│  │  │  decode     │  │               │  │                    │  │  │
│  │  └──────┬───────┘  └───────┬───────┘  └────────┬───────────┘  │  │
│  │         │                  │                    │              │  │
│  │         ▼                  ▼                    ▼              │  │
│  │  ┌─────────────────────────────────────────────────────────┐  │  │
│  │  │              Dictionary Encoder (XXH3-128)               │  │  │
│  │  │    IRI / Blank Node / Literal  ──→  i64 identifier       │  │  │
│  │  │         Shared-Memory LRU Cache (64 shards)              │  │  │
│  │  └────────────────────────┬────────────────────────────────┘  │  │
│  │                           │                                    │  │
│  │                           ▼                                    │  │
│  │  ┌─────────────────────────────────────────────────────────┐  │  │
│  │  │                VP Storage Engine (HTAP)                  │  │  │
│  │  │                                                         │  │  │
│  │  │   vp_{id}_delta  ──┐                                    │  │  │
│  │  │   (write inbox)    │                                    │  │  │
│  │  │                    ├──→  vp_{id} (read view)            │  │  │
│  │  │   vp_{id}_main   ──┤    = (main − tombstones)          │  │  │
│  │  │   (BRIN archive)   │      UNION ALL delta               │  │  │
│  │  │                    │                                    │  │  │
│  │  │   vp_{id}_tombstones                                    │  │  │
│  │  │   (pending deletes) │                                    │  │  │
│  │  │                    │                                    │  │  │
│  │  │   vp_rare ─────────┘  (consolidated rare predicates)    │  │  │
│  │  └─────────────────────────────────────────────────────────┘  │  │
│  │                           │                                    │  │
│  │  ┌────────────────────────┴────────────────────────────────┐  │  │
│  │  │           Background Merge Worker (BGW)                  │  │  │
│  │  │   delta + main − tombstones ──→ new main (BRIN)          │  │  │
│  │  │   Polling interval: merge_interval_secs (default 60s)   │  │  │
│  │  │   Threshold: merge_threshold (default 10,000 rows)       │  │  │
│  │  └─────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  _pg_ripple schema: dictionary, predicates, vp_*, statements    │ │
│  │  pg_ripple schema:  public SQL functions (sparql, insert, etc.) │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Dictionary Encoder

The dictionary encoder is the foundation of pg_ripple's storage model. Every RDF term — IRI, blank node, plain literal, typed literal, or language-tagged literal — is mapped to a compact `i64` identifier before being stored.

### How Encoding Works

1. The input term is classified by kind: IRI (0), blank node (1), literal (2), typed literal (3), or language-tagged literal (4).
2. The kind discriminant is mixed into the hash input as two little-endian bytes, so the same string encoded as an IRI and as a blank node always produces distinct dictionary rows.
3. An XXH3-128 hash is computed over `(kind_le_bytes || term_utf8)`.
4. The full 16-byte hash is stored in the `_pg_ripple.dictionary` table with an `ON CONFLICT (hash) DO NOTHING` upsert. The dense `i64` join key is an `IDENTITY`-generated column — sequential and independent of the hash.
5. The resulting `i64` is used in all VP table columns.

```admonish info title="Why integer encoding?"
VP tables never contain raw strings. All joins, comparisons, and index lookups operate on `i64` values. This eliminates collation overhead, reduces storage by 5–20x, and makes B-tree index scans uniformly fast regardless of IRI length.
```

### Shared-Memory Cache

The dictionary cache sits in PostgreSQL shared memory (allocated at postmaster start) and is organized as a 64-shard set-associative structure. Each backend reads and writes to the shared cache through atomic operations — no per-backend duplication.

Key parameters:
- **`pg_ripple.dictionary_cache_size`** — Number of cache entries (default: 65,536). Requires restart.
- **`pg_ripple.cache_budget`** — Memory budget cap in MB (default: 64). Bulk loads throttle at 90% utilization.

The cache hit ratio is reported by `pg_ripple.stats()` and should stay above 95% in production.

---

## VP (Vertical Partitioning) Tables

pg_ripple uses vertical partitioning: one physical table per unique predicate. This is the storage model used by research systems like SW-Store and column-oriented triple stores.

### Table Layout

Each predicate with at least `vp_promotion_threshold` (default: 1,000) triples gets a dedicated VP table:

```sql
-- Columns in every VP table
s      BIGINT  NOT NULL   -- subject dictionary ID
o      BIGINT  NOT NULL   -- object dictionary ID
g      BIGINT  NOT NULL DEFAULT 0  -- graph ID (0 = default graph)
i      BIGINT  NOT NULL DEFAULT nextval('statement_id_seq')  -- unique SID
source SMALLINT NOT NULL DEFAULT 0  -- 0 = explicit, 1 = inferred
```

Dual B-tree indexes on `(s, o)` and `(o, s)` support both subject-to-object and object-to-subject access patterns.

### Rare Predicate Consolidation

Predicates with fewer triples than the promotion threshold are stored in a shared `_pg_ripple.vp_rare` table with an extra `p BIGINT` column. This avoids schema bloat for infrequent predicates. When a rare predicate's count crosses the threshold, it is automatically promoted to a dedicated VP table.

### Predicate Catalog

The `_pg_ripple.predicates` table maps each predicate ID to its VP table OID and current triple count:

```sql
SELECT id, table_oid, triple_count
FROM _pg_ripple.predicates
ORDER BY triple_count DESC;
```

```admonish warning title="No dynamic SQL string concatenation"
The SPARQL-to-SQL translator never concatenates table names into SQL strings. It looks up the OID in `_pg_ripple.predicates` and uses parameterized queries with format-safe quoting. This prevents SQL injection by design.
```

---

## HTAP Storage Architecture

Since v0.6.0, pg_ripple uses an HTAP (Hybrid Transactional/Analytical Processing) storage architecture that separates write and read paths for each VP table.

### Three-Table Split

For each predicate, the storage layer maintains:

| Table | Purpose | Index Type |
|---|---|---|
| `vp_{id}_delta` | Write inbox — all INSERTs land here | B-tree on `(s, o)` |
| `vp_{id}_main` | Read-optimized archive | BRIN (block range) |
| `vp_{id}_tombstones` | Pending deletes from main | B-tree on `(s, o, g)` |

A read view `vp_{id}` combines them:

```sql
(main EXCEPT tombstones) UNION ALL delta
```

### Background Merge Worker

The merge worker is a PostgreSQL background worker (`BGWorker`) that runs in a polling loop:

1. **Poll** — Wake every `merge_interval_secs` (default: 60) or when poked by the write-path latch.
2. **Scan** — Check each HTAP predicate's delta row count against `merge_threshold` (default: 10,000).
3. **Merge** — For qualifying predicates: create a new main table from `(old_main − tombstones) UNION ALL delta`, swap atomically via `ALTER TABLE ... RENAME`, drop the old main after `merge_retention_seconds`.
4. **Maintain** — Rebuild subject/object pattern tables, promote rare predicates that crossed the threshold, run `ANALYZE` on new main tables (when `auto_analyze` is on), evict expired federation cache entries.

```admonish tip title="Write path"
Writers never block on the merge. All INSERTs go directly to the delta table (heap + B-tree). The merge worker operates asynchronously and uses PostgreSQL's MVCC for isolation.
```

---

## SPARQL Query Execution Flow

When a client calls `pg_ripple.sparql('SELECT ...')`, the query goes through five stages:

### 1. Parse

The SPARQL text is parsed by the `spargebra` crate into an algebraic representation. This handles the full SPARQL 1.1 grammar: SELECT, CONSTRUCT, DESCRIBE, ASK, property paths, subqueries, aggregation, federation (SERVICE), and SPARQL-star.

### 2. Optimize

The `sparopt` optimizer rewrites the algebra tree:
- **BGP reordering** — Triple patterns are sorted by estimated selectivity (smallest VP table first) when `bgp_reorder` is on.
- **Filter pushdown** — FILTER constants are encoded to `i64` at translation time and pushed into the WHERE clause of the generated SQL.
- **Self-join elimination** — Star patterns (same subject, multiple predicates) are collapsed into multi-way joins instead of redundant subqueries.
- **SHACL hints** — If `sh:maxCount 1` is declared, `DISTINCT` is omitted; if `sh:minCount 1`, `LEFT JOIN` is upgraded to `INNER JOIN`.

### 3. Generate SQL

The optimized algebra is compiled into PostgreSQL SQL:
- Each triple pattern becomes a scan of the corresponding VP table (or `vp_rare` with a predicate filter).
- Joins between patterns become SQL `JOIN` clauses with `i64` equality predicates.
- Property paths compile to `WITH RECURSIVE ... CYCLE` using PostgreSQL 18's hash-based cycle detection.
- `SERVICE` clauses are compiled into HTTP calls to remote SPARQL endpoints.
- Aggregates, `ORDER BY`, `LIMIT`, and `OFFSET` translate directly to their SQL equivalents.

### 4. SPI Execute

The generated SQL is executed through PostgreSQL's Server Programming Interface (SPI). Results are arrays of `i64` dictionary IDs.

The plan cache (`plan_cache_size`, default: 256) stores compiled SQL for recently-seen SPARQL queries to avoid repeated parse/optimize/generate cycles.

### 5. Decode

The `i64` result columns are decoded back to human-readable RDF terms (IRIs, literals, blank nodes) using the dictionary. The shared-memory cache accelerates this step — a cache hit avoids a dictionary table lookup per value.

```admonish note title="Integer joins everywhere"
The SPARQL engine encodes all bound constants to i64 *before* generating SQL, and decodes results *after* execution. VP table queries never contain string comparisons — this is a hard architectural invariant.
```

---

## Schema Organization

pg_ripple uses two PostgreSQL schemas:

| Schema | Contents | Visibility |
|---|---|---|
| `pg_ripple` | Public SQL functions (`sparql()`, `insert_triple()`, `stats()`, etc.) | User-facing |
| `_pg_ripple` | Dictionary table, predicates catalog, VP tables, statement mappings, internal state | Internal |

```admonish warning title="Do not modify _pg_ripple directly"
The internal schema is managed by the extension. Direct modifications to `_pg_ripple` tables can corrupt the dictionary or break VP table invariants.
```

---

## Subsystem Summary

| Subsystem | Source Directory | Purpose |
|---|---|---|
| Dictionary | `src/dictionary/` | Term ↔ i64 encoding with XXH3-128 |
| Storage | `src/storage/` | VP tables, HTAP partitions, rare predicate consolidation |
| SPARQL | `src/sparql/` | Parse → optimize → SQL generation → SPI → decode |
| Datalog | `src/datalog/` | Rule parsing, stratification, semi-naive fixpoint, magic sets |
| SHACL | `src/shacl/` | Shape validation, DDL constraints, async pipeline |
| Export | `src/export/` | Turtle, N-Triples, JSON-LD serialization |
| Worker | `src/worker.rs` | Background merge worker, embedding queue, SHACL async |
| Stats | `src/stats/` | Monitoring, cache metrics, health checks |
| Federation | `src/sparql/federation` | Remote SERVICE call execution, connection pooling, caching |
| HTTP | `pg_ripple_http/` | SPARQL Protocol endpoint (standalone companion service) |
