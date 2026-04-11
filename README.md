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

### Standard RDF storage

Store triples and quads using the standard RDF data model. Every IRI, blank node, and literal is dictionary-encoded to a compact 64-bit integer for fast joins and minimal storage.

```sql
SELECT pg_triple.insert_triple(
  'http://example.org/Alice',
  'http://xmlns.com/foaf/0.1/knows',
  'http://example.org/Bob'
);
```

### SPARQL query engine

Full SPARQL 1.1 support — SELECT, ASK, CONSTRUCT, DESCRIBE, property paths, aggregates, subqueries, UNION, OPTIONAL, FILTER, BIND, VALUES, and full-text search.

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

### SPARQL Update

Standard write operations — INSERT DATA, DELETE DATA, DELETE/INSERT WHERE, LOAD, CLEAR, DROP, CREATE — so existing RDF tools (Protégé, TopBraid, SPARQL workbenches) work without adapters.

### SHACL data quality

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

### Datalog reasoning

Automatically derive new facts from rules and logic. Ships with built-in RDFS (13 rules) and OWL 2 RL (~80 rules) entailment. Write your own rules in a Turtle-flavoured Datalog syntax.

```sql
-- Load RDFS entailment rules
SELECT pg_triple.load_rules_builtin('rdfs');

-- Now SPARQL queries automatically infer subclass relationships:
-- If Dog rdfs:subClassOf Animal, and Rex rdf:type Dog,
-- then Rex rdf:type Animal is inferred
```

### SPARQL Protocol (HTTP)

A companion HTTP service (`pg_triple_http`) exposes a standard W3C SPARQL 1.1 Protocol endpoint, so web applications, YASGUI, Postman, and any SPARQL client can query pg_triple over HTTP with full content negotiation.

### SPARQL Federation

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

### RDF-star / RDF 1.2

Make statements about statements — essential for provenance, temporal annotations, and trust.

```sql
SELECT pg_triple.load_turtle('
  << ex:Alice ex:knows ex:Bob >> ex:assertedBy ex:Carol ;
                                  ex:assertedOn "2024-01-15"^^xsd:date .
');
```

### Named graphs with access control

Organise facts into named graphs, then control access per graph using PostgreSQL's Row-Level Security.

```sql
SELECT pg_triple.grant_graph('analyst_role', 'http://example.org/public-data', 'read');
SELECT pg_triple.grant_graph('admin_role', 'http://example.org/internal', 'admin');
```

### Incremental SPARQL views

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
| SPARQL parser | [spargebra](https://crates.io/crates/spargebra) |
| RDF parsers | [rio_turtle](https://crates.io/crates/rio_turtle), [rio_xml](https://crates.io/crates/rio_xml) |
| Hashing | [xxhash-rust](https://crates.io/crates/xxhash-rust) (XXH3-128) |
| HTTP service | [tokio](https://tokio.rs/) + [tokio-postgres](https://crates.io/crates/tokio-postgres) + [deadpool-postgres](https://crates.io/crates/deadpool-postgres) |

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

pg_triple is planned as 17 incremental releases from v0.1.0 to v1.0.0:

| Phase | Versions | What you get |
|---|---|---|
| **Foundation** | 0.1.0 – 0.2.0 | Store triples, bulk import, vertical partitioning |
| **Query** | 0.3.0 – 0.4.0 | Full SPARQL 1.1 querying (BGPs, paths, aggregates, FTS) |
| **Concurrency** | 0.5.0 | HTAP architecture — reads and writes at full speed |
| **Data quality** | 0.6.0 – 0.7.0 | SHACL validation (sync + async) |
| **Interop** | 0.8.0 | All standard RDF file formats |
| **Intelligence** | 0.9.0 | Datalog reasoning (RDFS, OWL RL, custom rules) |
| **Reactivity** | 0.10.0 | Incremental SPARQL views |
| **Writes** | 0.11.0 | Standard SPARQL Update operations |
| **Production** | 0.12.0 – 0.13.0 | Performance tuning, admin tools, security, docs |
| **Ecosystem** | 0.14.0 – 0.16.0 | HTTP protocol, federation, RDF-star |
| **Release** | 1.0.0 | W3C conformance, stress testing, security audit |

See the full [Roadmap](ROADMAP.md) for details on every release.

### Beyond 1.0

Planned future directions include distributed storage (Citus), vector + graph hybrid search (pgvector), temporal queries (TimescaleDB), GeoSPARQL (PostGIS), and R2RML virtual graphs.

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

---

## Contributing

pg_triple is in early development. Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
