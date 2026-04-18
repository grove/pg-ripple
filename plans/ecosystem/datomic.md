# Gap Analysis: Datomic vs pg_ripple

> **Scope**: This document compares Datomic (v1.0.7387, June 2025) with pg_ripple (v0.25.0, April 2026) across data model, query language, storage architecture, transaction semantics, time-travel, schema, deployment, and ecosystem. The goal is to identify features Datomic offers that pg_ripple lacks, features pg_ripple offers that Datomic lacks, and areas of conceptual overlap where the two systems solve the same problem differently.
>
> **Datomic sources**: Official documentation at docs.datomic.com (overview, schema reference, query reference, transaction processing, database filters, index model), Wikipedia entry, and Rich Hickey's published talks. No proprietary source code was examined. Datomic binaries are licensed under Apache 2.0.

---

## 1. Executive Summary

Datomic and pg_ripple are both fact-oriented databases that decompose entities into atomic assertions (datoms / triples). Despite this surface similarity, they target fundamentally different ecosystems and design philosophies:

| Dimension | Datomic | pg_ripple |
|---|---|---|
| **Host platform** | Standalone JVM process (Clojure) | PostgreSQL 18 extension (Rust / pgrx) |
| **Data model** | E/A/V/Tx/Op datoms (5-tuple) | S/P/O/G quads (RDF) |
| **Query language** | Datalog (Clojure-flavoured) | SPARQL 1.1 + Datalog |
| **Schema** | Attribute-level (type + cardinality) | W3C ontologies (RDFS/OWL) + SHACL |
| **Immutability** | Core invariant — nothing is ever deleted | Mutable (INSERT/DELETE); history not retained by default |
| **Time travel** | First-class `as-of`, `since`, `history` filters | Not built-in (PostgreSQL WAL/pg_audit can approximate) |
| **Write scaling** | Single serialised transactor per database | PostgreSQL MVCC — concurrent writers |
| **Read scaling** | Peer model — query engine runs in the application process | PostgreSQL connection model + read replicas |
| **Standards** | Proprietary Datalog dialect | W3C RDF, SPARQL 1.1, SHACL, JSON-LD |
| **License** | Apache 2.0 (binaries only; source not published) | PostgreSQL extension (open source, Rust) |

**Key insight**: Datomic's greatest strength is its immutable, bitemporal data model — every fact records *when* it was asserted or retracted, and the database can be rewound to any point in time without special configuration. pg_ripple's greatest strength is its deep PostgreSQL integration, W3C standards compliance, and concurrent write throughput.

---

## 2. Data Model

### 2.1 Datomic: Datoms (E/A/V/Tx/Op)

A Datomic database is a single universal relation of **datoms**. Each datom is a 5-tuple:

```
[entity-id  attribute  value  transaction-id  operation]
```

- **Entity ID (E)**: a 64-bit integer identifying an entity.
- **Attribute (A)**: a keyword (e.g. `:person/name`) that must be declared in the schema.
- **Value (V)**: one of ~15 built-in types (string, long, double, instant, ref, uuid, uri, keyword, symbol, bigdec, bigint, float, boolean, bytes, tuple).
- **Transaction ID (Tx)**: the entity ID of the transaction that added this datom.
- **Operation (Op)**: boolean — `true` for assertion, `false` for retraction.

Datoms are immutable and append-only. A "delete" is a retraction datom that records the fact is no longer true; the original assertion is preserved forever.

### 2.2 pg_ripple: RDF Quads (S/P/O/G)

pg_ripple stores RDF quads: `(subject, predicate, object, graph)`. Internally, all terms are dictionary-encoded to `i64` via XXH3-128 hashing and stored in per-predicate VP (Vertical Partitioning) tables:

```
_pg_ripple.vp_{predicate_id} (s BIGINT, o BIGINT, g BIGINT, i BIGINT, source SMALLINT)
```

