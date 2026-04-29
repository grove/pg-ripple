# pg-ripple

[![CI](https://github.com/grove/pg-ripple/actions/workflows/ci.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/ci.yml)
[![Release](https://github.com/grove/pg-ripple/actions/workflows/release.yml/badge.svg)](https://github.com/grove/pg-ripple/actions/workflows/release.yml)
[![Roadmap](https://img.shields.io/badge/Roadmap-view-informational)](ROADMAP.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![PostgreSQL 18](https://img.shields.io/badge/PostgreSQL-18-blue?logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![pgrx 0.18](https://img.shields.io/badge/pgrx-0.18-orange)](https://github.com/pgcentralfoundation/pgrx)
[![WatDiv correctness](https://img.shields.io/badge/WatDiv-100%25%20correct-brightgreen)](docs/src/reference/watdiv-results.md)
[![LUBM conformance](https://img.shields.io/badge/LUBM-14%2F14%20pass-brightgreen)](docs/src/reference/lubm-results.md)

**A knowledge graph engine built into PostgreSQL.**

pg_ripple is a PostgreSQL 18 extension that turns your database into a knowledge graph store. You can model data as a web of connected facts — entities, relationships, and properties — and then query, validate, and reason over those connections, all from within the database you already run.

No separate graph database. No data pipelines. No extra infrastructure.

> **New to knowledge graphs?** Think of a knowledge graph as a smarter, more connected way to store data. Instead of rows in tables, you store facts: *Alice knows Bob*, *Bob works at Acme Corp*, *Acme Corp is in Oslo*. You can then ask questions that span many hops: *"Who are all the people in Alice's extended professional network?"* — the kind of question that is painful in SQL but natural in a graph.

---

## What works today (v0.71.0)

pg_ripple passes **100% of the W3C SPARQL 1.1, SHACL Core, and OWL 2 RL conformance test suites** — the industry benchmarks for correctness in knowledge graph systems. After 67 releases it covers the full feature set described below.

| What you can do | How it works |
|---|---|
| **Import knowledge** | Load data in standard formats: Turtle, N-Triples, N-Quads, TriG, or RDF/XML — from files, inline text, or remote URLs. Named graphs let you organize facts into logical groups (e.g. one graph per data source or topic). |
| **Query with SPARQL** | Ask complex questions using SPARQL 1.1 — the W3C standard query language for linked data (similar to SQL, but designed for graphs). Follow chains of relationships, apply filters, aggregate results, and query across multiple graphs. Fully W3C conformant. Configurable DoS limits (`pg_ripple.sparql_max_algebra_depth`, `pg_ripple.sparql_max_triple_patterns`) reject malformed deep queries at parse time. |
| **AI and LLM integration** | Store vector embeddings alongside graph facts. Combine semantic similarity search (*"find things similar to X"*) with SPARQL graph traversal in one query. Built-in RAG pipeline retrieves graph-contextualized context for language model prompts. Use `sparql_construct_jsonld()` with a JSON-LD frame to generate structured, token-efficient system prompts directly from a SPARQL CONSTRUCT query. |
| **Microsoft GraphRAG** | Export entities and relationships in GraphRAG's BYOG (Bring Your Own Graph) Parquet format. Enrich the graph with Datalog rules. Validate export quality with SHACL. Connect your knowledge graph to Microsoft's GraphRAG pipeline with a single SQL call. |
| **Validate data quality** | Define quality rules with SHACL: *"every Person must have exactly one name"*, *"age must be a positive integer"*. Violations are caught on insert (immediate feedback) or checked in the background. Full SHACL Core conformance, including `sh:equals`, `sh:disjoint`, and complex property path traversal (inverse, alternative, sequence, zero-or-more, one-or-more). Violation reports include decoded focus-node IRIs for easy debugging. |
| **Infer new facts automatically** | Write Datalog rules to derive conclusions from what you already know — *"if Alice manages Bob and Bob manages Carol, then Alice indirectly manages Carol"*. Includes built-in support for standard RDFS and OWL reasoning. Goal-directed mode (`infer_goal()`) and demand-filtered mode (`infer_demand()`) derive only the facts relevant to your query, reducing inference work by 50–90% on large programs. `owl:sameAs` entity canonicalization is applied automatically before inference, so equivalent entities are treated as one. Well-founded semantics (`infer_wfs()`) handles non-stratifiable programs with mutual negation. Tabling caches repeated inference sub-goals (2–5× speedup). Parallel stratum evaluation runs independent rule groups concurrently. Worst-case optimal joins accelerate cyclic graph queries. Incremental retraction (DRed) keeps derived predicates consistent after deletions without full recomputation. |
| **Stream and inspect queries** | Use `sparql_cursor()` to stream large result sets page-by-page via the PostgreSQL portal API — peak memory is bounded by `pg_ripple.export_batch_size`, not the full result set. Export results as W3C CSV or TSV via `sparql_csv()` / `sparql_tsv()`. Use `explain_sparql()` and `explain_datalog()` to introspect query plans and rule compilation. Pass `citus := true` to `explain_sparql()` for a Citus shard-pruning section showing which shard the query was pruned to and how many rows were avoided. `streaming_metrics()` returns live atomic counters for cursor pages, Arrow batches, and ticket rejections. OpenTelemetry span tracing is available, with a configurable OTLP endpoint (`pg_ripple.tracing_otlp_endpoint`). |
| **Live change notifications** | Subscribe to graph changes with `pg_ripple.create_subscription(name, filter_sparql)`. pg_ripple notifies your application via a PostgreSQL NOTIFY channel (`pg_ripple_cdc_{name}`) whenever matching triples are added or removed — no polling required. CDC lifecycle events (`pg_ripple.cdc_lifecycle_events`) record subscription creation, deletion, and error events. |
| **Export and share** | Export your graph as Turtle, N-Triples, JSON-LD, or RDF/XML. Use JSON-LD framing to produce nested documents shaped for REST APIs or LLM prompts. `COPY rdf FROM` loads bulk RDF files directly via PostgreSQL's COPY protocol. Arrow IPC bulk export via `pg_ripple_http` (experimental): HMAC-SHA256 signed tickets, binary IPC stream over `POST /flight/do_get`, authenticated endpoint. |
| **Standard HTTP endpoint** | The companion `pg_ripple_http` service exposes a W3C SPARQL Protocol endpoint over HTTP/HTTPS. Supports JSON, XML, CSV, Turtle, and JSON-LD responses; authentication; Prometheus metrics; Docker Compose for easy deployment; full OpenAPI 3.1 specification; and an Arrow/Flight bulk-export endpoint. |
| **Query remote graph services** | Use the SPARQL `SERVICE` keyword to query external SPARQL endpoints as part of a single query — your local data and a remote public dataset in one request. Includes connection pooling, result caching, safe timeouts, and a circuit breaker (`pg_ripple.federation_circuit_breaker_threshold`) that stops retrying failed endpoints. |
| **Horizontal scaling with Citus** | Enable `pg_ripple.citus_sharding_enabled` to distribute VP tables across Citus worker nodes. Bound-subject SPARQL patterns are automatically pruned to the correct shard (10–100× speedup). `citus_rebalance()` emits NOTIFY signals so pg-trickle can pause CDC during rebalancing. `citus_rebalance_progress()` reports live shard-move status. |
| **Temporal RDF queries** | `point_in_time(ts TIMESTAMPTZ)` restricts all SPARQL queries in the current session to facts that existed at the given timestamp — enabling as-of queries, audit trails, and temporal joins without schema changes. |
| **PROV-O data provenance** | Enable `pg_ripple.prov_enabled` to automatically record W3C PROV-O `prov:Activity` + `prov:Entity` triples for every bulk-load operation. `prov_stats()` summarises load history. |
| **Geospatial queries** | GeoSPARQL 1.1: filter by `geof:within`, `geof:intersects`, and `geof:distance`; compute `geof:buffer`, `geof:convexHull`, `geof:envelope`. Geometry values stored as WKT literals and processed via PostGIS. |
| **OWL 2 EL/QL reasoning profiles** | Activate `load_rules_builtin('owl-el')` or `load_rules_builtin('owl-ql')` for profile-specific reasoning. OWL 2 QL rewrites SPARQL BGPs at translation time for DL-Lite ontologies. Control with `pg_ripple.owl_profile`. |
| **Knowledge graph embeddings** | Enable `pg_ripple.kge_enabled` to train TransE or RotatE entity embeddings stored in `_pg_ripple.kge_embeddings` with an HNSW index. Use `find_alignments()` to propose cross-graph `owl:sameAs` candidates by cosine similarity. |
| **SPARQL audit log** | Enable `pg_ripple.audit_log_enabled` to record all SPARQL UPDATE operations (role, transaction ID, query text) in `_pg_ripple.audit_log`. `purge_audit_log(before)` cleans up old entries. |
| **Multi-tenant graph isolation** | `create_tenant()` registers a named graph with a triple-count quota. Triggers enforce the quota on insert; `tenant_stats()` reports usage per tenant. |
| **SPARQL-DL OWL axiom queries** | `sparql_dl_subclasses(IRI)` and `sparql_dl_superclasses(IRI)` route OWL vocabulary BGPs (`owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`) directly to VP table T-Box data — no separate index required. |
| **SHACL-SPARQL rules** | SHACL Advanced Features: `sh:SPARQLRule` and `sh:SPARQLConstraint` are evaluated as native SPARQL queries against the VP store, enabling complex cross-shape validation that cannot be expressed with pure property-path SHACL. |
| **R2RML direct mapping** | `pg_ripple.r2rml_load(mapping_ttl)` applies an R2RML mapping document to convert relational tables in the same database into RDF triples, inserted directly into the VP store. |
| **Live, auto-updating views** | Define a SPARQL query as a view; pg_ripple (with the optional `pg_trickle` companion) keeps it automatically up to date as data changes. |
| **Access control** | Named graphs have row-level security backed by PostgreSQL's built-in permission system. Each graph can be granted to specific database roles, just like a table. Read-replica routing sends read queries to replicas automatically when `pg_ripple.read_replica_dsn` is configured. |
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

One release remains on the path to v1.0.0.

The v0.64.0–v0.69.0 Assessment Remediation & Release Trust sequence is complete: honest feature-status SQL API (`feature_status()`), GitHub Actions SHA pinning and Docker release digest integrity, CONSTRUCT writeback incremental delta maintenance, true portal-based SPARQL cursor streaming (bounded memory), HMAC-SHA256 signed Arrow Flight tickets, real Arrow IPC streaming in `pg_ripple_http`, 72-hour soak test artifacts, security audit closure, public benchmark baselines, upgrade and backup acceptance tests, operator runbooks, a mandatory release evidence dashboard, and a module restructuring that splits sparql/mod.rs, pg_ripple_http/main.rs, construct_rules.rs, and storage/mod.rs along single-responsibility boundaries (v0.69.0).

### v1.0.0 — Production Release

The final milestone: full API and documentation freeze, long-term support commitment, and public production dossier. All conformance suites (SPARQL 1.1, SHACL Core, OWL 2 RL, LUBM) remain required gates; performance regression CI and the release evidence dashboard are mandatory artifacts.

---

## Why pg_ripple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_ripple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_ripple

### How it compares

> **Note**: pg_ripple features marked "Yes" in the table below are implemented across v0.1.0–v0.71.0. W3C SPARQL 1.1 Query, Update, SHACL Core, and OWL 2 RL conformance is 100%. Competitor capabilities reflect publicly documented feature sets.

| Capability | pg_ripple | Blazegraph | Virtuoso | Apache Fuseki |
|---|---|---|---|---|
| Runs inside PostgreSQL | Yes | No | No | No |
| SPARQL 1.1 Query | Yes | Yes | Yes | Yes |
| SPARQL 1.1 Update | Yes | Yes | Yes | Yes |
| SHACL validation | Yes (sync + async) | No | No | Plugin |
| Datalog reasoning (RDFS, OWL RL) | Yes | No | Limited | Partial |
| Incremental SPARQL views (IVM) | Yes (via pg_trickle) | No | No | No |
| RDF-star / RDF 1.2 | Yes | No | No | Yes |
| Temporal RDF queries | Yes | No | Limited | No |
| Horizontal sharding (Citus) | Yes | No | No | No |
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
| PostgreSQL binding | [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.18 |
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

## pg_trickle dependency matrix

[pg_trickle](https://github.com/grove/pg-trickle) is an optional companion extension.
The table below shows which pg_ripple features require pg_trickle and which ship standalone.

| Feature | Ships standalone | Requires pg_trickle |
|---|---|---|
| SPARQL SELECT / ASK / CONSTRUCT / DESCRIBE | ✓ | — |
| SPARQL UPDATE (INSERT/DELETE/CLEAR/LOAD) | ✓ | — |
| Property paths (ZeroOrMorePath, InversePath, …) | ✓ | — |
| Federation (SERVICE, parallel, cost-based) | ✓ | — |
| SHACL validation (Core + SPARQL constraints) | ✓ | — |
| Datalog rules (RDFS/OWL RL, seminaïve, magic sets) | ✓ | — |
| HTAP merge worker | ✓ | — |
| Bulk load (Turtle / N-Triples / RDF-XML) | ✓ | — |
| GeoSPARQL 1.1 (geof:distance, ST_DWithin, …) | ✓ | — |
| Full-text search (RDF-FTS) | ✓ | — |
| Vector + SPARQL hybrid search | ✓ | — |
| pg_dump / pg_restore round-trip | ✓ | — |
| CDC subscriptions (NOTIFY on triple changes) | ✓ | — |
| Incremental SPARQL views (IVM) | — | ✓ required |
| ExtVP materialised statistics | — | ✓ required |
| Live auto-updating CONSTRUCT views | — | ✓ required |
| Citus rebalance pause/resume during CDC | ✓ (NOTIFY signal) | ✓ (pause/resume logic) |
| Read-replica routing for federation | ✓ | — |

---

## Getting Started

### Prerequisites

- PostgreSQL 18
- Rust stable toolchain (pg_ripple is a compiled extension)
- [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.18

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

## Known limitations in v0.71.0

The following features are available but have documented limitations in the current release. Use `SELECT feature_name, status, degraded_reason FROM pg_ripple.feature_status() WHERE status != 'implemented'` for a machine-readable summary.

| Feature | Status | Detail |
|---|---|---|
| **Arrow Flight bulk export** | Implemented | `/flight/do_get` streams tombstone-excluded Arrow IPC record batches with HMAC-SHA256 signed tickets (v0.67.0 FLIGHT-SEC-02). Streaming validation targeted for v0.71.0. |
| **WCOJ (Worst-Case Optimal Joins)** | Planner hint | WCOJ re-orders cyclic BGP joins at plan time. A true Leapfrog Triejoin executor is not implemented. |
| **SHACL-SPARQL rules (`sh:SPARQLRule`)** | Not supported | When detected, the extension emits a WARNING (PT481) and skips the rule. Full `sh:SPARQLRule` support is targeted for v1.0.0. `sh:TripleRule` and `sh:SPARQLConstraint` are fully supported. |
| **Citus multi-node integration test** | Planned | Citus integration test is planned for v0.71.0 (CITUS-INT-01). |
| **Citus, pgvector, pg_trickle** | Optional | Citus, pgvector, and pg_trickle are optional dependencies. Dependent features degrade gracefully when these extensions are not installed. |

These are not bugs — they are known partial implementations. All limitations are surfaced in `pg_ripple.feature_status()`.

---

## Quality & Testing

pg_ripple is built to production-grade standards:

- **W3C conformance** — 100% pass rate on the official SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites (~3 000 tests, parallelized, complete in under 2 minutes)
- **Apache Jena test suite** — ~1 000 additional tests covering XSD numeric promotions, timezone-aware date/time, blank-node scoping, and all SPARQL string functions
- **WatDiv benchmark** — all 100 WatDiv query templates (star, chain, snowflake, complex) validated for correctness against a 10 M-triple dataset with ±0.1% row-count baselines
- **LUBM conformance suite** — all 14 canonical LUBM queries pass against a synthetic university OWL ontology; includes a Datalog validation sub-suite confirming that `infer('owl-rl')` produces correct supertype entailments (v0.44.0)
- **W3C OWL 2 RL conformance suite** — W3C OWL 2 RL test manifests (entailment, consistency, and inconsistency tests) run in CI; **100% pass rate (66/66) achieved at v0.51.0** — blocking gate in CI (v0.51.0)
- **Property-based testing** — `proptest` suites assert algebraic invariants: SPARQL algebra round-trips produce byte-identical SQL, dictionary encode/decode is always stable and collision-free for 10,000 random distinct terms, JSON-LD framing preserves all matching IRIs (v0.51.0)
- **Extensive test suite** — automated tests cover every SQL-exposed function, every feature, and every edge case
- **Security testing** — resistance to injection attacks, malformed inputs, and resource exhaustion
- **Fuzz testing** — the federation result decoder and query pipeline are continuously fuzz-tested; arbitrary XML/JSON from remote SERVICE endpoints cannot cause a crash or panic (v0.51.0)
- **Performance regression CI** — BSBM benchmark (1M-triple product dataset, 12 explore queries) and automated throughput benchmarks fail the build if performance drops by more than 10% (v0.51.0)
- **Security CI** — `cargo audit --deny warnings` runs on every pull request; SBOM (CycloneDX) generated and attached to every release; GitHub Actions refs pinned to full SHA; Docker release images scanned via Trivy with immutable digest
- **Stability** — 72-hour soak test with published artifacts (memory trend, merge latency, query p50/p95/p99, error counts), memory leak detection, and crash recovery testing (v0.67.0)
- **Upgrade and backup acceptance** — migration chain from all supported 0.x versions, `pg_dump`/restore round trip, and rollback guidance tested in CI (v0.67.0)
- **Public benchmark baselines** — BSBM, WatDiv, LUBM, bulk N-Triples/Turtle load, HTAP merge throughput, construct-rule incremental maintenance, Datalog DRed, vector hybrid search, Arrow IPC export, and Citus fan-out benchmarks published with hardware, dataset size, and raw output (v0.67.0)

---

## Contributing

Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
