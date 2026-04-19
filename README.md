# pg-ripple

[![CI](https://github.com/grove/pg-ripple/actions/workflows/ci.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/ci.yml)
[![Release](https://github.com/grove/pg-ripple/actions/workflows/release.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/release.yml)
[![Roadmap](https://img.shields.io/badge/Roadmap-view-informational)](ROADMAP.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![PostgreSQL 18](https://img.shields.io/badge/PostgreSQL-18-blue?logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![pgrx 0.17](https://img.shields.io/badge/pgrx-0.17-orange)](https://github.com/pgcentralfoundation/pgrx)

**A knowledge graph engine built into PostgreSQL.**

pg_ripple is a PostgreSQL 18 extension that turns your database into a knowledge graph store. You can model data as a web of connected facts — entities, relationships, and properties — and then query, validate, and reason over those connections, all from within the database you already run.

No separate graph database. No data pipelines. No extra infrastructure.

> **New to knowledge graphs?** Think of a knowledge graph as a smarter, more connected way to store data. Instead of rows in tables, you store facts: *Alice knows Bob*, *Bob works at Acme Corp*, *Acme Corp is in Oslo*. You can then ask questions that span many hops: *"Who are all the people in Alice's extended professional network?"* — the kind of question that is painful in SQL but natural in a graph.

---

## What works today (v0.31.0)

pg_ripple passes **100% of the W3C SPARQL 1.1 and SHACL Core conformance test suites** — the industry benchmarks for correctness in knowledge graph systems. After 31 releases it covers the full feature set described below.

| What you can do | How it works |
|---|---|
| **Import knowledge** | Load data in standard formats: Turtle, N-Triples, N-Quads, TriG, or RDF/XML — from files, inline text, or remote URLs. Named graphs let you organize facts into logical groups (e.g. one graph per data source or topic). |
| **Query with SPARQL** | Ask complex questions using SPARQL 1.1 — the W3C standard query language for linked data (similar to SQL, but designed for graphs). Follow chains of relationships, apply filters, aggregate results, and query across multiple graphs. Fully W3C conformant. |
| **AI and LLM integration** | Store vector embeddings alongside graph facts. Combine semantic similarity search (*"find things similar to X"*) with SPARQL graph traversal in one query. Built-in RAG pipeline retrieves graph-contextualized context for language model prompts. Use `sparql_construct_jsonld()` with a JSON-LD frame to generate structured, token-efficient system prompts directly from a SPARQL CONSTRUCT query. |
| **Microsoft GraphRAG** | Export entities and relationships in GraphRAG's BYOG (Bring Your Own Graph) Parquet format. Enrich the graph with Datalog rules. Validate export quality with SHACL. Connect your knowledge graph to Microsoft's GraphRAG pipeline with a single SQL call. |
| **Validate data quality** | Define quality rules with SHACL: *"every Person must have exactly one name"*, *"age must be a positive integer"*. Violations are caught on insert (immediate feedback) or checked in the background. Fully W3C conformant. |
| **Infer new facts automatically** | Write Datalog rules to derive conclusions from what you already know — *"if Alice manages Bob and Bob manages Carol, then Alice indirectly manages Carol"*. Includes built-in support for standard RDFS and OWL reasoning. Goal-directed mode (`infer_goal()`) and demand-filtered mode (`infer_demand()`) derive only the facts relevant to your query, reducing inference work by 50–90% on large programs. `owl:sameAs` entity canonicalization is applied automatically before inference, so equivalent entities are treated as one. |
| **Export and share** | Export your graph as Turtle, N-Triples, JSON-LD, or RDF/XML. Use JSON-LD framing to produce nested documents shaped for REST APIs or LLM prompts. |
| **Standard HTTP endpoint** | The companion `pg_ripple_http` service exposes a W3C SPARQL Protocol endpoint over HTTP/HTTPS. Supports JSON, XML, CSV, Turtle, and JSON-LD responses; authentication; Prometheus metrics; and Docker Compose for easy deployment. |
| **Query remote graph services** | Use the SPARQL `SERVICE` keyword to query external SPARQL endpoints as part of a single query — your local data and a remote public dataset in one request. Includes connection pooling, result caching, and safe timeouts. |
| **Live, auto-updating views** | Define a SPARQL query as a view; pg_ripple (with the optional `pg_trickle` companion) keeps it automatically up to date as data changes. |
| **Access control** | Named graphs have row-level security backed by PostgreSQL's built-in permission system. Each graph can be granted to specific database roles, just like a table. |
| **Full-text search** | Search the text of literal values (names, descriptions, notes) using PostgreSQL's fast full-text search indexes. |

Here is a taste of what working with pg_ripple looks like from SQL:

```sql
CREATE EXTENSION pg_ripple;

-- Import a Turtle file (a standard text format for RDF knowledge graphs)
SELECT pg_ripple.load_turtle(pg_read_file('/data/people.ttl'));

-- Query with a property path: find everyone Alice can reach via "knows"
-- (follows the chain Alice→Bob→Carol→… automatically)
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
-- After this, if :Dog is a subclass of :Animal, and :Rex is a Dog,
-- then SPARQL will also return :Rex when you ask for Animals.
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');

-- Write custom rules (transitive management chain)
SELECT pg_ripple.load_rules(
  '?x ex:indirectManager ?z :- ?x ex:manager ?z .
   ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .',
  'org_rules'
);
SELECT pg_ripple.infer('org_rules');

-- ── AI / LLM integration ──────────────────────────────────────────────

-- Hybrid retrieval: graph pattern + vector similarity in one query
-- Find papers semantically similar to a topic, authored by co-authors
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  PREFIX pg:  <http://pg-ripple.io/fn/>
  SELECT ?paper ?title ?score WHERE {
    <http://example.org/Alice> ex:coAuthor+ ?colleague .
    ?colleague ex:authored ?paper .
    ?paper ex:title ?title .
    BIND(pg:similar(?paper, "graph neural networks") AS ?score)
    FILTER(?score > 0.75)
  }
  ORDER BY DESC(?score)
');

-- Generate a structured JSON-LD system prompt for an LLM
-- The frame shapes the output to exactly the JSON your prompt template expects
SELECT pg_ripple.sparql_construct_jsonld(
  'CONSTRUCT { ?s ex:name ?name ; ex:role ?role ; ex:manages ?report }
   WHERE   { ?s a ex:Person ; ex:name ?name ; ex:role ?role .
             OPTIONAL { ?s ex:manages ?report } }',
  -- JSON-LD frame: produces nested {"name":..., "manages":[...]} objects
  '{"@type": "ex:Person", "ex:manages": {}}'
);

-- Graph-contextualized RAG retrieval
-- Returns a JSONB context block ready for use as an LLM system prompt
SELECT pg_ripple.rag_retrieve(
  query_embedding  => ai.embed('Who manages the Oslo team?'),
  graph_patterns   => ARRAY['?s ex:locatedIn ex:Oslo', '?s ex:role ?role'],
  top_k            => 10
);
```

---

## AI and LLM use cases

pg_ripple is a natural fit for AI applications that need structured, explainable context — not just a bag of vectors. Here are three concrete scenarios.

### Knowledge-augmented RAG

Pure vector search finds *similar* documents but loses the *relationships* between them. pg_ripple lets you combine both: a SPARQL graph pattern selects entities by relationship ("papers authored by Alice's co-authors in the last two years"), and a vector similarity filter (`pg:similar()`) ranks them by semantic closeness to the query. Reciprocal Rank Fusion merges the two result lists. The retrieval context sent to the LLM is more precise and more explainable than a flat top-k vector search.

### Entity resolution before embedding

Enterprise data has duplicates: `"Alice Smith"`, `"A. Smith"`, and `"alice.smith@example.com"` may all refer to the same person. pg_ripple's `owl:sameAs` entity canonicalization collapses these into a single canonical entity before inference or embedding. When the LLM asks about Alice, it gets a unified view — not three contradictory fragments.

### Structured prompts via JSON-LD framing

Token budgets matter. `sparql_construct_jsonld()` takes a SPARQL CONSTRUCT query and a JSON-LD frame — a template describing the exact shape of JSON you want — and produces a compact, structured prompt context with no redundant triples, no flat dumps, and no post-processing needed. The frame defines which properties to include, in what order, and how to nest them. The output plugs directly into a system prompt.

---

## Where we're headed

Two releases remain on the path to v1.0.0.

### v0.32.0 — Well-Founded Semantics & Tabling

The next release extends pg_ripple's Datalog engine to handle programs with mutual negation — the edge cases that stratified Datalog cannot resolve — using well-founded semantics (three-valued logic: true / false / unknown). It also adds tabling: a session-scoped cache that stores derived sub-goals so repeated sub-queries (in Datalog or SPARQL) are computed once and reused. For analytical workloads with repeated sub-query patterns, tabling delivers a 2–5× speedup.

### v1.0.0 — Production Release

With 100% W3C conformance achieved, GraphRAG integration complete, vector + SPARQL hybrid search in place, entity resolution, and demand-filtered Datalog reasoning delivered, the final release focuses on production hardening: a full 72-hour continuous load test, final security audit sign-off, stress testing at 100 M+ triple scale, and a hardened upgrade path from every prior version. This is the version intended for production deployments.

---

## Why pg_ripple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_ripple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_ripple

### How it compares

> **Note**: pg_ripple features marked "Yes" in the table below are implemented across v0.1.0–v0.31.0. W3C SPARQL 1.1 Query, Update, and SHACL Core conformance is 100% (achieved in v0.20.0). Competitor capabilities reflect publicly documented feature sets.

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

pg_ripple is built from the ground up for performance inside PostgreSQL.

> The diagram below shows the internal pipeline: a query enters as SPARQL text, is optimised, translated to SQL, and executed against the storage layer — all inside a single PostgreSQL session.

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

### How data is stored

- **Compact IDs for everything**: every value — URIs, labels, literals — is assigned a short integer ID. Internal joins use these integers, not raw strings, which keeps storage small and queries fast.
- **One table per relationship type**: facts about `worksAt`, `knows`, `birthDate`, etc. are stored in separate tables. A query asking only about `worksAt` scans only that table, not your entire dataset.
- **Separate lanes for reads and writes**: new data goes into a fast "delta" area; a background worker continuously moves it to an optimised "main" area. Heavy insert workloads and complex queries never slow each other down.

### Performance targets

| Operation | Target | At scale |
|---|---|---|
| Bulk load | >100,000 facts/sec | Batch import with deferred indexing |
| Transactional insert | >10,000 facts/sec | Delta partition, async validation |
| Simple query | <5 ms | 10 million facts |
| Multi-hop query (5 patterns) | <20 ms | 10 million facts |
| Deep path traversal (depth 10) | <100 ms | 10 million facts |
| Dictionary lookup (cache hit) | <1 μs | Sharded in-memory cache |

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
- Rust stable toolchain (pg_ripple is a compiled extension)
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

pg_ripple is built to production-grade standards:

- **W3C conformance** — 100% pass rate on the official SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites
- **Extensive test suite** — automated tests cover every SQL-exposed function, every feature, and every edge case
- **Security testing** — resistance to injection attacks, malformed inputs, and resource exhaustion
- **Fuzz testing** — the query pipeline is continuously fuzz-tested for robustness
- **Performance regression CI** — automated benchmarks fail the build if throughput drops by more than 10%
- **Stability** — 72-hour soak test, memory leak detection, and crash recovery testing

---

## Contributing

Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
