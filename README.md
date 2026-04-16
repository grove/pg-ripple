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

## What works today (v0.9.0)

v0.9.0 completes the core stack: store, query, validate, and exchange RDF data — all inside PostgreSQL.

| Area | What's included |
|---|---|
| **Storage** | VP tables (one table per predicate), HTAP delta/main split, background merge worker, shared-memory dictionary cache |
| **Encoding** | Dictionary encoding (IRI, blank node, literal → i64), inline encoding for numbers and dates, RDF-star / quoted triples |
| **Import** | N-Triples, Turtle, TriG, N-Quads, RDF/XML; named graphs; bulk load |
| **SPARQL** | Full SPARQL 1.1 — SELECT, CONSTRUCT, DESCRIBE, ASK; property paths, aggregates, UNION/MINUS, subqueries, BIND, VALUES, OPTIONAL, named graphs |
| **Output formats** | SELECT → JSONB; CONSTRUCT/DESCRIBE → JSONB, Turtle, or JSON-LD |
| **Export** | `export_turtle()`, `export_jsonld()`, `export_ntriples()`, streaming variants |
| **SHACL** | Core constraints (`sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, …); combinators (`sh:or`, `sh:and`, `sh:not`); sync and async validation modes |
| **Write** | `insert_triple`, `delete_triple`, SPARQL INSERT/DELETE DATA, deduplication |
| **Full-text search** | `fts_search()` over literal values via PostgreSQL GIN indexes |

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
```

**Storage architecture**: every IRI, blank node, and literal is dictionary-encoded to a compact integer; numeric and date literals use *inline encoding* (bit-packed integers, no dictionary round-trip). Facts are stored in per-predicate VP tables. From v0.6.0, each VP table is split into a write-optimised delta and a read-optimised BRIN-indexed main partition — a background worker continuously merges them, so heavy reads and writes never block each other.

---

## Where we're headed

Each release adds a self-contained layer of capability, building toward a complete knowledge graph platform inside PostgreSQL.

### v0.10.0 — Datalog reasoning

Automatically derive new facts from rules. Built-in rulesets cover RDFS (13 rules) and OWL 2 RL (~80 rules). You can also write custom rules. Once enabled, SPARQL queries see both explicit and inferred facts transparently.

```sql
-- Load built-in RDFS entailment
SELECT pg_ripple.load_rules_builtin('rdfs');
-- If :Dog rdfs:subClassOf :Animal, and :Rex rdf:type :Dog,
-- then pg_ripple automatically infers :Rex rdf:type :Animal

-- Or write custom rules to derive new relationships
SELECT pg_ripple.load_rules('
  -- Transitive manager relationship
  ?x ex:indirectManager ?z :- ?x ex:manager ?z .
  ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .

  -- Flag people without an email (negation-as-failure)
  ?x ex:missingEmail "true"^^xsd:boolean :- 
    ?x rdf:type foaf:Person, 
    NOT ?x foaf:mbox ?_ .
', rule_set := 'company_data');
```

### v0.11.0 — Incremental SPARQL & Datalog views

Pin a SPARQL query or a Datalog rule set to a live, automatically-updated result table. Only changed rows are reprocessed, so updates are near-instantaneous. Requires the companion [pg_trickle](https://github.com/grove/pg-trickle) extension.

### v0.15.0 — SPARQL Protocol (HTTP)

A companion `pg_ripple_http` service exposes a standard W3C SPARQL 1.1 Protocol endpoint so browsers, dashboards (YASGUI, Metaphacts), and any SPARQL client can query over HTTP — no PostgreSQL driver needed.

### v0.16.0 — SPARQL Federation

Query remote SPARQL endpoints alongside local data in a single query using the standard `SERVICE` keyword. Remote calls execute in parallel.

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX ex:  <http://example.org/>
  PREFIX dbo: <http://dbpedia.org/ontology/>
  SELECT ?name ?abstract WHERE {
    ?person ex:worksAt ex:AcmeCorp ;
            ex:name    ?name .
    SERVICE <https://dbpedia.org/sparql> {
      ?person dbo:abstract ?abstract .
      FILTER(LANG(?abstract) = "en")
    }
  }
');
```

---

## Why pg_ripple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_ripple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_ripple

### How it compares

> **Note**: pg_ripple features marked "Yes" are *planned* across v0.1.0–v1.0.0; see the [Roadmap](ROADMAP.md) for delivery versions. Competitor capabilities reflect publicly documented feature sets.

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

> The diagram below shows the target v0.6.0+ architecture after the HTAP split and shared-memory cache land.

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
| RDF parsers | [rio_turtle](https://crates.io/crates/rio_turtle), [rio_xml](https://crates.io/crates/rio_xml) — Turtle, N-Triples, RDF/XML; custom N-Triples-star parser (v0.4.0); [oxttl](https://crates.io/crates/oxttl) / [oxrdf](https://crates.io/crates/oxrdf) planned at v0.5.x for Turtle-star |
| Hashing | [xxhash-rust](https://crates.io/crates/xxhash-rust) (XXH3-128) — fast non-cryptographic hash for dictionary dedup |
| Serialization | [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) — SHACL reports, SPARQL results, config |
| HTTP server | [axum](https://crates.io/crates/axum) (built on [tokio](https://tokio.rs/)) — SPARQL Protocol HTTP endpoint (`pg_ripple_http` binary) |
| PG client (HTTP service) | [tokio-postgres](https://crates.io/crates/tokio-postgres) + [deadpool-postgres](https://crates.io/crates/deadpool-postgres) — async connection pool from HTTP service to PostgreSQL |
| HTTP client (federation) | [reqwest](https://crates.io/crates/reqwest) — outbound calls to remote SPARQL endpoints (SERVICE keyword) |
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

Datalog reasoning is coming in a later milestone — see the roadmap below.

---

## Roadmap

18 releases from v0.1.0 to v1.0.0, estimated at 98–131 person-weeks total.

| Version | Name | What it delivers | Effort | Status |
|---|---|---|---|---|
| **0.1.0** | **Foundation** | Dictionary encoding, VP storage, basic triple CRUD | 6–8 pw | ✅ Done |
| **0.2.0** | **Bulk Loading & Named Graphs** | Turtle/N-Triples/N-Quads/TriG import, named graphs, rare-predicate table | 6–8 pw | ✅ Done |
| **0.3.0** | **SPARQL Basic** | SELECT, ASK, BGPs, FILTER, OPTIONAL, GRAPH patterns, plan cache | 6–8 pw | ✅ Done |
| **0.4.0** | **RDF-star** | Quoted triples, statement metadata, LPG-ready storage | 8–10 pw | ✅ Done |
| **0.5.0** | **SPARQL Advanced (Query)** | Property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES | 6–8 pw | ✅ Done |
| **0.5.1** | **SPARQL Advanced (Write)** | Inline encoding, CONSTRUCT/DESCRIBE, INSERT/DELETE DATA, full-text search | 6–8 pw | ✅ Done |
| **0.6.0** | **HTAP Architecture** | Concurrent reads/writes, shared-memory dictionary cache | 8–10 pw | ✅ Done |
| **0.7.0** | **SHACL Core** | Constraint shapes, synchronous validation on insert | 4–6 pw | ✅ Done |
| **0.8.0** | **SHACL Advanced** | Complex shapes, async background validation pipeline | 4–6 pw | ✅ Done |
| **0.9.0** | **Serialization** | Turtle/N-Triples/JSON-LD/RDF-XML export, RDF-star formats | 3–4 pw | ✅ Done |
| 0.10.0 | Datalog Reasoning | RDFS (13 rules), OWL 2 RL (~80 rules), custom rules | 10–12 pw | Planned |
| 0.11.0 | SPARQL & Datalog Views | Incremental live views via pg_trickle, ExtVP | 5–7 pw | Planned |
| 0.12.0 | SPARQL Update (Advanced) | DELETE/INSERT WHERE, LOAD, CLEAR, DROP, CREATE | 3–4 pw | Planned |
| 0.13.0 | Performance | BSBM benchmarks, prepared statements, planner statistics | 6–8 pw | Planned |
| 0.14.0 | Admin & Security | Graph-level RLS, vacuum/reindex, packaging, full docs | 4–6 pw | Planned |
| 0.15.0 | SPARQL Protocol | Standard W3C HTTP endpoint (`pg_ripple_http` binary) | 3–4 pw | Planned |
| 0.16.0 | SPARQL Federation | `SERVICE` keyword, parallel remote endpoint queries | 4–6 pw | Planned |
| 1.0.0 | Production Release | W3C conformance, stress testing, security audit | 6–8 pw | Planned |

See [ROADMAP.md](ROADMAP.md) for deliverables and exit criteria for every release.

### Beyond 1.0

Planned future directions: distributed storage (Citus), vector + graph hybrid search (pgvector), temporal queries (TimescaleDB), GeoSPARQL (PostGIS), Cypher/GQL query language, and R2RML virtual graphs.

---

## Quality & Testing

pg_ripple aims for production-grade quality:

- **Unit tests** — pgrx `#[pg_test]` for every SQL-exposed function, property-based testing with `proptest`
- **Integration tests** — 30+ pg_regress test files covering every feature
- **Security testing** — SQL injection prevention, malformed input resilience, resource exhaustion defence
- **Fuzz testing** — continuous fuzzing of the SPARQL→SQL pipeline with `cargo-fuzz`
- **Concurrency testing** — dictionary cache correctness, merge worker data integrity under concurrent writes
- **Performance regression CI** — automated benchmarks fail the build on >10% throughput regression
- **W3C conformance** — SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites
- **Stability hardening** — 72-hour soak test, Valgrind memory leak detection, crash recovery testing

---

## Documentation

| Document | Description |
|---|---|
| [ROADMAP.md](ROADMAP.md) | Version-by-version release plan with deliverables and effort estimates |
| [Implementation Plan](plans/implementation_plan.md) | Detailed technical architecture, module breakdown, and data flow |
| [Datalog Design](plans/ecosystem/datalog.md) | Reasoning engine: syntax, stratification, SQL compilation, built-in rules |
| [pg_trickle Integration](plans/ecosystem/pg_trickle.md) | IVM, SPARQL views, ExtVP, and live statistics via stream tables |
| [Cypher/GQL Analysis](plans/cypher/) | Exploratory analysis for post-1.0 Cypher/GQL query language support |

---

## Contributing

pg_ripple is in early development. Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