- **Subject (S)**: IRI or blank node, encoded to `i64`.
- **Predicate (P)**: IRI, implicit in the VP table identity.
- **Object (O)**: IRI, blank node, or literal, encoded to `i64`.
- **Graph (G)**: named graph identifier (0 = default graph).
- **Statement ID (i)**: globally-unique sequential ID from a shared sequence.
- **Source**: 0 = explicit, 1 = inferred (Datalog-derived).

Triples are mutable — `DELETE` physically removes rows.

### 2.3 Gap Analysis: Data Model

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Fact immutability | Core invariant | Not supported — deletes remove data | **pg_ripple lacks** append-only semantics |
| Transaction as entity | Every Tx is an entity that can carry arbitrary attributes | Statement IDs exist but transactions are PostgreSQL transactions, not first-class entities | **pg_ripple lacks** reified transaction metadata |
| Multi-valued attributes | `:db.cardinality/many` — native set semantics | Multi-valued by nature (RDF predicate can have multiple objects) | Equivalent |
| Entity identity | 64-bit entity IDs; `:db.unique/identity` for upsert | IRIs are the identity mechanism; blank nodes for anonymous entities | Equivalent (different mechanisms) |
| Component entities | `:db/isComponent` — cascade retraction | No direct equivalent; SHACL can model part-of relationships | **pg_ripple lacks** automatic cascade delete |
| Typed values | ~15 built-in types enforced at schema level | RDF typed literals (XSD types); validation via SHACL | Equivalent (different type systems) |
| Tuple types | Composite, heterogeneous, and homogeneous tuples | No tuple type; composite keys modelled as blank nodes or reification | **pg_ripple lacks** native tuple support |
| Named graphs | No direct equivalent (Tx metadata serves some use cases) | First-class named graphs with GRAPH patterns | **Datomic lacks** named graph support |
| RDF-star | Not applicable | Full RDF-star support (quoted triples) since v0.4.0 | **Datomic lacks** statement-about-statement |
| Standards compliance | Proprietary | W3C RDF 1.1, RDF-star, SPARQL 1.1, SHACL, JSON-LD | **Datomic lacks** W3C standards |

---

## 3. Query Language

### 3.1 Datomic Datalog

Datomic uses a Clojure-native Datalog dialect with the following features:

- **Data patterns**: `[?e :artist/name "The Beatles"]` — match datoms by position.
- **Implicit joins**: shared variables across clauses unify automatically.
- **Not/or/and clauses**: `not`, `not-join`, `or`, `or-join`, `and` for negation and disjunction.
- **Aggregates**: `min`, `max`, `sum`, `count`, `avg`, `median`, `stddev`, `variance`, `sample`, `rand`, `distinct`.
- **Pull expressions**: declarative, hierarchical entity traversal inline in `:find` — `(pull ?e [:artist/startYear :artist/endYear])`.
- **Rules**: reusable, composable query fragments passed as input — equivalent to views or macros.
- **Nested queries**: the `q` function can be called recursively inside a query.
- **Java/Clojure function calls**: arbitrary pure functions can be used as predicates or expression functions.
- **Collection/tuple/relation bindings**: flexible destructuring of input data.
- **Full-text search**: built-in `fulltext` function (Peer API only).
- **Return maps**: `:keys`, `:strs`, `:syms` for map-shaped results.

### 3.2 pg_ripple SPARQL + Datalog

pg_ripple implements the full SPARQL 1.1 query/update specification:

