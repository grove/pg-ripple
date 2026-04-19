# pg-ripple

[![CI](https://github.com/grove/pg-ripple/actions/workflows/ci.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/ci.yml)
[![Release](https://github.com/grove/pg-ripple/actions/workflows/release.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/release.yml)
[![Roadmap](https://img.shields.io/badge/Roadmap-view-informational)](ROADMAP.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![PostgreSQL 18](https://img.shields.io/badge/PostgreSQL-18-blue?logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![pgrx 0.17](https://img.shields.io/badge/pgrx-0.17-orange)](https://github.com/pgcentralfoundation/pgrx)

**A high-performance RDF triple store inside PostgreSQL.**

pg_ripple is a PostgreSQL 18 extension building toward a fully-featured knowledge graph inside the database. It stores RDF data, queries it with SPARQL, validates it with SHACL, and reasons over it with Datalog — all without leaving PostgreSQL.

---

## What works today (v0.29.0)

**pg_ripple is now 100% conformant with the W3C SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites.** Twenty-nine versions in, pg_ripple covers the full SPARQL 1.1 stack, SHACL validation, goal-directed Datalog reasoning with magic sets, streaming RDF export, incremental live views, a standard HTTP endpoint, high-performance federated queries across remote SPARQL services, frame-driven JSON-LD export, GraphRAG integration, and vector + SPARQL hybrid search — all inside PostgreSQL with no separate process required.

| Area | What's included |
|---|---|
| **Storage** | VP tables (one per predicate), HTAP delta/main split, background merge worker, shared-memory dictionary cache; `source` column (`0`=explicit, `1`=derived) |
| **Encoding** | Dictionary encoding (IRI, blank node, literal → i64), inline encoding for numbers and dates, RDF-star / quoted triples; hot dictionary tier for high-frequency IRIs |
| **Import** | N-Triples, Turtle, TriG, N-Quads, RDF/XML; named-graph bulk loaders; file variants; remote `LOAD <url>` via SPARQL Update; `strict := true` mode for abort-on-error loading |
| **SPARQL** | Full SPARQL 1.1 — SELECT, CONSTRUCT, DESCRIBE, ASK; property paths, aggregates, UNION/MINUS, subqueries, BIND, VALUES, OPTIONAL, named graphs; INSERT/DELETE DATA; pattern-based DELETE/INSERT WHERE; USING/WITH clauses; graph management (CLEAR ALL/DEFAULT/NAMED, DROP ALL/DEFAULT/NAMED, ADD, COPY, MOVE, CREATE); **100% W3C SPARQL 1.1 Query & Update conformance** |
| **Output formats** | SELECT → JSONB; CONSTRUCT/DESCRIBE → JSONB, Turtle, or JSON-LD |
| **Export** | `export_turtle()`, `export_jsonld()`, `export_ntriples()`, streaming variants |
| **JSON-LD Framing** | `export_jsonld_framed(frame)` — frame-driven CONSTRUCT → nested JSON-LD; `jsonld_frame_to_sparql(frame)` — inspect the generated SPARQL; `export_jsonld_framed_stream(frame)` — NDJSON one object per root; `jsonld_frame(input, frame)` — general-purpose framing; `create_framing_view` / `drop_framing_view` / `list_framing_views` |
| **HTTP API** | `pg_ripple_http` companion service: W3C SPARQL 1.1 Protocol over HTTP/HTTPS; content negotiation (JSON, XML, CSV, TSV, Turtle, N-Triples, JSON-LD); bearer/basic auth; CORS; Prometheus metrics; Docker Compose included |
| **Federation** | `SERVICE <url> { … }` in any SPARQL query; SSRF-safe endpoint allowlist; `SERVICE SILENT`; configurable timeout, result cap, and error mode; health monitoring; local view rewrite; connection pooling; result caching with TTL; explicit variable projection; batch `SERVICE` (two clauses to the same endpoint → one HTTP request); adaptive timeouts; endpoint complexity hints; partial-result tolerance |
| **SPARQL views** | `create_sparql_view(name, sparql, schedule, decode)` — always-fresh stream table from any SPARQL SELECT; `drop_sparql_view`, `list_sparql_views` |
| **Datalog views** | `create_datalog_view(name, rules, goal, …)` — self-refreshing table from inline rules + goal; `create_datalog_view_from_rule_set`; `drop_datalog_view`, `list_datalog_views` |
| **Framing views** | `create_framing_view(name, frame)` — incrementally-maintained JSON-LD stream table (requires pg_trickle) |
| **ExtVP** | `create_extvp(name, pred1_iri, pred2_iri, schedule)` — pre-computed semi-join stream table for star queries; `drop_extvp`, `list_extvp` |
| **SHACL** | Core constraints (`sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, `sh:closed`, …); combinators (`sh:or`, `sh:and`, `sh:not`); sync and async validation modes; SHACL-AF `sh:rule` bridge; **100% W3C SHACL Core conformance** |
| **Datalog** | Custom inference rules (Turtle-flavoured syntax); built-in RDFS (13 rules) and OWL RL (~20 core rules); stratified negation; arithmetic/string built-ins; integrity constraints; on-demand execution mode; **semi-naive evaluation** via `infer_with_stats(rule_set)` returning `{"derived": N, "iterations": K}` |
| **Performance** | Selectivity-based BGP reordering (subject-bound 1%, object-bound 5% of row estimates); plan cache with hit/miss stats; parallel query hints for star patterns; extended statistics on VP column pairs; SHACL-informed optimizer hints; streaming cursor-based export (`pg_ripple.export_batch_size` GUC); `pg_ripple.property_path_max_depth` GUC (default 64) to cap recursive property-path depth; post-merge `ANALYZE` via `pg_ripple.auto_analyze` GUC; BRIN index on SID column for range-scan acceleration; `pg_ripple.explain_sparql(query, format)` for SQL/algebra/plan introspection |
| **Admin & Security** | `vacuum()`, `reindex()`, `vacuum_dictionary()`, `dictionary_stats()`; graph-level Row-Level Security via `enable_graph_rls`, `grant_graph`, `revoke_graph`; `rls_bypass` GUC for superuser sessions; `canary()` health-check function |
| **Full-text search** | `fts_search()` over literal values via PostgreSQL GIN indexes |
| **GraphRAG** | `export_graphrag_entities()`, `export_graphrag_relationships()`, `export_graphrag_text_units()` — Parquet export matching Microsoft GraphRAG's BYOG schema; bundled ontology, SHACL shapes, and Datalog enrichment rules; Python CLI bridge (`scripts/graphrag_export.py`) |
| **Vector search & RAG** | `store_embedding()`, `similar_entities()`, `embed_entities()`, `refresh_embeddings()` — pgvector integration for RDF entities; `hybrid_search()` (SPARQL + k-NN via Reciprocal Rank Fusion); `rag_retrieve()` — end-to-end RAG pipeline returning structured JSONB or JSON-LD; `contextualize_entity()` — graph-serialized text for embeddings; incremental embedding worker; external vector service federation (Weaviate, Qdrant, Pinecone) |
| **Datalog optimization** | Goal-directed inference via `infer_goal(rule_set, goal)` (magic sets transformation); cost-based join reordering; anti-join negation (`LEFT JOIN … IS NULL` for large VP tables); predicate-filter pushdown; delta-table B-tree indexing; subsumption-based redundant-rule elimination |

```sql
CREATE EXTENSION pg_ripple;

-- Import a Turtle file
SELECT pg_ripple.load_turtle(pg_read_file('/data/people.ttl'));

-- Query with a property path: everyone Alice can reach via "knows"
SELECT * FROM pg_ripple.sparql('
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    <http://example.org/Alice> foaf:knows+ ?person .
    ?person foaf:name ?name .
  }
');

-- Enforce a SHACL constraint: every Person must have exactly one name
SELECT pg_ripple.load_shacl('
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  <http://example.org/PersonShape> a sh:NodeShape ;
    sh:targetClass <http://example.org/Person> ;
    sh:property [ sh:path foaf:name ; sh:minCount 1 ; sh:maxCount 1 ] .
');

-- Export the whole graph as Turtle
SELECT pg_ripple.export_turtle();

-- SPARQL CONSTRUCT → JSON-LD for a REST API
SELECT pg_ripple.sparql_construct_jsonld('
  CONSTRUCT { ?s ?p ?o } WHERE { ?s a <http://schema.org/Person> ; ?p ?o }
');

-- Load RDFS entailment rules and run inference
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');
-- Now SPARQL sees inferred triples: if :Dog rdfs:subClassOf :Animal
-- and :Rex rdf:type :Dog, then ?x rdf:type :Animal binds :Rex too

-- Write custom rules (transitive management chain)
SELECT pg_ripple.load_rules(
  '?x ex:indirectManager ?z :- ?x ex:manager ?z .
   ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .',
  'org_rules'
);
SELECT pg_ripple.infer('org_rules');
```

**Storage architecture**: every IRI, blank node, and literal is dictionary-encoded to a compact integer; numeric and date literals use *inline encoding* (bit-packed integers, no dictionary round-trip). Facts are stored in per-predicate VP tables. From v0.6.0, each VP table is split into a write-optimised delta and a read-optimised BRIN-indexed main partition — a background worker continuously merges them, so heavy reads and writes never block each other.

---

## Where we're headed

One release remains on the path to v1.0.0.

### v1.0.0 — Production Release

With 100% W3C conformance achieved, GraphRAG integration complete, vector + SPARQL hybrid search in place, and goal-directed Datalog reasoning delivered, the final release focuses on production hardening: a full 72-hour continuous load test, final security audit sign-off, stress testing at 100 M+ triple scale, and a hardened upgrade path from every prior version. This is the version intended for production deployments.

---

## Why pg_ripple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_ripple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_ripple

### How it compares

> **Note**: pg_ripple features marked "Yes" in the table below are implemented across v0.1.0–v0.29.0. W3C SPARQL 1.1 Query, Update, and SHACL Core conformance is 100% (achieved in v0.20.0). Competitor capabilities reflect publicly documented feature sets.

| Capability | pg_ripple | Blazegraph | Virtuoso | Apache Fuseki |
|---|---|---|---|---|
| Runs inside PostgreSQL | Yes | No | No | No |
| SPARQL 1.1 Query | Yes | Yes | Yes | Yes |
| SPARQL 1.1 Update | Yes | Yes | Yes | Yes |
| SHACL validation | Yes (sync + async) | No | No | Plugin |
| Datalog reasoning (RDFS, OWL RL) | Yes | No | Limited | Partial |
| Incremental SPARQL views (IVM) | Yes (via pg_trickle) | No | No | No |
| RDF-star / RDF 1.2 | Yes | No | No | Yes |
| SPARQL Federation | Yes | No | Yes | Yes |
| Named graph access control | Yes (PostgreSQL RLS) | No | ACL | Apache Shiro |
| Full-text search | Yes (PostgreSQL GIN) | Yes | Yes | Yes |
| Backup & replication | PostgreSQL WAL | Custom | Custom | Custom |
| Language | Rust | Java | C | Java |

---

## Architecture

pg_ripple is built from the ground up for performance.

> The diagram below shows the v0.6.0+ architecture with the HTAP split and shared-memory cache.

```
 SPARQL Query / Update                   HTTP API
        │                                   │
        ▼                                   ▼
 ┌─────────────────┐              ┌──────────────────┐
 │  SPARQL Parser   │              │  pg_ripple_http   │
 │  (spargebra)     │              │  (Rust binary)    │
 └────────┬────────┘              └────────┬─────────┘
          │                                │
          ▼                                │
 ┌─────────────────┐                       │
 │  Algebra         │◄──────────────────────┘
 │  Optimizer       │
 │  · Self-join     │
 │    elimination   │
 │  · Filter        │
 │    pushdown      │
 │  · SHACL hints   │
 └────────┬────────┘
          │
          ▼
 ┌─────────────────┐    ┌──────────────────┐
 │  SQL Generator   │───▶│  PostgreSQL       │
 │  (integer joins) │    │  Executor (SPI)   │
 └─────────────────┘    └────────┬─────────┘
                                 │
                    ┌────────────┴────────────┐
                    │                         │
              ┌─────▼─────┐           ┌───────▼──────┐
              │ VP Tables  │           │  Dictionary   │
              │ (per-      │           │  (XXH3-128    │
              │ predicate) │           │   → i64)      │
              │            │           │              │
              │ Delta      │           │  Sharded LRU │
              │ (writes)   │           │  Cache (shmem)│
              │ Main       │           └──────────────┘
              │ (reads)    │
              └────────────┘
```

### Storage design

- **Dictionary encoding**: Every IRI, blank node, and literal is mapped to a dense sequential `BIGINT` via a hash-backed sequence. XXH3-128 is computed over the term (with the term-kind discriminant mixed in) and stored in full as a 16-byte `BYTEA` collision-detection key; a PostgreSQL IDENTITY sequence generates the actual join key. All VP-table joins operate on integers — no string comparisons in the hot path.
- **Vertical partitioning**: Each predicate gets its own table (`_pg_ripple.vp_{id}`) with columns `(s, o, g)`. This means queries that bind a predicate touch only one compact, heavily-indexed table.
- **Rare-predicate consolidation**: Predicates with fewer than 1,000 triples share a single table to avoid catalog bloat on predicate-rich datasets.
- **HTAP architecture**: Writes go to a small delta partition (B-tree indexed); a background worker asynchronously merges deltas into the read-optimised main partition (BRIN indexed). Reads and writes never block each other.

### Performance targets

| Operation | Target | At scale |
|---|---|---|
| Bulk load | >100,000 triples/sec | Batch COPY with deferred indexing |
| Transactional insert | >10,000 triples/sec | Delta partition, async validation |
| Simple query (BGP) | <5 ms | 10M triples |
| Star query (5 patterns) | <20 ms | 10M triples |
| Property path (depth 10) | <100 ms | 10M triples |
| Dictionary lookup (cache hit) | <1 μs | Sharded shared-memory LRU |

---

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (Edition 2024) |
| PostgreSQL binding | [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.17 |
| PostgreSQL version | 18.x |
| SPARQL parser | [spargebra](https://crates.io/crates/spargebra) — W3C-compliant SPARQL 1.1 algebra |
| SPARQL optimizer | [sparopt](https://crates.io/crates/sparopt) — first-pass algebra optimizer (filter pushdown, constant folding) |
| RDF parsers | [rio_turtle](https://crates.io/crates/rio_turtle), [rio_xml](https://crates.io/crates/rio_xml) — Turtle, N-Triples, RDF/XML; [oxttl](https://crates.io/crates/oxttl) / [oxrdf](https://crates.io/crates/oxrdf) — RDF-star / Turtle-star |
| Hashing | [xxhash-rust](https://crates.io/crates/xxhash-rust) (XXH3-128) — fast non-cryptographic hash for dictionary dedup |
| Serialization | [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) — SHACL reports, SPARQL results, config |
| HTTP server | [axum](https://crates.io/crates/axum) (built on [tokio](https://tokio.rs/)) — SPARQL Protocol HTTP endpoint (`pg_ripple_http` binary) |
| PG client (HTTP service) | [tokio-postgres](https://crates.io/crates/tokio-postgres) + [deadpool-postgres](https://crates.io/crates/deadpool-postgres) — async connection pool from HTTP service to PostgreSQL |
| HTTP client (federation) | [ureq](https://crates.io/crates/ureq) 2.12 — outbound calls to remote SPARQL endpoints (`SERVICE` keyword); connection-pooled `Agent` per backend session |
| IVM / stream tables | [pg_trickle](https://github.com/grove/pg-trickle) *(optional companion extension)* — incremental SPARQL views, ExtVP, live statistics |
| Dictionary cache | [lru](https://crates.io/crates/lru) — backend-local LRU cache (v0.1.0–v0.5.1); replaced by sharded shared-memory map in v0.6.0 |
| Error handling | [thiserror](https://crates.io/crates/thiserror) — typed error enums with PT error code constants (PT001–PT799) |
| Testing | pgrx `#[pg_test]`, `cargo pgrx regress`, [proptest](https://crates.io/crates/proptest), [cargo-fuzz](https://crates.io/crates/cargo-fuzz) |

---

## Getting Started

### Prerequisites

- PostgreSQL 18
- Rust stable toolchain
- [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.17

### Build and install

```bash
git clone https://github.com/grove/pg-ripple.git
cd pg-ripple

# Initialise pgrx for PostgreSQL 18
cargo pgrx init --pg18 $(which pg_config)

# Run tests
cargo pgrx test pg18

# Install into your local PostgreSQL
cargo pgrx install --pg-config $(which pg_config)
```

### Enable the extension

```sql
CREATE EXTENSION pg_ripple;
```



---

## Quality & Testing

pg_ripple aims for production-grade quality:

- **Unit tests** — pgrx `#[pg_test]` for every SQL-exposed function, property-based testing with `proptest`
- **Integration tests** — 78 pg_regress test files covering every feature
- **Security testing** — SQL injection prevention, malformed input resilience, resource exhaustion defence
- **Fuzz testing** — continuous fuzzing of the SPARQL→SQL pipeline with `cargo-fuzz`
- **Concurrency testing** — dictionary cache correctness, merge worker data integrity under concurrent writes
- **Performance regression CI** — automated benchmarks fail the build on >10% throughput regression
- **W3C conformance** — 100% pass rate on SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites (verified in `w3c_sparql_query_conformance`, `w3c_sparql_update_conformance`, `w3c_shacl_conformance` pg_regress tests)
- **Stability hardening** — 72-hour soak test, Valgrind memory leak detection, crash recovery testing

---

## Contributing

pg_ripple is in early development. Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
