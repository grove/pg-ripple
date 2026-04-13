# pg_triple

**A high-performance RDF triple store inside PostgreSQL.**

pg_triple is a PostgreSQL 18 extension that turns your database into a fully-featured knowledge graph. Store RDF data, query it with SPARQL, enforce data quality with SHACL, and reason over it with Datalog — all without leaving PostgreSQL.

```sql
-- Install and start using
CREATE EXTENSION pg_triple;

-- Load some data
SELECT pg_triple.load_turtle('
  @prefix ex: <http://example.org/> .
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .

  ex:Alice foaf:knows ex:Bob .
  ex:Alice foaf:name "Alice" .
  ex:Bob   foaf:name "Bob" .
  ex:Bob   foaf:knows ex:Carol .
');

-- Query with SPARQL
SELECT * FROM pg_triple.sparql('
  SELECT ?name WHERE {
    ex:Alice foaf:knows+ ?person .
    ?person foaf:name ?name .
  }
');
-- Returns: Alice → knows → Bob → knows → Carol
-- Result: [{"name": "Bob"}, {"name": "Carol"}]
```

---

## Why pg_triple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_triple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_triple

### How it compares

| Capability | pg_triple | Blazegraph | Virtuoso | Apache Fuseki |
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

## Key Features

### Standard RDF storage *(planned — v0.1.0)*

Store triples and quads using the standard RDF data model. Every IRI, blank node, and literal is dictionary-encoded to a compact 64-bit integer for fast joins and minimal storage.

```sql
SELECT pg_triple.insert_triple(
  'http://example.org/Alice',
  'http://xmlns.com/foaf/0.1/knows',
  'http://example.org/Bob'
);
```

### SPARQL query engine *(planned — v0.3.0 basic, v0.5.0–v0.5.1 advanced)*

Full SPARQL 1.1 support — SELECT, ASK, CONSTRUCT, DESCRIBE, property paths, aggregates, subqueries, UNION, OPTIONAL, FILTER, BIND, VALUES, and full-text search. Basic graph patterns, FILTER, and OPTIONAL land in v0.3.0; property paths, aggregates, and subqueries in v0.5.0; inline value encoding, CONSTRUCT/DESCRIBE, full-text search, and basic write support in v0.5.1.

```sql
-- Find everyone Alice can reach through "knows" links (any depth)
SELECT * FROM pg_triple.sparql('
  SELECT ?person ?name WHERE {
    ex:Alice foaf:knows+ ?person .
    ?person foaf:name ?name .
  }
  ORDER BY ?name
');
```

### SPARQL Update *(planned — v0.5.1 basic, v0.12.0 advanced)*

Basic write operations (INSERT DATA, DELETE DATA) land in v0.5.1, enabling standard RDF tools to write to pg_triple. Pattern-based updates (DELETE/INSERT WHERE), LOAD, CLEAR, DROP, and CREATE complete the full SPARQL 1.1 Update specification in v0.12.0.

### SHACL data quality *(planned — v0.7.0 core, v0.8.0 advanced)*

Define data integrity rules using the W3C SHACL standard. Constraints are enforced at insert time (synchronous mode) or checked in the background (asynchronous mode).

```sql
-- Load a shape that requires every Person to have exactly one email
SELECT pg_triple.load_shacl('
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  @prefix ex: <http://example.org/> .

  ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
      sh:path ex:email ;
      sh:minCount 1 ;
      sh:maxCount 1 ;
    ] .
');
```

### Datalog reasoning *(planned — v0.10.0)*

Automatically derive new facts from rules and logic. Ships with built-in RDFS (13 rules) and OWL 2 RL (~80 rules) entailment. Write your own rules in a Turtle-flavoured Datalog syntax.

```sql
-- Load RDFS entailment rules
SELECT pg_triple.load_rules_builtin('rdfs');

-- Now SPARQL queries automatically infer subclass relationships:
-- If Dog rdfs:subClassOf Animal, and Rex rdf:type Dog,
-- then Rex rdf:type Animal is inferred
```

### SPARQL Protocol (HTTP) *(planned — v0.15.0)*