- **SELECT/CONSTRUCT/DESCRIBE/ASK** query forms.
- **Property paths** with `+`, `*`, `?`, `/`, `|`, `^`, `!` — compiled to `WITH RECURSIVE ... CYCLE`.
- **FILTER/BIND/VALUES/OPTIONAL/UNION/MINUS/EXISTS/NOT EXISTS**.
- **Aggregates**: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT`, `SAMPLE`.
- **Subqueries**, **SERVICE** (federation), **GRAPH** patterns.
- **SPARQL Update**: `INSERT DATA`, `DELETE DATA`, `INSERT/DELETE WHERE`, `LOAD`, `CLEAR`, `DROP`, `CREATE`, `COPY`, `MOVE`, `ADD`.
- **SPARQL Protocol** via `pg_ripple_http` companion service.
- **Datalog reasoning** with stratified negation, semi-naive evaluation, and built-in RDFS/OWL RL rule sets.

### 3.3 Gap Analysis: Query Language

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Datalog | Native, primary query language | Supported (separate from SPARQL) | Equivalent |
| SPARQL | Not supported | Full SPARQL 1.1 | **Datomic lacks** SPARQL |
| Property paths | Not directly supported (rules can emulate) | Full W3C property path syntax | **Datomic lacks** property paths |
| Pull / entity traversal | First-class `pull` API with patterns, wildcards, recursion, reverse lookup | DESCRIBE + JSON-LD framing | Similar capability, different API |
| Rules / reusable query fragments | Composable rule sets passed as `:in %` | Datalog rules stored in `_pg_ripple.rules` | Equivalent |
| Arbitrary function calls | Any Clojure/Java function in predicates and expressions | SPARQL built-in functions (~60); no user-defined function extension | **pg_ripple lacks** extensible function predicates |
| Nested queries | `q` function inside query | SPARQL subqueries | Equivalent |
| Full-text search | `fulltext` built-in (Peer only) | `pg_ripple.fts_search()` via GIN/tsvector | Equivalent |
| Federation | Cross-database queries via multiple `$` sources | SPARQL `SERVICE` with connection pooling and caching | Equivalent (different mechanisms) |
| Return shape control | `:keys`, `:strs`, `:syms` for map results | JSON-LD framing, `CONSTRUCT` templates | Equivalent |
| Speculative queries | `d/with` — apply hypothetical transactions without committing | Not supported | **pg_ripple lacks** speculative `with` |
| Query stats/explain | `query-stats`, `io-stats` | `explain_sparql()` with SQL/algebra/EXPLAIN ANALYZE | Equivalent |

---

## 4. Time Travel & Immutability

### 4.1 Datomic: First-Class Bitemporality

Datomic's most distinctive feature is its immutable, time-aware data model:

- **`as-of(t)`**: returns a database value as it existed at time `t` — subsequent transactions are invisible.
- **`since(t)`**: returns only datoms added after time `t`.
- **`history`**: returns the complete history of all assertions and retractions, suitable for full audit queries.
- **`filter(pred)`**: arbitrary predicate-based filtering of datoms (e.g. exclude erroneous transactions, restrict by security level).
- **`d/with`**: speculative — apply transactions to a database value in memory without persisting, for "what-if" analysis.

Because Datomic never updates or deletes datoms, these views are not reconstructed from WAL or snapshots — they are efficient index scans over the existing data.

### 4.2 pg_ripple: No Built-in Time Travel

pg_ripple does not maintain historical versions of triples. Deletes are physical. The statement ID (`i` column) provides insertion ordering but not temporal semantics.

Approximate workarounds exist in the PostgreSQL ecosystem:
- PostgreSQL's own WAL + `pg_audit` for compliance logging.
- Temporal tables (SQL:2011) via extensions like `temporal_tables`.
- pg_ripple's HTAP architecture preserves pre-merge snapshots briefly, but this is an implementation detail, not a query-time feature.

### 4.3 Gap Analysis: Time Travel

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| As-of queries | `d/as-of` — first-class, efficient | Not supported | **pg_ripple lacks** |
| Since queries | `d/since` | Not supported | **pg_ripple lacks** |
| Full history | `d/history` — all assertions and retractions | Not supported | **pg_ripple lacks** |
| Custom filters | `d/filter` with arbitrary predicates | Not supported | **pg_ripple lacks** |
| Speculative transactions | `d/with` | Not supported | **pg_ripple lacks** |
| Transaction wall-clock time | `:db/txInstant` on every Tx entity | PostgreSQL `xact_start`; not per-triple | **pg_ripple lacks** per-triple timestamps |
| `:db/noHistory` opt-out | Per-attribute history suppression for high-churn data | N/A (no history to suppress) | N/A |

**Assessment**: Time travel is Datomic's defining capability and the largest functional gap in pg_ripple. Implementing an equivalent would require fundamental architectural changes — either an append-only storage model or a separate temporal index.

---

## 5. Schema & Data Modelling

### 5.1 Datomic Schema

Datomic schema is **attribute-level**: every attribute must be declared with a value type and cardinality before use. Schema entities are themselves datoms (schema-as-data).

Key schema attributes:
- `:db/valueType` — one of ~15 types (string, long, ref, instant, uuid, etc.).
- `:db/cardinality` — `:db.cardinality/one` or `:db.cardinality/many`.
- `:db/unique` — `:db.unique/identity` (upsert) or `:db.unique/value` (reject duplicates).
- `:db/isComponent` — cascade retraction for part-of relationships.
- `:db/noHistory` — opt out of history for high-churn attributes.
- `:db/fulltext` — enable full-text search index.
- `:db.attr/preds` — custom validation predicates (arbitrary Clojure functions).
- `:db.entity/attrs` — required attributes (checked at transaction time via `:db/ensure`).
- `:db.entity/preds` — custom entity predicates (functions of db + entity).
- Tuple types: composite (auto-derived), heterogeneous (fixed types), homogeneous (variable length).

Datomic schema is open: any entity can possess any attribute. There are no "tables" — the universal relation of datoms is the only structure. Schema elements are limited to fewer than 2^20.

### 5.2 pg_ripple Schema

pg_ripple uses W3C standards for schema:
- **RDFS/OWL ontologies**: class hierarchies (`rdfs:subClassOf`), property domains/ranges, cardinality restrictions.
- **SHACL shapes**: declarative validation constraints (min/max count, datatype, pattern, value range, node kind, closed shapes, `sh:lessThan`, `sh:greaterThan`, etc.).
- **Datalog rules**: derived predicates and entailment rules (RDFS, OWL RL, custom).

Schema is expressed as triples in the same store — fully self-describing.

### 5.3 Gap Analysis: Schema

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Required attributes | `:db.entity/attrs` + `:db/ensure` | `sh:minCount 1` | Equivalent |
| Uniqueness constraints | `:db.unique/identity`, `:db.unique/value` | No built-in unique constraint; SHACL `sh:maxCount 1` approximates | **pg_ripple lacks** native upsert-on-unique |
| Upsert semantics | Automatic with `:db.unique/identity` | Not supported — manual check-then-insert | **pg_ripple lacks** automatic upsert |
| Custom validation predicates | `:db.attr/preds` (arbitrary Clojure functions) | SHACL `sh:sparql` (SPARQL-based constraints) | Equivalent |
| Entity predicates | `:db.entity/preds` (functions of db + entity) | SHACL node shapes with SPARQL constraints | Equivalent |
| Component/cascade | `:db/isComponent` | Not built-in | **pg_ripple lacks** |
| Composite tuples | Auto-derived multi-attribute keys | Not supported | **pg_ripple lacks** |
| Schema-as-data | Schema entities are datoms | Schema triples are RDF | Equivalent |
| Class hierarchies | Not built-in (can be modelled) | RDFS/OWL with Datalog reasoning | **Datomic lacks** |
| Property hierarchies | Not built-in | `rdfs:subPropertyOf` with entailment | **Datomic lacks** |
| Shape validation | `:db/ensure` at transaction time | SHACL with sync and async modes | **pg_ripple advantage** — richer constraint language |
| Schema evolution | Additive only — attribute types cannot be changed after creation | Triples can be freely added/removed/changed | **pg_ripple advantage** — more flexible |

---

## 6. Storage Architecture

### 6.1 Datomic: Immutable Persistent Trees

Datomic maintains four covering indexes over the universal datom relation:

| Index | Sort Order | Purpose |
|---|---|---|
| **EAVT** | Entity → Attribute → Value → Tx | "Row" access — all facts about an entity |
| **AEVT** | Attribute → Entity → Value → Tx | "Column" access — all values of an attribute |
| **AVET** | Attribute → Value → Entity → Tx | Key/value lookup — find entity by attribute value |
| **VAET** | Value → Attribute → Entity → Tx | Reverse navigation — `:db.type/ref` attributes only |

Each index is an immutable persistent tree (wide branching factor, typically thousands of items per node). Index segments are stored in pluggable storage backends (DynamoDB, PostgreSQL, Cassandra, local filesystem). Segments are cached at multiple levels: application-process object cache, optional Memcached/Valcache tier, and storage.

The **transaction log** is a separate chronological index of all transactions, queryable via the Log API.

### 6.2 pg_ripple: Vertical Partitioning + HTAP

pg_ripple uses **Vertical Partitioning** (one table per predicate) with an HTAP (Hybrid Transactional/Analytical Processing) architecture:

- **VP tables**: `_pg_ripple.vp_{predicate_id}` with columns `(s, o, g, i, source)`.
- **Rare-predicate consolidation**: predicates below `vp_promotion_threshold` (default 1000) share `_pg_ripple.vp_rare`.
- **HTAP split**: writes go to `vp_{id}_delta` (B-tree indexed); a background merge worker periodically combines `delta` into `main` (BRIN-indexed) minus `tombstones`.
- **Query path**: `(main EXCEPT tombstones) UNION ALL delta`.
- **Dictionary encoding**: all terms → `i64` via XXH3-128 + sequence ID; cached per-backend with LRU.
- **Shared memory**: dictionary cache in `PgSharedMem` for cross-backend sharing.

### 6.3 Gap Analysis: Storage

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Index strategy | 4 covering immutable B+ trees (EAVT, AEVT, AVET, VAET) | Per-predicate VP tables with B-tree + BRIN | Different approach; both effective |
| Write path | Single serialised transactor | PostgreSQL MVCC — concurrent writers via `vp_delta` | **pg_ripple advantage** — higher write concurrency |
| Read scaling | Peer model — in-process query engine with local cache | PostgreSQL read replicas + connection pooling | **Datomic advantage** — in-process, zero-latency cache |
| Storage backends | Pluggable: DynamoDB, SQL, Cassandra, local FS | PostgreSQL only (leverages PG's own WAL, replication, etc.) | Trade-off: Datomic more flexible; pg_ripple more integrated |
| Immutable segments | All index segments immutable — cache anywhere without coordination | Mutable B-tree pages — standard PG buffer cache | **Datomic advantage** — cache coherence for free |
| Background indexing | Sublinear-time index rebuilds; live index covers recent transactions | HTAP merge worker; BRIN indexing on main partitions | Similar approach |
| Transaction log | First-class queryable log | PostgreSQL WAL (not directly queryable from SQL) | **pg_ripple lacks** queryable transaction log |
| Dictionary encoding | Entity IDs are 64-bit integers; attribute idents are interned keywords | XXH3-128 hash → sequence ID; LRU-cached encode/decode | Equivalent approach |

---

## 7. Transaction Semantics

### 7.1 Datomic Transactions

- **Serialised**: all transactions for a database pass through a single transactor process, ensuring a total order.
- **ACID**: full ACID guarantees with serialisable isolation.
- **Transaction functions**: Clojure/Java functions that execute inside the transactor, enabling compare-and-swap, conditional logic, and arbitrary validation.
- **Transaction entities**: every transaction is itself an entity carrying `:db/txInstant` (wall-clock time) and any user-defined attributes (provenance, source, confidence level, etc.).
- **Speculative transactions**: `d/with` applies transactions to an in-memory database value for what-if analysis.
- **Transaction monitoring**: peers receive a real-time queue of all transaction reports (`db-before`, `db-after`, `tx-data`, `tempids`).

### 7.2 pg_ripple Transactions

- **PostgreSQL MVCC**: standard PostgreSQL transaction isolation (read committed by default; serialisable available).
- **Concurrent writers**: multiple connections can insert/delete simultaneously.
- **No transaction entities**: transactions are PostgreSQL transactions — no reified metadata stored alongside triples.
- **CDC**: change-data-capture module (`src/cdc.rs`) for downstream notification, but no built-in "transaction report queue".
- **No speculative transactions**: `d/with` equivalent not available.

### 7.3 Gap Analysis: Transactions

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Write concurrency | Single serialised transactor — one writer at a time | MVCC — concurrent writers | **pg_ripple advantage** |
| Transaction functions | Arbitrary Clojure/Java code in the transactor | PostgreSQL triggers and `#[pg_extern]` functions | Equivalent (different mechanism) |
| Transaction as entity | First-class; carries metadata | Not supported | **pg_ripple lacks** |
| Speculative `with` | `d/with` for hypothetical transactions | Not supported | **pg_ripple lacks** |
| Transaction monitoring | `tx-report-queue` — real-time push to all peers | CDC module; PostgreSQL LISTEN/NOTIFY | Partial — **pg_ripple lacks** structured tx reports |
| Isolation level | Serialisable (inherent from single transactor) | Read committed (default); serialisable available | Equivalent (configurable in pg_ripple) |

