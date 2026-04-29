# Storage Tiering with SlateDB and DuckDB
## Can pg_ripple offload its cold tier to SlateDB or DuckDB?

> **Status:** Research report — speculative, not a roadmap commitment.
> **Relates to:** [future-directions.md § D.3 Storage tiering — hot, warm, cold](future-directions.md#d3-storage-tiering--hot-warm-cold)
> **Date:** 2026-04-29

---

## 1. Why this question matters

Wikidata is approximately 16 billion triples today and growing. DBpedia, Wikibase
instances, biomedical knowledge graphs (ChEMBL, UniProt, DrugBank), and
enterprise LOD datasets routinely exceed 1–10 billion triples. pg_ripple's
current storage model — VP tables split into `_delta` (B-tree heap) and `_main`
(BRIN-indexed heap) — is excellent for hot and warm workloads but keeps everything
on local SSD. At Wikidata scale this means 2–4 TB of local SSD just for the VP
tables; 100B-triple graphs would need 20–40 TB. Neither is economically realistic
for most deployments.

The goal of a cold tier is to keep **frequently-queried data on fast local
storage** and **rarely-accessed data on cheap, durable object storage**,
with a query path that reads across tiers transparently.

Two concrete candidates have emerged from the Rust and PostgreSQL ecosystems:
**SlateDB** (an embedded LSM-tree KV engine backed by object storage) and
**DuckDB** (an embedded analytical engine that reads Parquet natively from S3/GCS).
This report evaluates both, analyses integration paths with pg_ripple's existing
architecture, scores each on strategic fit, and recommends a forward path.

---

## 2. Current pg_ripple storage architecture

A brief recap of the layers in play before adding a cold tier:

| Layer | Tables | Storage | Index | Access pattern |
|---|---|---|---|---|
| **Hot** (delta) | `_pg_ripple.vp_{id}_delta` | PostgreSQL heap | B-tree `(s,o)`, `(o,s)` | Writes land here; random reads |
| **Warm** (main) | `_pg_ripple.vp_{id}_main` | PostgreSQL heap | BRIN block-range | Post-merge; sequential scans |
| **Rare** | `_pg_ripple.vp_rare` | PostgreSQL heap | GIN + B-tree | Low-cardinality predicates |
| **Tombstones** | `_pg_ripple.vp_{id}_tombstones` | PostgreSQL heap | B-tree `(s,o)` | Deletes of main-resident triples |

The background merge worker periodically consolidates delta into main, removing
tombstoned rows in the process. The query path for any VP table is:

```sql
(SELECT s, o, g, i FROM vp_{id}_main
 EXCEPT
 SELECT s, o, g, i FROM vp_{id}_tombstones)
UNION ALL
SELECT s, o, g, i FROM vp_{id}_delta
```

A cold tier sits beneath `vp_{id}_main`. Its rows would be the oldest, least-recently-
touched triples — still correct and queryable, but not worth keeping on fast disk.
The query path then becomes:

```sql
(cold_scan(predicate_id, filter) EXCEPT tombstones)
UNION ALL vp_{id}_main
UNION ALL vp_{id}_delta
```

The rest of this report analyses how to implement `cold_scan`.

---

## 3. Candidate A — SlateDB

### 3.1 What SlateDB is

[SlateDB](https://slatedb.io) is a Rust-native, embedded LSM-tree storage engine that
writes **all** data directly to object storage (S3, GCS, Azure Blob, MinIO, Tigris).
Version 0.12.x (latest as of 2026-04-29) is at production quality, ships as a single
crate, and is in use by several commercial systems (TensorLake, Goldsky, Responsive,
Gadget). It is a member of the [Commonhaus Foundation](https://www.commonhaus.org/).

Key properties:

- **Single-writer, multiple-readers**: exactly one writer at a time; any number of
  read-only clients can read concurrently using snapshot isolation.
- **Async Rust API**: `put`, `get`, `delete`, `scan`, `merge_put` all return `Future`s.
  The backing I/O runtime is `tokio`.
- **Compaction is pluggable**: the built-in leveled compaction can be replaced with a
  custom strategy, or run as a separate process.
- **Durability model**: writes are durable after the MemTable flush to object storage
  (50–100 ms default). Clients needing lower latency can use `await_durable = false`.
- **Caching**: in-memory block cache + optional local SST disk cache to amortise
  GET costs on hot cold-tier rows.
- **Features shipped**: range scans, bloom filters, compression, CDC, checkpoints,
  snapshots, TTL, merge operator, writer fencing, clones.

### 3.2 Encoding triples in SlateDB

SlateDB is a key-value store. A triple `(s, o, g, i, source)` with predicate `p`
must be encoded into `(key: Bytes, value: Bytes)`. Two encodings are worth considering:

**Encoding E1 — One row per triple (naïve)**

```
key   = big-endian(p: i64) || big-endian(s: i64) || big-endian(o: i64) || big-endian(g: i64)
value = big-endian(i: i64) || source: u8
```

Properties:
- Simple; exact lookups and `s`-bound range scans work naturally.
- Object-bound queries (`WHERE o = ?`) cannot exploit sorted key order; full prefix
  scan required.
- BloomFilter per SST block means point-lookups are efficient.

**Encoding E2 — Subject-aggregated with merge operator (recommended)**

```
key   = big-endian(p: i64) || big-endian(s: i64)
value = set of (o: i64, g: i64, i: i64, source: u8) entries, appended with merge_put
```

SlateDB's merge operator is invoked at compaction time to merge accumulated
appends into a compact value. The result is functionally a columnar layout over
objects-per-subject, which matches the dominant access pattern (star queries).

- `WHERE s = ?` for predicate `p` → single KV lookup.
- `SCAN p_prefix` for all subjects of a predicate → sorted prefix scan.
- `WHERE o = ?` → full predicate scan, bloom filter cannot help (same as E1).

For object-bound queries the cold tier inherits the same weakness as the warm tier
today; these are typically infrequent analytical lookups where the latency tradeoff
is acceptable.

### 3.3 Integration architecture

The fundamental constraint is that **SlateDB requires a tokio async runtime**,
while **PostgreSQL backends are synchronous processes**. These two models cannot
share a single thread. There are two viable integration points:

**Integration point 1: Background merge worker (recommended)**

The existing `merge` background worker (`src/storage/merge.rs`) is already the
single writer into `vp_{id}_main`. Extending it to also flush cold rows from main
into SlateDB is architecturally clean:

1. Merge worker runs in its own OS thread (pgrx `BackgroundWorker`).
2. The cold-tier flush is spawned into a per-worker tokio runtime (`Runtime::new()`).
3. The tokio runtime is hidden behind a synchronous wrapper:

```rust
// Inside merge background worker — safe because we're not in a PG backend
fn flush_to_cold_tier(rows: Vec<ColdTriple>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio rt");
    rt.block_on(async {
        let db = slate_db_handle(); // shared across merge iterations
        for row in rows {
            db.put(&encode_key(&row), &encode_value(&row)).await?;
        }
        db.flush().await
    }).expect("slatedb flush");
}
```

This is safe: the background worker is not a PostgreSQL-transaction-bearing
backend; it does not use `SpiClient` during the cold flush. tokio is brought up
and torn down around each flush batch.

4. A new GUC `pg_ripple.cold_tier = 'slatedb'` (default: `'none'`) gates the feature.
5. A new GUC `pg_ripple.cold_tier_object_store` takes an object store URI
   (`s3://bucket/prefix`, `gs://bucket/prefix`, `file:///local/path` for dev).

**Integration point 2: Read path (custom scan provider)**

pgrx supports custom scan providers via `#[pg_extern]` aggregate functions and
SRF (set-returning functions). A cold-tier read would be exposed as:

```sql
-- Internal SRF, called by the query planner's VP view union
SELECT s, o, g, i, source
FROM _pg_ripple.vp_cold_scan($1::bigint, $2::bigint, $3::bigint)
-- $1 = predicate_id, $2 = s filter (-1 = unbound), $3 = o filter (-1 = unbound)
```

The SRF would:
1. Open a read-only snapshot of the SlateDB instance (multiple-reader safe).
2. Scan the relevant key range using `db.scan()`.
3. Decode and yield rows, filtered by any bound `s` or `o` value.
4. Use `tokio::runtime::Handle::block_on` — safe in a SRF because the
   SRF runs to completion before returning to the executor.

The VP view for cold-enabled predicates becomes:

```sql
CREATE OR REPLACE VIEW _pg_ripple.vp_{id} AS
SELECT s, o, g, i, source FROM (
    SELECT s, o, g, i, source
    FROM _pg_ripple.vp_cold_scan({id}, -1, -1)
    EXCEPT ALL
    SELECT s, o, g, i FROM _pg_ripple.vp_{id}_tombstones
) cold
UNION ALL
SELECT s, o, g, i, source FROM _pg_ripple.vp_{id}_main
UNION ALL
SELECT s, o, g, i, source FROM _pg_ripple.vp_{id}_delta;
```

### 3.4 Data lifecycle management

The full lifecycle of a triple in a cold-enabled pg_ripple:

```
WRITE → vp_{id}_delta (hot)
      → [merge worker, normal cycle] → vp_{id}_main (warm)
      → [cold eviction: age > pg_ripple.cold_eviction_age] → SlateDB on S3 (cold)
      → [delete from vp_{id}_main]

READ → check delta → check main → cold_scan (lazy, on miss)
```

Cold eviction criteria (configurable via GUC):
- `pg_ripple.cold_eviction_age = '30 days'` — triples older than N days by `i` (SID) age.
- `pg_ripple.cold_eviction_named_graph = 'cold:*'` — any named graph whose IRI matches a prefix.
- `pg_ripple.cold_predicate_threshold = 100` — predicates with fewer than N triples
  in main never get promoted to dedicated VP tables and go to cold tier directly.

The `vp_rare` table is the natural cold-tier candidate at lower cardinalities. All
three eviction policies can coexist.

### 3.5 SlateDB: strengths and weaknesses for this use case

| | Assessment |
|---|---|
| **Rust-native** | Direct Cargo dependency. No FFI. No C headers. Minimal `unsafe`. |
| **object_store crate** | Same crate already used in the Rust ecosystem; S3/GCS/Azure/MinIO all work. |
| **Single-writer model** | Perfectly aligned with pg_ripple's single merge worker. |
| **Multiple-reader model** | Snapshot isolation — all PostgreSQL backends can read concurrently. |
| **Async-only API** | Requires tokio runtime. Manageable via `block_on` in background worker; awkward in SRF. |
| **50–100 ms write latency** | Acceptable for cold-tier flush; this is not the hot write path. |
| **KV-only model** | No native SQL; no column pruning; no vectorised scan. Full-row decode per key. |
| **Range-scan support** | Present, but not columnar. Object-bound scans are full prefix scans. |
| **No native SPARQL plan awareness** | SlateDB cannot see the query predicates. Push-down is limited to key-range prefix. |
| **Maturity** | v0.12, ~3 years old, Commonhaus Foundation, 83 contributors, 2.9k stars, commercial adopters. Solid but not battle-tested at Wikidata scale. |
| **License** | Apache-2.0. Compatible with pg_ripple's Apache-2.0 licence. |

---

## 4. Candidate B — DuckDB via pg_duckdb

### 4.1 What pg_duckdb is

[pg_duckdb](https://github.com/duckdb/pg_duckdb) is the official PostgreSQL
extension that embeds DuckDB's columnar-vectorised analytics engine inside
PostgreSQL. It was built in collaboration with Hydra and MotherDuck, supports
PostgreSQL 14–18, and is at v1.1.1 (December 2025). Key capabilities:

- **Automatic query interception**: with `SET duckdb.force_execution = true`,
  analytical `SELECT` queries are transparently routed to DuckDB's engine.
- **Native object storage**: reads and writes Parquet, CSV, JSON, Apache Iceberg,
  and Delta Lake from S3, GCS, Azure Blob, and Cloudflare R2.
- **No data export required**: can scan existing PostgreSQL heap tables directly.
- **pg_duckdb tables**: `CREATE TABLE t USING duckdb` creates DuckDB-managed
  columnar tables inside PostgreSQL.
- **Parquet on S3**: `read_parquet('s3://bucket/prefix/*.parquet')` is a first-class
  table-valued function callable from SQL.

### 4.2 How pg_ripple would use pg_duckdb

The architecture for a DuckDB-backed cold tier is different from SlateDB. Instead of
a KV row-per-triple layout, triples are archived to **Parquet files on object storage**
and queried back via DuckDB:

**Write path (cold eviction)**

The merge worker, when evicting old rows from `vp_{id}_main`, writes them to Parquet
files on S3 using the `parquet` crate. The file naming convention encodes the
predicate and time range:

```
s3://my-bucket/pg_ripple/cold/vp_{predicate_id}/year=2024/month=03/batch_{uuid}.parquet
```

Columns: `s BIGINT, o BIGINT, g BIGINT, i BIGINT, source SMALLINT`.

This is columnar storage: DuckDB can push down `WHERE s = ?` and `WHERE o = ?`
predicates, perform column pruning, and leverage Parquet statistics (min/max per
row group) for partition elimination without decoding irrelevant rows.

**Read path (cold scan)**

A SQL view (or function) queries the Parquet cold tier via pg_duckdb:

```sql
-- Function registered by pg_ripple bootstrap
CREATE OR REPLACE FUNCTION _pg_ripple.cold_scan_vp(
    p_id   BIGINT,
    s_val  BIGINT DEFAULT NULL,
    o_val  BIGINT DEFAULT NULL
) RETURNS TABLE(s BIGINT, o BIGINT, g BIGINT, i BIGINT, source SMALLINT)
LANGUAGE sql AS $$
    SELECT s, o, g, i, source
    FROM read_parquet(
        format('s3://my-bucket/pg_ripple/cold/vp_%s/**/*.parquet', p_id)
    )
    WHERE (s_val IS NULL OR s = s_val)
      AND (o_val IS NULL OR o = o_val)
$$;
```

The `WHERE` clauses are pushed through DuckDB's Parquet reader into row-group
statistics and dictionary filters, making subject- and object-bound lookups fast
even over many Parquet files.

The VP union view becomes:

```sql
CREATE VIEW _pg_ripple.vp_{id} AS
  SELECT * FROM _pg_ripple.cold_scan_vp({id})
  UNION ALL
  SELECT * FROM _pg_ripple.vp_{id}_main
  UNION ALL
  SELECT * FROM _pg_ripple.vp_{id}_delta
  EXCEPT ALL
  SELECT s, o, g, i FROM _pg_ripple.vp_{id}_tombstones;
```

### 4.3 Compatibility concern: pgrx + pg_duckdb in the same process

This is the most significant risk in the DuckDB path and must be stated plainly.

Both pg_ripple (via pgrx) and pg_duckdb modify PostgreSQL's internal hooks:

| Hook | pg_duckdb | pgrx extensions |
|---|---|---|
| `ExecutorRun_hook` | Yes (intercepts analytical queries) | Some pgrx extensions |
| `ProcessUtility_hook` | Yes | pg_ripple: yes (for DDL tracking) |
| `planner_hook` | Yes | Not currently in pg_ripple |
| Memory allocator | DuckDB uses `jemalloc` internally | PostgreSQL MemoryContexts |

PostgreSQL hooks are chainable (each hook must call the previous value in the chain),
so co-existence is *possible* but requires careful audit. The pg_duckdb team has
tested against PG18 beta and release, but they have not specifically tested
co-installation with pgrx-based extensions that also use DDL hooks.

**Two risks:**

1. **Hook ordering**: if pg_duckdb's `ProcessUtility_hook` runs after pg_ripple's,
   DDL events that pg_ripple intercepts (to update `_pg_ripple.predicates`) may
   be skipped. Or vice versa. This would cause catalog corruption.

2. **Memory safety**: DuckDB loads `jemalloc` for its internal allocator inside the
   PostgreSQL process. pgrx relies on `palloc`/`pfree`. In pathological cases,
   freeing a DuckDB-allocated buffer through `pfree` (or vice versa) would
   crash the backend. This requires audit.

Mitigation: load pg_duckdb *after* pg_ripple in `shared_preload_libraries`, and
ensure pg_ripple's `ProcessUtility_hook` chains correctly. Run the full pg_regress
suite with both extensions loaded. A compatibility CI job is mandatory before
shipping.

**Alternative (avoids co-loading):** pg_ripple manages the Parquet files and
exposes them via a dedicated `parquet_fdw` foreign data wrapper, not via pg_duckdb.
This keeps the two extensions completely separate. The tradeoff is losing DuckDB's
vectorised scan engine — queries over cold Parquet fall back to slower sequential
processing. This is acceptable for cold-tier data that is accessed infrequently.

### 4.4 DuckDB: strengths and weaknesses for this use case

| | Assessment |
|---|---|
| **Columnar storage** | Parquet column pruning, row-group statistics, dictionary encoding. Far more scan-efficient than SlateDB's KV for object-bound queries. |
| **Native SQL** | Cold-tier queries are SQL, composable with the rest of the SPARQL→SQL pipeline without a separate read path. |
| **Parquet on S3** | First-class support. The `object_store` crate is already part of DuckDB's Rust layer. |
| **Predicate pushdown** | `WHERE s = ?` and `WHERE o = ?` pushed into Parquet statistics. Bloom filters in Parquet files supplement this. |
| **Vectorised execution** | DuckDB's SIMD-accelerated execution is much faster than row-level KV decode for large cold scans. |
| **pgrx co-loading risk** | High. Requires careful hook audit and dedicated CI. |
| **C++ dependency** | pg_duckdb is C++; pgrx is Rust. Mixing both in one extension binary is not straightforward. pg_duckdb must be a separate `.so` loaded via `shared_preload_libraries`. |
| **Parquet write from Rust** | The `parquet` crate (from Apache Arrow) is mature and allows pg_ripple to write Parquet without depending on DuckDB at write time. |
| **Iceberg / Delta Lake** | pg_duckdb supports both, opening the door to Iceberg-managed cold storage with time-travel and schema evolution. |
| **Maturity** | v1.1.1, PG14–18 supported, MotherDuck commercial backing, 3.1k stars, 47 contributors. More mature than SlateDB as a Postgres integration. |
| **License** | MIT. Compatible with pg_ripple's Apache-2.0. |

---

## 5. Hybrid architecture: SlateDB for warm-cold boundary, DuckDB for analytics

The two technologies are not mutually exclusive. Their strengths are complementary:

| Layer | Technology | Role |
|---|---|---|
| **Hot** (< 1 hour) | PostgreSQL heap `_delta` | Writes + random reads. Full ACID. |
| **Warm** (1 hour – 30 days) | PostgreSQL heap `_main` | Post-merge. BRIN scans. |
| **Warm-cold boundary** | SlateDB on local SST disk cache | Recent cold data cached locally. Object storage is the real store. |
| **Cold** (> 30 days) | Parquet on S3/GCS (written by merge worker) | Historical data. DuckDB scans via pg_duckdb or FDW. |
| **Archive analytics** | DuckDB / Iceberg on S3 | Analytical queries over years of history. |

The merge worker has two outputs:
1. Normal merge → `_main` (warm tier, stays as today).
2. Cold eviction → write Parquet to S3 (cold tier, via `parquet` crate from Rust).

SlateDB's role in this hybrid is as a **warm-cold cache**: instead of hitting S3
Parquet files for every cold read, a SlateDB instance on the same node acts as an
indexed cache of recently evicted rows. Cache hits are fast KV lookups; misses
fall through to the Parquet scan.

This is architecturally similar to how RocksDB is used as a block cache on top of
object storage in Cassandra's Bigtable-inspired designs, but it is a more complex
system to operate. Whether the SlateDB warm-cold cache is worth the complexity
depends on the access pattern:

- If cold data is almost never queried → skip SlateDB, go straight to Parquet.
- If cold data is queried infrequently but repeatedly (graph temporal queries,
  provenance lookups) → SlateDB cache saves repeated S3 GETs.

---

## 6. Scored comparison

Scoring 1 (poor) – 5 (excellent) on five axes relevant to pg_ripple:

| Criterion | SlateDB alone | DuckDB (Parquet) alone | Hybrid |
|---|---|---|---|
| **Integration complexity** | 3 — Rust-native but async/sync mismatch | 2 — C++ co-loading risk is real | 2 — both risks compound |
| **Query performance (cold scan)** | 2 — KV row-decode; no column pruning | 4 — columnar, vectorised, predicate pushdown | 4 — DuckDB handles scan; SlateDB handles cache |
| **Write throughput** | 4 — batched LSM writes; 50–100 ms latency | 4 — Parquet batch writes; latency depends on file size | 3 — two write paths to manage |
| **Operational simplicity** | 3 — single dependency; object store config | 3 — requires pg_duckdb co-install and hook audit | 2 — two external systems |
| **Alignment with existing architecture** | 4 — single-writer model matches merge worker | 3 — needs hook compatibility work | 3 |
| **Scalability ceiling** | 4 — bottomless object storage | 5 — Iceberg/Delta Lake for PB scale | 5 |
| **Risk** | Medium | Medium-High | High |
| **Recommended posture** | **Watch → Invest** | **Opportunistic** | **Post-v2.0** |

---

## 7. Recommended path

### Phase 1 (post-v1.0.0): Parquet cold tier, no DuckDB dependency

The lowest-risk, highest-value first step is to **write cold VP data as Parquet on
object storage without depending on either SlateDB or pg_duckdb at runtime**:

1. Add a `pg_ripple.cold_tier_bucket` GUC (empty = feature disabled).
2. Extend the merge worker: after a main-partition exceeds
   `pg_ripple.cold_eviction_rows` rows older than `pg_ripple.cold_eviction_age`,
   flush the oldest N rows to a Parquet file on S3 using the `parquet` crate.
3. Register the Parquet files in `_pg_ripple.cold_segments
   (predicate_id, min_i, max_i, s3_uri, row_count, created_at)`.
4. Expose a `pg_ripple.cold_stats()` function that reports segment count, byte size,
   and age distribution per predicate.
5. Cold data is **not yet queryable** in this phase; this phase proves the eviction
   pipeline works and lets operators observe what would move to cold storage.

**Cargo dependency added:**

```toml
# Cargo.toml — new optional feature
[features]
cold-tier = ["parquet", "object_store/aws", "object_store/gcp", "tokio"]

[dependencies]
parquet     = { version = "53", optional = true }
object_store = { version = "0.12", optional = true, features = ["aws", "gcp"] }
tokio        = { version = "1", optional = true, features = ["rt"] }
```

### Phase 2: SlateDB read path

Add read support via the SlateDB-backed warm-cold cache:

1. Add `slatedb` as an optional Cargo dependency behind `cold-tier-slatedb` feature flag.
2. Background worker opens/creates a SlateDB instance backed by the configured object store.
3. Evicted rows are written to SlateDB (via merge worker's tokio runtime) instead of
   directly to Parquet. SlateDB handles SST generation and compaction to object storage.
4. Implement `_pg_ripple.vp_cold_scan(predicate_id, s_filter, o_filter)` SRF backed
   by SlateDB range scan.
5. VP union views updated to include the cold SRF.
6. Benchmark: compare cold-read latency (SlateDB on S3 vs. direct S3 GET of raw Parquet).

At this point the system is fully functional with a three-tier storage stack.

### Phase 3: DuckDB analytical integration (opportunistic)

If Phase 2 benchmarks show that analytical queries spanning large cold ranges are
slow (the KV-per-row scan limit of SlateDB), add DuckDB as an optional analytical
read path:

1. Mirror cold VP data from SlateDB to Parquet via a background export job.
2. Register pg_duckdb as an optional sibling extension.
3. Implement `cold_analytics_scan` that routes large unbound scans through
   `read_parquet()` via pg_duckdb, while subject-bound lookups continue to use
   the SlateDB SRF.
4. Run the full pg_regress suite with both pg_ripple and pg_duckdb loaded.
5. Gate on a GUC: `pg_ripple.cold_analytics_engine = 'duckdb' | 'none'`.

---

## 8. What would block each option

### SlateDB blockers

- **tokio inside pgrx background worker**: needs a proof-of-concept. The `pgrx::BackgroundWorker`
  abstraction is a standard OS thread; `tokio::runtime::Builder::new_current_thread()` should
  work, but this is untested in the pgrx ecosystem.
- **Object store credentials**: AWS/GCS credentials inside a PostgreSQL extension
  process need to be handled securely (IAM role via EC2 instance metadata, or
  explicit GUC with `pg_catalog.pg_read_file` access restriction).
- **Snapshot consistency**: SlateDB's snapshot isolation is per-SlateDB-snapshot,
  not tied to PostgreSQL's MVCC. A triple evicted to cold storage mid-transaction
  and read back in the same transaction may observe a different snapshot. This
  requires a design decision: either (a) cold eviction is not visible within open
  transactions (safe but requires flush delay), or (b) the cold SRF always reads
  the latest SlateDB snapshot (simpler, slight MVCC impurity for cold rows).

### DuckDB blockers

- **Hook compatibility audit with pgrx**: this is a mandatory pre-requisite.
  Until a passing CI run with both extensions loaded is established, DuckDB
  integration is a prototype only.
- **pgrx 0.18 + pg_duckdb C++ build**: combining Rust and C++ in the same
  PostgreSQL installation requires careful `Makefile` / build system work. The two
  must compile to separate shared libraries.
- **Parquet write correctness**: Parquet files written by pg_ripple's merge worker
  must use a schema version and encoding that pg_duckdb's reader can parse without
  manual schema declarations. Arrow Schema metadata in the Parquet footer must
  be written correctly.

---

## 9. Summary recommendation

| | Recommendation |
|---|---|
| **Immediate (post-v1.0.0)** | Implement Parquet write-only cold eviction (Phase 1). No SlateDB, no DuckDB. Low risk, observable value, proves the eviction pipeline. |
| **Short-term (v1.2)** | Add SlateDB read path (Phase 2). This delivers the full hot/warm/cold lifecycle. Benchmark against direct Parquet scan to validate SlateDB cache value. |
| **Medium-term (v1.4+)** | Add pg_duckdb analytical integration (Phase 3) *only if* Phase 2 benchmarks reveal column-scan bottlenecks for large cold scans. Gate behind a feature flag; validate hook compatibility first. |
| **Skip entirely** | The hybrid SlateDB + DuckDB combination (§5). Too much operational complexity for the marginal gain over SlateDB alone. Revisit post-v2.0 if workload demands it. |

The **single biggest risk** is not technical — it is the tokio/async runtime mismatch
for SlateDB and the hook compatibility risk for pg_duckdb. Both are solvable and
neither is a deal-breaker. The right approach is to address them in dedicated
proof-of-concept branches before committing to either path.