A companion HTTP service (`pg_triple_http`) exposes a standard W3C SPARQL 1.1 Protocol endpoint, so web applications, YASGUI, Postman, and any SPARQL client can query pg_triple over HTTP with full content negotiation.

### SPARQL Federation *(planned — v0.16.0)*

Query remote SPARQL endpoints from within pg_triple queries using the standard `SERVICE` keyword. Multiple remote calls execute in parallel.

```sql
SELECT * FROM pg_triple.sparql('
  SELECT ?person ?abstract WHERE {
    ?person ex:worksAt ex:AcmeCorp .
    SERVICE <https://dbpedia.org/sparql> {
      ?person dbo:abstract ?abstract .
      FILTER(LANG(?abstract) = "en")
    }
  }
');
```

### RDF-star / RDF 1.2 *(planned — v0.4.0)*

Make statements about statements — essential for provenance, temporal annotations, and trust.

```sql
SELECT pg_triple.load_turtle('
  << ex:Alice ex:knows ex:Bob >> ex:assertedBy ex:Carol ;
                                  ex:assertedOn "2024-01-15"^^xsd:date .
');
```

### Named graphs with access control *(planned — v0.2.0 graphs, v0.14.0 RLS)*

Organise facts into named graphs, then control access per graph using PostgreSQL's Row-Level Security.

```sql
SELECT pg_triple.grant_graph('analyst_role', 'http://example.org/public-data', 'read');
SELECT pg_triple.grant_graph('admin_role', 'http://example.org/internal', 'admin');
```

### Incremental SPARQL views *(planned — v0.11.0)*