---

## 8. Deployment & Operations

### 8.1 Datomic Editions

Datomic is available in three editions:
- **Datomic Local**: embedded single-process database (local filesystem).
- **Datomic Pro**: distributed database with pluggable storage; you manage transactors, peers, and caches.
- **Datomic Cloud**: AWS-managed deployment with CloudFormation, ALB, Lambda, Auto Scaling.

All editions are free under Apache 2.0 (binaries only; source is not published).

### 8.2 pg_ripple Deployment

- **PostgreSQL extension**: `CREATE EXTENSION pg_ripple` in any PostgreSQL 18 instance.
- **HTTP companion**: `pg_ripple_http` Axum-based service for SPARQL Protocol.
- **Docker**: `docker-compose.yml` provided.
- **No managed service**: users provision and operate PostgreSQL themselves (or use managed PG services like RDS, Cloud SQL, etc.).

### 8.3 Gap Analysis: Deployment

| Feature | Datomic | pg_ripple | Gap |
|---|---|---|---|
| Embedded/local mode | Datomic Local | Not applicable (requires PG server) | **Datomic advantage** for embedded use |
| Managed cloud | Datomic Cloud (AWS) | None — relies on managed PG services | **Datomic advantage** |
| On-premise / self-hosted | Datomic Pro | Any PostgreSQL 18 installation | Equivalent |
| Language ecosystem | JVM (Clojure, Java, Kotlin, Scala) | Any language with a PostgreSQL driver | **pg_ripple advantage** — polyglot |
| HTTP API | REST API (limited); Ions for custom HTTP endpoints | SPARQL Protocol via `pg_ripple_http` | Equivalent |
| SQL analytics | Datomic Analytics (Presto-based) — SQL view over datoms | Native PostgreSQL SQL + any BI tool | **pg_ripple advantage** — native SQL |
| Monitoring | CloudWatch (Cloud), custom metrics (Pro) | `pg_stat_statements`, `canary()`, `cache_stats()`, native PG monitoring | Equivalent |
| Backup/restore | `datomic backup/restore` CLI | `pg_dump`/`pg_restore`; standard PG PITR | Equivalent |

---

## 9. Ecosystem & Interoperability

| Dimension | Datomic | pg_ripple | Assessment |
|---|---|---|---|
| **Standards body** | None | W3C (RDF, SPARQL, SHACL, JSON-LD, RDF-star) | **pg_ripple advantage** — interoperable with any W3C-compliant tool |
| **Serialisation formats** | EDN (Extensible Data Notation) | Turtle, N-Triples, N-Quads, RDF/XML, JSON-LD | **pg_ripple advantage** — industry-standard formats |
| **Graph query interop** | Datomic Datalog only | SPARQL federation to any SPARQL endpoint | **pg_ripple advantage** |
| **Linked Data** | Not designed for Linked Data | First-class IRI-based identity; Linked Data native | **pg_ripple advantage** |
| **Clojure ecosystem** | Deeply integrated; schema-as-data in EDN | No Clojure-specific integration | **Datomic advantage** for Clojure shops |
| **BI / SQL tools** | Datomic Analytics (Presto bridge) | Native PostgreSQL — Metabase, Grafana, dbt, etc. | **pg_ripple advantage** |
| **Vector search** | Not supported | pgvector integration (v0.27.0+) | **pg_ripple advantage** |
| **GraphRAG** | Not supported | Microsoft GraphRAG integration (v0.26.0+) | **pg_ripple advantage** |
| **GeoSPARQL** | Not supported | GeoSPARQL 1.1 geometry primitives (v0.25.0+) | **pg_ripple advantage** |