Pin a SPARQL query as a live view that updates incrementally when the underlying data changes — no full recomputation. Requires the companion [pg_trickle](https://github.com/grove/pg-trickle) extension.

```sql
SELECT pg_triple.create_sparql_view(
  'active_employees',
  'SELECT ?name ?dept WHERE {
     ?p rdf:type ex:Employee .
     ?p foaf:name ?name .
     ?p ex:department ?dept .
   }',
  '5s'  -- refresh interval
);

-- Queries against the view are sub-millisecond table scans
SELECT * FROM _pg_triple.sparql_view_active_employees;
```

---

## Architecture

pg_triple is built from the ground up for performance:

```
 SPARQL Query / Update                   HTTP API
        │                                   │
        ▼                                   ▼
 ┌─────────────────┐              ┌──────────────────┐
 │  SPARQL Parser   │              │  pg_triple_http   │
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

- **Dictionary encoding**: Every IRI, blank node, and literal is mapped to a 64-bit integer via XXH3-128 hashing. All joins operate on integers — no string comparisons in the hot path.
- **Vertical partitioning**: Each predicate gets its own table (`_pg_triple.vp_{id}`) with columns `(s, o, g)`. This means queries that bind a predicate touch only one compact, heavily-indexed table.
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
| RDF parsers | [rio_turtle](https://crates.io/crates/rio_turtle), [rio_xml](https://crates.io/crates/rio_xml) — Turtle, N-Triples, RDF/XML; [oxttl](https://crates.io/crates/oxttl) / [oxrdf](https://crates.io/crates/oxrdf) added at v0.4.0 for RDF-star (Turtle-star, N-Triples-star) |
| Hashing | [xxhash-rust](https://crates.io/crates/xxhash-rust) (XXH3-128) — fast non-cryptographic hash for dictionary dedup |
| Serialization | [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) — SHACL reports, SPARQL results, config |
| HTTP server | [axum](https://crates.io/crates/axum) (built on [tokio](https://tokio.rs/)) — SPARQL Protocol HTTP endpoint (`pg_triple_http` binary) |
| PG client (HTTP service) | [tokio-postgres](https://crates.io/crates/tokio-postgres) + [deadpool-postgres](https://crates.io/crates/deadpool-postgres) — async connection pool from HTTP service to PostgreSQL |
| HTTP client (federation) | [reqwest](https://crates.io/crates/reqwest) — outbound calls to remote SPARQL endpoints (SERVICE keyword) |
| IVM / stream tables | [pg_trickle](https://github.com/grove/pg-trickle) *(optional companion extension)* — incremental SPARQL views, ExtVP, live statistics |
| Testing | pgrx `#[pg_test]`, `cargo pgrx regress`, [proptest](https://crates.io/crates/proptest), [cargo-fuzz](https://crates.io/crates/cargo-fuzz) |

---

## Project Status

pg_triple is in the **design and planning phase**. No code has been written yet. The architecture, roadmap, and implementation plan are documented and ready for development.

See the [Roadmap](ROADMAP.md) for the full release plan (v0.1.0 through v1.0.0) and the [Implementation Plan](plans/implementation_plan.md) for detailed technical design.

---

## Getting Started

> **Note**: pg_triple is not yet released. The instructions below describe the intended installation workflow.

### Prerequisites

- PostgreSQL 18
- Rust toolchain (stable)
- [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.17

### Build and install

```bash
# Clone the repository
git clone https://github.com/grove/pg_triple.git
cd pg_triple

# Initialise pgrx for PostgreSQL 18
cargo pgrx init --pg18 $(which pg_config)

# Run tests
cargo pgrx test pg18

# Install into your local PostgreSQL
cargo pgrx install --pg-config $(which pg_config)
```

### Enable the extension

```sql
CREATE EXTENSION pg_triple;
```

### Load some data

```sql
SELECT pg_triple.load_turtle('
  @prefix ex: <http://example.org/> .
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .

  ex:Alice a foaf:Person ;
    foaf:name "Alice" ;
    foaf:knows ex:Bob .

  ex:Bob a foaf:Person ;
    foaf:name "Bob" .
');
```

### Query with SPARQL

```sql
SELECT * FROM pg_triple.sparql('
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    ?person a foaf:Person .
    ?person foaf:name ?name .
  }
');
```

---

## Roadmap

pg_triple is planned as 18 incremental releases from v0.1.0 to v1.0.0 (~98–131 person-weeks):

| Phase | Versions | What you get |
|---|---|---|
| **Foundation** | 0.1.0 – 0.2.0 | Store triples, bulk import, VP storage, named graphs, statement identifiers |
| **Query (Basic)** | 0.3.0 | SPARQL SELECT and ASK with BGPs, FILTER, OPTIONAL, GRAPH patterns |
| **RDF-star** | 0.4.0 | Quoted triples, statement-level metadata, LPG-ready storage |
| **Query (Advanced)** | 0.5.0 – 0.5.1 | Property paths, aggregates, subqueries, inline encoding, CONSTRUCT/DESCRIBE, INSERT DATA/DELETE DATA, full-text search |
| **Concurrency** | 0.6.0 | HTAP architecture — reads and writes at full speed, shared-memory cache |
| **Data quality** | 0.7.0 – 0.8.0 | SHACL validation (sync + async), complex shapes |
| **Interop** | 0.9.0 | RDF/XML import, Turtle/JSON-LD export, RDF-star serialization |
| **Intelligence** | 0.10.0 | Datalog reasoning (RDFS, OWL RL, custom rules), constraint rules |
| **Reactivity** | 0.11.0 | Incremental SPARQL & Datalog views, ExtVP (requires pg_trickle) |
| **Writes (Advanced)** | 0.12.0 | Pattern-based SPARQL Update — DELETE/INSERT WHERE, LOAD, CLEAR, DROP |
| **Production** | 0.13.0 – 0.14.0 | Performance tuning, BSBM benchmarks, admin tools, graph-level RLS, docs |
| **Ecosystem** | 0.15.0 – 0.16.0 | HTTP SPARQL Protocol, SPARQL Federation |
| **Release** | 1.0.0 | W3C conformance, stress testing, security audit |

See the full [Roadmap](ROADMAP.md) for details on every release.

### Beyond 1.0

Planned future directions include distributed storage (Citus), vector + graph hybrid search (pgvector), temporal queries (TimescaleDB), GeoSPARQL (PostGIS), Cypher/GQL query language, and R2RML virtual graphs. See the [Roadmap](ROADMAP.md) for the full post-1.0 horizon.

---

## Quality & Testing

pg_triple aims for production-grade quality:

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

pg_triple is in early development. Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