---

## 10. Performance Characteristics

| Dimension | Datomic | pg_ripple |
|---|---|---|
| **Write throughput** | Bottlenecked by single transactor; thousands of datoms/tx, tens of tx/s typical | Concurrent writers; 100k+ triples/s in bulk load |
| **Read latency** | Sub-millisecond for cached data (in-process); storage roundtrip on cache miss | PostgreSQL buffer cache + shared dictionary cache; typically <1ms for hot data |
| **Read scaling** | Horizontal — add more peer processes | PostgreSQL read replicas; connection pooling |
| **Working set > memory** | Multi-tier LRU (object cache → Memcached → Valcache → storage) | PostgreSQL shared_buffers + OS page cache |
| **Index rebuild** | Sublinear background indexing; live index covers recent transactions | HTAP merge worker; BRIN on main partitions |
| **High-churn data** | Documented tradeoff — not suitable for logs/telemetry | HTAP delta absorbs high write rates; merge amortises |

---

## 11. Lessons for pg_ripple

### 11.1 Features Worth Considering

1. **Temporal / audit log layer**: Datomic's `as-of` / `since` / `history` is its killer feature. pg_ripple could offer an opt-in append-only mode where deletes are logical (retraction datoms) rather than physical, enabling time-travel queries. This could be implemented as a shadow table per VP table recording `(s, o, g, i, tx_id, op)` — similar to the existing `source` column but with transaction identity and operation type.

2. **Transaction-as-entity**: Recording transaction metadata (timestamp, source, confidence level) as triples in a dedicated graph (e.g. `<urn:pg_ripple:tx:{txid}>`) would enable provenance queries without external audit infrastructure.

3. **Speculative `with`**: A read-only "what-if" mode that applies triples to an in-memory snapshot (using PostgreSQL's `SAVEPOINT` / `ROLLBACK TO`) could serve similar use cases — e.g. "what would this SHACL validation produce if I added these triples?"

4. **Upsert semantics**: Datomic's `:db.unique/identity` upsert is ergonomic for ETL pipelines. pg_ripple could support `ON CONFLICT`-style upsert for triples with unique-property predicates, potentially driven by SHACL `sh:maxCount 1`.

5. **Pull-style entity traversal**: Datomic's `pull` API is more concise than SPARQL CONSTRUCT for common "give me everything about entity X" patterns. JSON-LD framing covers some of this, but a dedicated `pg_ripple.pull(iri, pattern)` function could be more ergonomic.

### 11.2 pg_ripple Strengths to Preserve

1. **PostgreSQL ecosystem integration**: The ability to join triple data with relational tables, use PG extensions (PostGIS, pgvector, pg_trgm), and connect any BI tool is a moat Datomic cannot match.

2. **W3C standards compliance**: SPARQL federation, JSON-LD, SHACL, and RDF-star make pg_ripple interoperable with the entire Semantic Web ecosystem.

3. **Write concurrency**: Datomic's single-transactor bottleneck is a fundamental architectural constraint. pg_ripple's MVCC-based concurrent writes are a significant advantage for write-heavy workloads.

4. **Datalog + SPARQL**: Having both query paradigms gives pg_ripple flexibility that Datomic (Datalog only) and traditional triple stores (SPARQL only) lack.

---

## 12. Summary Comparison Matrix

| # | Capability | Datomic | pg_ripple | Winner |
|---|---|---|---|---|
| 1 | Immutable data model | Yes | No | Datomic |
| 2 | Time-travel queries (as-of, since, history) | Yes | No | Datomic |
| 3 | Speculative transactions | Yes | No | Datomic |
| 4 | Transaction-as-entity with metadata | Yes | No | Datomic |
| 5 | Pull API (declarative entity traversal) | Yes | Partial (JSON-LD framing) | Datomic |
| 6 | In-process query engine (peer model) | Yes | No | Datomic |
| 7 | Embedded / local mode | Yes | No | Datomic |
| 8 | Upsert on unique identity | Yes | No | Datomic |
| 9 | Arbitrary function predicates in queries | Yes | No | Datomic |
| 10 | Write concurrency | No (single transactor) | Yes (MVCC) | pg_ripple |
| 11 | W3C standards (RDF, SPARQL, SHACL, JSON-LD) | No | Yes | pg_ripple |
| 12 | SPARQL 1.1 query/update | No | Yes | pg_ripple |
| 13 | Property paths | No | Yes | pg_ripple |
| 14 | Named graphs | No | Yes | pg_ripple |
| 15 | RDF-star (statements about statements) | No | Yes | pg_ripple |
| 16 | SHACL validation | No | Yes | pg_ripple |
| 17 | Ontology reasoning (RDFS/OWL RL) | No | Yes (Datalog engine) | pg_ripple |
| 18 | SPARQL federation (SERVICE) | No | Yes | pg_ripple |
| 19 | SQL ecosystem / BI tool integration | Limited (Analytics/Presto) | Native PostgreSQL | pg_ripple |
| 20 | Polyglot language support | JVM only | Any PG driver | pg_ripple |
| 21 | Vector search integration | No | Yes (pgvector) | pg_ripple |
| 22 | GraphRAG integration | No | Yes | pg_ripple |
| 23 | GeoSPARQL | No | Yes | pg_ripple |
| 24 | Multiple serialisation formats | EDN only | Turtle, N-Triples, RDF/XML, JSON-LD | pg_ripple |
| 25 | Datalog queries | Yes (native) | Yes | Tie |
| 26 | ACID transactions | Yes | Yes | Tie |
| 27 | Full-text search | Yes (Peer only) | Yes | Tie |
| 28 | Schema-as-data | Yes | Yes (RDF) | Tie |

**Score**: Datomic leads 9, pg_ripple leads 15, tied 4.

---

## 13. Conclusion

Datomic and pg_ripple occupy different niches. Datomic excels at **audit-grade data-of-record applications** where the complete history of every fact matters — finance, healthcare, legal — and where the Clojure/JVM ecosystem is already in use. pg_ripple excels at **standards-based knowledge graph workloads** embedded in a PostgreSQL-centric stack — semantic data integration, SPARQL-driven analytics, ontology-based reasoning, and hybrid search (graph + vector + full-text).

The most impactful feature pg_ripple could borrow from Datomic is **opt-in temporal tracking**: an append-only mode where retractions are recorded rather than physical deletes, enabling `as-of` and `history` queries over the triple store. This would close the single largest capability gap while preserving pg_ripple's write-concurrency and standards-compliance advantages.
