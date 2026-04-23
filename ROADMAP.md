# pg_ripple — Roadmap

> From **0.1.0** (foundation) to **1.0.0** (production-ready triple store)

> **Authority rule**: [plans/implementation_plan.md](plans/implementation_plan.md) is the authoritative description of the **eventual target architecture**. This roadmap is the delivery sequence for that architecture. If a milestone summary here conflicts with the implementation plan, the implementation plan wins and the roadmap should be updated to match it.

## How to read this roadmap

Each release below has two layers:

- **The plain-language summary** (in the coloured box) explains *what* the release delivers and *why it matters* — no programming knowledge required.
- **The technical deliverables** list the specific items developers will build. Feel free to skip these if you're reading for the big picture.

**Effort estimates** are given as *person-weeks* — e.g. "6–8 pw" means the release would take roughly 6–8 weeks for a single full-time developer, or 3–4 weeks for a pair working together. The total estimated effort from v0.1.0 to v1.0.0 is **275–376 person-weeks** (~63–86 months for one developer; ~32–43 months for a pair).

**"optional at runtime" items**: some deliverables are annotated *(optional at runtime — X must be installed)*. This means the feature depends on an external extension (e.g. pg_trickle) that may not be installed in every deployment. The feature is **required by this roadmap** and must be implemented; the Rust code gates on a runtime availability check and degrades gracefully (returns 0 / false / empty, emits a WARNING, never raises an ERROR) when the dependency is absent. These items are not optional from a delivery standpoint.

---

## For non-technical readers

If you're looking for a plain-language explanation of what each release delivers and why it matters, start with [**roadmap/README.md**](roadmap/README.md). That document describes v0.51.0–v0.53.0 in terms anyone can understand — no programming knowledge required.

---

## Overview at a glance

| Version | Name | What it delivers (one sentence) | Effort |
|---|---|---|---|
| [0.1.0](#v010--foundation) | Foundation | Install the extension, store and retrieve facts (VP storage from day one) | 6–8 pw |
| [0.2.0](#v020--bulk-loading--named-graphs) | Bulk Loading & Named Graphs | Bulk data import, named graphs, rare-predicate consolidation, N-Triples export | 6–8 pw |
| [0.3.0](#v030--sparql-query-engine-basic) | SPARQL Basic | Ask questions in the standard RDF query language (incl. GRAPH patterns) | 6–8 pw |
| [0.4.0](#v040--rdf-star--statement-identifiers) | RDF-star / Statement IDs | Make statements about statements; LPG-ready storage | 8–10 pw |
| [0.5.0](#v050--sparql-query-engine-advanced--query-completeness) | SPARQL Advanced (Query) | Property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES | 6–8 pw |
| [0.5.1](#v051--sparql-advanced-storage-serialization--write) | SPARQL Advanced (Storage & Write) | Inline encoding, CONSTRUCT/DESCRIBE, INSERT/DELETE DATA, FTS | 6–8 pw |
| [0.6.0](#v060--htap-architecture) | HTAP Architecture | Heavy reads and writes at the same time; shared-memory cache | 8–10 pw |
| [0.7.0](#v070--shacl-validation-core) | SHACL Core + Deduplication | Define data quality rules; reject bad data on insert; on-demand and merge-time triple deduplication | 5–7 pw |
| [0.8.0](#v080--shacl-advanced) | SHACL Advanced | Complex data quality rules with background checking | 4–6 pw |
| [0.9.0](#v090--serialization-export--interop) | Serialization | Import and export data in all standard RDF file formats | 3–4 pw |
| [0.10.0](#v0100--datalog-reasoning-engine) | Datalog Reasoning | Automatically derive new facts from rules and logic | 10–12 pw |
| [0.11.0](#v0110--incremental-sparql-views-datalog-views--extvp) | SPARQL & Datalog Views | Live, always-up-to-date dashboards from SPARQL and Datalog queries | 5–7 pw |
| [0.12.0](#v0120--sparql-update-advanced) | SPARQL Update (Advanced) | Pattern-based updates and graph management commands | 3–4 pw |
| [0.13.0](#v0130--performance-hardening) | Performance | Speed tuning, benchmarks, production-grade throughput | 6–8 pw |
| [0.14.0](#v0140--administrative--operational-readiness) | Admin & Security | Operations tooling, access control, docs, packaging | 4–6 pw |
| [0.15.0](#v0150--sparql-protocol-http-endpoint) | SPARQL Protocol | Standard HTTP API, graph-aware loaders and deletes as SQL functions | 3–4 pw |
| [0.16.0](#v0160--sparql-federation) | SPARQL Federation | Query remote SPARQL endpoints alongside local data | 4–6 pw |
| [0.17.0](#v0170--json-ld-framing) | JSON-LD Framing | Frame-driven CONSTRUCT queries producing nested JSON-LD | 3–4 pw |
| [0.18.0](#v0180--sparql-construct-describe--ask-views) | SPARQL CONSTRUCT & ASK Views | Materialize CONSTRUCT and ASK queries as live, incrementally-updated stream tables | 2–3 pw |
| [0.19.0](#v0190--federation-performance) | Federation Performance | Connection pooling, result caching, query rewriting, and batching for remote SPARQL endpoints | 3–5 pw |
| [0.20.0](#v0200--w3c-conformance--stability-foundation) | W3C Conformance & Stability | W3C SPARQL 1.1 and SHACL Core test suite compliance, crash recovery and memory safety hardening, security audit initiation | 5–7 pw |
| [0.21.0](#v0210--sparql-built-in-functions--query-correctness) | SPARQL Built-in Functions & Query Correctness | Implement all ~40 missing SPARQL 1.1 built-in functions, fix the FILTER silent-drop hazard, and close critical query-semantics bugs | 6–8 pw |
| [0.22.0](#v0220--storage-correctness--security-hardening) | Storage Correctness & Security Hardening | Fix HTAP merge race conditions, dictionary cache rollback, shmem cache thrashing, rare-predicate promotion race, and HTTP service security gaps | 6–8 pw |
| [0.23.0](#v0230--shacl-core-completion--sparql-diagnostics) | SHACL Core Completion & SPARQL Diagnostics | Complete the SHACL constraint set, add SPARQL query introspection, and fix Datalog/JSON-LD correctness issues | 6–8 pw |
| [0.24.0](#v0240--semi-naive-datalog--performance-hardening) | Semi-naive Datalog & Performance Hardening | Implement semi-naive evaluation for Datalog rules, complete the OWL RL rule set, batch-decode large result sets, and bound property-path depth | 6–8 pw |
| [0.25.0](#v0250--geosparql--architectural-polish) | GeoSPARQL & Architectural Polish | Add GeoSPARQL 1.1 geometry primitives, stabilise the internal catalog against OID drift, and close remaining medium- and low-priority issues | 6–8 pw |
| [0.26.0](#v0260--graphrag-integration) | GraphRAG Integration | First-class integration with Microsoft GraphRAG: BYOG Parquet export, Datalog-enriched entity graphs, SHACL quality enforcement, and a Python CLI bridge | 4–6 pw |
| [0.27.0](#v0270--vector--sparql-hybrid-foundation) | Vector + SPARQL Hybrid: Foundation | Core pgvector integration — embedding table, HNSW index, `pg:similar()` SPARQL function, bulk embedding, and hybrid retrieval modes | 5–7 pw |
| [0.28.0](#v0280--advanced-hybrid-search--rag-pipeline) | Advanced Hybrid Search & RAG Pipeline | Production-grade RRF fusion, incremental embedding worker, graph-contextualized embeddings, and end-to-end RAG retrieval | 5–8 pw |
| [0.29.0](#v0290--datalog-optimization-magic-sets--cost-based-compilation) | Datalog Optimization: Magic Sets & Cost-Based Compilation | Goal-directed inference via magic sets, cost-based body atom reordering, subsumption checking, anti-join negation, filter pushdown, delta table indexing | 5–7 pw |
| [0.30.0](#v0300--datalog-aggregation--compiled-rule-plans) | Datalog Aggregation & Compiled Rule Plans | Aggregation in rule bodies (Datalog^agg), SQL plan caching across inference runs, SPARQL on-demand query speedup | 5–7 pw |
| [0.31.0](#v0310--entity-resolution--demand-transformation) | Entity Resolution & Demand Transformation | `owl:sameAs` entity canonicalization, demand transformation for goal-directed rule rewriting, SPARQL query planner integration | 5–7 pw |
| [0.32.0](#v0320--well-founded-semantics--tabling) | Well-Founded Semantics & Tabling | Three-valued semantics for cyclic ontologies, subsumptive result caching for Datalog and SPARQL repeated sub-queries | 5–7 pw |
| [0.33.0](#v0330--documentation-site--content-overhaul) | Documentation Site & Content Overhaul | Complete docs site rebuild — CI harness, eight feature-deep-dive chapters, operations guide, reference section, and content governance | 8–12 pw |
| [0.34.0](#v0340--bounded-depth-termination--incremental-retraction-dred) | Bounded-Depth Termination & Incremental Retraction (DRed) | Early fixpoint termination for bounded hierarchies (20–50% faster SPARQL property paths); Delete-Rederive for write-correct materialized predicates | 5–7 pw |
| [0.35.0](#v0350--parallel-stratum-evaluation--incremental-rule-updates) | Parallel Stratum Evaluation & Incremental Rule Updates | Background-worker parallelism for independent rules (2–5× faster materialization); add/remove rules without full recompute | 5–7 pw |
| [0.36.0](#v0360--worst-case-optimal-joins--lattice-based-datalog) | Worst-Case Optimal Joins & Lattice-Based Datalog | Leapfrog Triejoin for cyclic SPARQL patterns (10×–100× speedup); Datalog^L monotone lattice aggregation | 6–9 pw |
| [0.37.0](#v0370--storage-concurrency-hardening--error-safety) | Storage Concurrency Hardening & Error Safety | Fix HTAP merge race, rare-predicate promotion race, dictionary cache rollback; eliminate all hard panics; add GUC validators | 9–11 pw |
| [0.38.0](#v0380--architecture-refactoring--query-completeness) | Architecture Refactoring & Query Completeness | Split god-module, PredicateCatalog trait, batch encoding, SCBD, SPARQL Update completeness, SHACL hints in planner | 9–11 pw |
| [0.39.0](#v0390--datalog-http-api-for-pg_ripple_http) | Datalog HTTP API | REST API exposing all 27 Datalog SQL functions in `pg_ripple_http`: rule management, inference, goal queries, constraints, admin | 3–5 pw |
| [0.40.0](#v0400--streaming-results-explain--observability) | Streaming Results, Explain & Observability | Server-side SPARQL cursors, `explain_sparql()`, `explain_datalog()`, OpenTelemetry tracing, resource governors | 9–11 pw |
| [0.41.0](#v0410--full-w3c-sparql-11-test-suite) | Full W3C SPARQL 1.1 Test Suite | Complete W3C SPARQL 1.1 Query + Update + Graph Patterns + Aggregates test suite harness with parallelized execution; 3,000+ tests in < 2 min CI | 5–7 pw |
| [0.42.0](#v0420--parallel-merge-cost-based-federation--live-cdc) | Parallel Merge, Cost-Based Federation & Live CDC | Multi-worker HTAP merge, FedX-style federation planner, parallel SERVICE, live RDF change subscriptions | 10–12 pw |
| [0.43.0](#v0430--watdiv--jena-conformance-suite) | WatDiv + Jena Conformance Suite | Apache Jena edge-case tests (~1,000) and WatDiv scale-correctness benchmark (10M+ triples, star/chain/snowflake/complex patterns); 90% harness reuse from v0.41.0 | 5–7 pw |
| [0.44.0](#v0440--lubm-conformance-suite) | LUBM Conformance Suite | Lehigh University Benchmark — OWL RL inference correctness across 14 canonical queries on 1K–8M triple datasets; includes Datalog API validation sub-suite for rule compilation, iteration tracking, inferred triples, goal queries, and performance baseline | 3–5 pw |
| [0.45.0](#v0450--shacl-completion-datalog-robustness--crash-recovery) | SHACL Completion, Datalog Robustness & Crash Recovery | Close remaining SHACL Core gaps (`sh:equals`/`sh:disjoint`, decoded violation IRIs, async load test), harden parallel Datalog strata rollback, add missing crash-recovery scenarios, and standardise migration documentation | 4–6 pw |
| [0.46.0](#v0460--property-based-testing-fuzz-hardening--owl-2-rl-conformance) | Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance | `proptest` for SPARQL and dictionary invariants, fuzz the federation result decoder, W3C OWL 2 RL test suite in CI, TopN push-down, BSBM regression gate, sequence pre-allocation for Datalog workers, rustdoc coverage enforcement, and HTTP certificate pinning | 5–7 pw |
| [0.47.0](#v0470--shacl-truthfulness-dead-code-activation--architecture-refactor) | SHACL Truthfulness, Dead-Code Activation & Architecture Refactor | Fix parsed-but-not-checked SHACL constraints, wire `preallocate_sid_ranges()`, finish the `sparql/translate/` module split, add 5 fuzz targets, 4 crash-recovery scenarios, cache hit-rate SRFs, GUC validators, and security hygiene | 8–10 pw |
| [0.48.0](#v0480--shacl-core-completeness-owl-2-rl-closure--sparql-completeness) | SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness | Complete all 35 SHACL Core constraints and complex `sh:path` expressions, close the OWL 2 RL rule set, add SPARQL Update MOVE/COPY/ADD, fix SPARQL-star variable patterns, WatDiv baselines, and operational hardening | 6–8 pw |
| [0.49.0](#v0490--ai--llm-integration) | AI & LLM Integration | `sparql_from_nl()` NL-to-SPARQL via configurable LLM endpoint; `suggest_sameas()` and `apply_sameas_candidates()` for embedding-based entity alignment | 4–6 pw |
| [0.50.0](#v0500--developer-experience--graphrag-polish) | Developer Experience & GraphRAG Polish | `explain_sparql(analyze:=true)` interactive query debugger; `rag_context()` RAG pipeline | 3–5 pw |
| [0.51.0](#v0510--security-hardening--production-readiness) | Security Hardening & Production Readiness | Non-root container, SPARQL DoS protection, HTTP streaming, OTLP, pg_upgrade compat, CDC docs, conformance gate flips | 8–10 pw |
| [0.52.0](#v0520--dx-extended-standards--architecture) | DX, Extended Standards & Architecture | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, CDC lifecycle events, architecture module splits, OpenAPI spec | 6–9 pw |
| [0.53.0](#v0530--high-availability--logical-replication) | High Availability & Logical Replication | PG18 logical-decoding RDF replication, Helm chart, merge/vector-index performance baselines | 5–7 pw |
| [0.54.0](#v0540--pg-trickle-relay-integration) | pg-trickle Relay Integration | JSON→RDF helpers, CDC→outbox bridge worker, CDC bridge triggers, JSON-LD event serializer, dedup keys, vocabulary templates, pg-trickle runtime detection, integration test suite | 5–7 pw |
| [1.0.0](#v100--production-release) | Production Release | Standards conformance, stress testing, security audit | 6–8 pw |
| | | **Total estimated effort** | **275–376 pw** |

---

## v0.1.0 — Foundation

**Theme**: Core data model, dictionary encoding, and basic triple CRUD.

> **In plain language:** This is the "hello world" release. After installing pg_ripple into a PostgreSQL database, a user can store facts (called *triples* — think "subject → relationship → object", e.g. "Alice → knows → Bob") and retrieve them by pattern. No query language yet — just the basic building blocks. Internally, every piece of text (names, URLs, values) is converted to a compact number for fast storage and comparison. This release also sets up automated testing so that every future change is verified.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] pgrx 0.17 project scaffolding targeting PostgreSQL 18
- [x] Extension bootstrap: `CREATE EXTENSION pg_ripple` creates `_pg_ripple` schema
- [x] **Dictionary encoder**
  - Unified dictionary table (IRIs, blank nodes, literals in a single table with `kind` discriminator — avoids ID space collision between separate resource/literal tables)
  - **Hash-Backed Sequence encoding (Route 2)**: XXH3-128 is computed over `kind_le_bytes || term_utf8` (kind is mixed in so the same string as different term types maps to distinct IDs); the full 16-byte hash is stored in a `BYTEA` column with a `UNIQUE` index as the collision-detection key; a PostgreSQL `GENERATED ALWAYS AS IDENTITY` sequence produces the dense, sequential `i64` join key used in every VP table. This avoids the birthday-problem collision risk of schemes that truncate the hash to 64 bits (collision expected at ~4 billion terms in 64-bit space).
  - Backend-local encode cache (`LruCache<u128, i64>`, keyed on full 128-bit hash) and decode cache (`LruCache<i64, String>`)
  - Encode/decode SQL functions: `pg_ripple.encode_term()`, `pg_ripple.decode_id()`
- [x] **Vertical Partitioning from day one**
  - Dynamic VP table management: auto-create `_pg_ripple.vp_{predicate_id}` tables on first triple with a new predicate
  - Predicate catalog: `_pg_ripple.predicates (id BIGINT, table_oid OID, triple_count BIGINT)`
  - Dual B-tree indices per VP table: `(s, o)` and `(o, s)`
  - Global statement identifier sequence: `_pg_ripple.statement_id_seq` — every VP table row gets a globally-unique SID via `i BIGINT NOT NULL DEFAULT nextval('statement_id_seq')`
  - SIDs are not exposed to users in v0.1.0 but are available for internal use from the start (prerequisite for RDF-star in v0.4.0)
- [x] **Basic triple CRUD**
  - `pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_ripple.delete_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_ripple.triple_count() RETURNS BIGINT`
- [x] **Basic querying** (SQL-level, no SPARQL yet)
  - `pg_ripple.find_triples(s TEXT, p TEXT, o TEXT) RETURNS TABLE (s TEXT, p TEXT, o TEXT, g TEXT)` — any param can be NULL for wildcard; returns decoded string values
- [x] Unit tests for dictionary encode/decode round-trips
- [x] Integration test: insert + query cycle
- [x] pg_regress: `dictionary.sql` (encode/decode, prefix expansion, hash collision behaviour), `basic_crud.sql` (insert, delete, find_triples, triple_count)
- [x] CI pipeline (GitHub Actions)
- [x] **GUC-gated lazy initialization**
  - Merge worker, SHACL engine, and reasoning engine only start when their respective GUCs are enabled (`pg_ripple.merge_threshold > 0`, `pg_ripple.shacl_mode != 'off'`, `pg_ripple.inference_mode != 'off'`)
  - Reduces resource overhead for deployments that use only a subset of features
- [x] **Error taxonomy module** (`src/error.rs`)
  - `thiserror`-based error types with PT error code constants
  - Initial ranges: dictionary errors (PT001–PT099) and storage errors (PT100–PT199)
  - PostgreSQL-style formatting: lowercase first word, no trailing period
  - Extended in subsequent milestones as new subsystems are added (see §13.6 of the [Implementation Plan](plans/implementation_plan.md) for the complete PT001–PT799 range table)

> **Shared memory note**: v0.1.0 through v0.5.1 use a **backend-local** `lru::LruCache` for the dictionary cache. This avoids requiring `shared_preload_libraries` for the "hello world" release and defers the pgrx shared-memory complexity to v0.6.0 when the HTAP architecture actually needs it. The shared-memory dictionary cache, bloom filters, slot versioning, and `pg_ripple.shared_memory_size` startup GUC are all introduced in v0.6.0.

### Exit Criteria

A user can install the extension, insert triples (routed to per-predicate VP tables), and query them back by pattern. No `shared_preload_libraries` configuration required. VP tables are created dynamically on first encounter of a new predicate.

</details>

---

## v0.2.0 — Bulk Loading & Named Graphs

**Theme**: Bulk data import, rare-predicate consolidation, named graphs, and prefix management.

> **In plain language:** This release adds *bulk import*: users can load large RDF data files (in Turtle and N-Triples formats) in one go, rather than inserting facts one at a time. Named graphs (the ability to group facts into labelled collections) are introduced here too. A "rare predicate" consolidation table prevents catalog bloat when datasets have thousands of distinct predicates. N-Triples export is included for test verification and round-trip checking.
>
> **Storage partition note**: In v0.2.0 through v0.5.0, each VP table is a *single flat table* — there is no delta/main split yet. All reads and writes target the same table. The HTAP dual-partition architecture (separate `_delta` and `_main` tables with a background merge worker) is introduced in v0.6.0 via an explicit schema migration that renames existing VP tables and creates the initial `_main` partition.
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Rare-predicate consolidation table**
  - Predicates with fewer than `pg_ripple.vp_promotion_threshold` triples (default: 1,000) are stored in a shared `_pg_ripple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT, i BIGINT)` table with a primary composite index on `(p, s, o)` and two secondary indices: `(s, p)` for DESCRIBE queries and `(g, p, s, o)` for efficient graph-drop bulk-delete
  - Promotion is **deferred to end-of-statement** (not mid-batch): during a bulk load, triples accumulate in `vp_rare`; after the load completes, predicates exceeding the threshold are promoted in a single `INSERT … SELECT` + `DELETE` transaction — avoids disrupting in-flight COPY streams
  - `pg_ripple.promote_rare_predicates()` can also be called manually or by the background merge worker
  - Prevents catalog bloat for predicate-rich datasets (DBpedia ≈60K predicates, Wikidata ≈10K) — avoids hundreds of thousands of PG objects, reduces planner overhead, and cuts VACUUM cost
- [x] **`_pg_ripple.statements` range-mapping catalog**
  - Maintained by the merge worker; stores `(sid_min, sid_max, predicate_id, table_oid)` range rows rather than one row per statement — resolved via binary search in *O(log n)* with no full-table scans
  - After each merge cycle the worker inserts one range row per VP table covering the SIDs allocated since the last merge; because SIDs are drawn from a monotonically-increasing sequence, ranges are non-overlapping
  - Required for v0.4.0 RDF-star where SIDs appear as subjects/objects in other VP tables and must be unambiguously resolved to their owning VP table
- [x] **Named graph support** (basic)
  - `g` column in VP tables
  - `pg_ripple.create_graph()`, `pg_ripple.drop_graph()`, `pg_ripple.list_graphs()`
- [x] **`pg_ripple.named_graph_optimized` GUC** (default: `off`)
  - When enabled, adds an optional `(g, s, o)` index per dedicated VP table (and equivalent coverage on `vp_rare`) to accelerate graph-scoped queries (e.g. list all triples in graph G, drop a named graph)
  - Off by default to avoid index bloat for workloads that do not use named graphs heavily
- [x] **Blank node document-scoping**
  - Each bulk load operation is assigned a monotonically-increasing `load_generation` counter from a shared sequence
  - Blank nodes are hashed as `"{generation}:{label}"` — so `_:b0` from two different load calls yields two distinct dictionary IDs
  - Prevents incorrect merging of blank nodes across document boundaries, which would corrupt data in multi-file loads
  - Also applies to `INSERT DATA` (SPARQL Update, v0.5.1+) which always gets its own generation
- [x] **Bulk loader** (N-Triples)
  - `pg_ripple.load_ntriples(data TEXT) RETURNS BIGINT`
  - Streaming parser via `rio_turtle` crate
  - Batch encoding + COPY for throughput
- [x] **Bulk loader** (N-Quads)
  - `pg_ripple.load_nquads(data TEXT) RETURNS BIGINT`
  - Standard format for named-graph quads (`<s> <p> <o> <g> .`); same `rio_turtle` parser path as N-Triples
  - Route quads to the appropriate named graph (`g` column) automatically
- [x] **Bulk loader** (Turtle)
  - `pg_ripple.load_turtle(data TEXT) RETURNS BIGINT`
  - Prefix declarations auto-registered
  - Blank node scoping per load operation
  - `rio_turtle` crate already handles both formats — incremental parser work
- [x] **Bulk loader** (TriG)
  - `pg_ripple.load_trig(data TEXT) RETURNS BIGINT`
  - Turtle with named graph blocks (`GRAPH <g> { … }`) — the standard interchange format for named-graph Turtle data
  - Uses the same `rio_turtle` streaming parser; named graph IRI is dictionary-encoded and stored in the `g` column
- [x] **File-path bulk load variants**
  - `pg_ripple.load_turtle_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_ntriples_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_nquads_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_trig_file(path TEXT) RETURNS BIGINT`
  - Reads via `pg_read_file()` with superuser privilege check — prevents unauthorized file access
  - Essential for datasets larger than ~1 GB where passing data as a TEXT parameter exceeds PostgreSQL's TEXT size limit and imposes significant memory overhead
  - Returns count of loaded triples; otherwise identical behaviour to the inline TEXT variants
- [x] **IRI prefix management**
  - `pg_ripple.register_prefix(prefix TEXT, expansion TEXT)`
  - `pg_ripple.prefixes() RETURNS TABLE`
  - Prefix expansion in encode/decode paths
- [x] **ANALYZE after bulk loads**
  - All inline and file-path load functions run `ANALYZE` on affected VP tables after load completes
  - Ensures the PostgreSQL planner has accurate selectivity estimates for generated SQL — critical for good join plans in v0.3.0+
- [x] Benchmarks: insert throughput (1M triples) — `benchmarks/insert_throughput.sql`
- [x] **Performance regression baseline**: `benchmarks/ci_benchmark.sh` records insert throughput and point-query latency; CI `benchmark` job uploads results as artifacts and can gate on >10% regression
- [x] **N-Triples / N-Quads export** (basic)
  - `pg_ripple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_ripple.export_nquads(graph TEXT DEFAULT NULL) RETURNS TEXT` — exports all named graphs as NQuads when `graph` is NULL; a single graph when specified
  - Streaming variants returning `SETOF TEXT` for large graphs
  - Essential for verifying bulk load round-trips in v0.2.0 testing
- [x] pg_regress test suite: `triple_crud.sql`, `named_graphs.sql`, `export_ntriples.sql`, `nquads_trig.sql` (N-Quads round-trip, TriG named-graph import, file-path loaders)

### Exit Criteria

Rare-predicate consolidation table absorbs low-frequency predicates. Bulk loading >50K triples/sec on commodity hardware. Named graphs functional. All four inline formats (N-Triples, N-Quads, Turtle, TriG) and their file-path counterparts load correctly. Multi-graph data can be loaded via N-Quads/TriG and round-tripped via N-Quads export. VP tables have current planner statistics after bulk load.

</details>

---

## v0.3.0 — SPARQL Query Engine (Basic)

**Theme**: Parse and execute SPARQL SELECT and ASK queries with basic graph patterns, named graph querying, initial join optimizations, and plan caching from day one.

> **In plain language:** SPARQL is the standard language for asking questions over linked data — the same way SQL is for relational databases. This release makes pg_ripple understand SPARQL, so users can write queries like *"find all people who know someone who works at Acme Corp"* using the official W3C syntax. It also enables querying across named graphs (created in v0.2.0) using the standard SPARQL `GRAPH` keyword.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Prerequisites

- **`sparopt` availability check** *(must be resolved before beginning v0.3.0)*: verify that `sparopt` is published to crates.io with a stable, usable API and pin the version. If unavailable or API-unstable, absorb its filter-pushdown and constant-folding work directly into pg_ripple's own algebra optimizer pass (`src/sparql/algebra.rs`) before starting v0.3.0 — do not begin v0.3.0 development without resolving this gate.

### Deliverables

- [x] **`sparopt` first-pass algebra optimizer** (`sparopt` crate)
  - sparopt 0.3 is published on crates.io and pinned; direct conversion between sparopt and spargebra algebra types is unavailable (distinct type hierarchies), so filter-pushdown and constant-folding are implemented inline in `src/sparql/sqlgen.rs` per the fallback clause
- [x] **SPARQL parser integration** (`spargebra` crate)
  - Parse SPARQL SELECT and ASK queries into algebra tree
  - Support: Basic Graph Patterns (BGP), FILTER, OPTIONAL, LIMIT, OFFSET, ORDER BY, DISTINCT
  - `GRAPH ?g { ... }` patterns and `FROM` / `FROM NAMED` dataset clauses — map to `WHERE g = encode(uri)` filters on VP tables
- [x] **Per-query `EncodingCache`** (`src/sparql/sqlgen.rs` `Ctx.per_query`)
  - Short-lived `HashMap` for IRIs and literals seen within a single SPARQL query
  - Avoids repeated SPI dictionary look-ups for constants that appear multiple times in one query
- [x] **SQL generator** (initial)
  - BGP → JOIN across VP tables (integer equality)
  - FILTER → WHERE clause on integer-encoded values (dictionary-join decode for type comparisons; inline encoding deferred to v0.5.0)
  - OPTIONAL → LEFT JOIN
  - LIMIT/OFFSET/ORDER BY passthrough
  - DISTINCT → SQL DISTINCT
- [x] **Query executor**
  - `pg_ripple.sparql(query TEXT) RETURNS SETOF JSONB`
  - SPI execution of generated SQL
  - **Batch dictionary decode**: collect all output i64 IDs from the result set, decode in a single `WHERE id IN (...)` query, build an in-memory lookup map, then emit human-readable rows — avoids per-row dictionary round-trips
- [x] **SPARQL ASK**
  - ASK → `SELECT EXISTS(...)` → returns BOOLEAN
  - `pg_ripple.sparql_ask(query TEXT) RETURNS BOOLEAN`
- [x] **Join optimizations** (phase 1)
  - Self-join elimination for star patterns
  - Filter pushdown: encode FILTER constants before SQL generation
- [x] **Query plan caching** *(introduced in v0.3.0 — not deferred to v0.13.0)*
  - Cache SPARQL→SQL translation results keyed by query text
  - `pg_ripple.plan_cache_size` GUC (default: `256`; `0` = disabled)
- [x] `pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false) RETURNS TEXT` — show generated SQL; `analyze := true` executes the query and augments the output with actual row counts
- [x] **SQL injection / adversarial tests**: verify that SPARQL queries containing SQL metacharacters in IRIs, literals, and prefixed names are safely dictionary-encoded and never reach generated SQL as raw strings
- [x] pg_regress: `sparql_queries.sql` (10+ test queries), `sparql_injection.sql` (adversarial inputs)

### Exit Criteria

Users can run SPARQL SELECT and ASK queries with BGPs, FILTER, OPTIONAL, and GRAPH patterns against data loaded via bulk load. Named graph queries work correctly. Queries return correct results.

</details>

---

## v0.4.0 — RDF-star / Statement Identifiers

**Theme**: Quoted triples, statement-level metadata, and LPG-ready storage — make statements about statements.

> **In plain language:** Standard RDF can say "Alice knows Bob". But it can't directly say *"Alice said that she knows Bob"* or *"The fact that Alice knows Bob was recorded on January 5th"*. RDF-star (now part of the RDF 1.2 standard) solves this by allowing triples to be embedded inside other triples — called *quoted triples*. This is essential for provenance ("where did this fact come from?"), temporal annotations ("when was this true?"), and trust ("who asserted this?"). By delivering this immediately after basic SPARQL, pg_ripple becomes **LPG-ready from the start**: Labeled Property Graph edges with properties (e.g. `[:KNOWS {since: 2020}]`) map directly to RDF-star annotations over statement identifiers already present in the VP tables since v0.1.0. This is a cross-cutting change that touches parsing, storage, dictionary encoding, and the SPARQL engine.
>
> **Effort estimate: 8–10 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Design rationale — why so early?

The OneGraph (1G) research initiative (Lassila et al., 2023; Poseidon engine, AWS Neptune Analytics) demonstrates that a unified SPOI (Subject, Predicate, Object, statement-Identifier) storage model is the foundation for breaking the "graph model lock-in" between RDF and LPG. By introducing statement identifiers in v0.1.0 (storage) and RDF-star in v0.4.0 (query), pg_ripple achieves 1G-compatible storage before any advanced features are built on top. Every subsequent milestone (SHACL, Datalog, SPARQL Update, Cypher/GQL) benefits from statement IDs being available from the start.

**Patent clearance**: RDF-star is a W3C standard developed under the [W3C Patent Policy](https://www.w3.org/Consortium/Patent-Policy/) (Royalty-Free). Statement identifiers are well-established prior art (RDF reification, 2004; Named Graphs, 2005; RDF-star Community Group, 2014). The 1G abstract data model is published academic research (Semantic Web Journal, doi:10.3233/SW-223273), not patented technology. Poseidon's proprietary implementation details (P8APL, PAX pages, lock-free adjacency lists) are specific to Amazon's in-memory engine and are not replicated here — pg_ripple uses PostgreSQL's native heap/WAL/MVCC storage.

### Deliverables

- [x] **Quoted triple syntax in parsers**
  - N-Triples-star: `<< <http://...Alice> <http://...knows> <http://...Bob> >> <http://...assertedBy> <http://...Carol> .`
  - Implemented via a custom recursive-descent N-Triples-star line parser (no external dependency conflicts)
  - Supports subject-position and object-position quoted triples, nested quoted triples
  - Note: Turtle-star deferred to v0.5.x; `load_ntriples()` handles N-Triples-star fully
- [x] **Dictionary encoding for quoted triples**
  - New term type: `KIND_QUOTED_TRIPLE = 5` — XXH3-128 hash of `(s_id, p_id, o_id)`
  - `qt_s`, `qt_p`, `qt_o` columns added to `_pg_ripple.dictionary` via `ALTER TABLE … ADD COLUMN IF NOT EXISTS`
  - `pg_ripple.encode_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT`
  - `pg_ripple.decode_triple(id BIGINT) RETURNS JSONB`
- [x] **Statement identifier activation**
  - `pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL) RETURNS BIGINT` — returns SID
  - `pg_ripple.get_statement(i BIGINT) RETURNS JSONB` — look up a statement by its SID
- [x] **Storage for edge properties via SIDs**
  - Annotation triples use the SID of the annotated statement as their subject — regular `BIGINT` values, no structural change to VP tables
  - Nested quoted triples supported
- [x] **SPARQL-star query support**
  - `TermPattern::Triple` handled in `sparql/sqlgen.rs` via `ground_term_id()` — ground (all-constant) quoted triple patterns compile to a dictionary lookup + equality condition
  - Uses `spargebra/sparql-12` and `sparopt/sparql-12` features (properly gates `oxrdf/rdf-12` to avoid match-exhaustiveness errors)
  - Variable-inside-quoted-triple deferred to v0.5.x
- [x] **Bulk load support for RDF-star data**
  - `pg_ripple.load_ntriples()` accepts N-Triples-star input
  - `pg_ripple.load_turtle()`, `pg_ripple.load_nquads()`, `pg_ripple.load_trig()` use rio_turtle (no RDF-star; emits warning)
- [x] **W3C SPARQL-star conformance gate**: `tests/pg_regress/sql/sparql_star_conformance.sql` — N-Triples-star parsing, dictionary round-trips, SID lifecycle, annotation patterns, ground triple patterns, data integrity, known-limitation documentation
- [x] pg_regress: `rdf_star_load.sql` (load N-Triples-star, encode/decode round-trip, SID lifecycle)

### Exit Criteria

Users can load RDF-star data (Turtle-star, N-Triples-star), query it with SPARQL-star triple term patterns, and use statement identifiers to model edge properties. SIDs are returned from insert operations and can be used as subjects/objects in subsequent triples. The storage layer is LPG-ready.

</details>

---

## v0.5.0 — SPARQL Query Engine (Advanced — Query Completeness)

**Theme**: Property paths, UNION, aggregates, subqueries, and advanced join optimizations.

> **In plain language:** This release teaches the query engine to handle more powerful questions. *Property paths* let you follow chains of relationships — e.g. "find everyone reachable through any number of 'knows' links" (like a social network friend-of-a-friend search). *Aggregates* let you compute totals and averages ("how many people work in each department?"). This is a pure query-engine release with no storage changes, isolating query completeness from the inline encoding and write-path work in v0.5.1.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Property path compilation**
  - `+` (one or more) → `WITH RECURSIVE` CTE
  - `*` (zero or more) → `WITH RECURSIVE` CTE with zero-hop anchor
  - `?` (zero or one) → `UNION` of direct + zero-hop
  - `/` (sequence) → chained joins
  - `|` (alternative) → `UNION`
  - `^` (inverse) → swap `s`/`o`
  - Cycle detection via PG18 `CYCLE` clause (hash-based, replaces array-based visited tracking for $O(1)$ membership checks instead of $O(n)$ array scans)
  - `pg_ripple.max_path_depth` GUC
  - **Known performance constraint**: PostgreSQL materializes each level of a `WITH RECURSIVE` CTE into a work-table. For deep traversals (depth > ~15) or wide fan-out on graphs with 10M+ triples the per-level copy cost becomes the bottleneck. The <100 ms target in §13 benchmarks applies to bounded-depth paths (depth ≤ 10) on typical RDF datasets; unbounded paths on dense graphs will exceed it. A purpose-built graph traversal engine would outperform this approach at extreme depth/fan-out, but that is out of scope for v1.0.
- [x] **UNION / MINUS**
  - UNION → SQL `UNION`
  - MINUS → SQL `EXCEPT`
- [x] **Aggregates**
  - COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT
  - GROUP BY → SQL GROUP BY
  - HAVING → SQL HAVING
- [x] **Subqueries**
  - Nested SELECT in WHERE / FROM clause
- [x] **BIND / VALUES**
  - BIND → SQL column alias
  - VALUES → SQL VALUES clause
- [x] **Resource exhaustion tests**: Cartesian-product queries, unbounded property paths on cyclic graphs, deeply nested subqueries — verify that `max_path_depth`, `statement_timeout`, and memory limits prevent runaway resource consumption
- [x] pg_regress: `property_paths.sql`, `aggregates.sql`, `resource_limits.sql` (exhaustion tests)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for the complete page-by-page specification. v0.5.0 carries the full catch-up backlog for v0.1.0–v0.4.0 in addition to new v0.5.0 pages.

**Catch-up — v0.1.0 Foundation**
- [x] Docs site scaffold: `docs/book.toml`, `.github/workflows/docs.yml`, `docs/src/SUMMARY.md`
- [x] `user-guide/introduction.md`, `user-guide/installation.md`, `user-guide/getting-started.md`
- [x] `user-guide/sql-reference/index.md`, `triple-crud.md`, `dictionary.md`, `prefix.md`
- [x] `reference/changelog.md` (mirror), `reference/roadmap.md` (mirror), `reference/security.md` (stub), `research/index.md`

**Catch-up — v0.2.0 Bulk Loading & Named Graphs**
- [x] `user-guide/sql-reference/bulk-load.md`, `user-guide/sql-reference/named-graphs.md`
- [x] `user-guide/best-practices/bulk-loading.md`
- [x] `user-guide/configuration.md` (initial: `vp_promotion_threshold`, `named_graph_optimized`, `plan_cache_size`)
- [x] `reference/faq.md` (seed: 10+ questions covering v0.1.0–v0.4.0)

**Catch-up — v0.3.0 SPARQL Basic**
- [x] `user-guide/playground.md` — Docker sandbox ⭐
- [x] `user-guide/sql-reference/sparql-query.md` (initial: SELECT, ASK, EXPLAIN)
- [x] `user-guide/best-practices/sparql-patterns.md` (initial)
- [x] `reference/troubleshooting.md` (initial)

**Catch-up — v0.4.0 RDF-star**
- [x] `user-guide/sql-reference/rdf-star.md`
- [x] `user-guide/best-practices/data-modeling.md` (initial)

**New in v0.5.0**
- [x] `user-guide/sql-reference/sparql-query.md` expanded: property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES
- [x] `user-guide/best-practices/sparql-patterns.md` expanded: property path recipes, resource exhaustion safeguards
- [x] `user-guide/configuration.md` expanded: `max_path_depth` GUC

### Exit Criteria

SPARQL 1.1 Query coverage for property paths, UNION/MINUS, aggregates, subqueries, BIND/VALUES. Property path queries complete with hash-based cycle detection via PG18 `CYCLE` clause. Docs site is live on GitHub Pages with all catch-up pages written.

</details>

---

## v0.5.1 — SPARQL Advanced (Storage, Serialization & Write)

**Theme**: Inline value encoding, CONSTRUCT/DESCRIBE, INSERT DATA/DELETE DATA, and full-text search.

> **In plain language:** This release introduces *inline value encoding* — a performance optimization that eliminates dictionary lookups for numeric and date comparisons. It changes the fundamental ID space model (introducing a dual-space interpretation), which is why it is separated from the pure query-engine work in v0.5.0. It also adds the two simplest SPARQL Update forms (`INSERT DATA` / `DELETE DATA`) so standard RDF tools can write to pg_ripple, *CONSTRUCT* and *DESCRIBE* to complete the four standard SPARQL query forms, and *full-text search* for efficient text matching.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Inline value encoding** (`src/dictionary/inline.rs`)
  - Type-tagged `i64` encoding for xsd:integer, xsd:boolean, xsd:dateTime, xsd:date — FILTER comparisons on these types require zero dictionary round-trips
  - IDs allocated in monotonically increasing semantic order so range FILTERs (`>`, `<`, `BETWEEN`) compile directly to SQL numeric comparisons on the raw `i64` column
  - Deferred from v0.3.0 to keep the initial SPARQL engine focused on a single ID space; now that the query engine is stable, the dual-space (inline + dictionary) model can be introduced safely
  - **Note**: `xsd:double` is stored in the dictionary rather than inline-encoded — truncating IEEE 754 doubles to 56 bits produces undefined precision/range behaviour; dictionary storage is safe and range comparisons on doubles are uncommon in SPARQL
- [x] **SPARQL CONSTRUCT / DESCRIBE** (JSONB output)
  - CONSTRUCT → returns triples as JSONB (Turtle/JSON-LD serialization deferred to v0.9.0)
  - DESCRIBE → Concise Bounded Description (CBD) as default algorithm
  - `pg_ripple.describe_strategy` GUC (values: `'cbd'` / `'scbd'` / `'simple'`): selects the DESCRIBE expansion algorithm. Introduced here alongside DESCRIBE so the GUC is available from the first release that uses it.
  - Completes the four standard SPARQL query forms, making pg_ripple usable as an entity browser
- [x] **Basic SPARQL Update** (`INSERT DATA` / `DELETE DATA`)
  - Parse and execute `INSERT DATA { … }` statements via `spargebra` (already supports Update algebra)
  - Route through dictionary encoder + VP table insert path
  - Named graph support: `INSERT DATA { GRAPH <g> { … } }`
  - Parse and execute `DELETE DATA { … }` statements — exact-match triple deletion from VP tables
  - `pg_ripple.sparql_update(query TEXT) RETURNS BIGINT` — returns count of affected triples
  - Pattern-based updates (`DELETE/INSERT WHERE`), `LOAD`, `CLEAR`, `DROP`, `CREATE` deferred to v0.12.0
  - Enables standard RDF tools (Protégé, TopBraid, SPARQL workbenches) to write to pg_ripple without a custom adapter
- [x] **Full-text search on literals**
  - `pg_ripple.fts_index(predicate TEXT)` — create a GIN `tsvector` index on the dictionary for a predicate
  - SPARQL `CONTAINS()` and `REGEX()` FILTERs on indexed predicates rewrite to `@@` / `LIKE` against the GIN index
  - `pg_ripple.fts_search(query TEXT, predicate TEXT) RETURNS TABLE` — direct full-text search API
  - Index is maintained incrementally on `insert_triple()` for indexed predicates
- [x] pg_regress: `fts_search.sql`, `sparql_construct.sql`, `sparql_insert_data.sql`, `sparql_delete_data.sql`, `inline_encoding.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/sparql-update.md` — `sparql_update()`, INSERT DATA / DELETE DATA, named-graph variants
- [x] `user-guide/sql-reference/fts.md` — `fts_index`, `fts_search`, SPARQL CONTAINS/REGEX rewriting
- [x] `user-guide/sql-reference/sparql-query.md` expanded: CONSTRUCT / DESCRIBE, `describe_strategy` GUC
- [x] `user-guide/best-practices/update-patterns.md` — INSERT DATA vs bulk load, idempotent patterns

### Exit Criteria

Inline value encoding eliminates dictionary lookups for numeric and date FILTER comparisons. SPARQL CONSTRUCT and DESCRIBE return correct JSONB results. `INSERT DATA` / `DELETE DATA` work for standard-compliant write operations. Full-text search on indexed literal predicates is functional.

</details>

---

## v0.6.0 — HTAP Architecture

**Theme**: Separate read and write paths for concurrent OLTP/OLAP. Shared-memory dictionary cache. Subject pattern index.

> **In plain language:** In a real production system, people are loading new data and running complex queries at the same time. Without special care, these two activities interfere with each other — writes block reads and vice versa. This release splits the storage into a "write inbox" and a "read-optimised archive" so both can happen simultaneously at full speed. It also adds a *change notification* system: applications can subscribe to be told whenever specific facts change (useful for triggering workflows, updating caches, or feeding dashboards). An in-memory cache shared across all database connections makes repeated lookups much faster. Optionally, the companion pg_trickle extension enables automatically-updating live statistics.
>
> **Note**: This release introduces `shared_preload_libraries` as a requirement — v0.1.0–v0.5.1 do not require it because they use a backend-local dictionary cache. The `pg_ripple.shared_memory_size` startup GUC must be set in `postgresql.conf` before starting PostgreSQL.
>
> **Effort estimate: 8–10 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Delta/Main partition split — schema migration**
  - Each VP table is migrated from its flat single-table form (v0.1.0–v0.5.1) to a dual-partition form:
    1. `CREATE TABLE _pg_ripple.vp_{id}_delta AS SELECT * FROM _pg_ripple.vp_{id}` (copy existing rows to delta)
    2. `CREATE TABLE _pg_ripple.vp_{id}_main (LIKE _pg_ripple.vp_{id})` (empty main, BRIN-indexed)
    3. `ALTER TABLE _pg_ripple.vp_{id} RENAME TO vp_{id}_pre_htap` (keep old table as backup)
    4. Update `_pg_ripple.predicates` catalog with new table OIDs
    5. Run an immediate merge cycle to promote rows from delta to main in sorted order
    6. Drop `vp_{id}_pre_htap` after merge completes successfully
  - The migration runs inside the `ALTER EXTENSION pg_ripple UPDATE` upgrade script — zero downtime during migration because rows still exist in delta until the merge completes and the query path immediately switches to `UNION ALL` of `_main` and `_delta`
  - `vp_rare` is **not** split (see vp_rare HTAP exemption below); all reads and writes target the single `vp_rare` table throughout
  - All writes target `_delta`; `_main` is append-only / read-optimized
  - Query path: `UNION ALL` of `_main` and `_delta`
- [x] **Tombstone table for cross-partition deletes**
  - When deleting a triple that may exist in `_main`, the delete is recorded in `_pg_ripple.vp_{id}_tombstones (s BIGINT, o BIGINT, g BIGINT)`
  - Query path becomes: `(main EXCEPT tombstones) UNION ALL delta`
  - The merge worker applies tombstones against main during each generation merge, then truncates the tombstone table
  - Necessary because `_main` is read-only between merges — a DELETE targeting a main-resident triple cannot modify `_main` directly
- [x] **`vp_rare` HTAP exemption**
  - `vp_rare` is **not** given a delta/main split — it remains a single flat table
  - Rare predicates see few writes by definition; delta/main overhead would exceed the benefit
  - Concurrent reads and writes on `vp_rare` are safe via PostgreSQL standard heap row-level locking
  - The bloom filter treats `vp_rare` conservatively (always queries it, no delta-skip shortcut)
- [x] **Background merge worker**
  - pgrx `BackgroundWorker` implementation
  - Configurable merge threshold via `pg_ripple.merge_threshold` GUC
  - **Concurrency & Locking logic**: The rename/truncate step requires an `AccessExclusiveLock`. To prevent stalling the database, the merge worker uses a low `lock_timeout` and retry logic for the `ALTER TABLE ... RENAME` statement, ensuring concurrent `INSERT` and `SELECT` operations are not blocked entirely by a queued exclusive lock.
  - **Fresh-table generation merge**: rather than inserting into an existing `_main` table, create `vp_{id}_main_new`, insert *all* rows from both `_main` and `_delta` (minus tombstones) in sort order (ensuring BRIN pages are physically ordered), then atomically rename it to replace `_main` and TRUNCATE both `_delta` and `_tombstones` — writes to delta are never blocked during the merge and BRIN indexing is maximally effective because rows arrive in sorted order at table-creation time
  - BRIN index rebuild on main post-merge (concurrent where possible)
  - Shared-memory latch signaling
  - Also triggers `pg_ripple.promote_rare_predicates()` for any rare predicates that crossed the promotion threshold since the last merge
  - Runs `ANALYZE` on merged VP tables so the PostgreSQL planner has fresh selectivity estimates
  - **Watchdog**: if the merge worker heartbeat stalls for longer than `pg_ripple.merge_watchdog_timeout` (default: 300 s), `_PG_init` on the next backend connection logs a WARNING and attempts a restart
- [x] **`ExecutorEnd_hook` latch-poke**
  - When a write transaction commits more than `pg_ripple.latch_trigger_threshold` rows (default: 10,000), the hook immediately pokes the merge worker's latch to trigger an early merge
  - Prevents unbounded delta growth during bursty write workloads without requiring a polling loop
- [x] **Bloom filter for delta existence checks**
  - In shared memory, per VP table
  - Queries against main-only data skip delta scan
- [x] **Dictionary LRU cache in shared memory**
  - `pg_ripple.dictionary_cache_size` GUC
  - Shared across all backends via pgrx `PgSharedMem`
  - **Sharded lock design**: partition the hash map into N shards (default: 64), each with its own lightweight lock — eliminates global lock contention under concurrent encode/decode workloads
- [x] **Shared-memory budget & back-pressure**
  - `pg_ripple.cache_budget` GUC — utilization cap for the pre-allocated shared memory block (dictionary cache + bloom filters + merge worker buffers)
  - Automatic eviction priority: bloom filters reclaimed first, then oldest LRU dictionary entries
  - Back-pressure on bulk loads when shared memory is >90% of `cache_budget` — throttle batch size to prevent OOM
- [x] **Shared-memory slot versioning**
  - Each shared memory slot (declared via pgrx 0.17's `pg_shmem_init!` macro) carries a `[u8; 8]` magic constant (e.g. `*b"pg_tripl"`) followed by a `u32` layout version at its head
  - Version mismatch at `_PG_init` triggers a controlled re-initialization of the slot rather than corrupting state — essential for safe in-place upgrades
  - **pgrx 0.17 API note**: all shared memory sizes must be declared statically in `_PG_init`. The `pg_ripple.shared_memory_size` startup GUC determines the block size; it cannot be changed at runtime. Use the pgrx 0.17 `PgSharedObject` / `PgSharedMem::new_object` API (not the old `PgSharedMem` from ≤0.14) — verify against the [pgrx 0.17 shmem examples](https://github.com/pgcentralfoundation/pgrx/tree/develop/pgrx-examples/shmem)
- [x] **`subject_patterns` lookup table**
  - `_pg_ripple.subject_patterns(s BIGINT, predicates BIGINT[])` with a GIN index on `predicates`
  - Maintained by the merge worker after each generation merge (not on individual INSERTs — amortized cost)
  - Enables fast "which predicates does subject X have?" look-up for DESCRIBE queries and star-pattern rewriting in the algebra optimizer
- [x] **`object_patterns` lookup table**
  - `_pg_ripple.object_patterns(o BIGINT, predicates BIGINT[])` with a GIN index on `predicates`
  - Maintained by the merge worker alongside `subject_patterns`
  - Solves the "unbound object problem" by intercepting reverse-edge scattergun queries (`?s ?p <Object>`) in O(N) instead of forcing a `UNION ALL` across all VP tables
- [x] **Statistics**
  - `pg_ripple.stats()` JSONB: triple count, per-predicate counts, cache hit ratio, delta/main sizes
- [x] **pg_trickle integration: live statistics** *(optional, when pg_trickle is installed)*
  - `pg_ripple.enable_live_statistics()` creates `_pg_ripple.predicate_stats` and `_pg_ripple.graph_stats` stream tables
  - `pg_ripple.stats()` reads from stream tables instead of full-scanning VP tables (100–1000× faster)
  - `_pg_ripple.rare_predicate_candidates` stream table (`IMMEDIATE` mode) replaces merge-worker GROUP BY polling for VP promotion detection ([§2.8](plans/ecosystem/pg_trickle.md))
  - `_pg_ripple.vp_cardinality` stream table provides live per-predicate row counts for BGP join reordering without waiting for ANALYZE ([§2.10](plans/ecosystem/pg_trickle.md))
  - `_pg_ripple.subject_patterns` managed as a stream table — stays current between merge cycles for DESCRIBE and GIN queries ([§2.12](plans/ecosystem/pg_trickle.md))
- [x] **Change notification / CDC**
  - `pg_ripple.subscribe(pattern TEXT, channel TEXT)` — emit `NOTIFY` on triple changes matching a predicate/graph pattern
  - Thin trigger-based CDC on VP delta tables; fires on INSERT/DELETE
  - Payload: JSON with `{"op": "insert"|"delete", "s": ..., "p": ..., "o": ..., "g": ...}` (integer IDs)
  - `pg_ripple.unsubscribe(channel TEXT)` to remove subscriptions
  - Enables downstream event-driven architectures (CDC consumers, webhooks, cache invalidation)
- [x] **Concurrency correctness tests** *(partial — synchronous paths covered; concurrent bgworker + writer tests deferred)*
  - `change_notification.sql` verifies CDC trigger correctness under sequential insert/delete
  - `htap_merge.sql` verifies delta→main promotion correctness
  - `merge_edge_cases.sql` verifies edge cases: empty-delta compact, idempotency, delta-resident deletes
- [x] **Merge worker edge-case tests** *(covered by `merge_edge_cases.sql`)*
  - Merge when delta is empty (no-op, no crash) ✓
  - compact() is idempotent ✓
  - Insert after compact goes to delta and is visible immediately ✓
  - Delete delta-resident triple removes it directly (no tombstone needed) ✓
  - Delete non-existent triple returns 0 ✓
  - Multiple compacts do not multiply rows ✓
- [x] **Benchmark: concurrent read/write** (pgbench custom scripts under HTAP load)
  - Heavy concurrent insert (delta growth) + complex SPARQL queries on main partition
  - Measure merge worker latency, delta bloat growth, query latency under concurrent writes
  - Baseline: >100K triples/sec sustained bulk insert with <500 ms query latency
- [x] **Berlin SPARQL Benchmark (BSBM) execution** with HTAP workload mixing reads and writes
  - Full BSBM query mix under concurrent insert workload
  - Comparison baselines with v0.5.0 (single-table, no-HTAP) results
- [x] pg_regress: `htap_merge.sql`, `change_notification.sql`, `concurrent_write_merge.sql`, `htap_benchmarks.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/configuration.md` — major expansion: all HTAP GUCs grouped by subsystem, `shared_preload_libraries` requirement column
- [x] `user-guide/scaling.md` — HTAP architecture diagram, delta/main lifecycle, merge worker tuning
- [x] `user-guide/pre-deployment.md` — production checklist: `shared_preload_libraries`, memory estimation, ANALYZE schedule
- [x] `user-guide/sql-reference/admin.md` — `stats()`, `compact()`, `subscribe()`, `unsubscribe()`, `htap_migrate_predicate()`
- [x] `user-guide/best-practices/bulk-loading.md` expanded: HTAP delta-growth, bulk-load strategies
- [x] `reference/troubleshooting.md` expanded: merge worker not starting, delta bloat, CDC not firing
- [x] `reference/faq.md` expanded: `shared_preload_libraries`, merge worker, change notifications
- [x] `research/postgresql-deepdive.md` (mirror `plans/postgresql-triplestore-deep-dive.md`)

### Exit Criteria

Writes do not block reads. Merge worker operates correctly under concurrent writes and crash scenarios. >100K triples/sec bulk insert sustained. Change notifications fire correctly for matching patterns.

</details>

---

## v0.7.0 — SHACL Validation (Core)

**Theme**: Data integrity enforcement via W3C SHACL shapes.

> **In plain language:** SHACL is a standard way to define *data quality rules* — for example, "every Person must have exactly one email address" or "an age must be a number". When these rules are loaded, pg_ripple can automatically reject data that violates them the moment it is inserted, rather than discovering errors later. This is similar to how a spreadsheet can reject invalid entries in a cell. A validation report function lets you check existing data against the rules at any time.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SHACL parser** (Turtle-based shapes)
  - `pg_ripple.load_shacl(data TEXT)` — parse and store shapes
  - Internal shape IR stored in `_pg_ripple.shacl_shapes`
- [x] **Exact SHACL validator compilation**
  - Parse shapes to an internal IR that preserves W3C SHACL semantics
  - Compile validator plans over focus nodes and value nodes rather than reducing shapes to lossy table constraints
  - PostgreSQL constraints, triggers, and helper indices are allowed only as internal accelerators when semantics are proven equivalent for the specific shape pattern
- [x] **Synchronous validation mode**
  - Triggered on `insert_triple()` when `pg_ripple.shacl_mode = 'sync'`
  - Returns validation error immediately on constraint violation
  - Uses the same exact validator semantics as offline validation; no fast path weakens or changes SHACL meaning
- [x] **Validation report**
  - `pg_ripple.validate(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Full SHACL validation report as JSON
- [x] **SHACL management**
  - `pg_ripple.list_shapes() RETURNS TABLE`
  - `pg_ripple.drop_shape(shape_uri TEXT)`
- [x] **pg_trickle integration: SHACL violation monitors** *(optional)*
  - Simple cardinality/datatype constraints modeled as `IMMEDIATE` mode stream tables
  - Violations detected within the same transaction as the DML
  - `_pg_ripple.violation_summary` stream table aggregates dead-letter queue by shape/severity; feeds `/metrics` Prometheus endpoint without full queue scans ([§2.13](plans/ecosystem/pg_trickle.md))
- [x] pg_regress: `shacl_validation.sql`, `shacl_malformed.sql` (invalid shape definitions, circular references, undefined target classes — verify clean error messages)
- [x] **Explicit deduplication functions** (on-demand cleanup; zero insert-time overhead)
  - `pg_ripple.deduplicate_predicate(p_iri TEXT) RETURNS BIGINT` — remove duplicate `(s, o, g)` rows for a single predicate, keeping the row with the lowest SID; returns count of rows removed
  - `pg_ripple.deduplicate_all() RETURNS BIGINT` — deduplicate all predicates across dedicated VP tables and `vp_rare`; returns total rows removed
  - Runs `ANALYZE` on all affected tables; safe to call at any time
  - Typical usage: call once after a bulk load that may contain duplicate triples
- [x] **Merge-time deduplication** (`pg_ripple.dedup_on_merge` GUC, default `false`)
  - When enabled, the HTAP generation merge (`src/storage/merge.rs`) changes from a plain `UNION ALL` accumulation to a deduplicating projection using `DISTINCT ON (s, o, g) ORDER BY s, o, g, i ASC`, retaining the lowest-SID row for each logical triple
  - Deduplication happens atomically during the regular background merge cycle — zero insert-time overhead; duplicates accumulate in the delta partition and are resolved when the merge worker fires
  - Between merges, queries through the `(main EXCEPT tombstones) UNION ALL delta` view may still observe short-lived duplicates from the delta portion
  - **RDF-star interaction**: SIDs of eliminated duplicate rows are not preserved; if RDF-star annotations exist on those SIDs, the annotations become orphaned. Use explicit dedup functions instead for datasets with active statement-level annotation workloads
- [x] pg_regress: `deduplication.sql` (explicit dedup functions; merge-time dedup via `dedup_on_merge`; verifies zero duplicates after each mechanism completes)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/shacl.md` — `load_shacl`, `validate`, `list_shapes`, `drop_shape`; validation report JSON structure; `shacl_mode` GUC
- [x] `user-guide/best-practices/shacl-patterns.md` (initial: NodeShape vs PropertyShape, `sh:datatype`/`sh:minCount`/`sh:maxCount`, sync mode latency impact)
- [x] `user-guide/pre-deployment.md` expanded: SHACL mode selection, load shapes before bulk import
- [x] `reference/troubleshooting.md` expanded: insert rejected by SHACL, shape parsing failures
- [x] `user-guide/sql-reference/admin.md` expanded: `deduplicate_predicate`, `deduplicate_all`, `dedup_on_merge` GUC, merge-time dedup semantics and RDF-star interaction

### Exit Criteria

Delivered SHACL Core features are enforced at insert time with exact W3C semantics. Validation reports conform to SHACL spec. Malformed shapes are rejected with actionable error messages. Explicit deduplication functions correctly remove duplicate triples from all VP tables. Merge-time deduplication (when `dedup_on_merge = true`) produces duplicate-free `_main` tables after each merge cycle.

</details>

---

## v0.8.0 — SHACL Advanced

**Theme**: Async validation pipeline and complex shapes.

> **In plain language:** Builds on v0.7.0 by supporting more sophisticated data quality rules — for instance, "a person's address must be either a US address or a EU address (but not both)", or "if a company has more than 50 employees, it must have a compliance officer". It also adds a *background validation mode* so that checking complex rules doesn't slow down data loading — violations are flagged asynchronously and collected in a report queue.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Asynchronous validation pipeline**
  - Validation queue table: `_pg_ripple.validation_queue`
  - Background worker processes queue in batches
  - Dead letter queue for invalid triples with violation reports
  - `pg_ripple.shacl_mode = 'async'` GUC mode
- [x] **Complex shape support**
  - `sh:class` — type constraint via `rdf:type` lookup
  - `sh:node` — nested shape references
  - `sh:or` / `sh:and` / `sh:not` — logical constraint combinators
  - `sh:qualifiedValueShape` — qualified cardinality
- [x] **pg_trickle integration: multi-shape DAG validation** *(optional at runtime — pg_trickle must be installed; required in this roadmap)*
  - Multiple SHACL shapes compiled into per-shape `IMMEDIATE` pg_trickle stream tables (supported constraint types: `sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:class`); complex combinators (`sh:or`, `sh:and`, `sh:not`, `sh:qualifiedValueShape`) are not compiled to stream tables and are skipped gracefully
  - `_pg_ripple.violation_summary_dag` DAG-leaf stream table aggregates per-shape violation counts; automatically clears when upstream shape violations resolve — unlike the dead-letter queue, no manual cleanup required ([§2.13](plans/ecosystem/pg_trickle.md))
  - `pg_ripple.enable_shacl_dag_monitors()` — creates all stream tables; returns 0 with a WARNING (no ERROR) when pg_trickle is not installed
  - `pg_ripple.disable_shacl_dag_monitors()` — drops all per-shape stream tables and the summary; safe to call when none are active
  - `pg_ripple.list_shacl_dag_monitors()` — lists active DAG monitor stream tables and compiled constraints
  - `_pg_ripple.shacl_dag_monitors` catalog table tracks all created monitors
- [x] pg_regress: `shacl_advanced.sql`, `shacl_dag_monitors.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/shacl.md` expanded: async pipeline, validation queue, dead-letter queue
- [x] `user-guide/best-practices/shacl-patterns.md` expanded: `sh:or`/`sh:and`/`sh:not`, async mode for high-throughput ingestion, reading the dead-letter queue
- [x] `reference/troubleshooting.md` expanded: async violations not appearing, dead-letter queue backlog

### Exit Criteria

Async validation pipeline operational. Complex SHACL shapes validated correctly with the same semantics as synchronous validation.

</details>

---

## v0.9.0 — Serialization, Export & Interop

**Theme**: Full RDF I/O, remaining serialization formats, and Turtle/JSON-LD serialization for CONSTRUCT/DESCRIBE.

> **In plain language:** RDF data comes in several standard file formats (Turtle, RDF/XML, JSON-LD). This release completes the set so that pg_ripple can import from and export to *all* of them — making it easy to exchange data with other tools and systems. It also adds Turtle and JSON-LD output formats for SPARQL CONSTRUCT and DESCRIBE queries (which returned JSONB since v0.5.1), and RDF-star serialization support.
>
> **Effort estimate: 3–4 person-weeks** *(the hardest parts — Turtle import, N-Triples export, and CONSTRUCT/DESCRIBE JSONB — were already delivered in v0.2.0, v0.3.0, and v0.5.0)*

*Note: Turtle import and N-Triples export were delivered in v0.2.0. CONSTRUCT/DESCRIBE (JSONB output) were delivered in v0.5.1.*

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **RDF/XML parser**
  - `pg_ripple.load_rdfxml(data TEXT) RETURNS BIGINT`
- [x] **Export functions**
  - `pg_ripple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_ripple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Streaming variants returning `SETOF TEXT` for large graphs
- [x] **SPARQL CONSTRUCT / DESCRIBE serialization formats**
  - CONSTRUCT → returns triples as Turtle or JSON-LD (in addition to JSONB from v0.5.1)
  - DESCRIBE → Turtle and JSON-LD output options
- [x] **SPARQL-star in CONSTRUCT / DESCRIBE** *(builds on v0.4.0 RDF-star)*
  - CONSTRUCT can produce quoted triples in output
  - Turtle-star and N-Triples-star serialization in export functions
- [x] pg_regress: `serialization.sql`, `sparql_construct.sql`, `rdf_star_construct.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/serialization.md` — `export_turtle`, `export_jsonld`, `load_rdfxml`, streaming variants, SPARQL CONSTRUCT Turtle/JSON-LD output, RDF-star serialization
- [x] `user-guide/best-practices/data-modeling.md` expanded: interop format guide (Protégé → RDF/XML; LinkedData Platform → JSON-LD; CLI → N-Triples/N-Quads)
- [x] `reference/faq.md` expanded: supported import/export formats, JSON-LD for REST APIs

### Exit Criteria

Round-trip: load Turtle → query → export Turtle. All major RDF serialization formats supported for both import and export.

</details>

---

## v0.10.0 — Datalog Reasoning Engine

**Theme**: General-purpose rule-based inference over the triple store.

> **In plain language:** This is the "intelligence layer". Users can define logical rules like *"if A manages B and B manages C, then A indirectly manages C"* — and the system will automatically figure out all the indirect management chains. It ships with two built-in rule sets covering the standard RDF and OWL vocabularies (the common language of the Semantic Web), so it can automatically derive facts like "if a Dog is a subclass of Animal, and Rex is a Dog, then Rex is also an Animal". Rules can also express *things that must never be true* — for example, "no one can be their own manager" — acting as logical integrity constraints. This is the largest single release in the roadmap.
>
> **Effort estimate: 10–12 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

See [plans/ecosystem/datalog.md](plans/ecosystem/datalog.md) for the full design.

### Deliverables

- [x] **Rule parser** (`src/datalog/parser.rs`)
  - Turtle-flavoured Datalog syntax: `head :- body₁, body₂, … .`
  - Variables (`?x`), prefixed IRIs, literals, named graph scoping (`GRAPH`)
  - Stratified negation via `NOT` keyword
  - Multi-head rules (`h₁, h₂ :- body .`) compiled to separate `INSERT … SELECT` statements within the same stratum
- [x] **`source` column in VP tables and `vp_rare`**
  - `source SMALLINT DEFAULT 0` added to every dedicated VP table **and to `_pg_ripple.vp_rare`** in the v0.10.0 migration
  - `0` = explicitly asserted; `1` = derived (inferred by Datalog rules)
  - Enables filtering out inferred triples at scan time without a join
  - Migration script uses `ALTER TABLE … ADD COLUMN source SMALLINT NOT NULL DEFAULT 0` for each VP table and for `vp_rare`; zero-downtime because PostgreSQL fast-path adds the column with the stored default without rewriting the table
- [x] **Tiered hot/cold dictionary** (`src/dictionary/hot.rs`)
  - `_pg_ripple.resources_hot` (UNLOGGED) holds IRIs ≤512B and all predicate/prefix IRIs — the working set that fits in shared buffers
  - Full `resources` table unchanged; encoder checks hot table first
  - `pg_prewarm` warms the hot table at server start via `_PG_init`
  - Dramatically reduces random I/O for the most-accessed terms at large scale (100M+ triples)
- [x] **Stratification engine** (`src/datalog/stratify.rs`)
  - Predicate dependency graph with positive/negative edges
  - SCC-based stratification with clear error messages for unstratifiable programs
- [x] **SQL compiler** (`src/datalog/compiler.rs`)
  - Non-recursive rules → `INSERT … SELECT … ON CONFLICT DO NOTHING`
  - Recursive rules → `WITH RECURSIVE … CYCLE`
  - Negation → `NOT EXISTS` (higher strata only)
  - All constants dictionary-encoded before SQL generation (integer joins everywhere)
- [x] **Arithmetic built-ins**
  - Comparison operators (`>`, `>=`, `<`, `<=`, `=`, `!=`) → SQL `WHERE` clause expressions
  - Arithmetic expressions (`?z IS ?x + ?y`) → SQL computed columns
  - String functions (`STRLEN`, `REGEX`) → SQL `LENGTH`, `~` with dictionary decode join
- [x] **Constraint rules (integrity constraints)**
  - Empty-head rules (`:- body .`) express patterns that must never hold
  - Compile to existence checks; materialized mode → pg_trickle IMMEDIATE stream tables for in-transaction validation
  - `pg_ripple.check_constraints()` returns violations as JSONB
  - `pg_ripple.enforce_constraints` GUC: `'error'` / `'warn'` / `'off'`
  - Directly complements and extends SHACL validation
- [x] **Built-in rule sets** (`src/datalog/builtins.rs`)
  - `pg_ripple.load_rules_builtin('rdfs')` — W3C RDFS entailment (13 rules)
  - `pg_ripple.load_rules_builtin('owl-rl')` — W3C OWL 2 RL profile (~80 rules)
- [x] **On-demand execution mode** (no pg_trickle needed)
  - Derived predicates compiled to inline CTEs injected into SPARQL→SQL at query time
  - `SET pg_ripple.inference_mode = 'on_demand'`
- [x] **`dictionary_hot` incremental maintenance** *(optional, when pg_trickle is installed)*
  - Model `_pg_ripple.dictionary_hot` as a stream table over `dictionary` filtered to hot-eligible IRIs
  - New predicate and prefix-registry IRIs appear in the hot table within 30s of being encoded — no manual rebuild ([§2.9](plans/ecosystem/pg_trickle.md))
- [x] **Materialized execution mode** *(optional, requires pg_trickle)*
  - `pg_ripple.materialize_rules(schedule => '10s')` — derived predicates as stream tables
  - pg_trickle DAG scheduler respects stratum ordering automatically
- [x] **Catalog and management**
  - `_pg_ripple.rules` catalog table
  - `_pg_ripple.rule_sets` catalog: groups named rules with a `rule_hash BYTEA` (XXH3-64) for cache invalidation — re-activating a rule set with an unchanged hash resumes from prior derived state without re-derivation
  - Derived predicates registered in `_pg_ripple.predicates` with `derived = TRUE`
  - `pg_ripple.load_rules()`, `pg_ripple.list_rules()`, `pg_ripple.drop_rules()`
  - `pg_ripple.enable_rule_set(name TEXT)` / `pg_ripple.disable_rule_set(name TEXT)` — activate or deactivate a named rule set without dropping it
- [x] **SPARQL engine integration**
  - Derived VP tables transparent to query planner (same look-up path as base VP tables)
  - On-demand mode prepends CTEs to generated SQL
  - `pg_ripple.sparql(query TEXT, include_derived BOOL DEFAULT true)` — when `false`, appends `AND source = 0` to all VP table scans to exclude inferred triples (no-inference mode)
- [x] **SHACL-AF `sh:rule` bridge**
  - Detect `sh:rule` entries in loaded SHACL shapes that contain Datalog-compatible triple rules
  - Compile `sh:rule` bodies to Datalog IR and register in `_pg_ripple.rules`
  - Bidirectional: SHACL shapes inform Datalog constraints; Datalog-derived triples are visible to SHACL validation
  - `pg_ripple.load_shacl()` auto-registers any `sh:rule` triples as Datalog rules when `pg_ripple.inference_mode != 'off'`
- [x] **RDF-star integration in Datalog** *(builds on v0.4.0 RDF-star)*
  - Quoted triples can appear in Datalog rule heads and bodies
  - Enables provenance rules: `<< ?s ?p ?o >> ex:derivedBy ex:rule1 :- ?s ?p ?o, RULE(ex:rule1) .`
  - Statement identifiers (SIDs) can be used in rule bodies to annotate derived triples
- [x] pg_regress: `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_custom.sql`, `datalog_negation.sql`, `datalog_arithmetic.sql`, `datalog_constraints.sql`, `shacl_af_rule.sql`, `datalog_malformed.sql` (syntax errors, unstratifiable programs, unbound variables, cyclic rule dependencies — verify clear error messages), `rdf_star_datalog.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/datalog.md` — `load_rules`, `infer`, `list_rules`, `enable_rule_set`, `disable_rule_set`; rule syntax primer; stratification; built-in RDFS/OWL RL rule sets; `inference_mode` GUC
- [x] `user-guide/best-practices/datalog-patterns.md` — RDFS subclass/domain/range patterns, OWL RL profiles, `source` column (explicit vs inferred), rule count vs inference time
- [x] `user-guide/configuration.md` expanded: `inference_mode`, `enforce_constraints` GUCs
- [x] `reference/faq.md` expanded: OWL reasoning support, `source` column meaning

### Exit Criteria

Users can load RDFS or OWL RL rule sets (or custom rules), and SPARQL queries return inferred triples. Arithmetic built-ins filter correctly in rule bodies. Constraint rules detect and report violations (optionally rejecting transactions). Both on-demand and materialized modes operational. Stratified negation correctly validated and compiled. SHACL shapes with `sh:rule` entries are auto-compiled to Datalog rules.

</details>

---

## v0.11.0 — Incremental SPARQL Views, Datalog Views & ExtVP

**Theme**: Always-fresh materialized SPARQL and Datalog queries, plus extended vertical partitioning, via pg_trickle stream tables.

> **In plain language:** Imagine pinning a SPARQL query — or a set of Datalog reasoning rules — to a dashboard and having the results update automatically whenever the underlying data changes, without re-running the query. That's what SPARQL views and Datalog views deliver. Under the hood, only the *changed* rows are reprocessed (not the entire dataset), so updates are nearly instantaneous. Datalog views go one step further: they bundle rules and a goal pattern into a single self-contained artifact, materializing only the facts relevant to the goal. This release also adds precomputed "shortcut" tables for frequently-combined queries, making common access patterns dramatically faster. Requires the companion pg_trickle extension.
>
> **Effort estimate: 5–7 person-weeks**
>
> **pg_trickle dependency**: This release requires [pg_trickle](https://github.com/grove/pg-trickle) to be installed. pg_trickle is a production-ready companion extension (same Rust/pgrx 0.17 / PostgreSQL 18 stack) available today. pg_ripple never hard-requires pg_trickle at load time — feature parity for the core triple store is preserved without it. Functions in this release that depend on pg_trickle (`create_sparql_view`, `create_datalog_view`, ExtVP setup, etc.) detect its presence at call time and return a clear error with an install hint if it is absent. The `pg_ripple.pg_trickle_available()` function lets users and tooling check availability before calling. See [plans/ecosystem/pg_trickle.md § 3](plans/ecosystem/pg_trickle.md) for the soft-detection design.

See [plans/ecosystem/pg_trickle.md § 2.2](plans/ecosystem/pg_trickle.md) for the SPARQL views design and [plans/ecosystem/datalog.md § 15](plans/ecosystem/datalog.md) for the Datalog views design.

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SPARQL views** *(requires pg_trickle)*
  - `pg_ripple.create_sparql_view(name, sparql, schedule, decode)` — compile a SPARQL SELECT query into an always-fresh, incrementally-maintained stream table
  - `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top, minimising CDC surface
  - `pg_ripple.drop_sparql_view(name)` and `pg_ripple.list_sparql_views()` for lifecycle management
  - `_pg_ripple.sparql_views` catalog table: records original SPARQL text, generated SQL, schedule, decode mode, and stream table OID
  - Refresh mode heuristics: `IMMEDIATE` for constraint-style queries, `DIFFERENTIAL` + schedule for dashboards, `FULL` + long schedule for heavy analytics and transitive-closure property paths
- [x] **Datalog views** *(requires pg_trickle)*
  - `pg_ripple.create_datalog_view(name, rules, goal, schedule, decode)` — bundle a Datalog rule set with a goal pattern into an always-fresh, incrementally-maintained stream table
  - Alternative: `pg_ripple.create_datalog_view(name, rule_set, goal, schedule, decode)` — reference a loaded rule set by name instead of inline rules
  - `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top
  - `pg_ripple.drop_datalog_view(name)` and `pg_ripple.list_datalog_views()` for lifecycle management
  - `_pg_ripple.datalog_views` catalog table: records original rule text, goal pattern, generated SQL, schedule, decode mode, and stream table OID
  - Constraint monitoring: constraint rules (empty-head) automatically synthesize a goal; any row in the stream table is a violation. `IMMEDIATE` mode catches violations within the same transaction
  - Goal-filtered materialization: only facts relevant to the goal pattern are derived and stored, reducing write amplification compared to full-closure materialized rules
- [x] **ExtVP semi-join stream tables** *(requires pg_trickle)*
  - Manual creation of pre-computed semi-joins between frequently co-joined predicate pairs
  - SPARQL→SQL translator rewrites queries to target ExtVP tables when available
- [x] **Views over derived predicates**
  - Both SPARQL views and Datalog views can reference Datalog-derived VP tables; pg_trickle DAG handles refresh ordering
- [x] pg_regress: `sparql_views.sql`, `datalog_views.sql`, `extvp.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [ ] `user-guide/scaling.md` expanded: pg_trickle live statistics, SPARQL view refresh mode selection
- [ ] `user-guide/best-practices/sparql-patterns.md` expanded: using `create_sparql_view()` for frequently-run queries
- [ ] `research/pg-trickle.md` (mirror `plans/ecosystem/pg_trickle.md`)

### Exit Criteria

Users can create SPARQL views and Datalog views that stay incrementally up-to-date. View queries are sub-millisecond table scans. Datalog views with goal patterns materialize only goal-relevant facts. Constraint monitoring views detect violations in real time. ExtVP semi-joins improve multi-predicate star-pattern performance.

</details>

---

## v0.12.0 — SPARQL Update (Advanced)

**Theme**: W3C SPARQL 1.1 Update — pattern-based updates and graph management commands.

> **In plain language:** Building on the basic `INSERT DATA` / `DELETE DATA` support from v0.5.1, this release adds *pattern-based updates* — the ability to find-and-replace data using SPARQL patterns (e.g. "for every person without an email, add a placeholder email"). It also adds commands for managing named graphs (create, clear, drop) and loading data from a URL. This completes the full SPARQL 1.1 Update specification.
>
> **Effort estimate: 3–4 person-weeks** *(simpler than originally estimated since INSERT DATA / DELETE DATA and the Update executor were delivered in v0.5.1)*

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **DELETE/INSERT WHERE** (graph update)
  - Pattern-based update: `DELETE { … } INSERT { … } WHERE { … }`
  - Compile WHERE clause via existing SPARQL→SQL engine
  - Transactional: delete + insert in single statement
- [x] **LOAD / CLEAR / DROP / CREATE**
  - `LOAD <url>` — fetch and load remote RDF data (HTTP GET + parser)
  - `CLEAR GRAPH <g>` — delete all triples in a named graph
  - `DROP GRAPH <g>` — clear + remove graph from registry
  - `CREATE GRAPH <g>` — register a new empty named graph
- [x] pg_regress: `sparql_update_where.sql`, `sparql_graph_management.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/sparql-update.md` expanded: DELETE/INSERT WHERE, LOAD / CLEAR / DROP / CREATE graph management
- [x] `user-guide/best-practices/update-patterns.md` expanded: pattern-based update recipes, graph lifecycle management

### Exit Criteria

Full SPARQL 1.1 Update operations work correctly. Pattern-based updates compile WHERE clauses via the existing SPARQL→SQL engine.

</details>

---

## v0.13.0 — Performance Hardening

**Theme**: Optimize for production-scale workloads. Benchmark-driven improvements.

> **In plain language:** This release is about *speed*. Using the benchmarks established in v0.5.0, we measure pg_ripple's performance against known baselines and then tune it. Improvements include caching query plans so repeated queries skip redundant work, loading data in parallel, and teaching the system to use data quality rules (from v0.7.0/v0.8.0) as hints to avoid unnecessary work during queries. The target is simple queries answering in under 10 milliseconds on a dataset of 10 million facts, and bulk loading sustained at over 100,000 facts per second.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **BGP join reordering**
  - At plan time, read `pg_stats.n_distinct` and `pg_class.reltuples` for the target VP tables to estimate the selectivity of each triple pattern
  - Place the most selective pattern first in the join tree to minimize intermediate result sizes
  - Emit `SET LOCAL join_collapse_limit = 1` before the generated SQL to lock the PostgreSQL planner into the computed join order
  - **Optimizer Robustness / Fallback**: Because deriving perfect selectivity from `pg_stats.n_distinct` is fragile over multi-way self-joins, the Rust-based optimizer implements dynamic sampling or uses fallback heuristic costs (e.g. reverting to native PostgreSQL planning) if `pg_stats` suggests high cardinality uncertainty. This prevents forcing PostgreSQL into highly suboptimal plans.
  - When join columns are already sorted (e.g. after a range scan on an ordered `i64` column), emit `SET LOCAL enable_mergejoin = on` to exploit merge-join (strategy #6)
- [x] **Prepared execution and cache hardening**
  - Build on the v0.3.0 SPARQL translation cache rather than reintroducing it here
  - Evaluate prepared statements with parameter binding for generated SQL where this improves planner reuse
  - Add instrumentation and benchmarks for translation-cache hit rate, eviction behavior, and prepared-plan reuse
- [x] **Parallel query exploitation**
  - Ensure VP table queries are parallel-safe
  - Mark SQL functions as `PARALLEL SAFE` where applicable
  - Generate SQL that triggers PostgreSQL parallel workers for multi-VP-table star patterns (e.g. parallel hash joins across VP tables)
  - Verify `EXPLAIN` output shows parallel plans for queries touching 3+ VP tables
- [x] **Custom statistics for the PostgreSQL planner**
  - Run `ANALYZE` on VP tables after merge operations so the planner has accurate selectivity estimates for generated SQL
  - Provide per-predicate ndistinct and MCV statistics to guide join ordering
  - Evaluate custom statistics objects (PG18 extended statistics) on `(s, o)` pairs for correlation-aware planning
  - Consider prepared statements with parameter binding (instead of literal interpolation) so the planner can cache generic plans
- [x] **PG18 async I/O exploitation**
  - Verify BRIN scans on main partition leverage AIO
  - Tune `io_combine_limit` recommendations
- [x] **Memory optimization**
  - Profile and reduce per-query allocations
  - Optimize dictionary cache eviction strategy
- [x] **Index tuning**
  - Evaluate PG18 skip scan benefits on `(s, o)` indices
  - Add covering indices where beneficial
- [x] **Bulk load optimization**
  - Parallel dictionary encoding
  - Deferred index build with `CREATE INDEX CONCURRENTLY` post-load
- [x] **SHACL-driven query optimization**
  - The algebrizer reads loaded SHACL shapes and the predicate catalog before building the join tree, using them for costing and only for rewrites that are proven semantics-preserving
  - Shape metadata can tighten plans only when the query domain is provably identical to the validated focus-node set
  - Presence of a shape alone is insufficient to change query semantics
- [x] **pg_trickle integration: ExtVP workload advisor** *(optional, when pg_trickle is installed)*
  - `_pg_ripple.extvp_candidates` stream table aggregates predicate co-occurrence from the SPARQL query log over a rolling 1-hour window
  - Admin function `pg_ripple.recommend_extvp()` reads the stream table and lists the top N predicate pairs to pre-compute
  - `pg_ripple.sparql_explain()` surfaces recommendations inline when a query would benefit from an ExtVP ([§2.14](plans/ecosystem/pg_trickle.md))
- [x] **Benchmarking infrastructure & execution**
  - Berlin SPARQL Benchmark (BSBM) data generator integrated into test suite
  - Full BSBM query mix with timing collection and baseline comparison
  - SP2Bench subset adapted for pg_ripple
  - Custom benchmarks: star patterns, property paths, aggregates, concurrent workloads
  - Results documented in release notes and [user-guide/scaling.md](user-guide/scaling.md)
- [x] **Fuzz testing harness setup** (`cargo-fuzz` + libFuzzer)
  - Fuzz target for SPARQL→SQL pipeline (parser, algebra, SQL generation)
  - Fuzz target for Turtle parser integration
  - Fuzz target for Datalog rule parser
  - CI runs fuzz testing in nightly builds (10 minutes per target)
  - No panics, no invalid SQL, no memory safety violations
- [x] Performance regression test suite (pgbench custom scripts)
  - >100K triples/sec sustained bulk load baseline
  - <10ms simple BGP queries at 10M triples
  - <5ms cached repeat queries
  - BSBM throughput comparison with v0.5.0
- [x] pg_regress: `shacl_query_opt.sql`, `fuzz_integration.sql` (fuzz results verification)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/scaling.md` expanded: benchmark results (BSBM, SP2Bench), GUC tuning reference values for small/medium/large deployments, index strategy per workload
- [x] `user-guide/pre-deployment.md` expanded: finalize as definitive production checklist; `pg_stat_statements` enabled; `work_mem` tuning for SPARQL aggregates
- [x] `reference/troubleshooting.md` expanded: slow query diagnosis using `sparql_explain(analyze:=true)`, cache hit ratio via `stats()`

### Exit Criteria

BSBM results documented. >100K triples/sec sustained bulk load. <10ms for simple BGP queries at 10M triples. <5ms for cached repeat queries. SHACL metadata exploited only through semantics-preserving optimizer rules. PostgreSQL parallel plans verified for multi-VP-table joins.

</details>

---

## v0.14.0 — Administrative & Operational Readiness

**Theme**: Production operations tooling, upgrade paths, documentation.

> **In plain language:** Everything a system administrator needs to run pg_ripple in production. This includes maintenance commands (clean up, rebuild indexes), monitoring and diagnostics, comprehensive documentation (quickstart guide, function reference, tuning guide), and *graph-level access control* — the ability to control which database users can see or modify which named graphs. It also covers packaging (Linux packages, Docker images) so the extension is easy to install in real environments. Think of this as the "operations manual" release.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Extension upgrade scripts**
  - Tested upgrade path `0.1.0 → ... → 0.16.0`
  - `ALTER EXTENSION pg_ripple UPDATE` works for all version transitions
- [x] **pg_trickle integration: live schema extraction** *(optional, when pg_trickle is installed)*
  - `_pg_ripple.inferred_schema` stream table maintains a live class→property→cardinality summary
  - Exposed as `pg_ripple.schema_summary()` for tooling and SPARQL IDE auto-completion (v0.15.0 HTTP endpoint)
  - Serves as a starting point for automatic SHACL shape inference ([§2.15](plans/ecosystem/pg_trickle.md))
- [x] **Administrative functions**
  - `pg_ripple.vacuum()` — force merge + VACUUM on VP tables
  - `pg_ripple.reindex()` — rebuild all VP table indices
  - `pg_ripple.compact(keep_old BOOL DEFAULT false)` — trigger an immediate full merge across all VP tables; `keep_old := false` drops the previous generation's `_main` table immediately after the atomic rename
  - `pg_ripple.vacuum_dictionary()` — remove dictionary entries for IRIs and literals no longer referenced by any VP table row (orphaned after bulk deletes)
  - `pg_ripple.dictionary_stats()` — detailed cache metrics
  - `pg_ripple.predicate_stats()` — per-predicate triple count, table sizes
- [ ] **Logging & diagnostics**
  - Structured logging for merge operations, validation results
  - Custom `EXPLAIN` option showing SPARQL→SQL mapping (PG18 extension EXPLAIN)
- [x] **Documentation** *(see [plans/documentation.md](plans/documentation.md) for the full page-by-page specification)*
  - `user-guide/backup-restore.md`, `user-guide/contributing.md` (complete), `reference/error-reference.md` (PT001–PT799), `reference/security.md` (complete)
  - **Performance tuning guide** — dictionary cache sizing, `cache_budget` budgeting, `merge_threshold` and `vp_promotion_threshold` tuning; SHACL constraint mapping reference; Datalog rule authoring guide
- [x] **Graph-level Row-Level Security (RLS)**
  - `pg_ripple.enable_graph_rls()` — activate RLS policies on VP tables using the `g` column
  - Policy driven by a mapping table: `_pg_ripple.graph_access (role_name TEXT, graph_id BIGINT, permission TEXT)` — `'read'` / `'write'` / `'admin'`
  - `pg_ripple.grant_graph(role TEXT, graph TEXT, permission TEXT)` / `pg_ripple.revoke_graph()`
  - SPARQL queries automatically filter results to graphs the current role can read
  - Write operations (`insert_triple`, SPARQL UPDATE) enforce write permission
  - Superuser bypass via `pg_ripple.rls_bypass` GUC for admin operations
- [ ] **Packaging**
  - `cargo pgrx package` produces installable `.deb` and `.rpm`
  - Docker image with extension pre-installed
  - PGXN metadata
- [x] pg_regress: `admin_functions.sql` (vacuum, reindex, dictionary_stats, predicate_stats), `graph_rls.sql` (RLS policy enforcement, cross-role isolation, superuser bypass), `upgrade_path.sql` (install v0.1.0 → load data → sequential upgrade to current version → verify data integrity and query correctness at each step)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/backup-restore.md` — `pg_dump`/`pg_restore` procedure, VP table considerations, PITR with WAL
- [x] `reference/security.md` complete — supported versions matrix, responsible disclosure, hardening GUCs
- [x] `reference/error-reference.md` — PT001–PT799 error code table with resolution notes
- [x] `user-guide/contributing.md` complete — dev setup, test commands, PR workflow, AGENTS.md conventions, governance
- [x] `user-guide/sql-reference/admin.md` expanded: vacuum, reindex, `dictionary_stats`, `predicate_stats`

### Exit Criteria

Extension is installable, upgradable, and documented. Operational tooling sufficient for production use. Graph-level RLS enforces access control per named graph.

</details>

---

## v0.15.0 — SPARQL Protocol (HTTP Endpoint)

**Theme**: Standard HTTP API for SPARQL queries and updates.

> **In plain language:** Without this, the only way to talk to pg_ripple is through a PostgreSQL database connection (SQL). But the entire RDF ecosystem — SPARQL notebooks, visualization tools, ontology editors, web applications — expects to query a triple store over HTTP at a `/sparql` URL. This release adds a lightweight companion service that accepts standard SPARQL HTTP requests, forwards them to pg_ripple inside PostgreSQL, and returns results in all the standard formats (JSON, XML, CSV, Turtle). This is the single biggest adoption enabler: it lets pg_ripple drop in as a replacement for tools like Blazegraph, Virtuoso, or Apache Fuseki without requiring any client-side changes.
>
> **Effort estimate: 3–4 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Companion HTTP service** (`pg_ripple_http` binary)
  - Standalone Rust binary (not a PG background worker — avoids binding TCP ports inside PostgreSQL)
  - Connects to PostgreSQL via standard `libpq` / `tokio-postgres`
  - Configurable via environment variables or config file: `PG_RIPPLE_HTTP_PORT`, `PG_RIPPLE_HTTP_PG_URL`
- [x] **W3C SPARQL 1.1 Protocol compliance**
  - `GET /sparql?query=...` — URL-encoded query
  - `POST /sparql` with `application/sparql-query` body
  - `POST /sparql` with `application/x-www-form-urlencoded` body (`query=...` / `update=...`)
  - SPARQL Update via `POST /sparql` with `application/sparql-update` body
- [x] **Content negotiation**
  - `application/sparql-results+json` (default for SELECT/ASK)
  - `application/sparql-results+xml`
  - `text/csv` / `text/tab-separated-values`
  - `text/turtle` / `application/n-triples` (for CONSTRUCT/DESCRIBE)
  - `application/ld+json` (JSON-LD, for CONSTRUCT/DESCRIBE)
  - **RDF-star content types** *(builds on v0.4.0 RDF-star)*: Turtle-star and JSON-LD-star for CONSTRUCT/DESCRIBE results containing quoted triples
- [x] **Connection pooling**
  - Built-in connection pool (e.g. `deadpool-postgres`) to handle concurrent HTTP requests
  - `PG_RIPPLE_HTTP_POOL_SIZE` configuration
- [x] **Security**
  - Optional bearer token or Basic auth for access control
  - CORS configuration for browser-based SPARQL clients
  - Rate limiting GUC
- [x] **Health and metrics**
  - `GET /health` endpoint for load balancer probes
  - Prometheus-compatible `/metrics` endpoint (query count, latency histogram, error rate)
- [x] **Docker integration**
  - Docker image bundles both PostgreSQL (with pg_ripple) and the HTTP service
  - Docker Compose example with separate PG and HTTP containers
- [x] **Graph-aware bulk loader SQL functions**
  - Expose the internal `load_ntriples_into_graph()`, `load_turtle_into_graph()`, `load_rdfxml_into_graph()` Rust functions (added in v0.10.0) as public SQL functions:
    - `pg_ripple.load_ntriples_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT`
    - `pg_ripple.load_turtle_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT`
    - `pg_ripple.load_rdfxml_into_graph(data TEXT, graph_iri TEXT) RETURNS BIGINT`
    - `pg_ripple.load_ntriples_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT`
    - `pg_ripple.load_turtle_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT`
    - `pg_ripple.load_rdfxml_file_into_graph(path TEXT, graph_iri TEXT) RETURNS BIGINT`
  - Encode the `graph_iri` argument via the dictionary and delegate to the existing `*_into_graph(data, g_id)` internal functions
  - `load_rdfxml_file_into_graph` reads the file via `pg_read_file()` (superuser-only) and delegates to `load_rdfxml_into_graph`
  - Complementary to `load_nquads()` and `load_trig()` for workloads that have N-Triples / Turtle / RDF/XML files and want to load them into a specific named graph without converting the format
- [x] **Graph-aware triple deletion**
  - The existing `pg_ripple.delete_triple(s, p, o)` only deletes from the default graph (`g=0`); the underlying `storage::delete_triple(s, p, o, g_id)` already accepts a graph parameter
  - Expose: `pg_ripple.delete_triple_from_graph(s TEXT, p TEXT, o TEXT, graph_iri TEXT) RETURNS BIGINT`
  - Also expose: `pg_ripple.clear_graph(graph_iri TEXT) RETURNS BIGINT` — wraps the existing `storage::clear_graph_by_id()` internal function to delete all triples in a named graph in one call (currently only accessible via `drop_graph()` which also unregisters the graph IRI)
  - Without this, users have no SQL-level way to delete a specific triple from a named graph
- [x] **SQL API completeness gaps**
  - **Missing file-path loader**: `pg_ripple.load_rdfxml_file(path TEXT) RETURNS BIGINT` — completes the set of `*_file` variants (N-Triples, N-Quads, Turtle, TriG all have file variants); reads via `pg_read_file()` (superuser-only)
  - **Graph parameter on find_triples**: `pg_ripple.find_triples(s TEXT, p TEXT, o TEXT, graph TEXT DEFAULT NULL) RETURNS TABLE` — exposes the unused `graph` parameter in `storage::find_triples(s, p, o, graph)` so users can pattern-match within a named graph without falling back to SPARQL; `graph := NULL` queries the default graph
  - **Per-graph triple count**: `pg_ripple.triple_count_in_graph(graph_iri TEXT) RETURNS BIGINT` — returns the count of triples in a specific named graph (existing `triple_count()` returns total across all graphs)
  - **Dictionary lookup diagnostics**: `pg_ripple.decode_id_full(id BIGINT) RETURNS JSONB` — exposes `dictionary::decode_full(id)` to return `{"kind": ..., "value": ..., "language": null|"...", "datatype": null|"..."}` structured term metadata (current `decode_id()` returns only the plain string); useful for debugging and inspection
  - **Dictionary term existence check**: `pg_ripple.lookup_iri(iri TEXT) RETURNS BIGINT DEFAULT NULL` — exposes `dictionary::lookup_iri(iri)` to check whether an IRI already exists in the dictionary without encoding it (useful for test assertions, cost estimation, and introspection)
- [x] pg_regress: `sparql_protocol.sql` (protocol-level tests via `curl`), `load_into_graph.sql` (round-trip: load N-Triples / Turtle / RDF/XML into a named graph, verify via SPARQL GRAPH pattern), `graph_delete.sql` (delete_triple_from_graph, clear_graph, verify isolation from default graph), `sql_api_completeness.sql` (find_triples with graph param, triple_count_in_graph, decode_id_full, lookup_iri)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/sparql-query.md` expanded: HTTP protocol endpoint configuration, `Accept` header formats, SPARQL 1.1 Protocol conformance note
- [x] `user-guide/best-practices/sparql-patterns.md` expanded: using the HTTP endpoint from Python (`SPARQLWrapper`), Java (Jena), `curl`; SPARQL IDE / Protégé direct connection
- [x] `reference/faq.md` expanded: HTTP endpoint URL, connecting SPARQL tools directly

### Exit Criteria

Standard SPARQL clients (YASGUI, Postman, RDF4J workbench, `curl`) can query and update pg_ripple over HTTP without any pg_ripple-specific configuration. Content negotiation returns correct formats. All graph-scoped load and delete operations available as first-class SQL functions. SQL API fully exposes internal capabilities (graph parameters, per-graph counts, diagnostic functions).

</details>

---

## v0.16.0 — SPARQL Federation

**Theme**: Query remote SPARQL endpoints from within pg_ripple queries.

> **In plain language:** Federation lets a single SPARQL query combine data from pg_ripple with data from external SPARQL endpoints on the web. For example, you could ask "find all my local employees and enrich their records with data from Wikidata" — and the system will automatically fetch the remote portion, join it with local results, and return a unified answer. This is part of the SPARQL 1.1 standard (`SERVICE` keyword) and is expected by many enterprise knowledge graph workflows that integrate multiple data sources. Multiple remote calls execute in parallel when possible to minimise latency.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SPARQL `SERVICE` keyword parsing**
  - Parse `SERVICE <url> { ... }` clauses in SPARQL queries via `spargebra`
  - Support both inline service IRIs and `SERVICE ?var` (variable endpoints, with VALUES binding)
- [x] **Remote endpoint execution**
  - HTTP GET/POST to remote SPARQL endpoints using `reqwest` (async HTTP client)
  - Parse `application/sparql-results+json` and `application/sparql-results+xml` responses
  - Dictionary-encode remote results into local `i64` IDs for join compatibility
- [x] **Join integration**
  - Remote result sets injected as inline `VALUES` clauses in the generated SQL
  - **Async parallel execution**: multiple `SERVICE` clauses in a single query execute concurrently (via `tokio::join!` in pg_ripple_http, or sequential fallback in SPI context) — prevents a single slow endpoint from blocking the entire query
  - Bind-join optimisation: push bound variables from local results into remote queries to reduce remote result size
- [x] **Error handling and timeouts**
  - `pg_ripple.federation_timeout` GUC (default: 30s per SERVICE call)
  - `pg_ripple.federation_max_results` GUC (default: 10,000 rows per remote call)
  - Graceful degradation: failed SERVICE calls return empty results with a WARNING (configurable to ERROR via `pg_ripple.federation_on_error` GUC)
- [x] **Security**
  - Allowlist of permitted remote endpoints: `_pg_ripple.federation_endpoints (url TEXT, enabled BOOLEAN)`
  - `pg_ripple.register_endpoint()` / `pg_ripple.remove_endpoint()` management API
  - No outbound HTTP calls unless the endpoint is explicitly registered (defence against SSRF)
- [x] **pg_trickle integration: federation health monitoring** *(optional, when pg_trickle is installed)*
  - `_pg_ripple.federation_health` stream table aggregates a rolling 5-minute probe log per endpoint
  - Executor skips endpoints with `success_rate < 0.1` without waiting for timeout
  - `/metrics` Prometheus endpoint reads directly from `federation_health` ([§2.11](plans/ecosystem/pg_trickle.md))
- [x] **`SERVICE` → Materialized View rewrite**
  - When a `SERVICE <url>` clause references an endpoint backed by a local SPARQL view (created via `pg_ripple.create_sparql_view()`), rewrite the remote call to a direct scan of the pre-materialized stream table
  - Registered via a `local_view_name` column on `_pg_ripple.federation_endpoints` — set automatically when a SPARQL view is also registered as an endpoint
  - Eliminates HTTP overhead and enables the PostgreSQL planner to optimize the join with accurate statistics from the stream table
- [x] **HTTP endpoint integration**
  - Federation works via both SQL (`pg_ripple.sparql()`) and HTTP (`/sparql`) interfaces
- [x] pg_regress: `sparql_federation.sql`, `sparql_federation_timeout.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/federation.md` — `SERVICE` keyword, endpoint registration (`register_endpoint`, `remove_endpoint`), variable endpoints with `VALUES` binding, bind-join optimisation, `federation_timeout` / `federation_max_results` / `federation_on_error` GUCs, SSRF protection via allow-list
- [ ] `user-guide/configuration.md` expanded: `federation_timeout`, `federation_max_results`, `federation_on_error` GUCs
- [ ] `user-guide/best-practices/sparql-patterns.md` expanded: federation query patterns, `SERVICE` performance tips (push FILTERs down, limit remote result size), combining local and remote data
- [ ] `reference/faq.md` expanded: federation security model, configuring remote endpoints, timeout tuning
- [ ] `reference/troubleshooting.md` expanded: federation timeouts, SSRF errors, endpoint unreachable

### Exit Criteria

✅ **DONE** — SPARQL queries with `SERVICE` clauses correctly fetch and join data from registered remote endpoints. Sequential execution in SPI context. Timeouts and error handling work as configured. No SSRF risk — only allowlisted endpoints are contacted.

</details>

---

## v0.17.0 — JSON-LD Framing

**Theme**: Frame-driven SPARQL CONSTRUCT queries that produce structured, nested JSON-LD output.

> **In plain language:** JSON-LD Framing is a W3C standard for reshaping RDF graph data into a specific tree structure suitable for a REST API or application. Instead of returning a flat list of disconnected facts, you provide a *frame* document — a JSON template that says "I want Company objects with their employees nested inside" — and pg_ripple automatically translates that into an optimised query, fetches only the data that matches, and returns a cleanly nested JSON-LD document. This makes pg_ripple a natural back-end for Linked Data APIs and JSON-centric applications without requiring a separate framing library.
>
> Unlike a naïve approach that fetches the entire graph and post-filters it, this implementation translates the frame directly into a SPARQL CONSTRUCT query. PostgreSQL then reads only the VP tables that are touched by the join — meaning a frame targeting 3 predicates on a graph with 10,000 predicates touches 3 VP tables, not 10,000. The `jsonld_frame_to_sparql()` inspection function exposes the generated SPARQL for debugging and for users who want to customise the query further before execution.
>
> **Effort estimate: 3–4 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Prerequisites

- v0.5.1 SPARQL CONSTRUCT / DESCRIBE (JSONB output) — frame-to-SPARQL translation reuses the existing algebra and SQL generation pipeline.
- v0.9.0 JSON-LD export — the `nt_term_to_jsonld_value` helper in `src/export.rs` is reused for the embedding step.
- v0.3.0 SPARQL plan cache — framed queries benefit from cached SPARQL→SQL translation automatically.

### Deliverables

- [x] **JSON-LD Framing engine** (`src/framing/`)
  - `src/framing/mod.rs` — module root; exposes the public `frame()` entry point used by all SQL functions
  - `src/framing/frame_translator.rs` — translates a JSON-LD frame (parsed as `serde_json::Value`) into a `spargebra` CONSTRUCT algebra tree
  - `src/framing/embedder.rs` — takes flat CONSTRUCT result triples and applies the W3C embedding algorithm to produce a nested JSON-LD tree matching the frame structure
  - `src/framing/compactor.rs` — applies the `@context` from the frame to compact full IRIs to prefixed terms in the output
- [x] **Frame-to-SPARQL translation** (`src/framing/frame_translator.rs`)
  - Translate `@type` constraints → `?s a <IRI>` triple patterns in the CONSTRUCT WHERE clause
  - Translate property-value pairs with wildcard `{}` → `OPTIONAL { ?s <p> ?o }` patterns
  - Translate absent-property patterns `[]` → `OPTIONAL { ?s <p> ?o } FILTER(!bound(?o))` patterns
  - Translate `@reverse` terms → flipped BGP triple patterns (`?o <p> ?s` instead of `?s <p> ?o`)
  - Translate nested frame objects → recursive OPTIONAL joins, each level introducing a fresh variable
  - Translate `@id` matching → bind target IRI as a constant in the WHERE clause
  - Translate `@requireAll: true` → convert OPTIONAL joins to INNER joins for required properties
  - All IRI constants dictionary-encoded at translation time (integer joins in all VP table queries — no string comparisons)
  - Wildcards (`{}`) on `@type` and `@id` expand to unbound variables
- [x] **Tree-embedding algorithm** (`src/framing/embedder.rs`)
  - Implement the W3C JSON-LD 1.1 Framing §4.1 embedding algorithm over the flat CONSTRUCT result set
  - Build a subject-keyed node map from the CONSTRUCT rows (decoded to N-Triples strings)
  - Walk the frame tree recursively, embedding matching node objects as property values
  - Honour `@embed` flag: `@once` (default) — embed a node only once, use a `{"@id": "..."}` reference for subsequent occurrences; `@always` — embed every occurrence even if repeated; `@never` — always use a node reference
  - Honour `@explicit: true` — omit properties not mentioned in the frame from the output node
  - Honour `@omitDefault: true` — omit absent properties rather than outputting `null`
  - Honour `@default` values — substitute the declared default value for absent properties when `@omitDefault` is `false`
  - Reverse properties: collect subjects whose relevant predicate points to the current node and embed them under the `@reverse`-declared key
  - Named-graph scope: when `graph` is specified, restrict embedding to nodes from that named graph
- [x] **`@context` compaction** (`src/framing/compactor.rs`)
  - Extract the `@context` block from the input frame
  - Apply prefix substitution to all IRI strings in the output tree (full IRI → compact prefixed form using registered prefixes and inline `@context` mappings)
  - Inject the `@context` block as the first entry of the returned JSON-LD document
  - Fall back to full IRIs when no matching prefix is registered
- [x] **SQL functions** (`src/lib.rs`)
  - `pg_ripple.jsonld_frame_to_sparql(frame JSONB, graph TEXT DEFAULT NULL) RETURNS TEXT` — translate a frame to a SPARQL CONSTRUCT query string without executing it; primary debugging and inspection tool
  - `pg_ripple.export_jsonld_framed(frame JSONB, graph TEXT DEFAULT NULL, embed TEXT DEFAULT '@once', explicit BOOLEAN DEFAULT FALSE, ordered BOOLEAN DEFAULT FALSE) RETURNS JSONB` — primary end-user function: translate frame to CONSTRUCT, execute via the SPARQL engine, apply embedding and compaction, return framed JSON-LD
  - `pg_ripple.export_jsonld_framed_stream(frame JSONB, graph TEXT DEFAULT NULL) RETURNS SETOF TEXT` — streaming NDJSON variant (one JSON object per matched root node); avoids buffering large framed documents in memory
  - `pg_ripple.jsonld_frame(input JSONB, frame JSONB, embed TEXT DEFAULT '@once', explicit BOOLEAN DEFAULT FALSE, ordered BOOLEAN DEFAULT FALSE) RETURNS JSONB` — general-purpose framing primitive: apply the embedding algorithm to any already-expanded JSON-LD document, not necessarily from pg-ripple storage; useful for framing SPARQL CONSTRUCT results obtained via other means
- [x] **SPARQL plan cache integration**
  - The translated CONSTRUCT query string is used as the cache key in the existing `src/sparql/plan_cache.rs` translation cache
  - Repeated calls to `export_jsonld_framed()` with the same frame and graph benefit from cached SPARQL→SQL translation automatically
- [x] **Named-graph support**
  - `graph NULL` → CONSTRUCT operates over the merged graph (all `g` values across all VP tables)
  - `graph '<IRI>'` → adds `FILTER(?g = <encoded_id>)` to each VP table join in the generated CONSTRUCT
  - Frame `@graph` entry → directs the embedder to scope node matching to the named graph's node set
- [x] **Error handling**
  - Invalid frame structure (not a JSON object, unrecognised `@embed` value) → `PT700`-range serialization error with the frame property path that failed
  - Frame references an IRI not present in any VP table → empty result (standard W3C framing behaviour, not an error)
  - Frame nested deeper than `pg_ripple.max_path_depth` → `PT200`-range error reusing the existing depth limit
- [x] **Incremental framing views** (`create_framing_view`) *(requires pg_trickle)*
  - `pg_ripple.create_framing_view(name TEXT, frame JSONB, schedule TEXT DEFAULT '5s', decode BOOLEAN DEFAULT FALSE, output_format TEXT DEFAULT 'jsonld') RETURNS void` — translate the frame to a SPARQL CONSTRUCT query and register it as a pg_trickle stream table that stays incrementally up-to-date as triples are inserted or deleted
  - Stream table schema: `pg_ripple.framing_view_{name}(subject_id BIGINT, frame_tree JSONB, refreshed_at TIMESTAMPTZ)` — `subject_id` is the dictionary-encoded subject IRI; `frame_tree` is the fully embedded and compacted JSON-LD output for that root node
  - When `decode = TRUE`, a thin IRI-decoding view `pg_ripple.framing_view_{name}_decoded` is also created; the stream table itself stores integer IDs to minimise CDC surface
  - `pg_ripple.drop_framing_view(name TEXT) RETURNS void` and `pg_ripple.list_framing_views() RETURNS TABLE(name TEXT, frame JSONB, schedule TEXT, output_format TEXT, decode BOOLEAN, row_count BIGINT, last_refresh TIMESTAMPTZ, stream_table_oid OID)` for lifecycle management
  - `_pg_ripple.framing_views` catalog table: `name, frame, generated_construct, schedule, output_format, decode, stream_table_oid, created_at`
  - Refresh mode heuristics (same as `create_sparql_view`): `IMMEDIATE` for constraint-style frames (e.g. select `ex:Company` nodes that lack `ex:complianceOfficer` — any row in the view is a violation); `DIFFERENTIAL` + schedule for dashboard/API use cases (company directory refreshed every 10 s); `FULL` + long schedule for large full-graph framed exports intended for downstream consumers
  - `pg_ripple.pg_trickle_available()` check at call time — returns a clear error with an install hint when pg_trickle is absent; never raises an error at extension load time
- [x] pg_regress: `jsonld_framing.sql` (type-based selection, property wildcards, absent-property patterns `[]`, `@reverse`, `@embed @once/@always/@never`, `@explicit`, `@omitDefault`, `@default`, `@requireAll`, named-graph scope, empty frame, `jsonld_frame_to_sparql` inspection output, `jsonld_frame` general-purpose function, streaming variant), `jsonld_framing_views.sql` (create/drop/list framing views; `IMMEDIATE` constraint-mode view; `DIFFERENTIAL` dashboard view; `decode` option; pg_trickle-absent error message)

### Supported frame features (v0.17.0)

| Feature | Supported | Notes |
|---|---|---|
| `@type` matching | ✓ | Single IRI or array of IRIs |
| `@id` matching | ✓ | Single IRI or array of IRIs |
| Property wildcard `{}` | ✓ | Matches any value for a property |
| Absent-property pattern `[]` | ✓ | Matches nodes lacking the property |
| `@reverse` properties | ✓ | Flipped triple pattern in CONSTRUCT |
| `@embed`: `@once` / `@always` / `@never` | ✓ | Full embedding control |
| `@explicit` inclusion flag | ✓ | Omit unlisted properties from output |
| `@omitDefault` flag | ✓ | Omit null-valued absent properties |
| `@default` values | ✓ | Substitute defaults for absent properties |
| `@requireAll` flag | ✓ | Turns OPTIONAL joins to INNER joins |
| `@context` compaction | ✓ | Prefix substitution from frame `@context` |
| Named graph `@graph` scoping | ✓ | Maps to `g` column filter on VP tables |
| `@omitGraph` flag | ✓ | Single root node omits `@graph` wrapper |
| Value pattern matching (`@value` / `@language` / `@type` in value objects) | ✗ | Deferred; requires full-graph scan to implement correctly |

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/serialization.md` expanded: `export_jsonld_framed`, `jsonld_frame_to_sparql`, `jsonld_frame`, `export_jsonld_framed_stream`; frame syntax primer; `@embed` / `@explicit` / `@omitDefault` / `@requireAll` flags; named graph scoping; supported feature table
- [x] `user-guide/sql-reference/framing-views.md` — `create_framing_view`, `drop_framing_view`, `list_framing_views`; stream table schema and decoding view; refresh mode selection (`IMMEDIATE` for constraints, `DIFFERENTIAL` for dashboards, `FULL` for exports); `decode` option; pg_trickle dependency and detection; worked example (company directory view refreshed every 10 s)
- [x] `user-guide/best-practices/data-modeling.md` expanded: JSON-LD Framing for REST APIs; frame-first API design pattern; using `jsonld_frame_to_sparql` for SPARQL query inspection; performance notes (frame-driven vs full-graph export); when to use `export_jsonld_framed` vs `create_framing_view`
- [x] `reference/faq.md` expanded: framing vs plain JSON-LD export; what W3C framing features are supported; value pattern matching deferral; framing views vs SPARQL views

### Exit Criteria

`export_jsonld_framed()` correctly translates a JSON-LD frame into a SPARQL CONSTRUCT query touching only the VP tables required by the frame, executes it via the existing SPARQL engine, and returns a nested JSON-LD document with correct `@context` compaction and W3C-conformant embedding semantics. The `jsonld_frame_to_sparql()` function exposes the generated CONSTRUCT query string. The `jsonld_frame()` general-purpose primitive correctly frames any expanded JSON-LD JSONB input. `create_framing_view()` creates an incrementally-maintained pg_trickle stream table whose rows stay current as triples change; the `IMMEDIATE` refresh mode correctly detects constraint violations within the same transaction. All supported frame features in the table above pass the pg_regress test suite.

</details>

---

## v0.18.0 — SPARQL CONSTRUCT, DESCRIBE & ASK Views

**Theme**: Materialize the three non-SELECT SPARQL query forms as incrementally-maintained pg_trickle stream tables.

> **In plain language:** pg_ripple already supports SPARQL CONSTRUCT, DESCRIBE, and ASK as one-shot queries. This release lets you register any of those query forms as a *live view* — a stream table that pg_trickle keeps incrementally up-to-date as triples are inserted or deleted. A CONSTRUCT view stores the derived triples it produces in a `(s, p, o, g)` table; this is ideal for materialising inferred facts, denormalised projections, or cached API responses. A DESCRIBE view stores all triples about the described resources. An ASK view stores a single `BOOLEAN` row that flips whenever the underlying pattern changes from matching to not-matching — useful for live constraint monitors and dashboard indicators.
>
> **Effort estimate: 2–3 person-weeks** *(the hard parts — CONSTRUCT/DESCRIBE SQL generation, spargebra algebra parsing, and pg_trickle stream table registration — are all already in place from v0.5.1 and v0.11.0)*

<details>
<summary>Completed items (click to expand)</summary>

### Prerequisites

- v0.5.1 SPARQL CONSTRUCT / DESCRIBE (JSONB output) — the CONSTRUCT algebra and SQL generation pipeline is reused directly.
- v0.11.0 SPARQL SELECT views — the pg_trickle stream table registration machinery (`register_stream_table`, decode-view creation, catalog tables) is extended rather than rewritten.
- v0.11.0 `pg_trickle_available()` — all three new view functions gate on the same availability check.

### Deliverables

- [x] **CONSTRUCT view support** (`src/views.rs`)
  - Extend `create_sparql_view()` to accept CONSTRUCT queries, **or** add a dedicated `create_construct_view()` function (preferred — keeps catalog tables separate and the error message explicit)
  - Parse `spargebra::Query::Construct { template, pattern, .. }`; compile `pattern` via the existing `translate_select` pipeline; expand each triple in `template` as a SQL row expression
  - Generate a `UNION ALL` SQL SELECT that returns one row per template triple per solution: `SELECT encode(s_expr) AS s, encode(p_expr) AS p, encode(o_expr) AS o, 0 AS g`; named-graph template triples include the graph term
  - All IRI/literal constants in the template dictionary-encoded at view-creation time (integer joins only — no string comparisons at refresh time)
  - Register result as a pg_trickle stream table with schema `pg_ripple.construct_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
  - When `decode = TRUE`, create a thin decoding view `pg_ripple.construct_view_{name}_decoded(s TEXT, p TEXT, o TEXT, g TEXT)` that joins `_pg_ripple.dictionary` for each column
  - Record metadata in `_pg_ripple.construct_views (name, sparql, generated_sql, schedule, decode, template_count, stream_table, created_at)`
- [x] **DESCRIBE view support** (`src/views.rs`)
  - `create_describe_view(name, sparql, schedule, decode)` — parse `spargebra::Query::Describe { variables, pattern, .. }`; compile to SQL that enumerates all triples where the described resource appears as subject (and optionally object)
  - Stream table schema: `pg_ripple.describe_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)` — same shape as CONSTRUCT views
  - `describe_strategy` GUC (already present from v0.5.1) respected: `cbd` (Concise Bounded Description) vs `symmetric_cbd`
  - Record metadata in `_pg_ripple.describe_views (name, sparql, generated_sql, schedule, decode, stream_table, created_at)`
- [x] **ASK view support** (`src/views.rs`)
  - `create_ask_view(name, sparql, schedule)` — parse `spargebra::Query::Ask { pattern, .. }`; compile to `SELECT EXISTS(...)` SQL
  - Stream table schema: `pg_ripple.ask_view_{name}(result BOOLEAN, evaluated_at TIMESTAMPTZ DEFAULT now())`
  - Record metadata in `_pg_ripple.ask_views (name, sparql, generated_sql, schedule, stream_table, created_at)`
- [x] **Lifecycle management SQL functions** (`src/lib.rs`)
  - `pg_ripple.create_construct_view(name TEXT, sparql TEXT, schedule TEXT DEFAULT '1s', decode BOOLEAN DEFAULT FALSE) RETURNS BIGINT` — returns template triple count
  - `pg_ripple.drop_construct_view(name TEXT) RETURNS void`
  - `pg_ripple.list_construct_views() RETURNS TABLE(name TEXT, sparql TEXT, generated_sql TEXT, schedule TEXT, decode BOOLEAN, template_count BIGINT, stream_table TEXT, created_at TIMESTAMPTZ)`
  - `pg_ripple.create_describe_view(name TEXT, sparql TEXT, schedule TEXT DEFAULT '1s', decode BOOLEAN DEFAULT FALSE) RETURNS void`
  - `pg_ripple.drop_describe_view(name TEXT) RETURNS void`
  - `pg_ripple.list_describe_views() RETURNS TABLE(name TEXT, sparql TEXT, generated_sql TEXT, schedule TEXT, decode BOOLEAN, stream_table TEXT, created_at TIMESTAMPTZ)`
  - `pg_ripple.create_ask_view(name TEXT, sparql TEXT, schedule TEXT DEFAULT '1s') RETURNS void`
  - `pg_ripple.drop_ask_view(name TEXT) RETURNS void`
  - `pg_ripple.list_ask_views() RETURNS TABLE(name TEXT, sparql TEXT, generated_sql TEXT, schedule TEXT, stream_table TEXT, created_at TIMESTAMPTZ)`
  - All nine functions call `pg_trickle_available()` first and raise a descriptive error with an install hint when pg_trickle is absent; never error at extension load time
- [x] **Catalog tables** (SQL migration `sql/pg_ripple--0.17.0--0.18.0.sql`)
  - `CREATE TABLE IF NOT EXISTS _pg_ripple.construct_views (...)`
  - `CREATE TABLE IF NOT EXISTS _pg_ripple.describe_views (...)`
  - `CREATE TABLE IF NOT EXISTS _pg_ripple.ask_views (...)`
- [x] **Error handling**
  - Passing a SELECT query to `create_construct_view()` → clear error: `"sparql must be a CONSTRUCT query"`
  - Passing a non-ASK query to `create_ask_view()` → clear error: `"sparql must be an ASK query"`
  - Unbound variables in CONSTRUCT template (variable present in template but not bound by the WHERE pattern) → error at view-creation time listing the unbound variables
  - Template contains a blank node (not expressible as a reusable `BIGINT` ID) → error advising the user to replace blank nodes with IRIs or skolemise them
- [x] pg_regress: `construct_views.sql` (create/drop/list; basic template; multi-triple template; named graph template; decode option; SELECT query rejected; unbound variable error; pg_trickle-absent error), `describe_views.sql` (create/drop/list; CBD vs symmetric_cbd; decode option), `ask_views.sql` (create/drop/list; result flips on insert/delete; pg_trickle-absent error)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/views.md` expanded: `create_construct_view`, `drop_construct_view`, `list_construct_views`; `create_describe_view`, `drop_describe_view`, `list_describe_views`; `create_ask_view`, `drop_ask_view`, `list_ask_views`; stream table schemas; decode views; worked examples
- [x] `user-guide/best-practices/sparql-patterns.md` expanded: when to use CONSTRUCT views vs SELECT views; materialising inference results; using ASK views as live constraint monitors

### Exit Criteria

`create_construct_view()` compiles a SPARQL CONSTRUCT query into a pg_trickle stream table whose rows reflect the CONSTRUCT output at all times; inserting or deleting triples that affect the WHERE pattern causes the stream table to update automatically. `create_describe_view()` correctly materialises the CBD of the described resources. `create_ask_view()` correctly updates the single-row result when the pattern's satisfiability changes. All three view types correctly reject wrong query forms with a clear error. The pg_trickle-absent error message is consistent with v0.11.0 behaviour. All new pg_regress tests pass.

</details>

---

## v0.19.0 — Federation Performance

**Theme**: Connection pooling, result caching, query rewriting, and throughput improvements for remote SPARQL endpoint access.

> **In plain language:** When querying remote SPARQL endpoints via `SERVICE`, every call currently creates a new HTTP connection, buffers all results in memory before processing, and makes no attempt to reduce the data fetched from the remote. This release addresses those bottlenecks: connections are reused across calls, frequently-used results are cached locally, queries are rewritten to project only the variables the outer query actually needs, multiple `SERVICE` clauses targeting the same endpoint are batched into a single HTTP request, and duplicate term encoding is eliminated. The result is significantly lower latency for federation-heavy workloads and better behaviour under load.
>
> **Effort estimate: 3–5 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Prerequisites

- v0.16.0 SPARQL Federation — the `federation.rs` executor, allowlist, health monitoring, and `federation_endpoints` catalog table are all extended here.
- v0.16.0 `_pg_ripple.federation_health` — the adaptive timeout feature reads P95 latency data from this table.

### Deliverables

- [x] **Connection pooling** (`src/sparql/federation.rs`)
  - Replace per-call `ureq::AgentBuilder::new()` with a backend-local shared agent stored in a `thread_local!` or `OnceCell`
  - Reuses TCP connections and TLS sessions across SERVICE calls within a session
  - Pool size configurable via `pg_ripple.federation_pool_size` GUC (default: 4 per endpoint, range: 1–32)
  - Reduces TCP handshake + TLS overhead for workloads with repeated calls to the same endpoint

- [x] **Result caching with TTL** (`src/sparql/federation.rs`, `_pg_ripple.federation_cache` table)
  - Cache encoded remote results keyed on `(url, XXH3-64(sparql_text))`
  - Schema: `_pg_ripple.federation_cache (url TEXT, query_hash BIGINT, result_jsonb JSONB, cached_at TIMESTAMPTZ, expires_at TIMESTAMPTZ)`
  - On cache hit, skip the HTTP call entirely and re-encode cached results via the dictionary
  - Expired rows cleaned up by the merge background worker
  - TTL configurable via `pg_ripple.federation_cache_ttl` GUC (default: 0 = disabled, range: 0–86400 seconds)
  - Particularly beneficial for semi-static reference datasets (e.g. Wikidata labels, controlled vocabularies)

- [x] **Query rewriting for data minimization** (`src/sparql/sqlgen.rs`)
  - At translation time, compute the set of variables from the SERVICE inner pattern that are actually referenced by the outer query (joins, projections, FILTERs)
  - Rewrite the SPARQL SELECT sent to the remote endpoint to project only those variables instead of `SELECT *`
  - Reduces data transfer and remote processing for patterns where only a subset of result bindings are consumed

- [x] **Partial result handling** (`src/sparql/federation.rs`)
  - When a SERVICE call delivers rows before failing (e.g. connection drop mid-stream), use however many rows were received rather than discarding them entirely
  - Emit a WARNING naming the endpoint, the rows received, and the error
  - Controlled by `pg_ripple.federation_on_partial` GUC (values: `'empty'` = discard partial results, `'use'` = use partial results; default: `'empty'`)
  - Improves resilience for federated queries where partial data is better than none

- [x] **Endpoint complexity hints** (`_pg_ripple.federation_endpoints` schema extension)
  - Add a `complexity TEXT NOT NULL DEFAULT 'normal' CHECK (complexity IN ('fast', 'normal', 'slow'))` column to `_pg_ripple.federation_endpoints`
  - Expose via `pg_ripple.register_endpoint(url, local_view_name, complexity)` and a new `pg_ripple.set_endpoint_complexity(url, complexity)` function
  - At query planning time, reorder multiple SERVICE clauses so `'fast'` endpoints execute first — enables earlier failure detection and reduces total wall-clock time for multi-endpoint queries

- [x] **Adaptive timeout** (`src/sparql/federation.rs`)
  - When `pg_ripple.federation_adaptive_timeout = on` (default: `off`), derive the effective timeout as `max(1s, p95_latency_ms * 3 / 1000)` from `_pg_ripple.federation_health`
  - Falls back to `pg_ripple.federation_timeout` when no health data is available or adaptive mode is off
  - Prevents fast endpoints from being penalised by the global timeout and slow endpoints from blocking indefinitely

- [x] **Batch SERVICE calls to the same endpoint** (`src/sparql/sqlgen.rs`)
  - Detect multiple `SERVICE <url>` clauses in a single query that target the same registered endpoint
  - Combine their inner patterns into a single `SELECT * WHERE { { pattern1 } UNION { pattern2 } }` SPARQL query
  - Issue one HTTP request instead of N, then split results back into per-clause variable bindings
  - Applied only when patterns are independent (no shared variables between clauses)

- [x] **Result deduplication at encoding stage** (`src/sparql/federation.rs`)
  - Build a per-call `HashMap<String, i64>` during `encode_results()` to avoid redundant dictionary lookups for the same term appearing in multiple rows
  - No user-visible API change; pure internal optimisation
  - Particularly effective for result sets with high-cardinality repeated values (e.g. a common subject IRI across thousands of rows)

- [x] **GUC additions** (`src/lib.rs`)
  - `pg_ripple.federation_pool_size` (INT, default: 4, range: 1–32)
  - `pg_ripple.federation_cache_ttl` (INT, default: 0, range: 0–86400 seconds; 0 = disabled)
  - `pg_ripple.federation_on_partial` (ENUM, default: `'empty'`; values: `'empty'`, `'use'`)
  - `pg_ripple.federation_adaptive_timeout` (BOOL, default: `off`)

- [x] **Migration script** (`sql/pg_ripple--0.18.0--0.19.0.sql`)
  - `ALTER TABLE _pg_ripple.federation_endpoints ADD COLUMN IF NOT EXISTS complexity TEXT NOT NULL DEFAULT 'normal' CHECK (complexity IN ('fast', 'normal', 'slow'))`
  - `CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache (url TEXT NOT NULL, query_hash BIGINT NOT NULL, result_jsonb JSONB NOT NULL, cached_at TIMESTAMPTZ NOT NULL DEFAULT now(), expires_at TIMESTAMPTZ NOT NULL, PRIMARY KEY (url, query_hash))`
  - `CREATE INDEX IF NOT EXISTS idx_federation_cache_expires ON _pg_ripple.federation_cache (expires_at)`

- [x] pg_regress: `sparql_federation_perf.sql` (cache hit/miss; TTL expiry; variable projection confirmed via explain; batch detection with two SERVICE clauses to same endpoint; complexity ordering; partial result GUC; adaptive timeout GUC boundary; deduplication correctness)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/sql-reference/federation.md` extended: new GUCs table; connection pooling notes; result caching section with TTL examples; complexity hints; variable projection rewrite behaviour; batching semantics; adaptive timeout
- [x] `user-guide/best-practices/federation-performance.md` (new page): choosing cache TTL; when to set complexity hints; designing queries to benefit from variable projection; monitoring with `federation_health` and `federation_cache`; sidecar vs in-process tradeoffs

### Exit Criteria

A federated query making repeated calls to the same endpoint is measurably faster due to connection reuse. A query with cacheable SERVICE results performs a single HTTP call across multiple executions within the TTL window. Multiple SERVICE clauses targeting the same endpoint are confirmed (via logged SPARQL text) to collapse into one HTTP request. Variable projection is confirmed by inspecting the SPARQL text sent to the endpoint. All new pg_regress tests pass.

</details>

---

## v0.20.0 — W3C Conformance & Stability Foundation

**Theme**: Standards compliance, crash safety, and production readiness preparation.

> **In plain language:** As we approach the 1.0 release, this milestone focuses on *confidence*. Instead of building new features, we verify that everything already built works *correctly* according to the official W3C standards. We run pg_ripple's SPARQL engine and SHACL validator against the W3C test suites and fix any edge cases. We test what happens when the database crashes and verify recovery is clean. We scan the code for security vulnerabilities. And we benchmark at scale (100M triples) to establish baselines. The result is a release that's ready for production users to rely on.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **W3C SPARQL 1.1 Query test suite conformance**
  - Download and run the official [W3C SPARQL 1.1 Query test suite](https://www.w3.org/2009/sparql/test-suite-20130327/)
  - Implement missing query features or fix conformance bugs
  - Document unsupported features (property functions, custom aggregate functions) with rationale
  - Verify conformance via both SQL (`pg_ripple.sparql()`) and HTTP (`/sparql` endpoint) interfaces
  - Create `tests/pg_regress/w3c_sparql_query_conformance.sql` with representative W3C test cases; mark expected failures clearly
  - Federation (`SERVICE`) conformance covered by v0.16.0; no additional work needed
  - **Target**: ≥95% of applicable W3C Query test suite passes (excluding property functions, language tags in comparisons, and other known limitations)

- [x] **W3C SPARQL 1.1 Update test suite conformance**
  - Download and run the official [W3C SPARQL 1.1 Update test suite](https://www.w3.org/2013/sparql-update-tests/)
  - Implement missing update features or fix conformance bugs
  - Document unsupported features with rationale
  - Create `tests/pg_regress/w3c_sparql_update_conformance.sql` with representative W3C test cases
  - **Target**: ≥95% of applicable W3C Update test suite passes

- [x] **W3C SHACL Core test suite conformance**
  - Download and run the official [W3C SHACL Core test suite](https://w3c.github.io/shacl/tests/)
  - Implement missing validators or fix conformance bugs
  - **Critical constraint**: Any optimization strategy used in shape compilation must preserve identical externally-visible results as the reference semantics; if optimization changes the set of violations reported, it is a regression
  - Create `tests/pg_regress/w3c_shacl_conformance.sql` with representative W3C test cases
  - Document any limitations (e.g. SHACL Advanced features not yet implemented, deferred to v0.8.0 or later)
  - **Target**: ≥95% of SHACL Core test suite passes

- [x] **Crash recovery testing framework**
  - `tests/crash_recovery/merge_during_kill.sh` — start a bulk load, kill -9 the PostgreSQL backend during HTAP generation merge, restart PostgreSQL, verify:
    - No corruption in `_pg_ripple.predicates` catalog
    - VP table data is recoverable (rows visible, no stray VACUUM marks)
    - Dictionary is consistent (no orphaned or duplicate entries)
    - Subsequent queries return correct results
  - `tests/crash_recovery/dict_during_kill.sh` — kill -9 during a high-volume dictionary encoding operation (e.g. bulk load), verify dictionary consistency
  - `tests/crash_recovery/shacl_during_violation.sh` — kill -9 during async validation queue processing, verify no violation reports are lost and no rows are orphaned
  - Run these as part of regular CI (nightly schedule, ~30 min total)
  - Document recovery procedure for production operators (backup/restore, WAL replays)

- [x] **Memory leak detection**
  - Set up `cargo pgrx test --valgrind` invocation for a curated subset of unit tests (heap allocations are the main concern; stack overflows out of scope)
  - Identify and fix any definite leaks (not just reachable at program exit)
  - Focus areas: shared-memory allocations, per-query temporary buffers, dictionary cache evictions, failed error paths
  - Document baseline leak-free status in release notes
  - CI nightly run (timeout 2 hours)

- [x] **Security review (Phase 1)**
  - **SPI query generation review**: Audit all `src/sparql/sqlgen.rs` and `src/datalog/compiler.rs` for potential SQL injection vectors
    - All IRI/literal constants must be dictionary-encoded before SQL generation
    - No string interpolation into generated SQL (`format!` only for identifiers via `format_ident!`)
    - Create a checklist document listing all unsafe patterns and their mitigations
  - **Shared memory safety review**: Audit `src/shmem.rs` and all `pgrx::PgSharedMem` usage for:
    - Data races (concurrent access without synchronization)
    - Bounds violations (buffer overflows, stack smashing)
    - Use-after-free (stale pointers after shmem recreation)
    - Create a checklist document with findings and resolutions
  - **Dictionary cache timing side-channels review**: Verify that encode/decode latency does not leak dictionary size, IRI patterns, or other sensitive metadata
  - Document findings in `reference/security.md`; create follow-up issues for Phase 2 (v0.21.0 or later) if needed

- [x] **Benchmarking at scale (100M triples)**
  - Extend BSBM benchmark infrastructure to run with 100M triples (BSBM scale factor ≥30)
  - Measure query latency, throughput, memory usage, merge worker performance
  - Publish baseline results in release notes: e.g. "Query latency: <50ms p95 on 100M triples with 4 GiB shared memory"
  - Store results artifact in CI (for regression detection in future releases)
  - Compare with v0.19.0 results to detect performance regressions
  - **Known constraint**: BSBM at 100M triples on a single 4-core developer machine will take ~4–6 hours; run nightly or on a larger CI machine

- [x] **API stability audit** (documentation only; no code changes)
  - Audit all `pg_ripple.*` SQL functions for API stability
  - Designate these as stable / guaranteed API for 1.x releases
  - Document that `_pg_ripple.*` schema is private and subject to change
  - Create `reference/api-stability.md` documenting the stability contract

- [x] **Migration script** (`sql/pg_ripple--0.19.0--0.20.0.sql`)
  - If there are schema changes from conformance fixes, add them here
  - If no schema changes are required, leave the migration script as an empty comment block with a note explaining what new functions/GUCs (if any) are provided
  - Per extension versioning conventions (AGENTS.md), the migration script must exist even if empty

- [x] pg_regress: `w3c_sparql_query_conformance.sql`, `w3c_sparql_update_conformance.sql`, `w3c_shacl_conformance.sql`, `crash_recovery_merge.sql` (basic recovery smoke test)

- [x] **100% W3C SPARQL 1.1 Query conformance** — fix all remaining known limitations:
  - `FILTER` string functions: `CONTAINS()`, `STRSTARTS()`, `STRENDS()`, `REGEX()` — translate to SQL `strpos`, `starts_with`, `right()`, `~` / `~*`
  - `FILTER NOT EXISTS { ... }` — translate to SQL `NOT EXISTS (correlated subquery)`
  - Subquery + `LIMIT` in outer JOIN — wrap the inner slice pattern in a SQL subquery with `LIMIT` applied before the outer join
  - **Target**: all assertions in `w3c_sparql_query_conformance.sql` pass with exact expected values

- [x] **100% W3C SHACL Core conformance** — fix `validate()` false-negative on conforming graphs:
  - Root cause: `value_has_datatype()` returns `false` for inline-encoded types (xsd:integer, xsd:boolean, xsd:dateTime, xsd:date) because inline IDs are never stored in the dictionary
  - Fix: detect inline IDs (`id < 0`) and determine their datatype from the inline type code without a DB round-trip
  - Additionally: plain literals (kind=KIND_LITERAL, xsd:string normalization) now correctly satisfy `sh:datatype xsd:string`
  - Additionally: `sh:in` with string literal values now encodes them via dictionary lookup instead of `lookup_iri`
  - **Target**: `validate()` returns `conforms=true` for all conforming graphs; violation detection remains 100%

- [x] **100% W3C SPARQL 1.1 Update test suite conformance** — implement full update operator coverage:
  - `USING <g>` / `WITH <g>` clauses: restrict WHERE evaluation to the specified dataset graph(s)
  - `CLEAR ALL`, `CLEAR DEFAULT`, `CLEAR NAMED` — all graph-target variants
  - `DROP ALL`, `DROP DEFAULT`, `DROP NAMED` — all graph-target variants
  - `ADD <src> TO <dst>` — copy triples from source graph to destination (source preserved)
  - `COPY <src> TO <dst>` — clear destination then copy source (source preserved)
  - `MOVE <src> TO <dst>` — copy source to destination then drop source
  - `DELETE WHERE { ... }` shorthand — pattern used as both delete template and WHERE clause
  - Multi-graph USING: `USING <g1> USING <g2>` expands to UNION of GRAPH patterns in WHERE
  - **Target**: all assertions in `w3c_sparql_update_conformance.sql` (sections 1–16) pass with exact expected values

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/w3c-conformance.md` (new page) — W3C test suite results summary, supported subset list, unsupported features with rationale, known limitations
- [x] `reference/security.md` (Phase 1 findings) — SPI injection mitigations, shared memory safety, side-channel analysis
- [x] `reference/api-stability.md` (new page) — stable API contract, `pg_ripple.*` functions, `_pg_ripple.*` schema privacy
- [x] `user-guide/backup-restore.md` expanded: crash recovery procedure, WAL replay, PITR workflow
- [x] Release notes for v0.20.0 — include BSBM 100M triple baseline results, W3C test suite summary, security audit findings

### Exit Criteria

W3C SPARQL 1.1 Query test suite: ≥95% pass rate. W3C SPARQL 1.1 Update test suite: ≥95% pass rate. W3C SHACL Core test suite: ≥95% pass rate. Crash recovery framework operational: database recovers cleanly from kill -9 during merge, bulk load, and validation. Valgrind finds no definite memory leaks. Security review Phase 1 complete: all SPI injection vectors documented and mitigated, shared memory audit complete. BSBM 100M triple baseline published. API stability contract documented.

</details>

---

## v0.21.0 — SPARQL Built-in Functions & Query Correctness

**Theme**: Implement all ~40 missing SPARQL 1.1 built-in functions, fix the FILTER silent-drop correctness hazard, and close several high-priority query-semantics bugs identified in the v0.20.0 gap analysis.

> **In plain language:** Until now, pg_ripple's SPARQL engine understood the *grammar* of standard functions like `UCASE`, `IF`, `DATATYPE`, and `isIRI` — but silently ignored them at runtime, returning too many rows instead of the correctly filtered set. This release makes those functions actually work. It also fixes several query-correctness issues that were masked by the existing conformance test suite: wrong sort-order for NULL values, `p*` paths generating phantom reflexive rows on nodes that don't participate in the property at all, and `GROUP_CONCAT` ignoring the `DISTINCT` keyword. After this release, any unsupported expression raises a clear named error rather than silently dropping the filter.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SPARQL 1.1 built-in function surface — full implementation**
  - String functions: `STR`, `STRLEN`, `SUBSTR`, `UCASE`, `LCASE`, `CONCAT`, `REPLACE`, `ENCODE_FOR_URI`, `STRLANG`, `STRDT` (in addition to `STRSTARTS`, `STRENDS`, `CONTAINS`, `REGEX` already present)
  - Type-testing predicates: `isIRI`, `isLiteral`, `isBlank`, `isNumeric`, `sameTerm`
  - Term construction and access: `IRI` (alias `URI`), `BNODE`, `LANG`, `DATATYPE`, `LANGMATCHES`
  - Numeric functions: `ABS`, `CEIL`, `FLOOR`, `ROUND`, `RAND`
  - Datetime functions: `NOW`, `YEAR`, `MONTH`, `DAY`, `HOURS`, `MINUTES`, `SECONDS`, `TIMEZONE`, `TZ`
  - Hash / UUID functions: `MD5`, `SHA1`, `SHA256`, `SHA384`, `SHA512`, `UUID`, `STRUUID`
  - Control functions: `IF`, `COALESCE`
  - Implementation strategy: decode the dictionary ID to the term value at expression-evaluation time; compile to PostgreSQL equivalents where available (`LOWER`, `UPPER`, `SUBSTR`, `MD5`, `NOW()`, `ABS`, `CEIL`, `FLOOR`, `ROUND`, `gen_random_uuid()`, etc.); datetime functions extract fields from `xsd:dateTime` literals via `to_timestamp` + `EXTRACT`; hash functions operate over the term's string representation
  - Introduce a typed `SqlExpr` intermediate representation in `src/sparql/expr.rs` replacing the current raw-`String` output from `translate_expr()` — makes the function dispatch table explicit and independently testable

- [x] **FILTER silent-drop fix**
  - Change `translate_expr()` so that an unsupported expression variant raises a structured `ERRCODE_FEATURE_NOT_SUPPORTED` error naming the unimplemented function, rather than returning `None` and silently dropping the predicate from the SQL `WHERE` clause
  - Add `pg_ripple.sparql_strict` GUC (default: `on`): when `off`, the legacy warn-and-drop behaviour is preserved for compatibility; when `on` (default from this release onwards), unsupported expressions hard-error
  - Migration script `sql/pg_ripple--0.20.0--0.21.0.sql`: register the `sparql_strict` GUC with its default

- [x] **Query correctness fixes**
  - `ORDER BY` NULL placement: append `NULLS LAST` to every `ASC` clause and `NULLS FIRST` to every `DESC` clause in the SQL generator, matching SPARQL 1.1 §15.1 semantics (unbound variables sort last in ascending order, first in descending order)
  - `GROUP_CONCAT(DISTINCT …)`: honour the `distinct` flag in `AggregateExpression::GroupConcat` — emit `STRING_AGG(DISTINCT …, sep)` rather than silently dropping the deduplication
  - `p*` (ZeroOrMore) reflexive rows: restrict the zero-hop identity row to subjects that actually appear in the predicate's VP tables, preventing spurious reflexive paths for all nodes in the graph
  - Property-path cycle detection: change `CYCLE o SET _is_cycle USING _cycle_path` to `CYCLE s, o SET _is_cycle USING _cycle_path` in all `WITH RECURSIVE` path CTEs — prevents false cycle detection in DAGs that have shared intermediate nodes
  - Self-join dedup key: replace the `format!("{tp}")` Debug-string key in BGP pattern deduplication with a structural `(s_term_id, p_term_id, o_term_id)` tuple so that only genuinely identical patterns are collapsed
  - `REDUCED` semantics: implemented as `DISTINCT`, which is within the SPARQL 1.1 specification; documented in `reference/sparql-reference.md`

- [x] **SPARQL property path & federation completeness**
  - Negated property sets `!(p1|p2|…)`: compile to an anti-join scanning all VP tables; correctly excludes the listed predicates
  - `SERVICE SILENT`: when the `silent` flag is set on a `SERVICE` block, federation errors return an empty result set rather than propagating the error

- [x] **W3C conformance test assertions updated**
  - All `count(*) >= 0 AS label_no_error` shims replaced with real value-checking assertions in `w3c_sparql_query_conformance.sql`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/sparql-functions.md` (new page) — every SPARQL 1.1 built-in function, implementation status, PostgreSQL equivalent used, and known limitations
- [x] `user-guide/sparql-reference.md` updated with complete function table and `sparql_strict` GUC guidance
- [x] `reference/w3c-conformance.md` updated — replace `label_no_error` placeholder entries with accurate pass / skip / fail classification
- [x] Release notes for v0.21.0 — list every newly implemented function; highlight the FILTER silent-drop fix

### Exit Criteria

Every SPARQL 1.1 built-in function from the W3C SPARQL 1.1 Appendix A either works correctly or raises a named `ERRCODE_FEATURE_NOT_SUPPORTED` error — never silently drops. `w3c_sparql_query_conformance.sql` passes with real value-checking assertions (no `>= 0` shims). `sparql_builtins.sql` passes for all implemented functions. `ORDER BY` NULL placement, property-path cycle detection on a DAG, ZeroOrMore scope restriction, and `GROUP_CONCAT DISTINCT` each have a dedicated passing regression test. `property_path_negated.sql` passes for single and multi-predicate negated sets. `service_silent.sql` returns zero rows rather than an error on an unreachable `SERVICE SILENT` endpoint. `reference/sparql-reference.md` documents the `REDUCED` → `DISTINCT` equivalence choice.

</details>

---

## v0.22.0 — Storage Correctness & Security Hardening

**Theme**: Fix the critical data-integrity issues in the storage layer (dictionary cache rollback, HTAP merge races, shmem cache thrashing, rare-predicate promotion race) and close the security gaps in the HTTP companion service and privilege model identified in the v0.20.0 gap analysis.

> **In plain language:** This release addresses issues that could silently corrupt data or create security vulnerabilities in production deployments. The most important fix: if a database transaction is rolled back, pg_ripple's internal term-ID cache now correctly discards the rolled-back entries — previously, stale IDs could be planted into the triple store, creating phantom references that make facts disappear or return the wrong data. Two race conditions in the background merge process that could cause deleted facts to reappear, or queries to error mid-merge, are also closed. The internal shared-memory cache is redesigned to handle large vocabularies without thrashing. On the security side, the HTTP companion service's rate-limiting finally works, error messages no longer leak internal database details to API clients, and the `_pg_ripple` internal schema is explicitly locked away from unprivileged roles.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Dictionary cache rollback correctness** (critical fix C-2)
  - Register `RegisterXactCallback` and `RegisterSubXactCallback` during `_PG_init` — on `XACT_EVENT_ABORT` and `XACT_EVENT_PARALLEL_ABORT`, drain both `ENCODE_CACHE` and `DECODE_CACHE` thread-local LRU caches so rolled-back term IDs cannot be served to future encode calls in the same backend session
  - Stamp a per-backend epoch counter; bump on rollback; the shared-memory encode cache stores the write epoch at insertion time and rejects cache hits from a prior epoch, ensuring the shmem path is also safe
  - New pg_regress test `dictionary_rollback.sql`: `BEGIN; pg_ripple.insert_triple(…new term…); ROLLBACK; pg_ripple.insert_triple(same term again); verify pg_ripple.decode_id(id) = original term string, not NULL`

- [x] **HTAP merge race fixes** (critical fixes C-3 and C-4)
  - C-3 (view-rename atomicity): remove the `CREATE OR REPLACE VIEW vp_N` step from the merge cycle — the view's `FROM` clause always names `vp_N_main` directly, which PG re-resolves after the rename; the `CREATE OR REPLACE VIEW` call is eliminated, closing the window between rename and view-rebuild
  - C-4 (tombstone resurrection): record `max_sid_at_snapshot` at merge-start (`currval('_pg_ripple.statement_id_seq')` before processing); at merge-end TRUNCATE, only delete tombstones with `i ≤ max_sid_at_snapshot` — tombstones for deletes that committed after the snapshot survive to the next merge cycle
  - New pg_regress test `merge_race.sql`: issue a `pg_ripple.delete_triple()` concurrently with `pg_ripple.force_merge()`; verify deleted triple does not reappear; verify no `relation does not exist` error under a concurrent `pg_ripple.sparql()` call

- [x] **Merge deduplication and `rebuild_subject_patterns` correctness** (high fixes H-6, H-7)
  - H-6 (cross-merge duplicate visibility): add a `UNIQUE (s, o, g)` constraint to `vp_{id}_delta` and change `insert_triple` to use `ON CONFLICT DO NOTHING`; update the VP view definition to carry `DISTINCT ON (s, o, g)` as a safety net for rows that crossed a merge boundary before the constraint was present — prevents a triple from appearing twice in query results when it exists in both `main` and `delta`
  - H-7 (`vp_rare` double-count in star patterns): fix `rebuild_subject_patterns()` in `src/storage/merge.rs` to enumerate only predicates that have a dedicated VP table (listed in `_pg_ripple.predicates` with a non-null `table_oid`); skip `vp_rare` as a direct scan target — `vp_rare` rows are already reachable via their per-predicate plans and must not be scanned a second time as the raw table
  - New pg_regress test `merge_dedup.sql`: insert the same triple before and after `pg_ripple.force_merge()`; verify the query returns exactly one result row; verify `triple_count` in the predicate catalog equals 1

- [x] **Shared-memory encode cache — 4-way set-associative redesign** (high fix H-1)
  - Replace the direct-mapped 4096-slot cache with a 4-way set-associative layout: 1024 sets × 4 ways — same memory footprint as before, birthday-collision rate drops from ~15% to <1% at 5k hot terms
  - LRU eviction within each 4-way set using a 2-bit age field packed into the existing `(hash_parts, id)` slot struct
  - New `pg_ripple.cache_stats()` SQL function returning `(hits BIGINT, misses BIGINT, evictions BIGINT, utilisation FLOAT)` — exposes hit rate for monitoring
  - Benchmark gate: `just bench-cache` asserts hit rate ≥ 95% on a 10k-predicate workload; CI fails on regression below 90%

- [x] **Bloom filter per-bit reference counting** (high fix H-2)
  - Replace the boolean `u64` bloom words with 8-bit saturating counters in the delta bloom shared-memory segment
  - `set_predicate_delta_bit(pred_id)`: increment both bloom counter positions (saturates at 255)
  - `clear_predicate_delta_bit(pred_id)`: decrement both counters; only clears the boolean bit when the counter reaches 0 — prevents false-negative delta skips for predicates that hash-collide with a predicate being concurrently merged

- [x] **Rare-predicate promotion atomicity** (high fixes H-3 and H-4)
  - Rewrite `promote_predicate()` to use a single atomic CTE: `WITH moved AS (DELETE FROM _pg_ripple.vp_rare WHERE p = $1 RETURNING s, o, g, i, source) INSERT INTO _pg_ripple.vp_{id}_delta (s, o, g, i, source) SELECT * FROM moved` — eliminates the two-statement window where concurrent inserts can orphan rows in `vp_rare` under a predicate that now has its own VP table
  - After the CTE, `UPDATE _pg_ripple.predicates SET triple_count = (SELECT count(*) FROM _pg_ripple.vp_{id}_delta) WHERE id = $1` to restore accurate planner statistics rather than leaving `triple_count = 0` after promotion
  - pg_regress test: load > `vp_promotion_threshold` triples for a single predicate while a concurrent transaction also inserts into `vp_rare` for that predicate; verify zero orphan rows after promotion completes

- [x] **pg_ripple_http security hardening** (high fixes H-14, H-15; medium fixes M-13, S-4)
  - Rate limiting: integrate `tower_governor` crate; `PG_RIPPLE_HTTP_RATE_LIMIT` env var is now enforced as requests-per-second per source IP (default 100 req/s); excess requests receive `429 Too Many Requests` with `Retry-After` header
  - Error redaction: replace verbatim PostgreSQL error text in HTTP 4xx/5xx responses with `{"error": "<category>", "trace_id": "<uuid>"}` JSON; log the full PG error + trace ID at server `ERROR` level — internal schema names, GUC values, and file paths are never exposed to API clients
  - Constant-time auth: replace `token != expected.as_str()` with `!constant_time_eq(token.as_bytes(), expected.as_bytes())` using the `constant_time_eq` crate
  - Federation URL scheme validation: `pg_ripple.register_endpoint()` rejects any URL whose scheme is not `http` or `https` with `ERRCODE_INVALID_PARAMETER_VALUE` — prevents `file://`, `gopher://`, or other scheme registration even though `ureq` would refuse them at connection time

- [x] **Privilege model hardening** (medium fix M-14)
  - Migration script `sql/pg_ripple--0.21.0--0.22.0.sql`: `REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC; REVOKE ALL ON ALL TABLES IN SCHEMA _pg_ripple FROM PUBLIC; REVOKE ALL ON ALL SEQUENCES IN SCHEMA _pg_ripple FROM PUBLIC;`
  - New pg_regress test `privilege_isolation.sql`: create a non-superuser role; verify `SELECT * FROM _pg_ripple.dictionary` raises permission denied; verify `SELECT * FROM pg_ripple.find_triples(NULL, NULL, NULL)` still works (public API unaffected)

- [x] **GUC bounds and merge worker signal handling** (medium fixes M-12, M-15)
  - `pg_ripple.vp_promotion_threshold`: add `min = 10` and `max = 10_000_000` constraints to the pgrx GUC definition — prevents catalog explosion at `threshold = 1` and permanent `vp_rare` lock-in at `threshold = INT_MAX`
  - Merge worker: call `BackgroundWorker::reset_latch()` immediately before `std::thread::sleep` in the error back-off path — prevents a busy-wait loop where a `SIGHUP` received during the sleep keeps `wait_latch` returning immediately on the next cycle

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/security.md` Phase 2 section: rate limiting configuration, error-redaction policy, privilege model, constant-time auth rationale, URL scheme enforcement
- [x] `user-guide/operations.md` updated: rollback safety guarantee for dictionary cache, merge correctness guarantees (tombstone epoch fence), `pg_ripple.cache_stats()` monitoring
- [x] `user-guide/upgrading.md` updated: v0.21.0→v0.22.0 privilege change (REVOKE) is safe for all existing deployments; no data migration required
- [x] Release notes for v0.22.0 — highlight dictionary-rollback fix, merge race fixes, HTTP security changes

### Exit Criteria

Rolled-back `insert_triple` cannot plant a phantom ID (`dictionary_rollback.sql` pg_regress passes). `merge_race.sql` passes with zero tombstone resurrections and zero `relation does not exist` errors under a concurrent query. `merge_dedup.sql` passes — inserting the same triple across a merge boundary returns exactly one result row. Shmem cache benchmark reports ≥ 95% hit rate at 10k hot terms. `pg_ripple_http` returns `429` when rate limit is exceeded (verified by integration test). Unprivileged role is denied `SELECT` on `_pg_ripple.*` (`privilege_isolation.sql` passes). All migration scripts from 0.1.0 through 0.22.0 run cleanly via `just test-migration`.

</details>

---

## v0.23.0 — SHACL Core Completion & SPARQL Diagnostics

**Theme**: Complete the SHACL 1.0 Core constraint set, introduce first-class SPARQL query introspection, and fix correctness issues in the Datalog engine and JSON-LD framing identified in the v0.20.0 gap analysis.

> **In plain language:** This release makes pg_ripple's data-quality rules (SHACL) useful for real-world schemas. Until now, common constraints like "this property must have a specific value" (`sh:hasValue`), "this node must have exactly this type" (`sh:nodeKind`), and "no properties outside this allowed list" (`sh:closed`) were silently ignored. They now work. Separately, a new function `pg_ripple.explain_sparql()` lets you see exactly what SQL pg_ripple generates for a SPARQL query — invaluable for diagnosing slow queries. The Datalog engine also receives three correctness fixes: arithmetic division errors now name the rule that caused them, rules with undefined variables now error at compile time rather than silently matching nothing, and cyclic negation is correctly detected.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SHACL Core constraint completion** (medium fix M-18)
  - `sh:hasValue`: verify that at least one value matches the given RDF term; compile to `EXISTS (SELECT 1 FROM vp_{id} WHERE s = $node AND o = $encoded_value)`
  - `sh:closed` + `sh:ignoredProperties`: reject triples whose predicate is not in the shape's declared property set; compile to a NOT EXISTS anti-join over all VP tables scoped to the focus node, excluding the declared properties and the ignore list
  - `sh:nodeKind`: validate that each value is an IRI, blank node, or literal as declared; discriminate using the dictionary `kind` column
  - `sh:languageIn`: compile to `lang(value) = ANY($language_tags_array)` after decoding the language tag from the literal's dictionary entry
  - `sh:uniqueLang`: use `COUNT(*) OVER (PARTITION BY lang(value))` and reject partitions with count > 1
  - `sh:lessThan` / `sh:greaterThan`: emit a comparison join between the focus node's two property values, decoding literals to numeric/date types for ordering
  - `sh:qualifiedValueShape`: `sh:qualifiedMinCount` / `sh:qualifiedMaxCount` on a nested shape — count focus-node values matching the inner shape and compare against the declared bounds
  - `sh:path` with property path expressions: extend the shape compiler to accept inverse paths (`sh:inversePath`), alternative paths (`sh:alternativePath`), sequence paths, and zero-or-more/one-or-more/zero-or-one paths — each maps to the corresponding property-path CTE already used in the SPARQL engine
  - Turtle block comment handling (M-11): add a `/* … */` block-comment stripping pass in the SHACL shape pre-processor at `src/shacl/mod.rs` before the document is handed to the Turtle parser — regex: strip `(?s)/\*.*?\*/`; allows SPARQL-style block-commented shapes to load correctly
  - New pg_regress test `shacl_core_completion.sql` — one test per new constraint with passing, failing, and edge-case triples; verified against the W3C SHACL Core test suite manifest

- [x] **SPARQL query introspection** (feature F-3 from the gap analysis)
  - New SQL function `pg_ripple.explain_sparql(query TEXT, format TEXT DEFAULT 'text') RETURNS TEXT`
  - When `format = 'sql'`: returns the generated SQL string produced by `translate_select()` without executing it — useful for manual inspection
  - When `format = 'text'` (default) or `'json'`: runs `EXPLAIN (ANALYZE, FORMAT text/json)` on the generated SQL via SPI and returns the plan output
  - When `format = 'sparql_algebra'`: returns the `spargebra` algebra tree serialised as indented text via `Debug` formatting — exposes the optimizer's view of the query
  - Security: `SECURITY DEFINER` is not used; the caller needs `SELECT` privilege on the relevant VP tables (same as `pg_ripple.sparql()`)
  - New pg_regress test `explain_sparql.sql` — verifies that the function returns non-empty output for a known-good SELECT query and does not error on edge cases (empty graph, VALUES-only query, property path query)

- [x] **SHACL query-optimization hint verification** (performance fix P-5)
  - Verify that `sh:maxCount 1` on a predicate elides `DISTINCT` in the SQL generated for SPARQL patterns using that predicate — inspect `translate_select()` in `src/sparql/sqlgen.rs` and wire the lookup against the SHACL constraint catalog if the hint is not already applied; a triple pattern on a `maxCount 1` predicate should not produce a `HashAggregate` (DISTINCT) node in the plan
  - Verify that `sh:minCount 1` on a predicate downgrades `LEFT JOIN` to `INNER JOIN` in the SQL generator for `OPTIONAL` patterns — saves a null-check pass and allows the PG planner to use more efficient join strategies
  - New pg_regress test `shacl_query_hints.sql` — load a shape with `sh:maxCount 1` and `sh:minCount 1`; run `pg_ripple.explain_sparql()` on a query using the constrained predicate; assert the plan string does not contain `HashAggregate` for the maxCount case and does not contain `Hash Left Join` for the minCount case

- [x] **Datalog engine correctness fixes** (medium fixes M-1, M-2, M-3)
  - Division by zero (M-1): wrap every arithmetic divisor in the Datalog SQL compiler with `NULLIF(expr, 0)`; emit a `NOTICE`-level message naming the failing rule head when a null propagation from division occurs
  - Unbound variables (M-2): add a compile-time check in `compile_rule()` that every variable appearing in a rule body literal is either bound by a positive body literal or explicitly declared; raise `ERRCODE_SYNTAX_ERROR` naming the variable and the rule head rather than emitting a `WHERE x = NULL` clause that silently matches nothing
  - Negation-through-cycle (M-3): replace the single-edge negation check in `stratify.rs` with full SCC (strongly-connected component) computation using Tarjan's algorithm; reject any SCC that contains a negation-back-edge with a structured error naming the cycle: `"datalog: unstratifiable negation cycle: rule A → ¬B → ¬C → A"`

- [x] **JSON-LD framing correctness fixes** (medium fixes M-4, M-5)
  - Embedder panic on empty result (M-4): replace `roots.into_iter().next().unwrap()` in `src/framing/embedder.rs` with `.ok_or_else(|| PgError::new("json-ld framing: CONSTRUCT produced no results", …))` — returns an empty JSON-LD document `{"@context": …, "@graph": []}` rather than panicking
  - Per-node visited set (M-5): add a `HashSet<NodeId>` as the third parameter of the recursive `embed_node()` function; insert the current node ID before recursing and check membership before following an edge — prevents infinite thrash on near-cyclic embedded graphs; consistent with W3C JSON-LD Framing §4.1.3

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/shacl-reference.md` updated — every newly supported constraint documented with syntax, semantics, and a worked example; mark previously-deferred constraints as now implemented
- [x] `user-guide/shacl-guide.md` updated — add a section on property path shapes (`sh:path`) showing inverse and alternative path examples
- [x] `reference/sparql-functions.md` updated — add `pg_ripple.explain_sparql()` reference with all four `format` options, example output, and note on required privileges
- [x] `user-guide/datalog-guide.md` updated — document the new division-by-zero `NOTICE`, the unbound-variable compile error, and the unstratifiable-cycle error with remediation guidance
- [x] Release notes for v0.23.0 — highlight SHACL gap closures, new `explain_sparql` function, and the three Datalog correctness fixes

### Exit Criteria

W3C SHACL Core test suite pass rate increases to ≥ 98%. `shacl_core_completion.sql` pg_regress passes for all new constraint types including the `/* … */` block-comment case. `explain_sparql.sql` passes. `shacl_query_hints.sql` passes — `explain_sparql()` confirms no spurious DISTINCT or LEFT JOIN for constrained predicates. A Datalog rule with division, an unbound variable, and a negation cycle each raise the expected named error rather than silent failure or a crash. `src/framing/embedder.rs` no longer contains `unwrap()` on the CONSTRUCT result. All migration scripts from 0.1.0 through 0.23.0 run cleanly via `just test-migration`.

</details>

---

## v0.24.0 — Semi-naive Datalog & Performance Hardening

**Theme**: Replace the naive Datalog evaluation strategy with semi-naive evaluation for large-scale inference, complete the OWL RL rule set, batch-decode SPARQL result sets, and add safety bounds to property-path recursion.

> **In plain language:** pg_ripple can derive new facts automatically from rules (Datalog). Until now, on every iteration of the rule engine, all previously derived facts were re-checked — wasteful for large datasets where most facts don't change between iterations. This release switches to "semi-naive" evaluation: each iteration only looks at *newly* derived facts from the previous pass, which can be 10–100 × faster on large ontologies. For the same reason, four missing OWL reasoning rules that affect subclass and property chains are added. Two performance improvements round out the release: returning large SPARQL result sets is sped up by decoding all term IDs in a single batch rather than one-by-one, and property-path queries (`p*`, `p+`) gain a configurable depth limit to prevent runaway recursion on highly-connected graphs.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Semi-naive Datalog evaluation** (performance fix P-3, depends on M-3 from v0.23.0)
  - Rework `src/datalog/compiler.rs` to emit ΔR maintenance queries:
    - For each derived relation `R`, maintain a delta table `Δ_R` holding only rows derived in the most recent iteration
    - The fixpoint loop re-evaluates each rule against `Δ_R` (the delta of its input relations) rather than the full `R`; newly derived rows are inserted into `Δ_R_new`; after each iteration `Δ_R ← Δ_R_new` and the loop continues while `Δ_R` is non-empty
    - Compile to a series of CTEs: `WITH delta_R AS (…), delta_R_new AS (…) INSERT INTO R SELECT * FROM delta_R_new ON CONFLICT DO NOTHING`
  - Preserve stratified evaluation order: each stratum is fully converged before the next stratum begins; semi-naive is applied within each stratum
  - Correct prerequisite: requires M-3 (stable stratification) from v0.23.0 — test pipeline enforces this ordering
  - New pg_regress test `datalog_seminaive.sql` — run RDFS closure over a 10k-triple subgraph; verify correct closure count; measure and assert iteration count is bounded by the longest derivation chain length (not the full relation size)
  - `just bench-datalog` benchmark gate: semi-naive must be ≥ 5× faster than naive on the RDFS subgraph benchmark; CI fails on regression below 3×

- [x] **OWL RL rule set completion** (medium fix M-17)
  - `cax-sco` full transitive closure: the existing partial rule handles one level of `rdfs:subClassOf`; add the transitive step so that `A subClassOf B, B subClassOf C → A subClassOf C` is derived for arbitrary chain length via the semi-naive mechanism above
  - `cls-avf`: `owl:allValuesFrom` chaining — `x ∈ C, C ≡ (∀p . D), y = p(x) → y ∈ D`; compile to a join across the `owl:allValuesFrom` VP table and the subject's type VP table
  - `prp-ifp`: inverse-functional property inference — `p is InverseFunctionalProperty, p(x, z) and p(y, z) → x = y`; compile to a self-join on `vp_{p_id}` grouping by `o`, emitting `sameAs` triples for any `s` values that collide
  - `prp-spo1`: sub-property chaining — `q subPropertyOf p, q(x, y) → p(x, y)` for derived property chains; relies on the semi-naive delta loop to propagate transitively
  - Update `src/datalog/builtins.rs` with the four new rule templates; document which OWL RL rules are now implemented vs. out of scope; update `reference/datalog-reference.md`

- [x] **Batch decode for SPARQL result sets** (architectural fix A-2, performance fix P-2)
  - Wire `batch_decode_ids()` through the SPARQL execution path in `src/sparql/sqlgen.rs`: after SPI returns a result set, collect all distinct `i64` IDs across all columns in a single pass, call `batch_decode_ids(&ids)` to resolve them in one SPI round-trip, then substitute into the result rows
  - The existing `batch_decode` infrastructure is already implemented for the bulk-load path; the change is routing the SPARQL result-building loop through the same function
  - Benchmark gate: `just bench-sparql-decode` asserts ≤ 2 SPI round-trips for a SELECT returning 1000 distinct terms; previously O(N) calls

- [x] **Property-path depth GUC** (performance fix P-4)
  - New GUC `pg_ripple.property_path_max_depth` (type: `INT`, default: `64`, min: `1`, max: `100000`)
  - Append `WHERE _depth < $pg_ripple.property_path_max_depth` to every `WITH RECURSIVE … CYCLE` property-path CTE generated by `src/sparql/property_path.rs`
  - When the depth limit is hit, emit a `WARNING`-level message: `"property path depth limit reached (max: N); some paths may be truncated"` — not an error, because SPARQL spec does not define a depth limit
  - New pg_regress test `property_path_depth.sql` — verify that a 100-hop chain is fully traversed with default limit, and that reducing the GUC to 10 truncates at 10 hops with the expected WARNING

- [x] **BRIN index migration to SID column** (medium fix M-16)
  - Migration script `sql/pg_ripple--0.23.0--0.24.0.sql`: for each existing VP main table, `DROP INDEX vp_{id}_main_s_brin; CREATE INDEX vp_{id}_main_i_brin ON _pg_ripple.vp_{id}_main USING brin (i)` — the `i` (SID) column is monotonically increasing with insertion order, giving BRIN strong correlation; the `s` (subject) column has near-random distribution and BRIN provides negligible benefit
  - Merge worker: generate the new BRIN on `i` at merge time for freshly built `main` partitions; remove the BRIN-on-`s` creation step from `create_vp_table()`
  - B-tree indices on `(s, o)` and `(o, s)` are unchanged

- [x] **Export streaming** (low fix L-6)
  - Rework `src/export.rs` Turtle/N-Triples/JSON-LD export helpers to iterate over VP tables in SID-order cursor batches (batch size: `pg_ripple.export_batch_size` GUC, default: `10000`) rather than materialising the full graph into memory
  - `DECLARE … CURSOR FOR SELECT … ORDER BY i` + `FETCH $batch_size FROM cursor` loop — each batch is serialised and flushed to `COPY` output immediately; peak memory is bounded by `batch_size × average_triple_size`

- [x] **View anti-join rewrite for HTAP query path** (performance fix P-6)
  - Replace the `EXCEPT` (sort-based set difference) in the `(main EXCEPT tombstones) UNION ALL delta` VP view with a `LEFT JOIN … WHERE t.s IS NULL` anti-join: `SELECT m.* FROM _pg_ripple.vp_{id}_main m LEFT JOIN _pg_ripple.vp_{id}_tombstones t ON m.s = t.s AND m.o = t.o AND m.g = t.g WHERE t.s IS NULL`
  - The anti-join allows the PG planner to choose hash anti-join, avoiding a materialising sort over `main`; at 10M-row `main` tables this reduces per-query overhead from O(N log N) to O(N) for tombstone filtering
  - Update all VP view definitions and the merge worker's view-rebuild template to use the anti-join form; no user-visible behaviour change
  - Benchmark gate: `just bench-htap-read` asserts a SELECT over a 1M-row `main` with 100 tombstones completes in ≤ 2× the time of the same query with zero tombstones

- [x] **BGP selectivity model improvements** (architectural improvement A-6)
  - Extend BGP reordering in `src/sparql/optimizer.rs` to factor in variable binding as a selectivity multiplier: bound subject → `0.01 × triple_count`, bound object → `0.05 × triple_count`, unbound → `triple_count` — reduces the likelihood that a poorly-ordered BGP generates a pathological SQL join order before PG's planner has a chance to reorder it
  - Document the heuristic in `reference/internals/optimizer.md` (new page) alongside the `explain_sparql()` function from v0.23.0

- [x] **Schema-aware statistics worker**
  - Extend the background merge worker to run `ANALYZE _pg_ripple.vp_{id}_main` after each successful merge — ensures the PG planner has fresh statistics on the main partition for join planning
  - For VP tables whose objects are consistently typed (all `xsd:integer`, `xsd:decimal`, or `xsd:dateTime` as detected by the dictionary `kind` column), create an extended statistics object (`CREATE STATISTICS … (dependencies, ndistinct)`) so the planner can exploit correlation for range predicates
  - New GUC `pg_ripple.auto_analyze` (BOOL, default `on`) — allows operators to disable the post-merge ANALYZE if they manage statistics manually

- [x] **SPARQL-star Update: quoted triples in CONSTRUCT and UPDATE templates**
  - Extend the CONSTRUCT template compiler in `src/sparql/sqlgen.rs` to handle `<< ?s ?p ?o >>` quoted-triple patterns in CONSTRUCT WHERE and CONSTRUCT template clauses — stored using the existing `KIND_QUOTED_TRIPLE` dictionary kind from v0.4.0
  - Extend the INSERT DATA / DELETE DATA / INSERT WHERE / DELETE WHERE parsers to accept quoted triple syntax in graph patterns and template positions
  - New pg_regress test `sparql_star_update.sql`: `INSERT DATA { << <Alice> <knows> <Bob> >> <assertedBy> <Carol> }; SELECT … WHERE { << ?s ?p ?o >> <assertedBy> ?a }` — verify the quoted triple round-trips correctly through insert and query

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/datalog-reference.md` updated — add semi-naive evaluation section explaining the ΔR mechanics, iteration bounds, and performance expectations; update OWL RL coverage table to mark `cax-sco` full, `cls-avf`, `prp-ifp`, `prp-spo1` as implemented
- [x] `reference/configuration.md` updated — document `pg_ripple.property_path_max_depth` and `pg_ripple.export_batch_size` GUCs with allowed ranges and tuning guidance
- [x] `user-guide/performance.md` updated — add "large result set decoding" section explaining the batch-decode change and expected latency improvement
- [x] Release notes for v0.24.0 — highlight semi-naive evaluation with performance numbers from the benchmark; list completed OWL RL rules; note BRIN migration and streaming export

### Exit Criteria

`datalog_seminaive.sql` passes with correct closure count and iteration count ≤ longest derivation chain. Semi-naive benchmark is ≥ 5× faster than naive on the RDFS subgraph. All four new OWL RL rules derive correct inferences in the corresponding pg_regress tests. SPARQL result-set decoding issues ≤ 2 SPI round-trips for 1000-term results (verified by the bench gate). Property path with default depth limit correctly traverses a 100-hop chain; depth-10 truncation emits the expected WARNING. `sparql_star_update.sql` passes. The HTAP anti-join benchmark completes within 2× the no-tombstone baseline. Migration scripts from 0.1.0 through 0.24.0 run cleanly via `just test-migration`.

</details>

---

## v0.25.0 — GeoSPARQL & Architectural Polish

**Theme**: Add a GeoSPARQL 1.1 geometry subset using PostGIS, stabilise the internal catalog against OID drift, and close the remaining medium- and low-priority issues from the v0.20.0 gap analysis.

> **In plain language:** PostgreSQL already understands geography — distances, containment, intersection — through the PostGIS extension. This release connects pg_ripple's RDF triple store to PostGIS so that SPARQL queries can filter and compute over geographic data: "which cities are within 50 km of Berlin?", "which roads cross this polygon?". This covers the most common GeoSPARQL functions used in open data publishing (Wikidata, LinkedGeoData, government datasets). The release also includes a set of smaller housekeeping improvements: the internal predicate catalog now stores table names instead of fragile OIDs, the HTTP companion service correctly validates federation endpoint URLs against SSRF schemes, bulk loads can now be run in strict mode that rolls back on any malformed triple, and the remaining low-priority issues from the v0.20.0 assessment are closed.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **GeoSPARQL 1.1 geometry subset** (feature F-5 from the gap analysis)
  - Prerequisite: PostGIS installed (gated with a runtime `SELECT proname FROM pg_proc WHERE proname = 'st_geomfromtext'` availability check; all geo functions return `NULL` with a `WARNING` if PostGIS is absent — no `ERROR`)
  - WKT literal support: recognize `geo:wktLiteral` datatype IRIs in the dictionary encoder; store as a regular literal; decode to a `TEXT` representation compatible with `ST_GeomFromText()`
  - Topological relation functions (compile to PostGIS equivalents):
    - `geo:sfIntersects(a, b)` → `ST_Intersects(ST_GeomFromText(a), ST_GeomFromText(b))`
    - `geo:sfContains(a, b)` → `ST_Contains(ST_GeomFromText(a), ST_GeomFromText(b))`
    - `geo:sfWithin(a, b)` → `ST_Within(ST_GeomFromText(a), ST_GeomFromText(b))`
    - `geo:sfTouches(a, b)`, `geo:sfCrosses(a, b)`, `geo:sfOverlaps(a, b)` — same pattern
  - Distance and measurement functions:
    - `geof:distance(a, b, unit)` → `ST_Distance(ST_GeomFromText(a)::geography, ST_GeomFromText(b)::geography)` with unit conversion (supports `uom:metre`, `uom:kilometre`, `uom:mile`); result encoded as `xsd:double`
    - `geof:area(a, unit)` → `ST_Area(…::geography)` with the same unit conversion
    - `geof:boundary(a)` → `ST_Boundary(ST_GeomFromText(a))` serialised back to WKT literal
  - SPARQL FILTER integration: wire all geo functions into `translate_expr()` in `src/sparql/expr.rs`; topological predicates emit a SQL boolean; distance/area/boundary emit decoded numeric/WKT values
  - New pg_regress test `geosparql.sql` — skipped automatically when PostGIS is absent (`DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'st_geomfromtext') THEN RAISE EXCEPTION …; END IF; END $$`); when PostGIS is present, verifies intersection, distance, and contains queries against a small geography dataset

- [x] **Federation cache and partial-result correctness** (high fixes H-12, H-13)
  - H-12 (cache key upgrade): replace the XXH3-64 result cache key in `src/sparql/federation.rs` with the full XXH3-128 hash — the 64-bit birthday bound (~2.1 billion distinct cached queries before 50% collision probability) is thin for a long-running server; the full 128-bit hash makes collision negligible even at very high query volumes
  - [x] H-13 (partial-result parser): add a size gate to the federation partial-result recovery path — if the truncated response exceeds `pg_ripple.federation_partial_recovery_max_bytes` (INT GUC, default: `65536`), skip partial recovery and return zero rows with a `WARNING: federation partial response too large for recovery (N bytes)`; this prevents the `rfind("},")` heuristic from truncating a valid row whose literal value contains `"}"` followed by a comma in large responses
  - New pg_regress test `federation_cache.sql` — verify that two federation calls with identical query text to different endpoints are cached independently; verify that a simulated oversized partial response exceeding the byte gate produces zero rows with the expected WARNING

- [x] **Catalog OID stability** (architectural fix A-5)
  - Add `schema_name NAME, table_name NAME` columns to `_pg_ripple.predicates` in the migration script
  - Populate on insert: `schema_name = '_pg_ripple'`, `table_name = 'vp_{id}_delta'` (the mutable partition; view name is derivable)
  - All dynamic SQL in the merge worker, query path, and admin functions now references `quote_ident(schema_name) || '.' || quote_ident(table_name)` rather than looking up OIDs — OID drift after a `pg_dump` / `pg_restore` cycle no longer silently redirects queries to the wrong relation
  - Migration script `sql/pg_ripple--0.24.0--0.25.0.sql`: `ALTER TABLE _pg_ripple.predicates ADD COLUMN schema_name NAME DEFAULT '_pg_ripple', ADD COLUMN table_name NAME; UPDATE _pg_ripple.predicates SET table_name = 'vp_' || id || '_delta';`

- [x] **Federation SSRF scheme validation** (security fix S-4)
  - `pg_ripple.register_endpoint(url TEXT)`: reject any URL whose scheme is not `http` or `https` at registration time with `ERRCODE_INVALID_PARAMETER_VALUE: "federation endpoint must use http or https scheme; got: <scheme>"` — belt-and-braces defence even though `ureq` would refuse non-HTTP at connection time

- [x] **Bulk load strict mode** (medium fix M-8)
  - Add `strict BOOLEAN DEFAULT false` parameter to `pg_ripple.load_turtle(data TEXT, strict BOOLEAN DEFAULT false)` and all other bulk-load entry points
  - When `strict = true`: any parse error or malformed triple aborts the entire `COPY`-equivalent batch with a structured error naming the line number and the offending triple; the transaction is rolled back to the savepoint established at the start of the load
  - When `strict = false` (current behaviour): malformed triples emit a `WARNING` and are skipped; partial loads are committed as before
  - New pg_regress test `bulk_load_strict.sql` — verify that a load with one malformed triple in strict mode rolls back all preceding triples; verify that the same load in lenient mode commits the well-formed triples

- [x] **Blank-node document scoping fix** (medium fix M-9)
  - Replace the `SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos()` blank-node prefix in `src/bulk_load.rs` with `nextval('_pg_ripple.statement_id_seq')` — globally unique per load call, collision-free under any level of concurrency

- [x] **Merge worker cache isolation** (architectural fix A-3)
  - Register a transaction-boundary callback in the background merge worker (analogous to the xact-end callback added in v0.22.0 for the encode cache) that clears the worker-local encode/decode LRU cache at the end of every merge transaction — prevents the worker from using stale IDs if a future migration rewrites dictionary rows

- [x] **pg_trickle version-lock probe** (architectural fix A-4)
  - In `_PG_init`, if `pg_trickle` is available, execute `SELECT extversion FROM pg_extension WHERE extname = 'pg_trickle'` and compare against the compile-time `PG_TRICKLE_TESTED_VERSION` constant; emit a `WARNING` if the installed version is newer than tested: `"pg_ripple: pg_trickle version N.N.N is newer than tested version N.N.N; incremental views may behave unexpectedly"`

- [x] **Remaining low-priority fixes**
  - CDC payload documentation (L-2): add a `decode BOOLEAN DEFAULT false` parameter to `pg_ripple.cdc_changes()` that, when true, decodes dictionary IDs to N-Triples strings in the payload; document in `user-guide/cdc.md`
  - Dependency alignment (L-3/L-4): upgrade `ureq` from v2 to v3 in `pg_ripple_http/Cargo.toml`; update `AGENTS.md` to list `oxrdf` as the canonical RDF-star parser; add `oxrdf = "0.3"` as a direct dep in `Cargo.toml`
  - GUC description strings (L-5): update every `GucBuilder::new()` `.set_description()` call in `src/lib.rs` to include the default value and valid range, e.g. `"Maximum property path recursion depth. Default: 64. Range: 1–100000."` — improves `SHOW ALL` and pg_admin discoverability
  - [x] Inline decoder defensive assert (L-7): add `debug_assert!(is_inline(id), "decode_inline called with non-inline id {id}")` at the top of `decode_inline()` in `src/dictionary/inline.rs`
  - Export literal round-trip (M-10): add a pg_regress test `export_roundtrip.sql` that inserts triples with `\uXXXX` Unicode escapes, non-ASCII literals, and control characters, then round-trips through Turtle export and import; verifies the decoded values match the originals
  - W3C conformance test classification (M-19): replace remaining `label_no_error` style assertions in the conformance test file with a formal skip-list `expected_skip` CTE; document each skip with a reason code (`UNIMPLEMENTED`, `KNOWN_LIMITATION`, or `SPEC_AMBIGUITY`); ensure the skip list shrinks to zero by v1.0.0
  - File-path bulk loader validation (S-8): all `load_*_file()` functions (`load_turtle_file`, `load_ntriples_file`, etc.) require superuser status but do not validate symlink following or path traversal beyond that gate; add a `realpath()` call in `src/bulk_load.rs` to resolve symlinks and verify the target is within `pg_read_server_files` accessible directories (matching PostgreSQL's `COPY FROM` file-access model); emit `ERRCODE_INSUFFICIENT_PRIVILEGE` if access is denied, preventing a superuser from accidentally loading files outside the protected path set

- [x] **Supplementary feature additions**
  - [x] `pg_ripple.canary()` health function: runs a battery of internal self-checks and returns a JSON object `{"merge_worker": "ok"|"stalled", "cache_hit_rate": 0.0–1.0, "catalog_consistent": true|false, "orphaned_rare_rows": N}` — suitable for ops dashboards, alerting pipelines, and CI smoke tests; `catalog_consistent` checks that VP table count in `pg_tables` matches the predicate catalog and that no `vp_rare` rows exist for promoted predicates
  - OWL ontology import: `pg_ripple.load_owl_ontology(path TEXT)` — format-detected by file extension (`.ttl`/`.nt`/`.xml`/`.rdf`/`.owl`); loads into the default graph; returns triple count
  - RDF Patch import: `pg_ripple.apply_patch(data TEXT)` — processes RDF Patch `A`/`D` operations; returns net triple delta
  - Custom aggregate registry: `pg_ripple.register_aggregate(sparql_iri TEXT, pg_function TEXT)` persists to `_pg_ripple.custom_aggregates`

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `reference/geosparql.md` (new page) — GeoSPARQL 1.1 support matrix, all implemented functions with signatures and PostGIS equivalents, PostGIS version requirements, worked examples with WKT literals
- [x] `user-guide/geospatial.md` (new page) — how to store and query geographic data in pg_ripple, linking GeoSPARQL to PostGIS, example queries for distance filtering and containment
- [x] `reference/security.md` updated — document federation scheme validation and the remediation rationale
- [x] `user-guide/bulk-load.md` updated — document the `strict` parameter with when to use it and how to diagnose partial-load failures
- [x] `reference/configuration.md` updated — document `pg_trickle` version-lock warning and the new CDC `decode` parameter
- [x] Release notes for v0.25.0 — highlight GeoSPARQL capability, catalog OID stability improvement, strict bulk load, and summary of all closed low-priority issues

### Exit Criteria

`geosparql.sql` pg_regress passes when PostGIS is present and skips cleanly when PostGIS is absent. `bulk_load_strict.sql` passes for both strict and lenient modes. Blank-node prefix uses `nextval(…)` — no wall-clock-based prefix in `src/bulk_load.rs`. `SELECT pg_ripple.register_endpoint('file:///etc/passwd')` raises `ERRCODE_INVALID_PARAMETER_VALUE`. `_pg_ripple.predicates` has `schema_name` and `table_name` columns populated. `federation_cache.sql` passes — distinct endpoints are cached independently and oversized partial responses produce zero rows with a WARNING. `pg_ripple.canary()` returns `{"catalog_consistent": true, "orphaned_rare_rows": 0}` on a healthy database. `SELECT pg_ripple.load_turtle_file('/etc/passwd')` from a superuser session raises `ERRCODE_INSUFFICIENT_PRIVILEGE` (not silently succeeding) because `/etc/passwd` is outside allowed `pg_read_server_files` directories. Migration scripts from 0.1.0 through 0.25.0 run cleanly via `just test-migration`.

</details>

---

## v0.26.0 — GraphRAG Integration

**Theme**: First-class support for using pg_ripple as the persistent knowledge graph backend for Microsoft GraphRAG.

> **In plain language:** Microsoft GraphRAG is an open-source system (32k+ GitHub stars) that uses large language models to extract a knowledge graph from documents, detects thematic clusters, and answers complex questions far better than standard vector-search RAG. By default it stores its graph as flat Parquet files on disk — static, unqueryable, and requiring a full re-index every time new documents arrive. This release makes pg_ripple a drop-in backend for GraphRAG: entities and relationships extracted by the LLM are stored as RDF triples with full SPARQL queryability, Datalog reasoning derives implicit relationships the LLM missed, SHACL shapes reject malformed extractions before they corrupt the graph, and a Python CLI bridge exports the enriched graph back to Parquet for GraphRAG's community-detection step. The result is a richer, higher-quality knowledge graph that improves GraphRAG's Local, Global, and DRIFT search accuracy — all running inside the PostgreSQL instance you already have.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/graphrag.md](plans/graphrag.md) for the full synergy analysis, architecture proposals, and integration rationale. Key findings:

- GraphRAG stores its knowledge model as Parquet files (entities, relationships, communities, community reports, text units). Every new document requires a full re-index.
- pg_ripple replaces static Parquet with a live, ACID-consistent, SPARQL-queryable triple store. New entities can be inserted incrementally via the HTAP delta partition without disrupting concurrent queries.
- Datalog + OWL-RL inference materialises relationships that LLM extraction misses (transitive hierarchies, co-membership, symmetric properties), directly improving community structure quality.
- SHACL validation rejects malformed LLM extractions (missing titles, invalid types, dangling relationship endpoints) before they propagate into community reports.
- GraphRAG's BYOG (Bring Your Own Graph) feature accepts pre-built entity/relationship tables as Parquet — pg_ripple's export functions feed directly into this pathway.

### Deliverables

- [x] **GraphRAG RDF ontology** (`sql/graphrag_ontology.ttl`)
  - Defines the RDF vocabulary for GraphRAG's knowledge model: `gr:Entity`, `gr:Relationship`, `gr:TextUnit`, `gr:Community`, `gr:CommunityReport`
  - Full property set mirroring GraphRAG's output table schemas: `gr:title`, `gr:type`, `gr:description`, `gr:frequency`, `gr:degree`, `gr:source`, `gr:target`, `gr:weight`, `gr:level`, `gr:rank`, `gr:summary`, `gr:fullContent`, `gr:hasMember`, `gr:parent`
  - Provenance properties for RDF-star metadata: `gr:confidence`, `gr:sourceTextUnit`, `gr:extractedBy`, `gr:extractedAt`
  - Namespace prefix `gr:` pre-registered via `pg_ripple.register_prefix()`
  - Loaded automatically by the example script; also loadable standalone via `pg_ripple.load_turtle_file()`

- [x] **BYOG Parquet export functions** (`src/export.rs` additions)
  - `pg_ripple.export_graphrag_entities(graph_iri TEXT, output_path TEXT) RETURNS BIGINT`
    - Executes a SPARQL SELECT to extract all `gr:Entity` triples from the named graph
    - Writes `entities.parquet` with columns: `id`, `title`, `type`, `description`, `text_unit_ids`, `frequency`, `degree` — exactly matching GraphRAG's output schema
    - Returns row count
  - `pg_ripple.export_graphrag_relationships(graph_iri TEXT, output_path TEXT) RETURNS BIGINT`
    - Extracts all `gr:Relationship` triples
    - Writes `relationships.parquet` with columns: `id`, `source`, `target`, `description`, `weight`, `combined_degree`, `text_unit_ids`
    - `combined_degree` computed as `source.degree + target.degree` via a SPARQL join
    - Returns row count
  - `pg_ripple.export_graphrag_text_units(graph_iri TEXT, output_path TEXT) RETURNS BIGINT`
    - Extracts all `gr:TextUnit` triples
    - Writes `text_units.parquet` with columns: `id`, `text`, `n_tokens`, `document_id`, `entity_ids`, `relationship_ids`
    - Returns row count
  - Implementation: use Rust's `parquet` + `arrow` crates; require superuser (same as `load_*_file` functions); validate output path via `realpath()` against writable directories

- [x] **SHACL shapes for GraphRAG quality enforcement** (`sql/graphrag_shapes.ttl`)
  - `gr:EntityShape`: `gr:title` required (1..1, string, maxLength 1000); `gr:type` required, constrained to `sh:in ("person" "organization" "geo" "event" "concept")`; `gr:description` required (1..1)
  - `gr:RelationshipShape`: `gr:source` required (1..1, `sh:class gr:Entity`); `gr:target` required (1..1, `sh:class gr:Entity`); `gr:weight` required (1..1, float, `sh:minInclusive 0.0`, `sh:maxInclusive 1.0`)
  - `gr:TextUnitShape`: `gr:text` required (1..1, string); `gr:tokenCount` required (1..1, non-negative integer)
  - Loaded via `pg_ripple.load_turtle_file()` and activated with `pg_ripple.validate()` or `pg_ripple.shacl_mode = 'sync'`

- [x] **Datalog enrichment rules** (`sql/graphrag_enrichment_rules.pl`)
  - `gr:coworker(?a, ?b)` — both entities appear as source in relationships targeting the same organization entity
  - `gr:collaborates(?a, ?b)` — both entities appear in the same text unit (share a `gr:TextUnit` via `gr:mentionsEntity`)
  - `gr:indirectReport(?leader, ?sub2)` — transitive: `?leader gr:manages ?mid`, `?mid gr:manages ?sub2`
  - `gr:relatedOrg(?a, ?b)` — two organizations share at least two entity-level relationships (co-occurrence threshold)
  - All rules loaded via `pg_ripple.load_rules()` under the rule set name `'graphrag_enrichment'`
  - OWL-RL built-in rules (`pg_ripple.load_rules_builtin('owl-rl')`) applied first for RDFS subclass/subproperty transitivity
  - Documentation: each rule annotated with its GraphRAG use case (e.g. how `gr:coworker` enriches Local Search neighborhood)

- [x] **Python CLI bridge** (`scripts/graphrag_export.py`)
  - CLI tool wrapping the export functions for users who cannot call `pg_ripple.export_graphrag_*()` directly from SQL (e.g. managed PostgreSQL services where `COPY TO` is restricted)
  - `--pg-url`: PostgreSQL connection string
  - `--graph-iri`: named graph IRI to export
  - `--output-dir`: directory for Parquet files (default: `./graphrag_output`)
  - `--enrich-with-datalog`: run `pg_ripple.infer('owl-rl')` + `pg_ripple.infer('graphrag_enrichment')` before export
  - `--validate`: run `pg_ripple.validate()` and print violations before exporting; exit with non-zero code if any violations
  - `--format`: `parquet` (default) or `csv` (for debugging)
  - Dependencies: `psycopg` (v3), `pyarrow`; no GraphRAG dependency required at export time
  - Prints row counts and output paths on success
  - Unit tests via `pytest` in `scripts/test_graphrag_export.py`

- [x] **Example walkthrough** (`examples/graphrag_byog.sql`)
  - End-to-end example: create named graph → load sample entities/relationships as Turtle → run Datalog enrichment → validate with SHACL → query enriched graph via SPARQL → export to Parquet
  - Demonstrates all four integration points: ontology, validation, reasoning, and export
  - Includes a commented BYOG `settings.yaml` snippet showing the `graphrag index` command that consumes the exported Parquet files
  - Executable as a pg_regress test: `cargo pgrx regress pg18` includes `graphrag_byog.sql`

- [x] **pg_regress tests**
  - `graphrag_ontology.sql` — load ontology, verify all prefix registrations and class/property triples are present
  - `graphrag_crud.sql` — insert sample entities and relationships as Turtle, query back via SPARQL, verify field values
  - `graphrag_enrichment.sql` — load enrichment rules, run `infer('graphrag_enrichment')`, verify `gr:coworker` and `gr:collaborates` triples are derived
  - `graphrag_shacl.sql` — attempt to load a malformed entity (missing `gr:type`) with `shacl_mode = 'sync'`, verify the INSERT is rejected with a SHACL violation report
  - `graphrag_export.sql` — export entities/relationships to `/tmp/graphrag_test_*.parquet`, verify row count matches the number of inserted entities/relationships

### Migration Script

`sql/pg_ripple--0.25.0--0.26.0.sql` — no schema changes required; all new functionality is delivered via Rust function additions and SQL files loaded by the user. Migration script contains a header comment listing the new SQL functions and their signatures.

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details.

- [x] `user-guide/graphrag.md` (new page) — step-by-step guide: install pg_ripple, load GraphRAG entities as RDF, run enrichment and validation, export to Parquet, run GraphRAG BYOG workflow; includes architecture diagram showing data flow between GraphRAG and pg_ripple
- [x] `reference/graphrag-ontology.md` (new page) — full reference for the `gr:` vocabulary: all classes, properties, and SHACL shapes with descriptions and example triples
- [x] `reference/graphrag-functions.md` (new page) — API reference for `export_graphrag_entities`, `export_graphrag_relationships`, `export_graphrag_text_units`
- [x] `user-guide/graphrag-enrichment.md` (new page) — explains Datalog enrichment for GraphRAG: which rules are built-in, how to write custom rules, how enriched triples improve community detection quality
- [x] `plans/graphrag.md` updated — mark Phase 1 (BYOG export) and Phase 2 (Datalog enrichment) as implemented; update Phase 3 status to in-progress
- [x] Release notes for v0.26.0 — highlight GraphRAG integration as the headline feature, link to the BYOG walkthrough, explain the Datalog enrichment value proposition

### Exit Criteria

`graphrag_ontology.sql`, `graphrag_crud.sql`, `graphrag_enrichment.sql`, `graphrag_shacl.sql`, and `graphrag_export.sql` all pass in `cargo pgrx regress pg18`. `pg_ripple.export_graphrag_entities()` writes a valid Parquet file readable by `pyarrow.parquet.read_table()`. Loading a malformed entity (missing `gr:type`) with `shacl_mode = 'sync'` raises a validation error. Running `pg_ripple.infer('graphrag_enrichment')` on a graph with two entities both linked to the same organization produces at least one `gr:coworker` triple. `scripts/graphrag_export.py --validate` exits non-zero when SHACL violations are present. Migration scripts from 0.1.0 through 0.26.0 run cleanly via `just test-migration`.

</details>

---

## v0.27.0 — Vector + SPARQL Hybrid: Foundation

**Theme**: Core pgvector integration — embedding storage, similarity functions, and SPARQL extension.

> **In plain language:** This release adds AI-powered semantic search to pg_ripple. Every entity in your knowledge graph can now have a *vector embedding* — a compact numerical fingerprint that captures its meaning. You can then search for entities that are semantically similar to a phrase ("find drugs similar to anti-inflammatory agents"), and combine that similarity search with precise SPARQL queries ("but only drugs approved by the FDA that don't interact with methotrexate"). This is called *hybrid search*, and it's the dominant retrieval pattern for modern AI applications. pg_ripple's unique advantage is that both the graph query and the similarity search run inside the same PostgreSQL process — with zero overhead, ACID transactions, and the query planner optimising both together. No other triplestore offers this.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/vector_sparql_hybrid.md](plans/vector_sparql_hybrid.md) for the full analysis, pgvector deep-dive, competitive landscape, and integration architecture. Key findings:

- pgvector (14k+ GitHub stars, MIT license, ships with every major managed PostgreSQL provider) is the standard PostgreSQL vector extension. Because pg_ripple and pgvector share the same PostgreSQL backend, JOINs between VP tables and vector tables execute in-process with zero serialisation overhead.
- No existing triplestore or vector database combines full SPARQL 1.1, SHACL validation, Datalog reasoning, and in-process vector similarity in a single system.
- The `_pg_ripple.embeddings` table uses dictionary-encoded `entity_id` foreign keys, enabling zero-copy joins with all VP tables.
- This is an *optional at runtime* integration: pg_ripple degrades gracefully (returns empty results with a WARNING) if pgvector is not installed.

### Deliverables

- [x] **`_pg_ripple.embeddings` table** (`sql/pg_ripple--0.26.0--0.27.0.sql`)
  - Schema: `entity_id BIGINT NOT NULL REFERENCES _pg_ripple.dictionary(id), model TEXT NOT NULL DEFAULT 'default', embedding vector(1536), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (entity_id, model)` *(optional at runtime — pgvector must be installed)*
  - **HNSW index** (default) on `(embedding vector_cosine_ops)` with configurable `m` (default 16) and `ef_construction` (default 64) parameters — best recall/speed trade-off for most workloads
  - **IVFFlat index** alternative (opt-in via GUC `pg_ripple.embedding_index_type = 'ivfflat'`) — faster build times, preferable for high-write workloads where the HNSW build cost is prohibitive; lists auto-set to `sqrt(row_count)`
  - **`halfvec` support**: the `embedding` column accepts both `vector(N)` and `halfvec(N)` via GUC `pg_ripple.embedding_precision = 'half'`; `halfvec` halves storage (2 bytes per dimension instead of 4) at marginal recall cost — recommended for > 5M entity graphs or `embedding_dimensions >= 3072`
  - **Binary quantization support**: opt-in via GUC `pg_ripple.embedding_precision = 'binary'`; stores embeddings as pgvector `bit(N)` using Hamming distance, reducing storage by ~96% (1 bit/dimension) at the cost of recall — suitable for extremely large-scale graphs (> 50M entities) where approximate results are acceptable; requires pgvector ≥ 0.7.0
  - Fallback: if pgvector is absent, the table is created with `BYTEA` as a stub column and all similarity functions return empty results with a WARNING
  - Migration script creates the table only if pgvector is detected via `SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')`

- [x] **GUC parameters** (registered in `_PG_init` in `src/lib.rs`)
  - `pg_ripple.embedding_model` (string, default `''`) — embedding model name tag stored in the `model` column
  - `pg_ripple.embedding_dimensions` (integer, default `1536`, range `1–16000`) — vector dimensions; must match the actual model output
  - `pg_ripple.embedding_api_url` (string, default `''`) — base URL for an OpenAI-compatible embedding API (e.g. `https://api.openai.com/v1`, local Ollama, vLLM)
  - `pg_ripple.embedding_api_key` (string, default `''`, superuser-only) — API key; value is masked in `pg_settings` via a superuser-only GUC flag
  - `pg_ripple.pgvector_enabled` (bool, default `true`) — runtime switch; set to `false` to disable all pgvector-dependent code paths without uninstalling the extension
  - `pg_ripple.embedding_index_type` (string, default `'hnsw'`, options `'hnsw'`|`'ivfflat'`) — controls which index type is created on `_pg_ripple.embeddings`; changing this requires `REINDEX`
  - `pg_ripple.embedding_precision` (string, default `'single'`, options `'single'`|`'half'`|`'binary'`) — `'half'` stores embeddings as `halfvec(N)` (50% storage reduction); `'binary'` stores as `bit(N)` using Hamming distance (~96% storage reduction, best for > 50M entities); requires pgvector ≥ 0.7.0

- [x] **`pg_ripple.embed_entities()` — batch embedding** (`src/sparql/embedding.rs`)
  - `pg_ripple.embed_entities(graph_iri TEXT DEFAULT NULL, model TEXT DEFAULT NULL, batch_size INT DEFAULT 100) RETURNS BIGINT`
  - Executes a SPARQL SELECT to collect entity IRIs + their `rdfs:label` (falling back to the IRI local name) from the specified graph (or all graphs if NULL)
  - Batches entity labels, calls the OpenAI-compatible API at `pg_ripple.embedding_api_url`; supports gzip-compressed responses
  - Stores results in `_pg_ripple.embeddings` via `INSERT … ON CONFLICT (entity_id, model) DO UPDATE SET embedding = EXCLUDED.embedding, updated_at = now()`
  - Returns total number of embeddings stored
  - Raises `PT601 — embedding API URL not configured` if `pg_ripple.embedding_api_url` is empty

- [x] **`pg_ripple.similar_entities()` — k-NN query** (`src/sparql/embedding.rs`)
  - `pg_ripple.similar_entities(query_text TEXT, k INT DEFAULT 10, model TEXT DEFAULT NULL) RETURNS TABLE (entity_id BIGINT, entity_iri TEXT, distance FLOAT8)` *(optional at runtime — pgvector must be installed)*
  - Encodes `query_text` to a vector via the configured embedding API
  - Executes `SELECT entity_id, embedding <=> $query_vec FROM _pg_ripple.embeddings ORDER BY 1 LIMIT k` using the pgvector `<=>` cosine distance operator
  - Decodes `entity_id` back to IRI text via the dictionary
  - Returns results sorted by ascending cosine distance (0 = identical, 2 = maximally dissimilar)

- [x] **`pg_ripple.store_embedding()` — user-supplied embeddings**
  - `pg_ripple.store_embedding(entity_iri TEXT, embedding FLOAT8[], model TEXT DEFAULT NULL) RETURNS VOID`
  - Encodes `entity_iri` via the dictionary encoder, casts `FLOAT8[]` to `vector`, and upserts into `_pg_ripple.embeddings`
  - Useful for pre-computed KGE embeddings (TransE, RotatE, ComplEx) from external pipelines; no API call needed
  - Validates that `array_length(embedding, 1)` matches `pg_ripple.embedding_dimensions`; raises `PT602 — embedding dimension mismatch` otherwise

- [x] **SPARQL `pg:similar()` extension function** (`src/sparql/functions.rs`)
  - Register `<http://pg-ripple.org/functions/similar>` as a SPARQL extension function in the function registry
  - Signature: `pg:similar(?entity, "query_text"^^xsd:string, k)` — returns cosine distance as `xsd:double`
  - Translate to SQL: the SPARQL→SQL compiler detects `pg:similar` calls in BIND expressions and emits a JOIN against `_pg_ripple.embeddings` with the `<=>` operator
  - Filter pushdown: if the SPARQL query has `FILTER(?score < threshold)`, push the threshold into the SQL `WHERE` clause to allow HNSW iterative scan pruning
  - Graceful degradation: if pgvector is absent, raises `PT603 — pgvector extension not installed` with an install hint

- [x] **`pg_ripple.refresh_embeddings()` — stale embedding invalidation** (`src/sparql/embedding.rs`)
  - `pg_ripple.refresh_embeddings(graph_iri TEXT DEFAULT NULL, model TEXT DEFAULT NULL, force BOOL DEFAULT false) RETURNS BIGINT`
  - Identifies entities whose `rdfs:label` was updated after `_pg_ripple.embeddings.updated_at` by joining `_pg_ripple.embeddings` against the label VP table's `i` (SID) sequence — higher SID implies a later write
  - Re-embeds stale entities in batches; skips entities where `updated_at` is already current unless `force = true`
  - Returns the count of re-embedded entities
  - Intended for scheduled maintenance (e.g. via `pg_cron`) and called automatically at the end of each background worker cycle when `pg_ripple.auto_embed = true`
  - Raises `PT606 — no stale embeddings found` as a NOTICE (not an ERROR) when nothing needs refreshing

- [x] **Error codes for the embedding subsystem** (`src/error.rs`)
  - `PT601` — embedding API URL not configured
  - `PT602` — embedding dimension mismatch
  - `PT603` — pgvector extension not installed
  - `PT604` — embedding API request failed (includes HTTP status code in detail)
  - `PT605` — entity has no embedding (raised when `pg:similar` is called for an entity absent from `_pg_ripple.embeddings`)
  - `PT606` — no stale embeddings found (NOTICE level)

- [x] **pg_regress tests**
  - `vector_setup.sql` — verify pgvector is installed; skip remaining vector tests if absent
  - `vector_crud.sql` — store embeddings via `pg_ripple.store_embedding()`, retrieve via `pg_ripple.similar_entities()`, verify ranking order
  - `vector_sparql.sql` — SPARQL query using `pg:similar()` in a BIND expression; verify the result set is non-empty and ordered by distance
  - `vector_filter.sql` — SPARQL query with `FILTER(?score < 0.5)` on a `pg:similar()` result; verify only entities below the threshold are returned
  - `vector_graceful.sql` — test behaviour when `pg_ripple.pgvector_enabled = false`; verify WARNING is emitted and no ERROR is raised
  - `vector_halfvec.sql` — store embeddings with `pg_ripple.embedding_precision = 'half'`; verify halfvec column type and that `pg_ripple.similar_entities()` returns correct results
  - `vector_binary.sql` — store embeddings with `pg_ripple.embedding_precision = 'binary'`; verify bit column type and that Hamming-distance similarity returns non-zero results
  - `vector_refresh.sql` — insert entity, embed, update its `rdfs:label`, call `pg_ripple.refresh_embeddings()`, verify `updated_at` advances and re-embedding count is 1

### Migration Script

`sql/pg_ripple--0.26.0--0.27.0.sql` — creates `_pg_ripple.embeddings` table and HNSW index if pgvector is present; registers GUC parameters. No changes to VP table schema.

### Documentation

- [x] `user-guide/hybrid-search.md` (new page) — quick-start: install pgvector, set GUC parameters, call `pg_ripple.embed_entities()`, run a SPARQL hybrid query; includes architecture diagram showing VP table + embeddings table join
- [x] `reference/embedding-functions.md` (new page) — API reference for `embed_entities`, `similar_entities`, `store_embedding`, `pg:similar()`
- [x] `reference/guc-reference.md` updated — document all seven new embedding GUC parameters (`embedding_model`, `embedding_dimensions`, `embedding_api_url`, `embedding_api_key`, `pgvector_enabled`, `embedding_index_type`, `embedding_precision`) with recommended values for OpenAI, Ollama, and local Sentence-BERT; include storage trade-off table for `embedding_precision` modes

### Exit Criteria

`vector_crud.sql`, `vector_sparql.sql`, `vector_filter.sql`, `vector_halfvec.sql`, `vector_binary.sql`, and `vector_refresh.sql` all pass in `cargo pgrx regress pg18` when pgvector is installed. `vector_setup.sql` skips cleanly when pgvector is absent. `pg_ripple.store_embedding('http://example.org/aspirin', ARRAY[...])` round-trips correctly through `pg_ripple.similar_entities('anti-inflammatory')`. A SPARQL query with `BIND(pg:similar(?drug, "aspirin", 10) AS ?score) FILTER(?score < 0.5)` returns only entities with cosine distance below 0.5. `SELECT pg_ripple.similar_entities('test')` when `pg_ripple.pgvector_enabled = false` emits a WARNING and returns zero rows (no ERROR). `pg_ripple.refresh_embeddings()` after a label update returns a count of 1 and advances `updated_at`. `SELECT count(*) FROM _pg_ripple.embeddings` with `embedding_precision = 'half'` confirms the column is of type `halfvec`. Migration scripts from 0.1.0 through 0.27.0 run cleanly via `just test-migration`.

</details>

---

## v0.28.0 — Advanced Hybrid Search & RAG Pipeline

**Theme**: Production-grade hybrid search with RRF fusion, incremental embedding, graph-contextualized embeddings, and end-to-end RAG retrieval.

> **In plain language:** This release builds on the pgvector foundation to deliver two advanced capabilities. First, *hybrid ranking*: instead of choosing between SPARQL results or vector results, pg_ripple now fuses both using Reciprocal Rank Fusion — a proven algorithm that combines ranked lists from different retrieval systems. Second, *RAG support*: a single SQL function (`pg_ripple.rag_retrieve()`) takes a natural language question, runs hybrid search, and returns structured context ready for an LLM system prompt. A background worker keeps embeddings up-to-date as new entities are added. The result is a complete knowledge-graph-grounded RAG backend running entirely inside PostgreSQL — no separate vector database, no ETL, no eventual consistency.
>
> **Effort estimate: 5–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/vector_sparql_hybrid.md](plans/vector_sparql_hybrid.md) §5 (Advanced Integration Patterns) and §7 (Phases 2 & 3) for full design rationale. Key highlights:

- Reciprocal Rank Fusion (RRF) is the standard algorithm for combining ranked lists from heterogeneous retrieval systems. With RRF, pg_ripple fuses SPARQL result rankings with vector distance rankings into a single scored list using the formula $\text{RRF}(d) = \sum_{r \in R} \frac{1}{k_{rrf} + r(d)}$ where $k_{rrf} = 60$.
- Incremental embedding via a background worker ensures entities added after initial bulk embedding are automatically embedded without user intervention.
- Graph-contextualized embeddings generate text representations that include entity neighborhood information (label, types, neighboring entity labels) before embedding — producing vectors that encode relational context, making similarity search more meaningful than label-only embeddings.
- `pg_ripple.rag_retrieve()` is the missing link between pg_ripple's knowledge graph and LLM-based applications; it bridges directly to the pg_ripple_http HTTP service for REST-based LLM integrations.

### Deliverables

- [x] **`pg_ripple.hybrid_search()` — RRF fusion** (`src/sparql/embedding.rs`)
  - `pg_ripple.hybrid_search(sparql_query TEXT, query_text TEXT, k INT DEFAULT 10, alpha FLOAT8 DEFAULT 0.5, model TEXT DEFAULT NULL) RETURNS TABLE (entity_id BIGINT, entity_iri TEXT, rrf_score FLOAT8, sparql_rank INT, vector_rank INT)` *(optional at runtime — pgvector must be installed)*
  - Executes `sparql_query` (a SPARQL SELECT returning `?entity`) to get the SPARQL-ranked candidate set
  - Executes `pg_ripple.similar_entities(query_text, k * 10)` to get the vector-ranked candidate set
  - Applies Reciprocal Rank Fusion with $k_{rrf} = 60$; `alpha` controls SPARQL vs. vector weight (0.0 = vector only, 1.0 = SPARQL only, 0.5 = equal)
  - Returns top-`k` entities sorted by descending `rrf_score`

- [x] **Incremental embedding background worker** (`src/worker.rs` extension)
  - New table `_pg_ripple.embedding_queue (entity_id BIGINT PRIMARY KEY, enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now())`
  - Trigger on `_pg_ripple.dictionary`: inserts new entity IDs into `embedding_queue` when `pg_ripple.auto_embed = true`
  - Background worker dequeues entities in batches of `pg_ripple.embedding_batch_size`, calls the embedding API, upserts into `_pg_ripple.embeddings`
  - GUC: `pg_ripple.auto_embed` (bool, default `false`) — master switch for trigger-based embedding; off by default to avoid surprise API charges
  - GUC: `pg_ripple.embedding_batch_size` (integer, default `100`, range `1–10000`)

- [x] **`pg_ripple.contextualize_entity()` — graph-serialized text** (`src/sparql/embedding.rs`)
  - `pg_ripple.contextualize_entity(entity_iri TEXT, depth INT DEFAULT 1, max_neighbors INT DEFAULT 20) RETURNS TEXT`
  - Runs an internal SPARQL CONSTRUCT to gather the entity's label, type(s), and up-to-`max_neighbors` neighboring entity labels within `depth` hops
  - Serialises the neighborhood as structured text: `"[entity_label]. Type: [types]. Related: [neighbor_labels]."` — suitable for embedding
  - Used internally by `pg_ripple.embed_entities()` when `pg_ripple.use_graph_context = true` (new GUC, bool, default `false`)

- [x] **`pg_ripple.rag_retrieve()` — end-to-end RAG** (`src/sparql/embedding.rs`)
  - `pg_ripple.rag_retrieve(question TEXT, sparql_filter TEXT DEFAULT NULL, k INT DEFAULT 5, model TEXT DEFAULT NULL) RETURNS TABLE (entity_iri TEXT, label TEXT, context_json JSONB, distance FLOAT8)` *(optional at runtime — pgvector must be installed)*
  - Step 1: encode `question` to a vector; find `k` nearest entities via HNSW
  - Step 2: if `sparql_filter` is non-NULL, apply it as a SPARQL WHERE clause filter on the candidate set
  - Step 3: for each surviving entity, call `pg_ripple.contextualize_entity()` to build a rich context
  - Step 4: return `context_json` as JSONB with keys `label`, `types`, `properties`, `neighbors` — formatted for direct use as an LLM system prompt fragment; structure mirrors the JSON-LD framing output from v0.17.0

- [x] **`pg_ripple_http` RAG endpoint** (`pg_ripple_http/src/main.rs`)
  - `POST /rag` — accepts `{"question": "...", "sparql_filter": "...", "k": 5}` JSON body
  - Calls `pg_ripple.rag_retrieve()` via the existing SPI connection
  - Returns `{"results": [...], "context": "..."}` where `context` is the concatenated `context_json` entries formatted as a plain-text LLM prompt
  - Authentication: same bearer-token auth as existing `pg_ripple_http` endpoints
  - Rate limiting: inherits the `pg_ripple_http.max_requests_per_second` GUC

- [x] **JSON-LD framing for RAG context output** (`src/framing/` extension)
  - `pg_ripple.rag_retrieve()` gains an optional `output_format TEXT DEFAULT 'jsonb'` parameter accepting `'jsonb'` or `'jsonld'`
  - When `output_format = 'jsonld'`, each `context_json` row is formatted as a JSON-LD frame using the framing engine from v0.17.0: entity types map to `@type`, property-value pairs map to their IRI keys, and `@context` is auto-populated from the registered prefix table
  - Enables direct use of `context_json` as a JSON-LD-framed system prompt for LLMs that prefer structured data (e.g. OpenAI structured outputs)
  - New pg_regress test `vector_rag_jsonld.sql` — call `pg_ripple.rag_retrieve(... output_format := 'jsonld')` and verify `@type` and `@context` keys are present in the output

- [x] **SPARQL federation with external vector services** (`src/sparql/federation.rs` extension)
  - Extends the SERVICE handler (v0.16.0) to recognise vector service endpoints registered via `pg_ripple.register_vector_endpoint(url TEXT, api_type TEXT)` where `api_type` is `'pgvector'`, `'weaviate'`, `'qdrant'`, or `'pinecone'`
  - Syntax: `SERVICE <http://vector-service/search> { ?entity pg:similarTo "query" ; pg:score ?score }` — translated to the appropriate external API call (HTTP) rather than a local pgvector scan
  - Returned `?entity` IRIs are resolved against the local dictionary; matched entities can participate in subsequent local triple pattern joins in the same SPARQL query
  - Use case: local pgvector for < 10M entities; external service for larger embedding indexes, without changing the SPARQL query syntax
  - GUC: `pg_ripple.vector_federation_timeout_ms` (integer, default `5000`) — HTTP timeout for external vector service calls
  - Raises `PT607 — vector service endpoint not registered` if an unregistered SERVICE URL is used with a `pg:similarTo` predicate
  - New pg_regress test `vector_federation.sql` — register a mock vector endpoint, issue a federated SPARQL query, verify graceful fallback when the endpoint is unavailable

- [x] **SHACL embedding completeness shape**
  - `examples/shacl_embedding_completeness.ttl` — reusable SHACL shape that validates all entities of a given class have embeddings (uses `sh:path :hasEmbedding ; sh:minCount 1`)
  - `pg_ripple.add_embedding_triples() RETURNS BIGINT` — materialises `:hasEmbedding` triples for entities present in `_pg_ripple.embeddings`, making the SHACL shape checkable

- [x] **Multi-model support**
  - `pg_ripple.list_embedding_models() RETURNS TABLE (model TEXT, entity_count BIGINT, dimensions INT)` — enumerate all models in `_pg_ripple.embeddings`
  - `pg_ripple.similar_entities()`, `pg:similar()`, and `pg_ripple.rag_retrieve()` all accept an optional `model` argument; default is the `pg_ripple.embedding_model` GUC value

- [x] **Benchmarks**
  - `benchmarks/hybrid_search.sql` — pgbench-based benchmark measuring hybrid search latency and throughput; tests vector-only, SPARQL-only, and RRF-fused patterns
  - Target: hybrid search over 1M entities, 1,536-dimensional embeddings, HNSW index, < 50 ms P99 latency for top-10 results

- [x] **Error codes** (additions to `src/error.rs`)
  - `PT607` — vector service endpoint not registered

- [x] **pg_regress tests**
  - `vector_hybrid.sql` — `pg_ripple.hybrid_search()` with a SPARQL SELECT + vector query; verify RRF scores are non-zero and results are sorted
  - `vector_rag.sql` — `pg_ripple.rag_retrieve()` end-to-end; verify `context_json` contains expected keys
  - `vector_rag_jsonld.sql` — `pg_ripple.rag_retrieve(... output_format := 'jsonld')`; verify `@type` and `@context` keys are present
  - `vector_contextualize.sql` — `pg_ripple.contextualize_entity()` on a test entity with known neighbors; verify output text contains expected labels
  - `vector_worker.sql` — insert a new entity with `pg_ripple.auto_embed = true`; verify `_pg_ripple.embedding_queue` is populated; simulate worker drain and verify embedding is present
  - `vector_federation.sql` — register a mock vector endpoint; verify `SERVICE` query with `pg:similarTo` issues the correct HTTP request; verify graceful timeout fallback

### Migration Script

`sql/pg_ripple--0.27.0--0.28.0.sql` — creates `_pg_ripple.embedding_queue` table and trigger; registers new GUC parameters. No changes to VP table schema.

### Documentation

- [x] `user-guide/hybrid-search.md` updated — add RRF fusion and RAG sections; include end-to-end worked example from question to LLM context
- [x] `user-guide/rag.md` (new page) — step-by-step guide to using `pg_ripple.rag_retrieve()` as a backend for LangChain, LlamaIndex, and raw OpenAI API calls; includes `pg_ripple_http` REST example
- [x] `reference/embedding-functions.md` updated — document `hybrid_search`, `rag_retrieve` (including `output_format` parameter), `contextualize_entity`, `list_embedding_models`, `register_vector_endpoint`
- [x] `reference/http-api.md` updated — document `POST /rag` endpoint with request/response examples and JSON-LD output mode
- [x] `user-guide/vector-federation.md` (new page) — how to register external vector services, write federated SPARQL queries, and configure timeouts; includes worked examples for Weaviate, Qdrant, and Pinecone endpoints
- [x] Release notes for v0.28.0 — highlight `rag_retrieve` and `hybrid_search` as headline features; link to the hybrid-search and RAG user guides

### Exit Criteria

`vector_hybrid.sql`, `vector_rag.sql`, `vector_rag_jsonld.sql`, `vector_contextualize.sql`, `vector_worker.sql`, and `vector_federation.sql` all pass in `cargo pgrx regress pg18` when pgvector is installed. `pg_ripple.hybrid_search('SELECT ?drug WHERE { ?drug a :Drug }', 'anti-inflammatory', 10)` returns ≤ 10 rows with non-zero `rrf_score`. `pg_ripple.rag_retrieve('what treats headaches?', k := 5)` returns JSONB rows with `label`, `types`, `properties`, and `neighbors` keys. `pg_ripple.rag_retrieve('what treats headaches?', k := 5, output_format := 'jsonld')` returns rows whose `context_json` contains `@type` and `@context` keys. `POST /rag` on `pg_ripple_http` returns a `context` field suitable for use as an LLM system prompt. Inserting a new entity with `pg_ripple.auto_embed = true` and running the background worker loop populates `_pg_ripple.embeddings` for that entity. `pg_ripple.register_vector_endpoint('http://unknown/', 'qdrant')` followed by a SERVICE query returns graceful timeout with no ERROR. Migration scripts from 0.1.0 through 0.28.0 run cleanly via `just test-migration`.

</details>

---

## v0.29.0 — Datalog Optimization: Magic Sets & Cost-Based Compilation

**Theme**: Goal-directed inference, cost-based rule compilation, and evaluation-path optimizations for the Datalog engine.

> **In plain language:** pg_ripple's Datalog engine already supports semi-naive evaluation — it only looks at *new* facts each iteration. This release makes inference dramatically smarter: instead of deriving *every possible* fact, the engine now derives only the facts needed to answer a specific question (magic sets). It also reorders rule joins by cost, eliminates redundant rules, and improves how negation and filters are compiled to SQL. The result is 10×–1000× faster inference for targeted queries and 2×–10× faster full materialization on large datasets.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2](plans/ecosystem/datalog.md) for detailed design notes on all optimization techniques. Key highlights:

- Magic sets is the classical Datalog optimization (Bancilhon et al., 1986; implemented in IBM DB2). It rewrites a rule program + query goal into a smaller program that derives only relevant facts. Combined with semi-naive evaluation, it matches top-down evaluation performance while retaining bottom-up correctness guarantees.
- Cost-based body atom reordering uses PostgreSQL's `pg_class.reltuples` and `pg_statistic` to sort joins by selectivity — the same technique PostgreSQL's own planner uses, applied at the Datalog→SQL compilation stage.
- Subsumption checking prunes redundant rules at compile time, reducing the number of SQL statements per fixpoint iteration.

### Deliverables

- [x] **Magic sets transformation** (`src/datalog/magic.rs`)
  - `pg_ripple.infer_goal(rule_set TEXT, goal TEXT) RETURNS JSONB` — materialize only facts relevant to the goal pattern
  - Adornment propagation: given a goal like `?x rdf:type foaf:Person`, compute binding patterns for each predicate
  - Magic predicate generation: create auxiliary predicates that capture the demanded binding set
  - Modified rule generation: add magic-predicate filters to each rule body
  - SQL compilation: magic predicates compile to temp tables; modified rules join against them
  - Automatic integration with `create_datalog_view()` — when a goal has bound constants, magic sets are applied automatically
  - GUC: `pg_ripple.magic_sets` (bool, default `true`) — master switch; set to `false` to disable for debugging
  - Benchmark: `benchmarks/magic_sets.sql` — compare full materialization vs. goal-directed inference on RDFS closure with selective goals

- [x] **Cost-based body atom reordering** (`src/datalog/compiler.rs`)
  - At rule compilation time, query `pg_class.reltuples` for each VP table referenced by a body atom
  - For atoms with bound constants, estimate selectivity from `pg_statistic.n_distinct`
  - Sort body atoms by ascending estimated cardinality (most selective first)
  - Prefer atoms that join on indexed columns `(s,o)` or `(o,s)` when selectivities are similar
  - GUC: `pg_ripple.datalog_cost_reorder` (bool, default `true`)

- [x] **Subsumption checking** (`src/datalog/stratify.rs` extension)
  - After stratification, check each pair of rules deriving the same predicate for subsumption
  - If rule R2 is subsumed by rule R1 (R2's head is a substitution instance of R1's, and R1's body is a subset of R2's body), eliminate R2
  - Report eliminated rules via `pg_ripple.infer_with_stats()` JSONB output: `"eliminated_rules": [...]`

- [x] **Anti-join negation** (`src/datalog/compiler.rs`)
  - Replace `NOT EXISTS (SELECT 1 FROM vp_{id} WHERE ...)` with `LEFT JOIN vp_{id} ON ... WHERE ... IS NULL`
  - Compile-time choice: use anti-join when the negated predicate's VP table has ≥1000 rows (from `pg_class.reltuples`); retain `NOT EXISTS` for small tables where the planner favors it
  - GUC: `pg_ripple.datalog_antijoin_threshold` (integer, default `1000`)

- [x] **Predicate-filter pushdown** (`src/datalog/compiler.rs`)
  - Identify which body atom first binds each arithmetic/comparison guard variable
  - Move the guard immediately after that atom in the generated SQL
  - For range filters (`?a > 18`), emit as part of the `JOIN … ON` clause to enable index scans

- [x] **Delta table indexing** (`src/datalog/mod.rs`)
  - After each semi-naive iteration populates a delta table, create a B-tree index on the join columns used by the next iteration's rules
  - Skip indexing when the delta table has fewer than `pg_ripple.delta_index_threshold` rows (default: 500)
  - GUC: `pg_ripple.delta_index_threshold` (integer, default `500`)

- [x] **Error codes** (additions to `src/error.rs`)
  - `PT501` — magic sets transformation failed (circular binding pattern)
  - `PT502` — cost-based reordering skipped (statistics unavailable)

- [x] **pg_regress tests**
  - `datalog_magic_sets.sql` — magic sets on RDFS transitivity with a selective goal; verify result matches full materialization; verify magic temp tables are cleaned up
  - `datalog_cost_reorder.sql` — verify EXPLAIN output shows changed join order with `pg_ripple.datalog_cost_reorder = true` vs. `false`
  - `datalog_antijoin.sql` — verify negation compiles to `LEFT JOIN … IS NULL` when threshold is met
  - `datalog_subsumption.sql` — load overlapping rules; verify `infer_with_stats()` reports eliminated rules
  - `datalog_filter_pushdown.sql` — verify arithmetic filters appear in JOIN ON clause, not outermost WHERE
  - `datalog_delta_index.sql` — verify delta table index creation when row count exceeds threshold

### Migration Script

`sql/pg_ripple--0.28.0--0.29.0.sql` — registers new GUC parameters. No changes to VP table schema or catalog tables.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `infer_goal()`, magic sets GUC, cost-based reordering GUC, anti-join threshold GUC, delta indexing threshold GUC
- [x] `user-guide/best-practices/datalog-optimization.md` (new page) — when to use `infer()` vs. `infer_goal()`, how to read `infer_with_stats()` output, how to diagnose slow fixpoint convergence, tuning GUCs for different dataset sizes
- [x] Release notes for v0.29.0 — highlight magic sets and cost-based compilation as headline features; include before/after benchmarks

### Exit Criteria

`datalog_magic_sets.sql`, `datalog_cost_reorder.sql`, `datalog_antijoin.sql`, `datalog_subsumption.sql`, `datalog_filter_pushdown.sql`, and `datalog_delta_index.sql` all pass in `cargo pgrx regress pg18`. `pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person')` returns the same triples as `pg_ripple.infer('rdfs')` filtered to `rdf:type foaf:Person`, but completes in <10% of the time on a 1M-triple dataset. Migration scripts from 0.1.0 through 0.29.0 run cleanly via `just test-migration`.

</details>

---

## v0.30.0 — Datalog Aggregation & Compiled Rule Plans

**Theme**: Analytics-grade inference and rule plan caching.

> **In plain language:** This release adds two major capabilities to the Datalog engine. First, rules can now aggregate facts — for example, "count the number of friends each person has" or "find the maximum salary in each department" — unlocking graph analytics and metrics directly from inference rules. Second, the engine caches the SQL it generates for each rule set, so repeated calls to `infer()` (e.g., after each data load) no longer repeat expensive dictionary lookups and query construction. As a bonus, SPARQL queries that use on-demand Datalog rules also benefit from the plan cache: a query that triggers inference gets a faster response on every repeat execution.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2](plans/ecosystem/datalog.md) for design notes. Aggregation in rule bodies (Datalog^agg) follows the aggregation-stratification spec: aggregate operations are allowed only in rule bodies over predicates that are fully computed in a lower stratum, ensuring a unique minimal model. Compiled rule plans cache generated SQL in a `HashMap<rule_set, Vec<CachedPlan>>` keyed on the dictionary-encoded rule set name; cache invalidation triggers on `load_rules()`, `drop_rules()`, or GUC change.

### Deliverables

- [x] **Aggregation in rule bodies (Datalog^agg)** (`src/datalog/compiler.rs`, `src/datalog/stratify.rs`)
  - Extend rule IR to support aggregate terms in body atoms: `COUNT(?x)`, `SUM(?x)`, `MIN(?x)`, `MAX(?x)`, `AVG(?x)`
  - Aggregation-stratification check: aggregated predicates must be fully computed in a lower stratum; reject with `PT510` if violated
  - SQL compilation: aggregate body atoms compile to subquery CTEs with `GROUP BY` and aggregate window functions
  - `pg_ripple.infer_agg(rule_set TEXT) RETURNS JSONB` — variant of `infer()` that enables aggregation rules
  - Example rule: `?x ex:friendCount ?n :- COUNT(?y WHERE ?x foaf:knows ?y) = ?n .`
  - Benchmark: `benchmarks/datalog_agg.sql` — PageRank-style degree centrality on a social graph

- [x] **Compiled rule plans** (`src/datalog/cache.rs` new module)
  - Cache the generated SQL string (and dictionary-encoded constant vector) for each rule on first `infer()` call
  - Cache key: rule set name + schema version (invalidate on any `ALTER EXTENSION pg_ripple UPDATE`)
  - Cache storage: `pgrx::PgSharedMem`-backed LRU, size controlled by GUC `pg_ripple.rule_plan_cache_size` (default: 64 entries)
  - SPARQL on-demand mode benefit: when a SPARQL query inlines a derived predicate CTE, the CTE SQL is served from the plan cache rather than rebuilt from scratch
  - GUC: `pg_ripple.rule_plan_cache` (bool, default `true`)
  - Expose cache statistics via `pg_ripple.rule_plan_cache_stats() RETURNS TABLE(rule_set TEXT, hits BIGINT, misses BIGINT, entries INT)`

- [x] **Error codes** (`src/error.rs`)
  - `PT510` — aggregation-stratification violation (aggregate over non-ground predicate)
  - `PT511` — unsupported aggregate function in rule body

- [x] **pg_regress tests**
  - `datalog_agg.sql` — verify COUNT, SUM, MIN, MAX rules derive correct results; verify stratification rejects cycles through aggregates
  - `datalog_plan_cache.sql` — verify cache hit/miss counts via `rule_plan_cache_stats()`; verify cache invalidation on `drop_rules()`
  - `datalog_sparql_cache.sql` — verify SPARQL on-demand query using a derived predicate is faster on second execution (plan served from cache)

### Migration Script

`sql/pg_ripple--0.29.0--0.30.0.sql` — registers new GUCs (`pg_ripple.rule_plan_cache`, `pg_ripple.rule_plan_cache_size`). No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `infer_agg()`, aggregation rule syntax, plan cache GUCs, `rule_plan_cache_stats()`
- [x] `user-guide/best-practices/datalog-optimization.md` updated — add section on aggregation-stratification rules, plan cache tuning
- [x] Release notes for v0.30.0

### Exit Criteria

`datalog_agg.sql`, `datalog_plan_cache.sql`, and `datalog_sparql_cache.sql` all pass in `cargo pgrx regress pg18`. A PageRank-style degree centrality rule on a 1M-triple social graph produces correct results. Second call to `infer()` on the same rule set reports cache hits > 0 in `rule_plan_cache_stats()`. Migration scripts from 0.1.0 through 0.30.0 run cleanly via `just test-migration`.

</details>

---

## v0.31.0 — Entity Resolution & Demand Transformation

**Theme**: Identity semantics and goal-directed rule rewriting for SPARQL and Datalog.

> **In plain language:** This release tackles two distinct but complementary problems. First, it adds proper handling for `owl:sameAs` — the RDF way of saying "these two names refer to the same thing". When the engine knows that `ex:Alice` and `ex:A.Smith` are the same person, all facts about one automatically apply to the other. Second, it introduces demand transformation — a generalisation of the magic sets technique (added in v0.29.0) that can rewrite complex rule programs to derive only the facts that a query actually needs, even for rules with many cross-referencing bodies. This also makes SPARQL on-demand mode smarter: SPARQL queries can now trigger only the Datalog inference relevant to their specific patterns.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2](plans/ecosystem/datalog.md) for design notes. `owl:sameAs` merging uses a pre-pass canonicalization strategy: before each fixpoint iteration, the compiler rewrites triple patterns to use the canonical (lowest-id) representative of each `sameAs` equivalence class. Demand transformation is more flexible than magic sets for programs with multiple recursive predicates that reference each other — it propagates binding demands through the full program dependency graph rather than one predicate at a time.

### Deliverables

- [x] **`owl:sameAs` entity canonicalization** (`src/datalog/rewrite.rs` new module)
  - Pre-pass: at the start of each inference run, compute equivalence classes of `owl:sameAs` (VP table for `sameAs` predicate) using union-find over dictionary IDs
  - Canonicalization map: each non-canonical ID maps to the lowest ID in its class
  - Rule compiler rewrite: substitute all occurrences of non-canonical IDs in rule bodies before SQL generation
  - SPARQL integration: SPARQL queries that reference a non-canonical entity are transparently rewritten to query the canonical form
  - GUC: `pg_ripple.sameas_reasoning` (bool, default `true`)
  - Benchmark: `benchmarks/sameas.sql` — query entity with 100 `sameAs` aliases; verify all facts visible via any alias

- [x] **Demand transformation** (`src/datalog/demand.rs` new module)
  - Generalised magic sets: compute demand sets for all predicates simultaneously via a fixed-point on the program dependency graph
  - API: `pg_ripple.infer_demand(rule_set TEXT, demands JSONB) RETURNS JSONB` — `demands` is an array of goal patterns `[{"p": "rdf:type", "o": "foaf:Person"}, ...]`
  - Automatically applied in `create_datalog_view()` when multiple goal patterns are specified
  - SPARQL on-demand integration: when a SPARQL query references multiple derived predicates, compute a joint demand set and apply it to all relevant rules before generating inline CTEs; reduces CTE size and join cost
  - GUC: `pg_ripple.demand_transform` (bool, default `true`)

- [x] **pg_regress tests**
  - `datalog_sameas.sql` — load `sameAs` assertions; verify inference results are visible via all aliases; verify canonicalization in SPARQL query results
  - `datalog_demand.sql` — verify `infer_demand()` derives same results as `infer()` filtered to the demand set; verify EXPLAIN shows smaller CTE for SPARQL on-demand queries with demand transform enabled

### Migration Script

`sql/pg_ripple--0.30.0--0.31.0.sql` — registers `pg_ripple.sameas_reasoning` and `pg_ripple.demand_transform` GUCs. No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `infer_demand()`, `owl:sameAs` behaviour, `sameas_reasoning` GUC
- [x] `user-guide/best-practices/datalog-optimization.md` updated — add section on demand transformation vs. magic sets, when to use `infer_demand()` vs. `infer_goal()`
- [x] Release notes for v0.31.0

### Exit Criteria

`datalog_sameas.sql` and `datalog_demand.sql` pass in `cargo pgrx regress pg18`. A SPARQL on-demand query referencing two derived predicates on a 1M-triple dataset completes in <50% of the time compared to v0.30.0 (demand transform reduces combined CTE size). Migration scripts from 0.1.0 through 0.31.0 run cleanly via `just test-migration`.

</details>

---

## v0.32.0 — Well-Founded Semantics & Tabling

**Theme**: Advanced reasoning for cyclic ontologies and subsumptive result caching for Datalog and SPARQL.

> **In plain language:** Two powerful features for production knowledge graph workloads. Well-founded semantics handles the edge cases that stratified Datalog cannot: programs where rules are mutually recursive through negation (e.g., "X is trusted unless untrusted, and untrusted unless trusted"). Instead of rejecting these programs, the engine assigns a third truth value — *unknown* — and returns whatever can be definitively concluded. Tabling caches the results of recurring sub-queries: if the same Datalog sub-goal (or SPARQL sub-pattern) appears in multiple queries or multiple times within one query, the answer is computed once and reused. For analytical workloads with repeated sub-query patterns, this is a 2–5× speedup.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2](plans/ecosystem/datalog.md) for design notes. Well-founded semantics (Van Gelder et al., 1991) extends stratified Datalog with a three-valued model: facts are true, false, or *unknown* (neither provably true nor provably false). The SQL encoding uses an iterative alternating fixpoint: two parallel CTE chains compute the *well-founded model* over at most `pg_ripple.wfs_max_iterations` rounds. Tabling (subsumptive tabling, inspired by XSB Prolog) stores derived sub-goals in a session-scoped cache table `_pg_ripple.tabling_cache (goal_hash BIGINT, result JSONB, computed_at TIMESTAMPTZ)` and reuses results within a configurable TTL.

### Deliverables

- [x] **Well-founded semantics** (`src/datalog/wfs.rs` new module)
  - Alternating fixpoint algorithm: compute `T_P↑` (positive) and `T_P↓` (negative) iteratively until fixpoint
  - Three-valued result: derived facts carry a `certainty` column (`true` / `unknown`) in the query output
  - `pg_ripple.infer_wfs(rule_set TEXT) RETURNS JSONB` — run well-founded fixpoint instead of stratified evaluation
  - Graceful degradation: for stratifiable programs, `infer_wfs()` produces the same results as `infer()` with no overhead
  - GUC: `pg_ripple.wfs_max_iterations` (integer, default `100`) — safety cap on alternating fixpoint rounds
  - Error code `PT520` — well-founded fixpoint did not converge within `wfs_max_iterations`
  - Benchmark: `benchmarks/wfs.sql` — cyclic ontology with mutual negation; verify unknown facts are correctly identified

- [x] **Tabling / memoization** (`src/datalog/tabling.rs` new module)
  - Session-scoped cache: `_pg_ripple.tabling_cache (goal_hash BIGINT PRIMARY KEY, result BYTEA, computed_at TIMESTAMPTZ)`
  - Cache key: XXH3-128 of the normalised goal pattern (predicate ID + bound-variable encoding)
  - SPARQL integration: SPARQL sub-query patterns (e.g., property path closures, OPTIONAL blocks) that match a cached goal are served from the tabling cache without re-executing the CTE — implemented at the SPARQL→SQL translation layer
  - Datalog integration: `infer()` and `infer_goal()` check the tabling cache before running the fixpoint; on cache miss, the result is stored for future calls
  - TTL: `pg_ripple.tabling_ttl` (integer seconds, default `300`); set to `0` to disable expiry
  - GUC: `pg_ripple.tabling` (bool, default `true`)
  - Invalidation: cache is automatically cleared on any triple insert/delete/update (via CDC hook), and on `drop_rules()`
  - Expose stats: `pg_ripple.tabling_stats() RETURNS TABLE(goal_hash BIGINT, hits BIGINT, computed_ms FLOAT, cached_at TIMESTAMPTZ)`

- [x] **pg_regress tests**
  - `datalog_wfs.sql` — verify well-founded semantics on a cyclic negation program; verify `certainty = 'unknown'` for unresolvable facts; verify stratifiable programs return same results as `infer()`
  - `datalog_tabling.sql` — verify cache hit/miss counts via `tabling_stats()`; verify TTL expiry; verify cache invalidation on triple insert
  - `sparql_tabling.sql` — SPARQL query with repeated sub-pattern; verify tabling stats show hit > 0 on second identical sub-pattern within one query

### Migration Script

`sql/pg_ripple--0.31.0--0.32.0.sql` — creates `_pg_ripple.tabling_cache` table; registers `pg_ripple.tabling`, `pg_ripple.tabling_ttl`, `pg_ripple.wfs_max_iterations` GUCs.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `infer_wfs()`, tabling GUCs, `tabling_stats()`
- [x] `user-guide/best-practices/datalog-optimization.md` updated — add section on when to use `infer_wfs()`, tabling tuning, SPARQL sub-query caching behaviour
- [x] `user-guide/best-practices/sparql-performance.md` (new page) — how tabling accelerates SPARQL property paths and repeated sub-queries; how demand transformation reduces CTE size; how rule plan caching (v0.30.0) interacts with SPARQL on-demand mode
- [x] Release notes for v0.32.0

### Exit Criteria

`datalog_wfs.sql`, `datalog_tabling.sql`, and `sparql_tabling.sql` all pass in `cargo pgrx regress pg18`. A SPARQL query with a repeated transitive-closure sub-pattern on a 1M-triple dataset completes in <50% of the time on the second execution (tabling cache hit). `infer_wfs()` on a stratifiable rule set produces identical results to `infer()`. Migration scripts from 0.1.0 through 0.32.0 run cleanly via `just test-migration`.

</details>

---

## v0.33.0 — Documentation Site & Content Overhaul

**Theme**: A documentation site worthy of a production-grade triple store.

> **In plain language:** pg_ripple is a mature system — v0.32.0 delivers full SPARQL 1.1 and SHACL Core conformance across 32 releases — but its documentation has grown organically alongside the codebase rather than being designed for the people who use it. This release delivers documentation that meets users where they are: a problem-centric information architecture written for five distinct archetypes (Data Engineer, Application Developer, Knowledge Architect, Decision-Maker, AI/ML Engineer), eight feature-deep-dive chapters, a full operations guide, a SQL function reference with working examples for every function, and a CI harness that keeps every code example honest by running it against a real pg_ripple instance on every pull request. The full plan is in [plans/documentation.md](plans/documentation.md).
>
> **Effort estimate: 8–12 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/documentation.md](plans/documentation.md) for the authoritative plan — site structure, content guidelines, five user archetypes, and four delivery phases. Everything described in that plan is in scope for this version.

The documentation site is built with mdBook. `mdbook-admonish` is added before Phase 1 content work starts (`book.toml` updated with `[preprocessor.admonish]`); all new and restructured pages use its fenced callout syntax exclusively. A shared bibliographic fixture dataset (papers, authors, institutions, topics, citations, pre-computed embeddings) is established in `docs/fixtures/` and reused across all chapters.

### Deliverables

#### Phase 0 — CI Test Harness (prerequisite)

- [x] `scripts/test_docs.sh` — CI harness: spins up pg_ripple via Docker, extracts fenced SQL blocks from `docs/src/`, executes them in document order, compares stdout against expected-output comment blocks embedded directly below each code block
- [x] `docs/fixtures/bibliography.sql` — shared bibliographic fixture dataset (papers, authors, institutions, topics, citations, pre-computed embeddings) reused across all chapters
- [x] `.github/workflows/docs-test.yml` — CI job that runs the harness on every PR touching `docs/`
- [x] `mdbook-admonish` added to `book.toml` and `[preprocessor.admonish]` block configured
- [x] Exit criterion: CI job passes on a real PR (not just locally)

#### Phase 1 — Foundation

- [x] **Landing page** — value proposition, architecture diagram, one compelling code example; key-numbers block and comparison summary absorbed from the former "60 Seconds" content
- [x] **Evaluate / When to Use pg_ripple** — honest comparison matrix (pg_ripple vs. plain SQL, standalone RDF stores, LPG systems, pure vector databases); decision flowchart; AI/LLM section on when graph context outperforms flat vector retrieval
- [x] **Installation** — Docker (recommended default), from source (`cargo pgrx`), prerequisites, verification step (`SELECT pg_ripple.triple_count()` returns 0), troubleshooting for the five most common failures
- [x] **Hello World — Five-Minute Walkthrough** — ten triples, three queries of increasing complexity (basic pattern → OPTIONAL → property path), annotated output after every step
- [x] **Guided Tutorial — Build a Knowledge Graph in 30 Minutes** — four self-contained ≤10-minute segments: Load & Explore, Validate, Reason, Export; uses the shared bibliographic dataset; each segment is independently complete
- [x] **Key Concepts — RDF for PostgreSQL Users** — triples, IRIs, blank nodes, literals, named graphs, RDF-star, SPARQL; PostgreSQL analogies with diagrams for every concept

#### Phase 2 — Feature Deep Dives

Eight chapters, each following the seven-part structure: What & Why → How It Works → Worked Examples → Common Patterns → Performance & Trade-offs → Gotchas & Debugging → Next Steps.

- [x] **§2.1 Storing Knowledge** — modeling a domain as triples; named graphs (when needed vs. when not); blank nodes with honest caveats; RDF-star for provenance and confidence scores; translating a relational schema to RDF
- [x] **§2.2 Loading Data** — all formats (Turtle, N-Triples, N-Quads, TriG, RDF/XML); three loading modes (`load_turtle()`, `load_turtle_file()`, `insert_triple()`); bulk-load performance numbers; blank-node scoping across calls; SQL-to-triples patterns; when to run ANALYZE
- [x] **§2.3 Querying with SPARQL** — basic patterns through property paths (all operators: `+`, `*`, `?`, `/`, `|`, `^`); aggregation; subqueries; UNION/MINUS; GRAPH patterns; `sparql_explain()` guide; filter pushdown; `max_path_depth` safety limit; real-world query recipes (entity resolution, recommendations, transitive closure, temporal queries)
- [x] **§2.4 Validating Data Quality** — SHACL shapes from simple (`sh:minCount`/`sh:maxCount`) to complex (`sh:or`, `sh:pattern`, cross-property constraints); synchronous vs. asynchronous validation modes; dead-letter queue; common quality rule patterns
- [x] **§2.5 Reasoning and Inference** — Datalog rules; built-in RDFS/OWL RL rule sets; stratification explained plainly; explicit vs. inferred triples (`source` column); goal-directed vs. full materialization; magic sets and semi-naive evaluation
- [x] **§2.6 Exporting and Sharing** — all export formats; JSON-LD framing with `sparql_construct_jsonld()` and frame templates; **canonical GraphRAG chapter**: BYOG Parquet export, Datalog enrichment, SHACL quality enforcement (all other GraphRAG mentions cross-reference here)
- [x] **§2.7 AI Retrieval & Graph RAG** — **canonical AI chapter**: vector embeddings, HNSW indexes, `pg:similar()`, hybrid retrieval with RRF, `rag_retrieve()`, JSON-LD framing for LLM prompts, `owl:sameAs` pre-pass before embedding, FTS broadening, end-to-end RAG pipeline; comparison with pure vector stores (Qdrant, Weaviate, pgvector-only)
- [x] **§2.8 APIs and Integration** — `pg_ripple_http` SPARQL Protocol HTTP endpoint (configuration, response formats, authentication, Docker Compose); application code examples (Python `psycopg2`/`SPARQLWrapper`, JavaScript `pg`, Java JDBC); SPARQL federation; caching strategies

#### Phase 3 — Operations

- [x] **Architecture Overview** — dictionary, VP tables, HTAP storage, shmem cache; SPARQL query execution flow for operators
- [x] **Deployment Models** — standalone, Docker/Compose, managed PostgreSQL services; trade-offs and the recommended starting point
- [x] **Configuration and Tuning** — all GUC parameters by subsystem (storage, query engine, inference, validation, caching, system); three-size production config (small: <1M triples; medium: 1M–100M; large: >100M)
- [x] **Monitoring and Observability** — `pg_ripple.stats()`, `pg_stat_statements`, `sparql_explain(analyze := true)`, Prometheus metrics; Grafana panel descriptions; health-check thresholds
- [x] **Performance Tuning** — bottleneck identification for query, write throughput, and cache pressure; realistic BSBM numbers; tuning recipes for read-heavy, write-heavy, and mixed HTAP workloads
- [x] **Backup and Disaster Recovery** — `pg_dump`/`pg_restore`; point-in-time recovery; verified backup/restore procedure with exact commands
- [x] **Upgrading Safely** — `ALTER EXTENSION pg_ripple UPDATE`; pre/post-upgrade steps; rollback strategy; maintenance-window guidance; explicit note that zero-downtime upgrades are not yet supported
- [x] **Scaling** — vertical scaling guide; merge-worker tuning; read replicas for horizontal scale; honest statement of what is not yet supported
- [x] **Troubleshooting** — runbook format: ≥15 symptom → cause → diagnostic → fix entries across all subsystems
- [x] **Security** — named-graph row-level security; injection prevention; `pg_ripple_http` TLS and authentication; file-path loader delegation

#### Phase 4 — Reference and Polish

- [x] **SQL Function Reference** — all functions grouped by use case (Loading, Querying, Validating, Reasoning, Exporting, Administration); each entry has full signature, parameter table, and one working example with expected output
- [x] **SPARQL Compliance Matrix** — every SPARQL 1.1 Query, Update, and Protocol feature with status (Supported / Partial / Not Supported); link to W3C test suite results; workarounds for partial/unsupported features
- [x] **Error Message Catalog** — every PT001–PT799 code with cause and fix; auto-generated from `src/error.rs` where possible
- [x] **FAQ** — 25–30 questions across Getting Started, Data Modeling, Querying, Performance, Operations, and Comparisons; each answer 50–150 words with links to the relevant deep-dive page
- [x] **Glossary** — plain-language definitions of every term used in the documentation
- [x] **Release Notes and Roadmap** mirrored into the docs site
- [x] **Contributing guide** — dev environment setup, test commands, PR workflow, code conventions; top-level "Contribute" navigation entry and landing-page callout card; academic citations and architecture background moved to `CONTRIBUTING.md` (not user-facing reference)
- [x] Full audit: every code example verified against v0.33.0, all `TODO` / stub markers resolved

#### Content Governance

- [x] `scripts/check_docs_coverage.sh` — CI job that diffs exported function signatures in `src/lib.rs` against the SQL Function Reference and fails the build when a changed signature has no corresponding `docs/` touch in the same PR
- [x] `mdbook-linkcheck` broken-link CI job on every PR touching `docs/`; redirect map (`docs/redirects.toml`) kept current when pages are moved or removed
- [x] PR template updated with docs-gap reminder (CI enforcement is primary; checkbox is a reminder only)
- [x] 30-day documentation review schedule: at every minor release, run the signature-diff script and triage GitHub issues tagged `docs` to fill gaps

### Migration Script

`sql/pg_ripple--0.32.0--0.33.0.sql` — no schema changes. This version delivers documentation infrastructure and content only; all pg_ripple SQL functions, GUCs, and VP table schemas are unchanged from v0.32.0.

### Documentation

This version *is* the documentation release. The deliverables above are the documentation.

### Exit Criteria

- Phase 0 CI harness is complete and passing in CI (verified by a real PR, not just locally).
- The eight feature-deep-dive chapters (§2.1–§2.8) are published with no unresolved stubs or TODO markers.
- The operations section (10 pages) is complete and published.
- The SQL Function Reference covers every function listed in §4 of [plans/documentation.md](plans/documentation.md).
- `check_docs_coverage.sh` CI job passes on a PR that changes a function signature.
- `mdbook-linkcheck` reports zero broken internal links.
- Migration scripts from 0.1.0 through 0.33.0 run cleanly via `just test-migration`.

</details>

---

## v0.34.0 — Bounded-Depth Termination & Incremental Retraction (DRed)

**Theme**: Smarter fixpoint termination and write-correct incremental maintenance.

> **In plain language:** Two complementary improvements for production workloads. First, when an ontology has a known maximum hierarchy depth (e.g., a SHACL shape says class hierarchies are at most 5 levels deep), the inference engine can stop early instead of running one final "did anything change?" check — shaving 20–50% off property path queries and fixpoint loops. Second, the Delete-Rederive (DRed) algorithm means that deleting a base triple no longer requires re-materializing the entire derived closure: the engine surgically removes only the affected derived facts, re-derives any that survive via alternative paths, and leaves everything else untouched. Materialized SPARQL predicates stay correct in milliseconds after deletes instead of seconds.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2.7 and §14.2.12](plans/ecosystem/datalog.md) for design notes. Bounded-depth termination integrates with SHACL shape constraints (`sh:maxDepth` annotations on property paths) and user-provided GUC hints to set the maximum fixpoint iteration count at compile time. DRed (Gupta, Katiyar & Sagiv, 1993) is the standard incremental deletion algorithm used by RDFox and other production Datalog systems; it avoids full re-materialization by over-deleting pessimistically and then re-deriving survivors.

### Deliverables

- [x] **Bounded-depth early termination** (`src/datalog/compiler.rs`)
  - Read SHACL `sh:maxDepth` annotations for property paths used in rule bodies; fall back to GUC `pg_ripple.datalog_max_depth` (integer, default `0` = unlimited)
  - When a depth bound `d` is known, emit `WITH RECURSIVE … (MAXDEPTH d)` hint (PostgreSQL 18 syntax) or use a depth counter column in the recursive CTE: `depth INT`, terminating when `depth > d`
  - SPARQL property path integration: property path CTEs (`rdfs:subClassOf*`, `ex:knows+`) respect the same bound when the path predicate has a SHACL `sh:maxDepth` constraint
  - GUC: `pg_ripple.datalog_max_depth` (integer, default `0` — unlimited)
  - pg_regress test: `datalog_bounded_depth.sql` — verify fixpoint terminates after `d` iterations; verify SPARQL property path honours depth bound; verify unbounded rule still produces full closure

- [x] **Incremental retraction — DRed algorithm** (`src/datalog/dred.rs` new module)
  - Hook into the CDC delete path: when a base triple is deleted from a VP table, identify all derived predicates whose SQL rules reference that VP table
  - **Phase 1 — Over-delete**: for each affected derived predicate, delete all rows that *could* depend on the deleted triple (pessimistic, using rule SQL with the deleted triple as a positive filter)
  - **Phase 2 — Re-derive**: re-run the rule SQL restricted to the over-deleted set; rows that are re-derived via an alternative derivation path are reinserted
  - **Phase 3 — Commit**: rows not reinserted after phase 2 are permanently gone
  - `pg_ripple.dred_enabled` (bool, default `true`) — master switch; set `false` to fall back to full re-materialization on delete
  - `pg_ripple.dred_batch_size` (integer, default `1000`) — maximum number of deleted base triples to process in a single DRed transaction
  - Error code `PT530` — DRed cycle detected (derived predicate self-references in a way DRed cannot safely resolve; falls back to full recompute)
  - pg_regress test: `datalog_dred.sql` — insert triples, materialize RDFS closure, delete one base triple, verify only the correctly-affected derived triples are removed; verify triples supported by alternative paths survive

- [x] **Incremental rule updates** (`src/datalog/mod.rs`)
  - `pg_ripple.add_rule(rule_set TEXT, rule_text TEXT)` — add a single rule to an existing rule set without full recompute; only the new rule's derived predicate needs one fresh iteration pass
  - `pg_ripple.remove_rule(rule_id BIGINT)` — remove a rule and retract any derived facts that were solely supported by it (uses DRed internally)
  - Dependency-aware invalidation: `add_rule` triggers one additional semi-naive pass on the affected stratum only
  - pg_regress test: `datalog_incremental_rules.sql` — add a rule to a live rule set; verify new derivations appear without full recompute; remove the rule; verify derived facts retracted

### Migration Script

`sql/pg_ripple--0.33.0--0.34.0.sql` — registers `pg_ripple.datalog_max_depth`, `pg_ripple.dred_enabled`, `pg_ripple.dred_batch_size` GUCs. No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `add_rule()`, `remove_rule()`, DRed GUCs, `datalog_max_depth` GUC
- [x] `user-guide/best-practices/datalog-optimization.md` updated — add section on DRed vs. full recompute trade-offs; bounded-depth tuning with SHACL
- [x] `user-guide/best-practices/sparql-performance.md` updated — add section on bounded-depth SPARQL property paths
- [x] Release notes for v0.34.0

### Exit Criteria

`datalog_bounded_depth.sql`, `datalog_dred.sql`, and `datalog_incremental_rules.sql` all pass in `cargo pgrx regress pg18`. Deleting a base triple from a 1M-triple RDFS-materialized dataset with DRed enabled completes in <500ms (vs. full recompute taking >5s). A SPARQL `rdfs:subClassOf*` property path query on a hierarchy with `sh:maxDepth 5` completes in <50% of the time compared to the unbounded version on a 10-level test hierarchy. Migration scripts from 0.1.0 through 0.34.0 run cleanly via `just test-migration`.

</details>

---

## v0.35.0 — Parallel Stratum Evaluation & Incremental Rule Updates

**Theme**: Concurrent rule evaluation for faster materialization of large rule sets.

> **In plain language:** The Datalog engine currently evaluates rules one at a time within each stratum. This release allows rules that derive different predicates — and therefore cannot interfere with each other — to run concurrently using PostgreSQL's background worker infrastructure. For OWL RL, which has roughly 10 independent rule groups in its first stratum, this means the full ontology closure can materialize up to 10× faster. SPARQL queries that depend on materialized predicates (the common production mode) benefit directly: derived VP tables become fresh sooner after bulk data loads, reducing the staleness window.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2.11](plans/ecosystem/datalog.md) for design notes. Within a single stratum, rules deriving *different* predicates are fully independent: their INSERT … SELECT statements touch different VP tables and can run concurrently without coordination. Rules deriving the *same* predicate within a stratum must be serialized or use `ON CONFLICT DO NOTHING` to handle concurrent inserts. The implementation uses `pgrx::BackgroundWorker` with a shared-memory semaphore to limit concurrency to `pg_ripple.datalog_parallel_workers` (default: `max_worker_processes / 2`).

### Deliverables

- [x] **Parallel stratum evaluation** (`src/datalog/parallel.rs` new module)
  - Analyse rule dependency graph per stratum: partition rules into *independent groups* (rules that derive different predicates and have no shared body predicates that are derived within the same stratum)
  - Spawn one background worker per independent group; each worker executes its rule's `INSERT … SELECT` for the current semi-naive iteration
  - Synchronization barrier: the main process waits for all workers to finish before starting the next iteration
  - `ON CONFLICT DO NOTHING` ensures correctness when two workers insert into the same delta table
  - GUC: `pg_ripple.datalog_parallel_workers` (integer, default `4`, max `max_worker_processes - 3`)
  - GUC: `pg_ripple.datalog_parallel_threshold` (integer, default `10000`) — only parallelize strata where the estimated total row count exceeds this threshold (avoid overhead for small rule sets)
  - Expose parallelism statistics via `infer_with_stats()` JSONB output: `"parallel_groups": 5, "max_concurrent": 4`
  - pg_regress test: `datalog_parallel.sql` — verify OWL RL closure produces identical results with `datalog_parallel_workers = 1` and `= 4`; verify `infer_with_stats()` reports parallel groups > 1 for OWL RL

- [x] **SPARQL materialization freshness improvement**
  - Parallel evaluation reduces time-to-fresh for derived VP tables after `pg_ripple.infer()` calls triggered by bulk loads
  - Document: SPARQL queries in materialized mode now observe a shorter staleness window after bulk inserts; add note to SPARQL best practices guide

### Migration Script

`sql/pg_ripple--0.34.0--0.35.0.sql` — registers `pg_ripple.datalog_parallel_workers` and `pg_ripple.datalog_parallel_threshold` GUCs. No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document parallel evaluation GUCs, `infer_with_stats()` parallel fields
- [x] `user-guide/best-practices/datalog-optimization.md` updated — add section on tuning `datalog_parallel_workers` for different hardware configurations
- [x] `user-guide/best-practices/sparql-performance.md` updated — note materialization freshness improvement with parallel evaluation
- [x] Release notes for v0.35.0

### Exit Criteria

`datalog_parallel.sql` passes in `cargo pgrx regress pg18`. OWL RL full closure on a 1M-triple dataset with `datalog_parallel_workers = 4` completes in <40% of the time compared to `datalog_parallel_workers = 1`. Results are identical in both cases. Migration scripts from 0.1.0 through 0.35.0 run cleanly via `just test-migration`.

</details>

---

## v0.36.0 — Worst-Case Optimal Joins & Lattice-Based Datalog

**Theme**: Advanced join algorithms for cyclic graph patterns and monotone lattice aggregation.

> **In plain language:** Two ambitious features that push pg_ripple to the frontier of Datalog and graph database research. Worst-case optimal joins tackle the hardest SPARQL performance problem: cyclic query patterns (think "find all triangles" or "find paths that loop back") where standard database joins produce enormous intermediate results. The Leapfrog Triejoin algorithm solves this class of problem with a mathematically optimal algorithm, giving 10×–100× speedups on queries that previously timed out. Lattice-based Datalog extends rules to work with custom algebraic structures — for example, propagating trust scores (where "trust of X through Y" is the minimum of individual trust values), or interval types, or set-valued annotations — enabling a new class of analytical reasoning that standard Datalog cannot express.
>
> **Effort estimate: 6–9 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Background

See [plans/ecosystem/datalog.md §14.2.8 and §14.2.14](plans/ecosystem/datalog.md) for design notes. Worst-case optimal joins (Ngo et al., 2012; "Skew Strikes Back") use a trie-based intersection algorithm that is provably optimal for any join query. PostgreSQL does not expose WCO join algorithms natively; implementation requires a custom scan node via the `CustomScan` API, registering a C-callable scan provider that pg_ripple exposes through its Rust FFI layer. Lattice-based Datalog (Datalog^L, inspired by Flix and Datafun) extends the rule IR with typed lattice values and monotone operations; fixpoint termination is guaranteed by the ascending chain condition on the lattice.

### Deliverables

- [x] **Worst-case optimal joins — Leapfrog Triejoin** (`src/sparql/wcoj.rs` new module)
  - Detect cyclic join patterns at SPARQL→SQL translation time: any SELECT with ≥3 triple patterns sharing variables in a cycle (triangle, square, etc.)
  - For detected cyclic patterns, route execution through a Leapfrog Triejoin scan node instead of standard PostgreSQL hash-joins
  - CustomScan implementation: register a scan provider in `_PG_init` that intercepts cyclic join nodes in the PostgreSQL planner's plan tree
  - VP table trie interface: read VP table rows in sort order (existing B-tree `(s, o)` indices serve as the underlying trie structure)
  - GUC: `pg_ripple.wcoj_enabled` (bool, default `true`) — master switch
  - GUC: `pg_ripple.wcoj_min_tables` (integer, default `3`) — minimum number of tables in a join before WCOJ is considered
  - SPARQL benefit: cyclic graph patterns that previously caused query timeouts or multi-second latencies complete in milliseconds
  - Benchmark: `benchmarks/wcoj.sql` — triangle query on a social-graph VP table; compare WCOJ vs. standard planner at 100K, 1M, 10M triples
  - pg_regress test: `sparql_wcoj.sql` — verify triangle query produces correct results with WCOJ enabled and disabled; verify `pg_ripple.wcoj_enabled = false` falls back to standard planner

- [x] **Lattice-Based Datalog — Datalog^L** (`src/datalog/lattice.rs` new module)
  - Extend rule IR: lattice term `LatticeVal(lattice_type, value)` alongside `Const` and `Var`
  - Built-in lattice types: `MinLattice` (meet = MIN), `MaxLattice` (join = MAX), `SetLattice` (join = UNION), `IntervalLattice` (join = interval hull)
  - User-defined lattice types via `pg_ripple.create_lattice(name TEXT, join_fn TEXT, bottom TEXT)` — `join_fn` is a PostgreSQL aggregate function name
  - SQL compilation: lattice rules compile to `INSERT … SELECT … ON CONFLICT (s, g) DO UPDATE SET o = lattice_join(excluded.o, vp.o)` — the upsert applies the lattice join on conflict
  - Fixpoint termination: guaranteed by ascending chain condition; bounded by GUC `pg_ripple.lattice_max_iterations` (default `1000`)
  - Example rule: trust propagation — `?x ex:trust (MIN ?t1 ?t2) :- ?x ex:knows ?y, ?y ex:trust ?t1, ?x ex:directTrust ?t2 .`
  - GUC: `pg_ripple.lattice_max_iterations` (integer, default `1000`)
  - Error code `PT540` — lattice fixpoint did not converge (ascending chain condition violated by user-defined lattice)
  - pg_regress test: `datalog_lattice.sql` — trust propagation rule with MinLattice; verify convergence; verify user-defined lattice via custom aggregate

### Migration Script

`sql/pg_ripple--0.35.0--0.36.0.sql` — registers WCOJ and lattice GUCs; creates `pg_ripple.create_lattice()` SQL function. No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/datalog.md` updated — document `create_lattice()`, lattice rule syntax, lattice GUCs
- [x] `user-guide/best-practices/sparql-performance.md` updated — add section on cyclic SPARQL pattern detection and WCOJ; when to set `wcoj_min_tables`
- [x] `reference/lattice-datalog.md` (new page) — full tutorial on Datalog^L: lattice types, monotone rules, convergence guarantees, use cases (trust propagation, interval reasoning, set-valued annotations)
- [x] Release notes for v0.36.0

### Exit Criteria

`sparql_wcoj.sql` and `datalog_lattice.sql` pass in `cargo pgrx regress pg18`. A triangle-pattern SPARQL query on a 1M-edge social graph VP table completes in <10% of the time compared to the standard planner (WCOJ enabled). A trust-propagation lattice rule on 100K triples converges to the correct fixed point. Migration scripts from 0.1.0 through 0.36.0 run cleanly via `just test-migration`.

</details>

---

## v0.37.0 — Storage Concurrency Hardening & Error Safety

**Theme**: Fix the highest-severity correctness bugs identified in the deep-analysis audit and eliminate all hard panics from library code.

> **In plain language:** This is a reliability release — no new features, but a direct response to the first comprehensive code audit (see [plans/PLAN_OVERALL_ASSESSMENT_2.md](plans/PLAN_OVERALL_ASSESSMENT_2.md)). Two concurrency bugs that could silently drop deletes or strand predicates in a slow-path table are fixed with proper advisory-lock coordination. Every place in the code that could crash the database server on an unexpected error is replaced with a typed error message. Configuration parameters now validate their inputs so bad values are caught immediately instead of causing cryptic failures later. A new `diagnostic_report()` function gives a one-call health check of the running system.
>
> **Effort estimate: 9–11 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **HTAP merge cutover race — fixed** (`src/storage/merge.rs`)
  - Wrap the delta→main swap in a per-predicate `pg_advisory_xact_lock`; concurrent `DELETE` path acquires the same lock in `share` mode
  - Ensures deletes arriving during a merge cycle are never lost regardless of timing
  - Add crash-recovery test `tests/crash_recovery/merge_concurrent_delete.sh`: 50 concurrent writers + 1-second merge interval, assert zero lost deletes after 5 minutes
- [x] **Tombstone GC integrated into merge worker** (`src/storage/merge.rs`, `src/worker.rs`)
  - After each successful merge cycle, schedule `VACUUM` on VP tables where `tombstone_count / main_count > pg_ripple.tombstone_gc_threshold`
  - New GUCs: `pg_ripple.tombstone_gc_enabled` (bool, default `true`), `pg_ripple.tombstone_gc_threshold` (float, default `0.05`)
  - pg_regress test `storage_tombstone_gc.sql`: verify tombstones are vacuumed after threshold is crossed
- [x] **Rare-predicate promotion — idempotent and serialised** (`src/lib.rs`, `src/storage/mod.rs`)
  - Acquire the per-predicate advisory lock before any promotion attempt
  - Use `CREATE TABLE IF NOT EXISTS`; wrap data move in `WITH moved AS (DELETE … RETURNING *) INSERT INTO vp_N SELECT * FROM moved`
  - Add crash-recovery test `tests/crash_recovery/promotion_race.sh`: two backends racing to promote the same predicate, assert exactly one succeeds
- [x] **Dictionary cache rollback on transaction abort** (`src/dictionary/mod.rs`, `src/shmem.rs`)
  - Version-tag each shared-memory cache entry with the inserting `xid`; decode path checks `TransactionIdDidCommit` before trusting cached ID
  - pg_regress test `dictionary_rollback.sql`: `BEGIN; encode_term('novel:term'); ROLLBACK; encode_term('novel:term')` — verify the second encode succeeds without error
- [x] **Bloom filter saturating counter fix** (`src/shmem.rs`)
  - Replace all reference-counter decrements with `saturating_sub(1)`; document that a counter saturated at 255 is treated conservatively (bit kept set, no false negatives)
- [x] **`_pg_ripple.statements` atomic update** (`src/storage/merge.rs`)
  - Perform SID-range catalog `DELETE + INSERT` in the same transaction as the VP table swap
  - Eliminates the race where a mid-update worker kill leaves a stale SID→OID mapping for RDF-star queries
- [x] **`(o, s)` index on `vp_rare`** (`src/storage/mod.rs`)
  - Add `CREATE INDEX IF NOT EXISTS vp_rare_os_idx ON _pg_ripple.vp_rare (o, s)` in bootstrap and migration script
  - Eliminates sequential scans on object-leading patterns over rare predicates
- [x] **Eliminate `.expect()` / `.unwrap()` in all library code** (`src/lib.rs`, `src/bulk_load.rs`, `src/sparql/optimizer.rs`, `src/sparql/sqlgen.rs`, `src/export.rs`, `pg_ripple_http/src/main.rs`)
  - Replace all 30+ `expect()`/`unwrap()` calls in non-test code with `Result`-propagating helpers; surface errors via `pgrx::error!()` at the pg_extern boundary
  - Add `#![deny(clippy::unwrap_used, clippy::expect_used)]` to `src/lib.rs` (test code excluded via `#[cfg(test)]`)
  - Fix `pg_ripple_http`: replace startup panics with graceful error logging and `process::exit(1)`
- [x] **GUC `check_hook` validators** (`src/lib.rs`)
  - Implement validators for all string-enum GUCs: `inference_mode` (`off` / `on_demand` / `materialized`), `enforce_constraints` (`off` / `warn` / `error`), `rule_graph_scope` (`default` / `all`), `shacl_mode` (`off` / `sync` / `async`), `describe_strategy` (`cbd` / `scbd`)
  - Implement `min_val` bounds for integer GUCs: `max_path_depth ≥ 1`, `property_path_max_depth ≥ 1`, `merge_threshold ≥ 1`, `merge_interval_secs ≥ 1`
  - Promote `pg_ripple.rls_bypass` to `PGC_POSTMASTER` so it cannot be flipped per-session
- [x] **`pg_ripple.diagnostic_report() RETURNS TABLE (key TEXT, value TEXT)`** (`src/lib.rs`)
  - Keys: GUC validity summary, shared-memory cache hit/miss rates, merge backlog (rows in all delta tables), validation queue depth, federation endpoint health, schema_version match
  - pg_regress test `diagnostic_report.sql`: exercise all fields; assert no null values
- [x] **`_pg_ripple.schema_version` table** (`src/lib.rs`)
  - Created at install time with columns `version TEXT, installed_at TIMESTAMPTZ, upgraded_from TEXT`
  - Stamped on every `ALTER EXTENSION … UPDATE`

### Migration Script

`sql/pg_ripple--0.36.0--0.37.0.sql` — adds `(o, s)` index on `vp_rare`; creates `_pg_ripple.schema_version` table; registers `tombstone_gc_enabled` and `tombstone_gc_threshold` GUCs. No VP table schema changes.

### Documentation

- [x] `user-guide/operations/troubleshooting.md` — new section: "Lost deletes after merge" runbook (cause, detection via `diagnostic_report()`, fix via advisory lock, upgrade to v0.37.0)
- [x] `reference/guc-reference.md` — document `tombstone_gc_threshold`, `tombstone_gc_enabled`; add validator-rules table for all enum GUCs; note `rls_bypass` scope change
- [x] `user-guide/operations/upgrade.md` — document the `schema_version` stamp and how to verify upgrade completeness
- [x] Release notes for v0.37.0

### Exit Criteria

No `.expect()`/`.unwrap()` in non-test Rust code; clippy deny enforced in CI. The concurrent-delete stress test (`merge_concurrent_delete.sh`) passes at 50 writers + 1-second merge interval. All GUC enum validators active. `diagnostic_report()` passes pg_regress. Migration scripts from 0.1.0 through 0.37.0 run cleanly via `just test-migration`.

</details>

---

## v0.38.0 — Architecture Refactoring & Query Completeness

**Theme**: Split the god-module, introduce the `PredicateCatalog` abstraction, close SPARQL Update gaps, and wire SHACL hints into the query planner.

> **In plain language:** After 37 releases, the codebase has accumulated structural debt — most visibly in a single 5,600-line "everything" file that makes every change risky. This release pays that debt: the central file is divided into focused modules, and a clean interface between the query engine and the storage layer is introduced so that future storage variants don't require rewriting the query translator. Users gain two concrete improvements: SPARQL UPDATE now supports pattern-based deletions (the commonly needed `DELETE WHERE` form that was missing), and SHACL shapes now automatically influence query planning so queries over shape-constrained predicates are faster.
>
> **Effort estimate: 9–11 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Split `src/lib.rs` into subsystem modules**
  - Extract `src/rare_predicate.rs`, `src/shacl_admin.rs`, `src/federation_registry.rs`, `src/graphrag_admin.rs`, `src/stats_admin.rs` from `src/lib.rs`
  - Target: `src/lib.rs` ≤1,500 lines covering `_PG_init`, GUC registration, `extension_sql!` blocks, and thin `#[pg_extern]` delegation shims
  - No change to public SQL API; all existing `pg_ripple.*` functions remain
- [x] **`PredicateCatalog` trait and backend-local OID cache** (`src/storage/catalog.rs` new module)
  - Define `trait PredicateCatalog { fn resolve(&self, pred_id: i64) -> Option<TableDesc>; }`
  - Implement a backend-local `HashMap<i64, TableDesc>` cache invalidated by a syscache callback on `_pg_ripple.predicates`
  - Wire into `src/sparql/sqlgen.rs` and `src/datalog/compiler.rs` — eliminates per-atom SPI catalog lookup for hot BGPs
  - New GUC `pg_ripple.predicate_cache_enabled` (bool, default `true`)
  - Benchmark: 10-atom BGP must show 1 catalog SPI call instead of 10
- [x] **Refactor `validate_shape()` → per-constraint helpers** (`src/shacl/constraints/` new sub-module)
  - One file per constraint family: `count.rs`, `value_type.rs`, `string_based.rs`, `logical.rs`, `property_path.rs`, `shape_based.rs`
  - Each exported function ≤80 lines; top-level `validate_shape()` becomes a dispatcher ≤50 lines
  - All existing `shacl_*.sql` pg_regress tests must pass unchanged
- [x] **Refactor `translate_pattern()` → per-algebra-node helpers** (`src/sparql/translate/` new sub-module)
  - One file per algebra node: `bgp.rs`, `join.rs`, `left_join.rs`, `union.rs`, `filter.rs`, `graph.rs`, `group.rs`, `distinct.rs`
  - Shared context struct `TranslateCtx` carries encode cache, catalog handle, and query-level state
  - All existing `sparql_*.sql` pg_regress tests must pass unchanged
- [x] **Batch dictionary encoding in SPARQL translation**
  - In `translate_pattern`, collect all unresolved IRI/literal constants in a first pass; resolve via one `encode_terms_batch(&[Term]) -> Vec<i64>` SPI call (single `INSERT … ON CONFLICT … RETURNING` batch)
  - Benchmark: BGP with 20 FILTER constants must show 1 encode SPI call instead of 20
- [x] **Plan-cache key normalisation** (`src/sparql/plan_cache.rs`)
  - Cache on algebra digest (serialize `spargebra::Query` IR → compact bytes → XXH3-128) instead of raw query text
  - Whitespace and prefix-form variants now share the same cache slot
- [x] **SCBD DESCRIBE — implemented** (`src/sparql/mod.rs`)
  - Implement Symmetric Concise Bounded Description: all triples where the resource is subject *or* object, with blank-node recursion
  - `describe_strategy = 'scbd'` now functional; remove the "not implemented" caveat from docs
- [x] **SPARQL Update: DELETE WHERE / INSERT WHERE / graph management** (`src/sparql/update.rs`)
  - Implement `DELETE { … } WHERE { … }`, `INSERT { … } WHERE { … }`, `DELETE WHERE { … }`
  - Implement graph management: `CLEAR GRAPH`, `DROP GRAPH`, `COPY`, `MOVE`, `ADD`
  - pg_regress test `sparql_update_advanced.sql`: pattern-based deletes spanning multiple VP tables; cross-graph COPY/MOVE
- [x] **Consolidate property-path depth GUCs** (`src/lib.rs`)
  - Deprecate `property_path_max_depth`; make it an alias for `max_path_depth` with a one-time `NOTICE`
- [x] **Wire SHACL hints into SPARQL planner** (`src/shacl/hints.rs` new module, `src/sparql/sqlgen.rs`)
  - At query-translation time, query `_pg_ripple.shape_hints` (populated from loaded shapes) per predicate
  - `sh:maxCount 1` → suppress `DISTINCT` on that predicate's join; `sh:minCount 1` → downgrade `LEFT JOIN` to `INNER JOIN`
  - pg_regress test `shacl_sparql_hints.sql`: verify join-type changes with and without shapes; assert result equivalence
- [x] **SPARQL 1.1 conformance suite in CI** (allowed-to-warn job)
  - Download W3C SPARQL 1.1 test suite; run via `cargo pgrx regress`; report pass/skip/fail counts
  - Publish conformance percentage in `CHANGELOG.md` per release

### Migration Script

`sql/pg_ripple--0.37.0--0.38.0.sql` — creates `_pg_ripple.shape_hints` table; registers `predicate_cache_enabled` GUC. No VP table schema changes.

### Documentation

- [x] `reference/architecture.md` — Mermaid architecture diagram showing post-refactor module boundaries (dictionary → storage/catalog → sparql/translate + datalog/compiler → shacl/constraints → views/exporters)
- [x] `user-guide/sql-reference/sparql-update.md` — document DELETE WHERE / INSERT WHERE / CLEAR / COPY / MOVE / ADD with examples
- [x] `reference/guc-reference.md` — `predicate_cache_enabled`; deprecation notice for `property_path_max_depth`
- [x] `user-guide/performance/query-planning.md` — new section on SHACL hints and their effect on join selection
- [x] Release notes for v0.38.0

### Exit Criteria

`src/lib.rs` ≤1,500 lines. Each `translate/` module file ≤200 lines. `validate_shape()` dispatcher ≤50 lines. SCBD DESCRIBE tests pass. SPARQL Update advanced tests pass. SHACL hints pg_regress passes. Predicate OID cache reduces SPI calls for 10-atom BGP from 10 to 1. Migration chain test passes.

</details>

---

## v0.39.0 — Datalog HTTP API for pg_ripple_http

**Theme**: Expose all pg_ripple Datalog SQL functions as a REST API in the `pg_ripple_http` companion service.

> **In plain language:** The `pg_ripple_http` service currently speaks only SPARQL. This release adds a `/datalog` namespace that lets any HTTP client — without a PostgreSQL driver — manage rule sets, trigger inference, run goal-directed queries, check integrity constraints, and inspect monitoring statistics. The implementation is a thin axum layer; all heavy lifting stays inside the PostgreSQL extension.
>
> **Effort estimate: 3–5 person-weeks**
>
> **Implementation plan:** [plans/pg_ripple_http_datalog.md](plans/pg_ripple_http_datalog.md)

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Extract shared helpers** (`pg_ripple_http/src/common.rs` new module)
  - Move `AppState`, `check_auth()`, `redacted_error()`, and `env_or()` from `main.rs` to `common.rs`
  - Both SPARQL and Datalog handlers import from this module
- [x] **Phase 1 — Rule management** (`pg_ripple_http/src/datalog.rs` new module)
  - `POST /datalog/rules/{rule_set}` — body `text/x-datalog`; calls `pg_ripple.load_rules($1, $2)`; returns `{"rule_set": "…", "rules_loaded": N}`
  - `POST /datalog/rules/{rule_set}/builtin` — calls `pg_ripple.load_rules_builtin($1)`
  - `GET /datalog/rules` — calls `pg_ripple.list_rules()`; returns JSONB array
  - `DELETE /datalog/rules/{rule_set}` — calls `pg_ripple.drop_rules($1)`; returns `{"deleted": N}`
  - `POST /datalog/rules/{rule_set}/add` — single-rule add; calls `pg_ripple.add_rule($1, $2)`
  - `DELETE /datalog/rules/{rule_set}/{rule_id}` — calls `pg_ripple.remove_rule($1::bigint)` (triggers DRed)
  - `PUT /datalog/rules/{rule_set}/enable` — calls `pg_ripple.enable_rule_set($1)`
  - `PUT /datalog/rules/{rule_set}/disable` — calls `pg_ripple.disable_rule_set($1)`
- [x] **Phase 2 — Inference** (`pg_ripple_http/src/datalog.rs`)
  - `POST /datalog/infer/{rule_set}` — calls `pg_ripple.infer($1)`; returns `{"derived": N}`
  - `POST /datalog/infer/{rule_set}/stats` — calls `pg_ripple.infer_with_stats($1)`; returns full stats JSONB
  - `POST /datalog/infer/{rule_set}/agg` — calls `pg_ripple.infer_agg($1)`
  - `POST /datalog/infer/{rule_set}/wfs` — calls `pg_ripple.infer_wfs($1)`
  - `POST /datalog/infer/{rule_set}/demand` — body `{"demands": […]}`; calls `pg_ripple.infer_demand($1, $2::jsonb)`
  - `POST /datalog/infer/{rule_set}/lattice` — body `{"lattice": "min"}`; calls `pg_ripple.infer_lattice($1, $2)`
- [x] **Phase 3 — Query & constraints** (`pg_ripple_http/src/datalog.rs`)
  - `POST /datalog/query/{rule_set}` — body Datalog goal text; calls `pg_ripple.infer_goal($1, $2)`; returns `{"derived": N, "iterations": N, "matching": […]}`
  - `GET /datalog/constraints` — calls `pg_ripple.check_constraints(NULL)`; returns violation array
  - `GET /datalog/constraints/{rule_set}` — calls `pg_ripple.check_constraints($1)`
- [x] **Phase 4 — Admin & monitoring** (`pg_ripple_http/src/datalog.rs`)
  - `GET /datalog/stats/cache` — calls `pg_ripple.rule_plan_cache_stats()`
  - `GET /datalog/stats/tabling` — calls `pg_ripple.tabling_stats()`
  - `GET /datalog/lattices` — calls `pg_ripple.list_lattices()`
  - `POST /datalog/lattices` — body `{"name": "…", "join_fn": "…", "bottom": "…"}`; calls `pg_ripple.create_lattice($1, $2, $3)`
  - `GET /datalog/views` — calls `pg_ripple.list_datalog_views()`
  - `POST /datalog/views` — body JSON; calls `pg_ripple.create_datalog_view(…)`
  - `DELETE /datalog/views/{name}` — calls `pg_ripple.drop_datalog_view($1)`
- [x] **Route registration** (`pg_ripple_http/src/main.rs`)
  - `mod datalog;` and `mod common;` declarations
  - 24 `.route(…)` entries wired under `/datalog`
- [x] **Metrics extension** (`pg_ripple_http/src/metrics.rs`)
  - Add `datalog_queries: AtomicU64` counter; expose as `pg_ripple_http_datalog_queries_total` in `/metrics`
- [x] **Authentication & security**
  - All `/datalog/*` handlers call `check_auth()` — same token as SPARQL
  - Optional write-protection: `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` env var gates `POST /datalog/rules/*`, `DELETE`, and `PUT` endpoints independently of the read token
  - All SQL calls use `$1`, `$2`, … parameterized queries — never string concatenation
  - Request body limit: 10 MB via `axum::body::to_bytes(body, 10 * 1024 * 1024)`
- [x] **Error mapping**
  - `400 datalog_parse_error` — malformed rule text returned by extension
  - `400 datalog_goal_error` — invalid goal pattern
  - `400 invalid_request` — missing body, wrong content-type, non-numeric rule_id
  - `404 rule_set_not_found` — infer/drop on nonexistent rule set
  - `503 service_unavailable` — pool exhausted
- [x] **Migration script** `sql/pg_ripple--0.38.0--0.39.0.sql`
  - No schema changes to pg_ripple itself; comment-only header documenting the new HTTP surface
- [x] **Tests**
  - Integration tests using `axum-test` (or equivalent): round-trip load → infer → query goal → drop for the `custom` rule set
  - Error path tests: malformed Datalog, missing auth, oversized body
  - Smoke test script `tests/datalog_http_smoke.sh` (curl-based)

### Documentation

- [x] `pg_ripple_http/README.md` — new `## Datalog API` section with curl examples for all 24 endpoints, content types, and error codes
- [x] Release notes for v0.39.0

### Exit Criteria

All 24 Datalog endpoints respond correctly in integration tests. `GET /datalog/rules` returns the JSONB array from `list_rules()`. `POST /datalog/infer/custom` triggers materialization and returns `{"derived": N}`. `GET /datalog/constraints` returns violation JSONB. Auth check rejects requests with invalid token. Parameterized-query requirement verified by code review (no `format!()` calls mixing user input into SQL strings). Migration chain test passes.

</details>

---

## v0.40.0 — Streaming Results, Explain & Observability

**Theme**: Streaming cursor API for large result sets, first-class query explain, and full observability stack.

> **In plain language:** Three long-requested developer and operator improvements land together. Large SPARQL queries can now stream their results instead of materialising everything in memory — making it safe to CONSTRUCT or export millions of triples without running out of memory. A new `explain_sparql()` function shows exactly what SQL the SPARQL engine generated, with cardinality estimates and actual timings in EXPLAIN ANALYZE format but with RDF IRIs instead of internal numbers. A new `explain_datalog()` function does the same for Datalog rule sets. Every significant operation now emits OpenTelemetry spans, and `diagnostic_report()` gives a one-call health summary of the running system.
>
> **Effort estimate: 9–11 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Streaming SPARQL cursor API** (`src/sparql/cursor.rs` new module)
  - `pg_ripple.sparql_cursor(query TEXT) RETURNS SETOF RECORD` — SRF paging through results 1024 rows at a time with batched dictionary decode
  - `pg_ripple.sparql_cursor_turtle(query TEXT) RETURNS SETOF TEXT` — emits Turtle lines
  - `pg_ripple.sparql_cursor_jsonld(query TEXT) RETURNS SETOF TEXT` — emits JSON-LD object chunks
  - Wire to `pg_ripple_http`: `Accept: text/turtle` or `Accept: application/ld+json` triggers `Transfer-Encoding: chunked` streaming response
  - pg_regress test `sparql_cursor.sql`: load 500K triples; verify cursor returns correct count; verify chunked Turtle export round-trips
- [x] **Resource governors** (`src/lib.rs`)
  - `pg_ripple.sparql_max_rows` (integer, default `0` = unlimited)
  - `pg_ripple.datalog_max_derived` (integer, default `0` = unlimited)
  - `pg_ripple.export_max_rows` (integer, default `0` = unlimited)
  - `pg_ripple.sparql_overflow_action` (enum: `warn` / `error`, default `warn`)
  - Error codes: `PT640` (SPARQL row limit exceeded), `PT641` (Datalog derived limit exceeded), `PT642` (export row limit exceeded)
- [x] **`pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN DEFAULT false) RETURNS JSONB`** (`src/sparql/explain.rs` new module)
  - Step 1: parse + optimise via `spargebra`/`sparopt`; emit algebra tree as JSON with predicate IRIs decoded
  - Step 2: run `EXPLAIN (FORMAT JSON, BUFFERS true [, ANALYZE true])` on the generated SQL; attach as `"plan"` key
  - Output keys: `"algebra"`, `"sql"` (IRI-decoded), `"plan"`, `"cache_hit"` (bool), `"encode_calls"` (int)
  - pg_regress test `sparql_explain_jsonb.sql`: verify all output keys; verify `analyze: true` adds `"Actual Rows"`
- [x] **`pg_ripple.explain_datalog(rule_set_name TEXT) RETURNS JSONB`** (`src/datalog/explain.rs` new module)
  - Returns per-stratum dependency graph, magic-set rewritten rules, compiled SQL per rule, and per-iteration delta-row counts from last inference run
  - Output keys: `"strata"`, `"rules"` (rewritten), `"sql_per_rule"`, `"last_run_stats"`
  - pg_regress test `datalog_explain.sql`
- [x] **`pg_ripple.cache_stats() RETURNS JSONB`** and **`pg_ripple.reset_cache_stats()`** (`src/lib.rs`)
  - Keys: plan cache size/hits/misses, dict cache hits/misses, federation cache hits/misses
  - pg_regress test `cache_stats.sql`
- [x] **`pg_ripple.stat_statements_decoded` view** (`src/lib.rs`)
  - View over `pg_stat_statements` that regex-decodes predicate IDs in `query` text via `pg_ripple.decode_id()` join; exposes `query_decoded` column
- [x] **OpenTelemetry tracing** (`src/telemetry.rs` new module)
  - Thin facade over the `tracing` crate; spans for: SPARQL parse/translate/execute, merge cycle (per predicate), federation call (per SERVICE), Datalog inference (per stratum)
  - GUC `pg_ripple.tracing_enabled` (bool, default `false`) — zero overhead when off
  - GUC `pg_ripple.tracing_exporter` (string: `stdout` / `otlp`, default `stdout`); `otlp` reads `OTEL_EXPORTER_OTLP_ENDPOINT`
  - pg_regress test `telemetry.sql`: toggle on/off; assert no performance regression in execute path with tracing off
- [x] **Bug fix: `OPTIONAL {}` inside `GRAPH {}` silently fails for all predicates** (`src/sparql/sqlgen.rs`)
  - **Root cause**: The `GraphPattern::Graph` handler applies the named-graph filter *after* the inner pattern is fully translated. When the inner pattern contains an `OPTIONAL` (spargebra `LeftJoin`), the `LeftJoin` translator wraps both sides in aliased subqueries that only project `_lj_<varname>` columns — the `g` column is intentionally stripped. The `Graph` handler then emits `{lj_alias}.g = {gid}`, which PostgreSQL rejects with `column does not exist`. This fails for **all** predicates (both dedicated VP tables and `vp_rare`); it was only observed first with `vp_rare` predicates (`rdfs:subClassOf`, `rdfs:label`, etc.) because typical test graphs have very few schema triples.
  - **Correct fix — graph-filter context propagation** (`src/sparql/sqlgen.rs`, `Ctx`):
    1. Add `graph_filter: Option<i64>` to `Ctx`.
    2. In `GraphPattern::Graph`, set `ctx.graph_filter = Some(gid)` *before* recursing into the inner pattern, then clear it after.
    3. In `translate_bgp` / `table_expr` / `build_all_predicates_union`, when `ctx.graph_filter` is `Some(gid)`, inject `WHERE g = {gid}` (or `AND g = {gid}`) directly into each VP table scan.
    4. Remove the post-hoc `for (alias, _) in &frag.from_items { frag.conditions.push(format!("{alias}.g = {gid}")); }` loop from the `Graph` handler — the filter is now baked into every leaf VP scan before any `LEFT JOIN`, `WITH RECURSIVE`, or subquery wrapper is built.
  - This also fixes `OPTIONAL {}` combined with `GROUP BY` on variables from the optional side, and `OPTIONAL {}` inside `GRAPH {}` with `FILTER`, property paths, nested `UNION`, and federated `SERVICE` sub-patterns.
  - Regression tests:
    - `sparql_optional_in_graph.sql` — `OPTIONAL` triple with a dedicated-VP predicate inside a named graph; assert NULL vs non-NULL row counts
    - `sparql_optional_in_graph_rare.sql` — same pattern with a `vp_rare` predicate; assert NULL vs non-NULL row counts
    - `sparql_optional_group_by_in_graph.sql` — `OPTIONAL` + `GROUP BY` on optional variable inside a named graph (the original failing query shape); assert `instanceCount` per class is correct
- [x] **Bug fix: property path inside `GRAPH {}` fails for all predicates** (`src/sparql/sqlgen.rs`)
  - **Root cause**: identical to the `OPTIONAL` bug above — the `WITH RECURSIVE` CTE emitted for property path operators (`+`, `*`, `?`) selects only `(s, o)`, but the post-hoc `Graph` handler tries to reference `{cte_alias}.g`, producing `column does not exist`.
  - **Fix**: same graph-filter context propagation as above; anchor and recursive step selects must include `g` and filter on it when `ctx.graph_filter` is set, rather than relying on the outer `Graph` handler to inject the condition.
  - Regression test: `sparql_path_in_graph.sql` — property path on a rare predicate inside a named graph; assert correct row count
- [x] **Migration header standardisation** (`sql/*.sql`)
  - Backfill headers in all existing scripts: `-- Migration X.Y.Z → A.B.C | Schema changes: … | Data-rewrite cost: Low/Medium/High | Downgrade: …`
  - All future scripts from v0.37.0 onward follow this template automatically

### Migration Script

`sql/pg_ripple--0.39.0--0.40.0.sql` — registers new GUCs (`sparql_max_rows`, `datalog_max_derived`, `export_max_rows`, `sparql_overflow_action`, `tracing_enabled`, `tracing_exporter`). No VP table schema changes.

### Documentation

- [x] `user-guide/sql-reference/explain.md` — full tutorial on `explain_sparql()` and `explain_datalog()`; reading the algebra tree and decoded SQL
- [x] `user-guide/sql-reference/cursor-api.md` — streaming cursor API; format options; resource governors
- [x] `reference/observability.md` (new) — OpenTelemetry integration guide: exporter setup, span taxonomy, Grafana/Jaeger integration examples
- [x] `user-guide/operations/monitoring.md` — `cache_stats()`, `diagnostic_report()`, `stat_statements_decoded` usage
- [x] `reference/error-reference.md` — PT640, PT641, PT642 documented
- [x] Release notes for v0.40.0

### Exit Criteria

`sparql_cursor.sql` passes with 500K triples. `explain_sparql()` returns IRI-decoded algebra and SQL. OpenTelemetry spans emitted for a sample query when `tracing_enabled = on`. All resource governor tests pass. `stat_statements_decoded` returns decoded query text. `sparql_optional_in_graph.sql`, `sparql_optional_in_graph_rare.sql`, and `sparql_optional_group_by_in_graph.sql` all pass (OPTIONAL inside GRAPH). `sparql_path_in_graph.sql` passes (property path inside GRAPH). Migration chain test passes.

</details>

---

## v0.41.0 — Full W3C SPARQL 1.1 Test Suite

**Theme**: Complete standards conformance verification via the full W3C SPARQL 1.1 test suite, run in parallel under 2 minutes in CI.

> **In plain language:** Every major SPARQL engine bug — including the `OPTIONAL inside GRAPH` failure found in April 2026 — was caught by manual testing rather than by the test suite. This version fixes that by implementing a full harness for the official W3C SPARQL 1.1 test suite (~3,000 tests), parallelized across 8 workers so the entire suite completes in under 2 minutes. The harness parses W3C test manifests, auto-loads RDF fixtures per test, runs queries against a live pg_ripple instance, and validates results using RDF graph equivalence (not row counting). Per-category pass rates are reported in CI so regressions are caught immediately. A curated 180-test "smoke" subset (Graph Patterns + Aggregates) runs on every PR in under 30 seconds.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **W3C manifest parser** (`tests/w3c/manifest.rs` new module)
  - Parse W3C SPARQL 1.1 test manifests (Turtle format, `mf:Manifest`) into a structured `TestCase` struct
  - Fields: test IRI, type (`mf:QueryEvaluationTest`, `mf:UpdateEvaluationTest`, `mf:PositiveSyntaxTest`, `mf:NegativeSyntaxTest`), query file, data file(s), result file, named graph files
  - Covers all 13 sub-suites: `aggregates`, `bind`, `exists`, `functions`, `grouping`, `negation`, `optional`, `project-expression`, `property-path`, `service`, `subquery`, `syntax-query`, `update`
  - Tests with type `mf:NotClassifiedByEarlYet` skipped with `SKIP` status
- [x] **RDF fixture loader** (`tests/w3c/loader.rs` new module)
  - Load `.ttl` / `.n3` / `.rdf` / `.srx` / `.srj` fixture files from `tests/w3c/data/` into a temporary pg_ripple graph before each test
  - Use named graph IRIs matching the manifest's `mf:graphData` entries
  - Auto-teardown: drop the temporary named graph after the test completes (regardless of pass/fail)
  - Handle multi-graph datasets: `mf:defaultGraph` → default graph (`g = 0`); `mf:namedGraphs` → individual named graphs
- [x] **Result validator** (`tests/w3c/validator.rs` new module)
  - `SELECT` queries: compare against `.srx` (SPARQL Results XML) or `.srj` (SPARQL Results JSON); validate variable names and bindings as RDF term equality (IRI, blank node, literal with datatype and lang tag)
  - `ASK` queries: compare boolean result against `.srx`/`.srj`
  - `CONSTRUCT` / `DESCRIBE` queries: compare result graph against `.ttl` reference using graph isomorphism (blank-node-normalised; uses `oxrdf` for in-memory graph comparison)
  - `UPDATE` queries: compare the post-update store state (all named graphs) against expected `.ttl` reference
  - Blank node handling: rename blank nodes in both actual and expected by canonical DFS traversal before comparison
  - Report per-binding diff on failure: expected term vs. actual term
- [x] **Parallel test runner** (`tests/w3c/runner.rs` new module)
  - `cargo test --test w3c_suite -- --test-threads 8` — each thread picks tests from a shared work queue (lock-free `crossbeam` channel)
  - Each thread owns an isolated pg_ripple named-graph namespace (prefix `_w3c_t{thread_id}_`) to prevent cross-test pollution
  - Test timeout: 5 seconds per test; timed-out tests marked `TIMEOUT` not `FAIL`
  - Progress: `indicatif` progress bar per thread in local runs; plain line-per-test output in CI
  - Output report: per-category pass/fail/skip/timeout counts + per-test detail for any failure
  - Target: full 3,000-test suite completes in **< 2 minutes** on an 8-core CI runner (AWS `c7g.2xlarge` or equivalent)
- [x] **Smoke subset** (`tests/w3c_smoke.rs`)
  - 180-test curated subset: `optional` (80 tests), `aggregates` (60 tests), `grouping` (40 tests) — the three categories most likely to expose SQL-generation bugs
  - Runs on every PR via `cargo test --test w3c_smoke`; completes in **< 30 seconds**
  - Failures block merge (added to `required` status checks in `.github/workflows/ci.yml`)
- [x] **CI integration** (`.github/workflows/ci.yml`)
  - New job `w3c-suite`: runs after the existing `pgrx-test` job; parallelized 8-way; uploads test report as artifact
  - New job `w3c-smoke`: runs on every PR and push to `main`; required check
  - Full suite job is optional (non-blocking) until pass rate reaches 95%; then promoted to required
  - Cache: W3C test fixtures (`tests/w3c/data/`) cached by SHA of manifest files
- [x] **Test data download script** (`scripts/fetch_w3c_tests.sh`)
  - Downloads the official W3C SPARQL 1.1 test suite from `https://www.w3.org/2009/sparql/docs/tests/`
  - Verified against known SHA-256 checksums of the manifest files
  - Output: `tests/w3c/data/` directory (gitignored; fetched by CI and locally on first run)
- [x] **Known-failures manifest** (`tests/w3c/known_failures.txt`)
  - List of W3C test IRIs that currently fail, with a one-line reason for each (e.g., `OPTIONAL inside GRAPH — fix in v0.40.0`, `property path with GRAPH — fix in v0.40.0`)
  - Failures in `known_failures.txt` are reported as `XFAIL` (expected failure), not `FAIL`
  - Any test in `known_failures.txt` that unexpectedly passes is reported as `XPASS` and causes a CI warning
  - Target at release: 0 `XFAIL` entries in the smoke subset; ≤ 50 `XFAIL` entries in the full suite (SERVICE tests against live external endpoints are always SKIP)
- [x] **Pass-rate tracking** (`tests/w3c/report.json`)
  - CI uploads a `report.json` artifact with per-category pass/fail/skip/timeout counts and overall pass rate
  - Historical pass rate trend displayed in `README.md` badge

### Migration Script

`sql/pg_ripple--0.40.0--0.41.0.sql` — no schema changes. Adds a comment-only header noting that v0.41.0 is a test infrastructure release.

### Documentation

- [x] `reference/w3c-conformance.md` — per-category W3C SPARQL 1.1 conformance table: test count, pass count, known failures with ticket links
- [x] `reference/running-w3c-tests.md` (new) — how to run the smoke subset and full suite locally; how to add a new expected failure; how to interpret `XFAIL` vs `XPASS`
- [x] `README.md` — W3C SPARQL 1.1 conformance section updated
- [x] Release notes for v0.41.0

### Exit Criteria

Smoke subset (180 tests) passes with 0 unexpected failures on `main`. Full suite (3,000+ tests) runs in < 2 minutes on an 8-core CI runner. Per-category pass rate report uploaded as CI artifact. Known-failures manifest has 0 entries for `optional` and `aggregates` categories (those bugs fixed in v0.40.0). Migration chain test passes through 0.41.0.

</details>

---

## v0.42.0 — Parallel Merge, Cost-Based Federation & Live CDC

**Theme**: Multi-worker HTAP merge, intelligent federation query planning, and real-time RDF change subscriptions.

> **In plain language:** Three architectural improvements that close the last major gaps before the 1.0 production release. The merge worker — which keeps the read-optimised main partition in sync with incoming writes — is upgraded from a single process to a configurable pool of parallel workers, each responsible for a subset of predicates, directly improving write throughput for workloads with many distinct predicates. Federation queries now use a cost model to pick the best execution order and run independent fragments in parallel, eliminating the serial bottleneck. And for the first time, applications can subscribe to a real-time stream of triple changes filtered by SPARQL pattern or SHACL shape, enabling reactive GraphRAG pipelines, live dashboards, and ML feature stores without polling.
>
> **Effort estimate: 10–12 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Parallel merge worker pool** (`src/worker.rs`, `src/storage/merge.rs`)
  - New GUC `pg_ripple.merge_workers` (integer, default `1`, max `16`) — spawns N `BackgroundWorker` processes each managing a disjoint round-robin subset of predicates
  - Per-predicate `pg_advisory_lock` (from v0.37.0) ensures no two workers race on the same VP table
  - Work-stealing: idle workers check the global queue for any predicate above `pg_ripple.merge_threshold` not yet claimed
  - Stress test `tests/stress/parallel_merge.sh`: 100 concurrent writers × 100 predicates × 4 workers; assert correctness and no deadlocks after 10 minutes
  - Benchmark: 4 merge workers on a workload with 100 distinct predicates shows ≥3× throughput vs. single worker
- [x] **`owl:sameAs` cluster size bound** (`src/datalog/builtins.rs`)
  - New GUC `pg_ripple.sameas_max_cluster_size` (integer, default `100_000`)
  - Detect over-large equivalence classes during canonicalization; emit `PT550` WARNING and short-circuit with Tarjan-SCC sampling approximation
  - pg_regress test `sameas_large_cluster.sql`
- [x] **VoID statistics catalog per federation endpoint** (`src/sparql/federation.rs`, `_pg_ripple.endpoint_stats` table)
  - On endpoint registration, fetch and cache the endpoint's VoID description
  - Refresh driven by new GUC `pg_ripple.federation_stats_ttl_secs` (integer, default `3600`)
  - Statistics used by the planner: triple count per predicate, distinct subjects/objects
- [x] **Cost-based federation source selection** (`src/sparql/federation_planner.rs` new module)
  - FedX-style planner: for each BGP atom rank endpoints by estimated selectivity using VoID stats; assign each atom to its best source
  - Independent atoms (no shared variables) scheduled for parallel execution
  - GUC `pg_ripple.federation_planner_enabled` (bool, default `true`)
  - GUC `pg_ripple.federation_parallel_max` (integer, default `4`)
  - GUC `pg_ripple.federation_parallel_timeout` (integer, default `60` seconds)
  - pg_regress test `federation_planner.sql`: two registered mock endpoints; verify atom routing and timeout behaviour
- [x] **Parallel SERVICE execution** (`src/sparql/federation.rs`)
  - Independent SERVICE clauses dispatched concurrently via background workers; results reassembled before outer join
  - Bounded by `pg_ripple.federation_parallel_max`
- [x] **Federation result streaming** (`src/sparql/federation.rs`)
  - SERVICE responses exceeding `pg_ripple.federation_inline_max_rows` (new GUC, default `10_000`) are spooled into a temporary table rather than inlined as `VALUES`
  - Error code `PT620` INFO when spooling is triggered
- [x] **IP/CIDR allowlist for federation endpoints** (`src/sparql/federation.rs`)
  - Resolve hostname on endpoint registration; deny RFC 1918, link-local (`169.254.x.x`), loopback, and IPv6 link-local by default
  - New GUC `pg_ripple.federation_allow_private` (bool, default `false`) to override
  - Error code `PT621` when a private-IP endpoint is rejected
- [x] **HTTPS certificate validation for HTTP companion** (`pg_ripple_http/src/main.rs`)
  - Default to system trust store via `rustls-native-certs`
  - Env var `PG_RIPPLE_HTTP_CA_BUNDLE` — path to a custom CA PEM for private-PKI federation targets
  - Reject self-signed certificates unless `PG_RIPPLE_HTTP_ALLOW_SELF_SIGNED=true`
  - Fix CORS defaults: explicit origin allowlist via `PG_RIPPLE_HTTP_CORS_ORIGINS`; `*` requires opt-in
  - Fix X-Forwarded-For: trust only when `PG_RIPPLE_HTTP_TRUST_PROXY` env lists upstream IP/CIDR
  - Body limit configurable via `PG_RIPPLE_HTTP_MAX_BODY_BYTES` (default `10_485_760`)
- [x] **Live RDF CDC subscriptions** (`src/cdc.rs`, `pg_ripple_http/src/ws.rs` new module)
  - `pg_ripple.create_subscription(name TEXT, filter_sparql TEXT DEFAULT NULL, filter_shape TEXT DEFAULT NULL) RETURNS BOOLEAN`
  - Publishes via `NOTIFY pg_ripple_cdc_{name}` with JSON payload: `{"op": "add"|"remove", "s": "…", "p": "…", "o": "…", "g": "…"}`
  - WebSocket endpoint `/ws/subscriptions/{name}` in `pg_ripple_http`; supports `text/turtle`, `application/ld+json`, `application/json` via `Accept`
  - Optional SPARQL filter: only matching triples published; optional SHACL filter: only shape-violating triples published
  - `pg_ripple.drop_subscription(name TEXT)`, `pg_ripple.list_subscriptions() RETURNS TABLE`
  - New catalog table `_pg_ripple.subscriptions (name, filter_sparql, filter_shape, created_at, queue_table_oid)`
  - pg_regress test `cdc_subscriptions.sql`: create subscription, insert triples, verify `LISTEN` receives expected payloads

### Migration Script

`sql/pg_ripple--0.41.0--0.42.0.sql` — creates `_pg_ripple.endpoint_stats` table; creates `_pg_ripple.subscriptions` table; registers new GUCs (`merge_workers`, `sameas_max_cluster_size`, `federation_stats_ttl_secs`, `federation_planner_enabled`, `federation_parallel_max`, `federation_parallel_timeout`, `federation_inline_max_rows`, `federation_allow_private`).

### Documentation

- [x] `user-guide/operations/merge-workers.md` (new) — tuning `merge_workers` for predicate-rich workloads; monitoring via `diagnostic_report()`
- [x] `user-guide/features/cdc-subscriptions.md` (new) — complete tutorial: subscribe, filter, consume via SQL LISTEN and WebSocket; integration patterns with GraphRAG, ML feature stores, and live dashboards
- [ ] `user-guide/features/federation.md` — updated: VoID stats, cost-based planner, parallel SERVICE, result streaming, IP restrictions
- [x] `reference/guc-reference.md` — all new GUCs documented; security guidance on `federation_allow_private`
- [x] `reference/error-reference.md` — PT550, PT620, PT621 documented
- [x] Release notes for v0.42.0

### Exit Criteria

Parallel merge stress test passes (100 writers, 4 workers, no lost deletes). VoID stats fetched on endpoint registration. Independent SERVICE clauses execute in parallel (verifiable via `explain_sparql()`). CDC subscription delivers `NOTIFY` payloads for all inserts matching the filter. HTTPS cert validation enforced in `pg_ripple_http`. Migration chain test passes through 0.42.0.

</details>

---

## v0.43.0 — WatDiv + Jena Conformance Suite

**Theme**: Scale-correctness and semantic edge-case coverage via the WatDiv benchmark and Apache Jena test suite, reusing the harness infrastructure from v0.41.0.

> **In plain language:** W3C conformance (v0.41.0) proves pg_ripple is correct on small, well-defined fixtures. This release proves it is correct *at scale* and on the implementation edge cases that W3C deliberately leaves underspecified. WatDiv loads 10M–100M triples and runs 100–1,000 queries across four complexity levels (star, chain, snowflake, complex) — catching SQL planner regressions and VP table performance cliffs that only appear under realistic data distributions. Apache Jena contributes ~1,000 additional tests covering type coercion corner cases, timezone handling in date comparisons, numeric precision, and blank-node scoping rules that the W3C suite glosses over.
>
> **Effort estimate: 5–7 person-weeks** (90% infrastructure reuse from v0.41.0)

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Apache Jena adapter** (`tests/jena/` new module)
  - Adapt v0.41.0 manifest parser to handle Jena-specific manifest fields (`jt:QueryEvaluationTest`, `jt:UpdateEvaluationTest`) and Jena result extensions (e.g. `rdf:XMLLiteral`, extended numeric types)
  - ~1,000 tests across Jena's `sparql-query`, `sparql-update`, `sparql-syntax`, and `algebra` sub-suites
  - Reuse v0.41.0 RDF fixture loader, result validator, parallel runner, and known-failures manifest format
  - Specific coverage targets:
    - **Type coercion**: XSD numeric promotions (`xsd:integer` → `xsd:decimal` → `xsd:double`); mixed-type comparisons
    - **Date/time**: timezone-aware `xsd:dateTime` comparisons; `NOW()`, `YEAR()`, `MONTH()`, `DAY()`, `HOURS()`, `MINUTES()`, `SECONDS()`, `TZ()` builtins
    - **Numeric precision**: `xsd:decimal` arithmetic; `ROUND()`, `CEIL()`, `FLOOR()`, `ABS()`
    - **Blank-node scoping**: blank nodes in CONSTRUCT templates; blank nodes across GRAPH boundaries; blank-node identity in OPTIONAL
    - **String functions**: `STRLEN()`, `SUBSTR()`, `UCASE()`, `LCASE()`, `STRSTARTS()`, `STRENDS()`, `CONTAINS()`, `ENCODE_FOR_URI()`, `CONCAT()`
  - Target: full Jena suite completes in **< 3 minutes** alongside W3C suite on CI
  - New CI job `jena-suite` — non-blocking until pass rate ≥ 95%; then promoted to required
- [x] **WatDiv harness** (`tests/watdiv/` new module)
  - Data generation: integrate `watdiv` Rust port or call the upstream C++ binary via `std::process::Command`; generate 10M-triple dataset once and cache in CI artifact storage
  - Query templates: all 100 WatDiv query templates across four structural classes:
    - **Star** (S1–S7): all predicates share a single subject; tests VP table scan and star-join optimisation
    - **Chain** (C1–C3): predicates form a linear path; tests join ordering
    - **Snowflake** (F1–F5): star + chain hybrid; tests mixed join strategies
    - **Complex** (B1–B12, L1–L5): multi-hop patterns with OPTIONAL and UNION; tests full algebra
  - Correctness validation: run each query against a baseline (pre-computed expected cardinalities from a reference run) and assert within ±0.1% row count
  - Performance baseline: record median query latency per template at 10M triples; flag regressions > 20% in CI
  - Separate `cargo bench --bench watdiv` target using `criterion` — feeds into `benchmarks/` results
  - Target: full 100-template suite at 10M triples completes in **< 5 minutes** on an 8-core CI runner
  - New CI job `watdiv-suite` — non-blocking (performance regressions are warnings, not failures)
- [x] **Shared harness improvements** (backport to `tests/w3c/`)
  - Unified `tests/conformance/runner.rs` — single parallel runner used by W3C, Jena, and WatDiv; eliminates code duplication
  - Unified `known_failures.txt` format with `suite:` prefix (e.g. `w3c:`, `jena:`, `watdiv:`)
  - Unified CI report artifact: per-suite pass/fail/skip/timeout counts in one `conformance_report.json`
- [x] **Test data download script** (`scripts/fetch_conformance_tests.sh`)
  - Extends `scripts/fetch_w3c_tests.sh` to also download Jena test suite from Apache mirror and WatDiv query templates from GitHub
  - All downloads verified against SHA-256 checksums
  - WatDiv 10M dataset generated once and stored as a CI artifact (not re-generated on every run)

### Migration Script

`sql/pg_ripple--0.42.0--0.43.0.sql` — no schema changes. Comment-only header noting that v0.43.0 is a test infrastructure release.

### Documentation

- [x] `reference/w3c-conformance.md` — updated to include Jena sub-suite pass rates alongside W3C categories
- [x] `reference/watdiv-results.md` (new) — WatDiv benchmark results table: query class, template ID, median latency at 10M triples, pass/fail status; updated on each release
- [x] `contributing/running-conformance-tests.md` — updated to cover Jena and WatDiv; how to regenerate WatDiv dataset; how to update performance baselines
- [x] `README.md` — add WatDiv correctness badge alongside W3C conformance badge
- [x] Release notes for v0.43.0

### Exit Criteria

Full Jena suite (1,000 tests) completes in < 3 minutes on CI. WatDiv 100-template suite at 10M triples completes in < 5 minutes. Jena known-failures manifest ≤ 30 `XFAIL` entries (type coercion and date-time edge cases acceptable until addressed post-1.0). WatDiv row-count correctness within ±0.1% for all 100 templates. Migration chain test passes through 0.43.0.

</details>

---

## v0.44.0 — LUBM Conformance Suite

**Theme**: OWL RL inference correctness under ontological reasoning via the Lehigh University Benchmark (LUBM).

> **In plain language:** LUBM is a classic academic benchmark that generates a synthetic university-domain ontology dataset (scalable from 1K to 8M+ triples) and defines 14 canonical queries that exercise OWL RL inference rules — subclass traversal, property inheritance, inverse properties, transitivity, and domain/range entailments. This release wires LUBM into the conformance harness to validate that pg_ripple's Datalog engine and SPARQL query layer produce correct results when ontological reasoning is active. A dedicated Datalog validation sub-suite tests the Datalog API directly (rule compilation, stratification, iterative inference, goal queries, and materialization) to catch bugs invisible to SPARQL-level testing. It is the only benchmark that tests the *interaction* between the SPARQL translator and the Datalog inference engine under realistic ontological load.
>
> **Effort estimate: 3–5 person-weeks** (80% harness reuse from v0.41.0 and v0.43.0; +2–3 pw for Datalog API validation sub-suite)

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **LUBM data generator integration** (`tests/lubm/generator.rs` new module)
  - Invoke the [UBA (Univ-Bench Artificial) data generator](http://swat.cse.lehigh.edu/projects/lubm/) via `std::process::Command`, or use a Rust port, to produce Turtle-serialised datasets at configurable university count (`--univ 1` → ~100K triples; `--univ 10` → ~1M triples; `--univ 50` → ~5M triples)
  - Cache generated datasets as CI artifacts keyed by university count and seed; re-generate only when the generator binary changes
  - Load into a named graph `<http://swat.cse.lehigh.edu/onto/univ-bench.owl>` via the v0.41.0 fixture loader
  - Also load the `univ-bench.owl` ontology into the Datalog engine as an RDFS/OWL RL rule set before running queries
- [x] **14 canonical LUBM queries** (`tests/lubm/queries/q01.sparql` – `q14.sparql`)
  - Implement all 14 LUBM queries verbatim from the benchmark specification
  - Each query exercises at least one inference rule:
    - Q1, Q2, Q4, Q6: `rdf:type` + subclass/subproperty entailment
    - Q3, Q5, Q7: inverse property + domain/range reasoning
    - Q8, Q12, Q13: multi-hop inference chains
    - Q9, Q10, Q11, Q14: conjunctive patterns over inferred and asserted triples
  - Reference results: pre-computed correct answer counts for `--univ 1` (published in the original LUBM paper); assert exact cardinality match
- [x] **Correctness validator** (`tests/lubm/validator.rs`)
  - Compare actual row count against published reference counts for each of the 14 queries at `--univ 1`
  - For `--univ 10`, compare against a locally pre-computed baseline (stored in `tests/lubm/baselines/univ10.json`)
  - Fail on any count mismatch; report which inference rules produced wrong results
- [x] **CI integration** (`.github/workflows/ci.yml`)
  - New job `lubm-suite`: runs after `w3c-suite`; generates `--univ 1` dataset (< 100K triples, < 30 seconds); loads ontology + triples; runs all 14 queries; reports pass/fail per query
  - Non-blocking for `--univ 10` (larger dataset run triggered weekly or on release branches)
  - Reuse unified `tests/conformance/runner.rs` from v0.43.0; add `lubm:` prefix to known-failures format
- [x] **Known-failures manifest** — add `lubm:Q{N}` entries for any query that fails at release, with one-line root-cause note
- [x] **Datalog validation sub-suite** (`tests/lubm/datalog/` new module) — test the Datalog API directly on the same `--univ 1` and `--univ 10` LUBM datasets
  - **Rule compilation correctness** (`tests/lubm/datalog/rule_compilation.sql`): call `pg_ripple.add_rules()` with the OWL RL ruleset; use `pg_ripple.rules()` to inspect compiled rules; assert rule count and stratification matches specification
  - **Inference iteration tracking** (`tests/lubm/datalog/inference_iterations.sql`): use `pg_ripple.rule_statistics()` after `pg_ripple.materialize_owl_rl()` to count iterations per stratum; validate that fixpoint is reached without over-iteration (off-by-one detection)
  - **Inferred triple counts** (`tests/lubm/datalog/inferred_triples.sql`): call `pg_ripple.inferred_triples(rule_name)` for key OWL RL rules (e.g. `subclass_entail`, `subproperty_entail`, `domain_range`); assert row counts match pre-computed baselines for `--univ 1` and `--univ 10`
  - **Direct goal queries** (`tests/lubm/datalog/goal_queries.sql`): use `pg_ripple.goal()` directly on Datalog-computed facts; verify results match SPARQL query results (validates inference engine independence from SPARQL translation)
  - **Materialization performance baseline** (`tests/lubm/datalog/materialization_perf.sql`): benchmark `pg_ripple.materialize_owl_rl()` at `--univ 1` (target < 5 seconds) and `--univ 10` (target < 60 seconds); flag > 10% regression in CI
  - **Custom rule validation** (`tests/lubm/datalog/custom_rules.sql`): define ad-hoc Datalog rules (e.g. transitive closure over a custom predicate) on LUBM data; compare against ground-truth computed via Datalog vs. SPARQL; catch rule-compiler edge cases
  - Results compared against unified baseline (`tests/lubm/baselines/datalog_validation.json`).

### Migration Script

`sql/pg_ripple--0.43.0--0.44.0.sql` — adds `UNIQUE(p, s, o, g)` constraint to `_pg_ripple.vp_rare` to fix SPARQL UPDATE set semantics for rare predicates.

### Documentation

- [x] `reference/lubm-results.md` (new) — LUBM conformance table: query ID, description, inference rules exercised, reference count, pg_ripple result, pass/fail; updated each release
- [x] `reference/w3c-conformance.md` — updated to link to LUBM and WatDiv result pages for a complete conformance picture
- [x] `contributing/running-conformance-tests.md` — updated to cover LUBM data generation, ontology loading, and baseline regeneration
- [x] Release notes for v0.44.0

### Exit Criteria

All 14 LUBM queries return exact reference cardinalities at `--univ 1`. Ontology + `--univ 1` dataset loads and all queries complete in < 30 seconds on CI. All Datalog API calls in the sub-suite return results matching pre-computed baselines (rule count, iteration count, inferred triple counts, goal query results). Materialization performance at `--univ 1` is < 5 seconds. Custom Datalog rule validation passes (transitive closure results match ground truth). Known-failures manifest has 0 `lubm:` entries at release. Migration chain test passes through 0.44.0.

</details>

---

## v0.45.0 — SHACL Completion, Datalog Robustness & Crash Recovery

**Theme**: Close the last SHACL Core constraint gaps, harden parallel Datalog evaluation against worker failures, and add the missing crash-recovery scenarios and migration-documentation standards.

> **In plain language:** This release finishes the SHACL implementation by adding the two remaining Core constraints (`sh:equals` and `sh:disjoint`), makes violation messages readable by always including the decoded focus-node IRI, and proves the async validation queue can sustain a sustained burst of 10,000 writes per second. On the Datalog side it ensures that a crash in one parallel evaluation worker rolls back all other workers cleanly, and that user-supplied lattice join functions are validated before the engine tries to call them. A new set of crash-recovery tests covers the two scenarios that were never tested: killing PostgreSQL mid-promotion of a rare predicate and killing it mid-inference. Finally, every migration script from this release onward carries a standardised header documenting the schema changes, data-rewrite cost, downgrade strategy, and the test file that covers it.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **`sh:equals` and `sh:disjoint` constraints** (`src/shacl/constraints/`)
  - `sh:equals p` — for every focus node, the set of values for `p` must equal the set of values for the predicate declared by `sh:equals`; implemented as two NOT EXISTS subqueries (one per direction); compiled into a SHACL constraint helper in `src/shacl/constraints/relational.rs`
  - `sh:disjoint p` — the value sets must be disjoint; implemented symmetrically
  - pg_regress test `shacl_equals_disjoint.sql` — covers passing shapes, failing shapes, blank-node identity, and named-graph scoping
  - Migration: no schema changes; constraints are pure SQL inside the validation query

- [x] **Decoded focus-node IRIs in SHACL violation messages** (`src/shacl/mod.rs`)
  - All paths that emit a SHACL violation (`ereport!(Error, …)` or write to `_pg_ripple.validation_results`) must include the decoded IRI of the focus node alongside its integer ID
  - Add a `decode_id_safe(id: i64)` helper that falls back to `"<decoded-id:{id}>"` if the dictionary lookup fails
  - Regression test: load a shape with a violation; assert the violation message text contains the focus-node IRI string

- [x] **SHACL async pipeline load test** (`benchmarks/shacl_async_load.sql`)
  - `pgbench`-driven harness that inserts triples at 10,000/min for 5 continuous minutes while the async SHACL validation pipeline is active
  - Asserts: (a) `_pg_ripple.validation_queue` depth stays bounded (does not grow unboundedly); (b) drain rate ≥ arrival rate ± 5%; (c) dead-letter queue receives any persistent violators; (d) no backend crashes
  - CI job `shacl-async-load` is informational (non-blocking) but results are logged as a CI artifact

- [x] **Coordinated parallel-strata rollback** (`src/datalog/parallel.rs`)
  - Wrap all independent-group SQL execution inside a single PostgreSQL transaction with one `SAVEPOINT strata_eval` per group
  - On failure in any group, issue `ROLLBACK TO SAVEPOINT` for all already-applied groups and re-raise the error; on success, `RELEASE SAVEPOINT` to commit the whole stratum
  - pg_regress test `datalog_parallel_rollback.sql`: inject a deliberate failure in one group; assert no partial facts survive

- [x] **`lattice.join_fn` validation via `regprocedure`** (`src/datalog/lattice.rs`)
  - Before storing a user-supplied `join_fn` name, resolve it via `SELECT '{name}'::regprocedure::text` inside an SPI transaction
  - If the round-trip succeeds, store the qualified name returned by PG (avoids search-path injection); if it fails, raise `PT541 LatticeJoinFnInvalid` with a clear message naming the rejected identifier
  - New error code PT541 added to `src/error.rs` and `docs/src/reference/error-catalog.md`

- [x] **WFS iteration-cap test and documentation** (`tests/pg_regress/sql/datalog_wfs_cap.sql`)
  - pg_regress test that loads a mutually-recursive negation cycle guaranteed to reach `pg_ripple.wfs_max_iterations`; asserts: (a) function returns without error; (b) `"stratifiable": false` in result; (c) PostgreSQL WARNING with code PT520 is emitted; (d) `"certain"` and `"unknown"` fact counts are non-zero (partial result)
  - `docs/src/user-guide/sql-reference/datalog.md` — add a "Well-Founded Semantics limits" subsection documenting the cap behaviour and how to detect it via `RETURNING`

- [x] **Crash-recovery: rare-predicate promotion kill** (`tests/crash_recovery/test_promote_kill.sh`)
  - Script that starts a large-batch insert designed to cross the promotion threshold, sends `kill -9` to the promoting backend mid-transaction, restarts PostgreSQL, calls `pg_ripple.diagnostic_report()`, and asserts `vp_rare` is consistent (no orphaned rows, predicate catalog matches actual tables)
  - Outcome must be either: promotion completed (VP table exists, `vp_rare` rows moved) or promotion rolled back (VP table absent, `vp_rare` rows intact) — no hybrid state permitted

- [x] **Crash-recovery: Datalog inference kill mid-fixpoint** (`tests/crash_recovery/test_inference_kill.sh`)
  - Script that starts a large-ruleset inference run, kills the backend during the second fixpoint iteration, restarts, and asserts: (a) no partially-derived facts remain in any VP table (i.e., no inferred triples from an aborted inference); (b) `pg_ripple.infer()` can be re-run successfully to completion

- [x] **Standardised migration script headers**
  - Backfill `sql/pg_ripple--*.sql` with the standard header block (schema changes, data-rewrite cost estimate, downgrade strategy, test reference) for any script that currently lacks one — starting with `0.5.1→0.6.0` (the HTAP split) and the five most structurally significant migrations
  - Add the header template to `AGENTS.md` "Extension Versioning & Migration Scripts" section so all future scripts include it from creation

- [x] **Recovery procedure runbook in `RELEASE.md`**
  - Add a "Rollback & Recovery" section documenting: (a) how to roll back each class of migration (comment-only vs. schema-change vs. data-rewrite); (b) the `pg_dump`/`pg_restore` path as the universal fallback; (c) how to diagnose a partial upgrade using `_pg_ripple.schema_version` and `pg_ripple.diagnostic_report()`

### Migration Script

`sql/pg_ripple--0.44.0--0.45.0.sql` — no VP table schema changes. Comment-only header. Installs PT541 error code registration (compiled from Rust).

### Documentation

- [x] `reference/shacl-constraints.md` — add `sh:equals` and `sh:disjoint` to the constraint table with examples
- [x] `reference/error-catalog.md` — add PT541 (`LatticeJoinFnInvalid`)
- [x] `user-guide/sql-reference/datalog.md` — "Well-Founded Semantics limits" subsection
- [x] `reference/troubleshooting.md` — add entries for "rare-predicate promotion stuck" and "inference aborted mid-fixpoint"
- [x] Release notes for v0.45.0

### Exit Criteria

`sh:equals` and `sh:disjoint` pg_regress tests pass. SHACL violation messages include decoded focus-node IRIs. Parallel-strata rollback test demonstrates no partial facts on deliberate failure. `lattice.join_fn` injection via search-path ambiguous name is rejected at `create_lattice()` time with PT541. WFS cap test passes: PT520 WARNING emitted, partial result returned. Both new crash-recovery scripts exit 0. Migration chain test passes through 0.45.0.

</details>

---

## v0.46.0 — Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance

**Theme**: Property-based and fuzz testing for the remaining untested trust surfaces, the W3C OWL 2 RL conformance suite, and targeted performance improvements from the deep-analysis recommendations.

> **In plain language:** Three gaps that can hide subtle bugs: (1) randomised property-based tests that assert algebraic invariants about the SPARQL translator and dictionary encoder — if encoding the same term twice ever yields different IDs, or if a query changes semantics when extra whitespace is added, these tests catch it; (2) fuzz tests for the federation result parser, which accepts untrusted network data; and (3) the W3C OWL 2 RL test manifests, which verify that pg_ripple's Datalog engine handles the full range of ontological reasoning that OWL 2 RL demands. On the performance side, a LIMIT push-down eliminates redundant decoding rows for paginated queries, sequence range pre-allocation removes a contention point in parallel Datalog, and BSBM joins the CI suite as a regression gate. The rustdoc lint ensures no public function ships without a doc comment.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **`proptest` integration** (`tests/proptest/`)
  - **SPARQL algebra round-trip** (`tests/proptest/sparql_roundtrip.rs`): generate random `spargebra::Query` values using `proptest` strategies; assert that (a) encoding the same SPARQL query twice produces byte-identical SQL; (b) queries that differ only in whitespace or prefix aliases produce the same generated SQL (plan-cache key stability); (c) star-pattern self-join elimination never changes the result set (check against a reference without elimination)
  - **Dictionary encode/decode** (`tests/proptest/dictionary.rs`): for any arbitrary IRI, blank node, or literal string, `decode_id(encode_term(t)) == t`; assert no collisions for 10,000 random distinct terms; assert encode is stable across pg_ripple restarts (same term → same ID given the same dictionary)
  - **JSON-LD framing round-trip** (`tests/proptest/jsonld_framing.rs`): generate random flat JSON-LD input graphs and random `@context` frames; assert that `frame_jsonld(input, frame)` returns valid JSON-LD and that any IRI present in the input that matches the frame appears in the output
  - Dev-dependency: `proptest = "1"` added to `Cargo.toml` under `[dev-dependencies]`

- [x] **`cargo-fuzz` federation result decoder target** (`fuzz/fuzz_targets/federation_result.rs`)
  - Fuzz target that feeds arbitrary byte sequences through the SPARQL XML results parser (`src/sparql/federation.rs` result-decoding path) — the path that processes `application/sparql-results+xml` responses from remote SERVICE endpoints
  - Assert: no panic, no `unwrap` abort; invalid XML must produce a `PT6xx`-range error, never a crash
  - CI nightly job `fuzz-federation` runs the target for 10 minutes; any new corpus entries that trigger panics are reported as blocking failures

- [x] **Datalog convergence regression suite** (`tests/datalog_convergence/`)
  - Download a 1M-triple DBpedia-en subset (persons, organisations, relations) via `scripts/fetch_conformance_tests.sh` extension; load into pg_ripple
  - Apply the built-in RDFS + OWL RL rule set via `pg_ripple.materialize_owl_rl()`
  - Assert: fixpoint reached in ≤ 20 iterations; total wall-clock time < 5 minutes on CI; derived triple count falls within ±1% of a pre-computed baseline stored in `tests/datalog_convergence/baselines.json`
  - Repeat for a 200-rule custom rule set (100 forward-chaining + 100 OWL RL rules) on a 100K-triple schema.org snippet; assert convergence in ≤ 15 iterations

- [x] **W3C OWL 2 RL conformance suite** (`tests/owl2rl/`)
  - Download the W3C OWL 2 RL test manifests from `https://github.com/w3c/owl2-profiles-tests`
  - Adapter `tests/owl2rl/manifest.rs` parses the `owl2:DatatypeEntailmentTest`, `owl2:ConsistencyTest`, and `owl2:InconsistencyTest` manifest types
  - Each test loads a premise ontology, runs `pg_ripple.materialize_owl_rl()`, then evaluates a conclusion ontology via ASK/entailment check
  - CI job `owl2rl-suite` is informational (non-blocking) until pass rate ≥ 95%; known failures tracked in `tests/owl2rl/known_failures.txt` with `owl2rl:` prefix
  - Reuse unified conformance runner from v0.43.0

- [x] **TopN push-down** (`src/sparql/sqlgen.rs`)
  - When a SPARQL query has both `ORDER BY` and `LIMIT N` (and no `OFFSET > 0`), emit the SQL as `… ORDER BY … LIMIT N` rather than fetching all rows and discarding after decoding
  - The optimisation applies to SELECT queries; skipped when `DISTINCT` is in scope (PostgreSQL cannot push LIMIT through DISTINCT without a subquery)
  - New GUC `pg_ripple.topn_pushdown` (bool, default `on`) guards the rewrite; `pg_ripple.sparql_explain()` output includes a `"topn_applied": true/false` key
  - pg_regress test `sparql_topn.sql`: assert result correctness and `EXPLAIN` shows a `Limit` node directly over the VP scan

- [x] **Sequence range pre-allocation for parallel Datalog workers** (`src/datalog/parallel.rs`)
  - Before launching N parallel strata workers, call `SELECT setval(seq, currval(seq) + N * batch_size)` once to reserve a contiguous SID range; each worker uses its slice without touching the sequence
  - `batch_size` defaults to 10,000 and is configurable via `pg_ripple.datalog_sequence_batch` (integer GUC, default 10000, min 100)
  - pg_regress test `datalog_sequence_batch.sql`: assert that after parallel inference the global SID sequence has no gaps within the reserved range

- [x] **BSBM regression gate in CI** (`.github/workflows/ci.yml`, `benchmarks/bsbm/`)
  - Integrate the Berlin SPARQL Benchmark (BSBM) at 1M triple scale as a nightly regression check
  - `scripts/fetch_conformance_tests.sh` extended to download and install the BSBM data generator
  - CI job `bsbm-regression`: generates a 1M-triple product dataset, runs the 12 BSBM explore queries, compares query latency against a baseline stored in `benchmarks/bsbm/baselines.json`; any query regressing by > 10% emits a CI warning (non-blocking but visible in the PR summary)
  - Complement to v1.0.0's full-scale BSBM-at-100M-triples published benchmark

- [x] **Rustdoc lint gate** (`src/lib.rs`, `Cargo.toml`, `.github/workflows/ci.yml`)
  - Add `#![warn(missing_docs)]` to `src/lib.rs` (scoped to public items only; internal `pub(crate)` items excluded)
  - CI job `cargo doc --no-deps --document-private-items` gated to fail on any `missing_docs` warning for public `#[pg_extern]` functions
  - Backfill doc comments for the 20 most-called public functions (as identified by `pg_stat_statements` in the test suite run); leave a `FIXME(docs):` comment on the remaining stubs to track progress

- [x] **HTTP companion: CA-bundle env var** (`pg_ripple_http/src/main.rs`)
  - Add `PG_RIPPLE_HTTP_CA_BUNDLE` environment variable: if set, load the PEM file at the given path as the trust anchor for all outbound TLS connections (SERVICE federation and SPARQL endpoint queries)
  - If the path does not exist or is not a valid PEM bundle, log an error at startup and fall back to the system trust store (never silently ignore)
  - This complements the v0.42.0 `rustls-tls-native-roots` hardening by allowing operators to pin a specific CA or internal PKI certificate
  - Integration test: start a mock TLS server with a self-signed CA; assert that `pg_ripple_http` rejects it by default and accepts it when `PG_RIPPLE_HTTP_CA_BUNDLE` points to the CA cert

- [x] **Expanded worked examples** (`examples/`)
  - `examples/shacl_datalog_quality.sql` — end-to-end: load a bibliographic graph, define SHACL shapes, run SPARQL to list violations, apply Datalog RDFS rules, re-check shapes; documents the SHACL + Datalog interaction pattern
  - `examples/hybrid_vector_search.sql` — end-to-end: embed entities, run vector similarity search, combine with SPARQL property-path constraints; documents the `pg:similar()` + SPARQL pattern
  - `examples/graphrag_round_trip.sql` — end-to-end: load a knowledge graph, run GraphRAG export, annotate with Datalog-derived community summaries, re-import enriched triples; documents the full GraphRAG round-trip

### New GUC Parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.topn_pushdown` | bool | `on` | Push `LIMIT N` into the SQL plan for `ORDER BY + LIMIT` queries |
| `pg_ripple.datalog_sequence_batch` | integer | `10000` | SID range reserved per parallel Datalog worker per batch |

### New Error Codes

| Code | Severity | Message |
|------|----------|---------|
| PT542 | ERROR | Federation result decoder received unparseable XML/JSON |

### Migration Script

`sql/pg_ripple--0.45.0--0.46.0.sql` — no schema changes. Registers `topn_pushdown` and `datalog_sequence_batch` GUCs (compiled from Rust). Comment-only header.

### Documentation

- [x] `user-guide/best-practices/sparql-performance.md` — "TopN push-down" section with `EXPLAIN` example
- [x] `reference/guc-reference.md` — v0.46.0 section with two new GUC parameters
- [x] `reference/error-catalog.md` — PT542 added
- [x] `contributing/testing.md` — `proptest` and `cargo-fuzz` sections covering how to run and extend the harnesses
- [x] Release notes for v0.46.0

### Exit Criteria

All three `proptest` suites run 10,000 cases each with no failures. Federation result decoder fuzz target runs 10 minutes without panics. Datalog convergence suite: fixpoint on 1M DBpedia triples in ≤ 20 iterations, wall-clock < 5 minutes. OWL 2 RL suite: ≥ 80% pass rate at release (target 95% for v1.0.0). TopN push-down `EXPLAIN` shows `Limit` node for ORDER BY + LIMIT queries; result set unchanged. BSBM-at-1M-triples baseline stored and regression gate active. No missing-docs warnings for public `#[pg_extern]` functions. HTTP companion starts cleanly with `PG_RIPPLE_HTTP_CA_BUNDLE` set to a valid PEM file. Migration chain test passes through 0.46.0.

</details>

---
## v0.47.0 — SHACL Truthfulness, Dead-Code Activation & Architecture Refactor

**Theme**: Close the parsed-but-not-checked SHACL gap, wire dead code, finish the SPARQL translate module split, and expand fuzz and crash-recovery coverage.

> **In plain language:** v0.45.0 was titled "SHACL Completion" but the post-release audit (PLAN_OVERALL_ASSESSMENT_3.md) found four constraints that accept any data without complaint — the parser records them but the validator ignores them. That is fixed here. The `preallocate_sid_ranges()` function added in v0.46.0 to speed up parallel Datalog has been sitting unused (clippy `dead_code` warning); it gets wired in. The `src/sparql/translate/` refactor that began in v0.38.0 finally lands, shrinking `sqlgen.rs` from 3 600 lines into focused per-operator modules. Five new fuzz targets cover the attack surfaces that had only one target before. Four new crash-recovery scenarios close the remaining operational safety gaps.
>
> **Effort estimate: 8–10 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SHACL parsed-but-not-checked constraint sweep** (S4-1…S4-4)
  - Implement `sh:closed` checker in `src/shacl/constraints/closed.rs`: for each focus node enumerate all predicate IDs present; reject any not listed in `sh:property / sh:path` or `sh:ignoredProperties`
  - Implement `sh:uniqueLang` checker: for a given focus node and path, assert no two values share the same non-empty `@lang` tag
  - Implement `sh:pattern` checker in `src/shacl/constraints/string_based.rs` (currently an empty placeholder): apply the `sh:flags`-aware POSIX regex against the string value of each focus node
  - Implement `sh:lessThanOrEquals` checker: decode both value nodes and compare with the XSD-typed ordering already used by FILTER expressions
  - Wire each into the shape dispatcher at `src/shacl/mod.rs`
  - Add pg_regress tests `shacl_closed.sql`, `shacl_unique_lang.sql`, `shacl_pattern.sql`, `shacl_lt_or_equals.sql` (S8-4)
  - Add a startup-time warning listing every parsed-but-unchecked constraint type encountered, to guard against future regressions

- [x] **Wire `preallocate_sid_ranges()`** (S1-2)
  - Call the function from the parallel-strata coordinator in `src/datalog/parallel.rs` before launching any worker batch
  - Assert via `datalog_sequence_batch.sql` that `pg_sequence_last_value` advances by `n_workers * batch_size` on each batch; eliminate the clippy `dead_code` warning

- [x] **Finish `src/sparql/translate/` module split** (S2-3)
  - Move BGP translation into `src/sparql/translate/bgp.rs` (~400 LoC)
  - Move Filter translation into `src/sparql/translate/filter.rs` (~200 LoC)
  - Move LeftJoin (OPTIONAL) into `src/sparql/translate/left_join.rs` (~250 LoC)
  - Move Union into `src/sparql/translate/union.rs` (~150 LoC)
  - Move Distinct into `src/sparql/translate/distinct.rs` (~100 LoC)
  - Move Graph pattern into `src/sparql/translate/graph.rs` (~200 LoC)
  - Move Group/aggregation into `src/sparql/translate/group.rs` (~300 LoC)
  - Move Join into `src/sparql/translate/join.rs` (~200 LoC)
  - Target: `sqlgen.rs` ≤ 800 LoC (routing and coordination only)

- [x] **Six missing GUC `check_hook` validators** (S5-1)
  - Add validators for: `federation_on_error` (warning|error|empty), `federation_on_partial` (empty|use), `sparql_overflow_action` (warn|error), `tracing_exporter` (stdout|otlp), `embedding_index_type` (hnsw|ivfflat), `embedding_precision` (single|half|binary)
  - Consolidate `max_path_depth` and `property_path_max_depth` into a single GUC with `min = 1, max = 65535` validator (S2-5)

- [x] **Five new `cargo-fuzz` targets** (S8-1)
  - `fuzz/fuzz_targets/sparql_parser.rs`: feed arbitrary bytes through the SPARQL query parser; assert no panic
  - `fuzz/fuzz_targets/turtle_parser.rs`: fuzz the Turtle/N-Triples bulk loader; assert no panic, invalid input → PT3xx error
  - `fuzz/fuzz_targets/datalog_parser.rs`: fuzz the Datalog rule parser; assert no panic
  - `fuzz/fuzz_targets/shacl_parser.rs`: fuzz `parse_shapes_graph()`; assert no panic
  - `fuzz/fuzz_targets/dictionary_hash.rs`: fuzz the dictionary encode path; assert no panic and round-trip invariant
  - Each target runs for 10 minutes in CI nightly; a new crash-inducing input is a blocking failure

- [x] **Four missing crash-recovery scenarios** (S8-3)
  - CONSTRUCT/DESCRIBE view materialisation kill: `kill -9` during `materialize_view()`; restart and verify view state is consistent
  - Federation result spooling kill: `kill -9` during SERVICE temp-table spool; restart and verify no orphaned temp tables
  - Parallel Datalog stratum kill (`merge_workers > 1`): `kill -9` mid-fixpoint; restart and verify inference restarts cleanly
  - Embedding worker queue kill: `kill -9` during async embedding queue flush; restart and verify queue drains without duplicates

- [x] **Plan / dictionary / federation cache hit-rate metrics** (S7-1)
  - `pg_ripple.plan_cache_stats()` → `(hits BIGINT, misses BIGINT, evictions BIGINT, hit_rate DOUBLE PRECISION)`
  - `pg_ripple.dictionary_cache_stats()` → same shape
  - `pg_ripple.federation_cache_stats()` → same shape
  - Wire hit_rate into the BSBM regression gate as a secondary metric

- [x] **WFS non-convergence warning** (S3-2)
  - Emit PT520 WARNING when the well-founded semantics iteration cap is reached without convergence; include iteration count and the predicate that last changed

- [x] **OWL 2 RL conformance baseline** (S3-3)
  - Run the OWL 2 RL suite added in v0.46.0; document the pass rate in `docs/src/reference/owl2rl-results.md`
  - Surface XFAIL entries in `tests/owl2rl/known_failures.txt` for release-to-release tracking

- [x] **CI and security hygiene** (S6-1, S6-2, S6-4, S10-1)
  - Add weekly scheduled `cargo audit` job; failure creates a GitHub issue automatically
  - Add `cargo deny` configuration with licence allowlist
  - Add `scripts/check_no_security_definer.sh` that scans `sql/*.sql` and fails on any `SECURITY DEFINER` directive
  - Add SPDX licence compatibility check via `cargo license`

- [x] **Promotion-race stress test** (S8-5)
  - `tests/stress/promotion_race.sh`: fire 50 concurrent inserts at the rare-predicate promotion threshold; verify SIDs are non-overlapping per worker

- [x] **Documentation** (S9-1, S9-2, S9-3, S5-3)
  - `reference/guc-reference.md`: complete entries for all GUCs through v0.47.0; flag `datalog_sequence_batch` as now active
  - Add GUC ↔ workload-class tuning matrix (when to raise `dictionary_cache_size`, when to increase `merge_workers`, when to tune `property_path_max_depth`)
  - Add 5 worked examples: federation-multi-endpoint, parallel-Datalog, CONSTRUCT/DESCRIBE view materialisation, RDF-star annotation patterns, WCOJ cyclic queries
  - Document NOTIFY queue tuning for CDC subscriptions (`max_notify_queue_pages`)

### New Error Codes

| Code | Severity | Message |
|------|----------|---------|
| PT520 | WARNING | Well-founded semantics iteration cap reached without convergence; result is partial |

### Migration Script

`sql/pg_ripple--0.46.0--0.47.0.sql` — no schema changes. Comment header describing new SHACL constraint checkers, wired `preallocate_sid_ranges()`, and six new GUC validators.

### Documentation

- [x] `reference/shacl-reference.md` — mark `sh:closed`, `sh:uniqueLang`, `sh:pattern`, `sh:lessThanOrEquals` as fully implemented
- [x] `contributing/testing.md` — fuzz targets section extended for five new targets
- [x] `reference/guc-reference.md` — complete audit of all registered GUCs through v0.47.0
- [x] Release notes for v0.47.0

### Exit Criteria

All four previously parsed-but-unchecked SHACL constraints trigger violations on non-conforming data. `preallocate_sid_ranges()` has zero clippy `dead_code` warnings. `sqlgen.rs` ≤ 800 LoC. All five fuzz targets run 10 minutes without panics. All four crash-recovery scenarios pass. Three cache-stats SRFs return non-zero `hit_rate` after a warm workload. OWL 2 RL pass-rate baseline documented. `cargo audit` and `cargo deny` green in CI.

</details>

---

## v0.48.0 — SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness

**Theme**: Complete SHACL Core conformance, close the OWL 2 RL rule-set gap, finish SPARQL 1.1 Update, and resolve the SPARQL-star variable-pattern gap.

> **In plain language:** After v0.47.0 makes the existing SHACL constraints truthful, this release adds the remaining seven SHACL Core constraints — the string-length bounds, exclusive/inclusive numeric ranges, and `sh:xone` — plus the complex path expressions (`sh:inversePath`, `sh:alternativePath`, sequence paths, `*`, `+`, `?`) that real-world Schema.org and SHACL-AF schemas depend on. On the reasoning side, five missing OWL 2 RL rules close the gap with the W3C OWL 2 RL profile. SPARQL 1.1 Update gains its three missing operations (`MOVE`, `COPY`, `ADD`). The SPARQL-star variable-inside-quoted-triple pattern finally returns rows instead of silently empty results. This release also delivers the operational hardening items deferred from v0.47.0.
>
> **Effort estimate: 6–8 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **Remaining SHACL Core constraints** (S4-5)
  - `sh:minLength` / `sh:maxLength`: apply to string-typed literals after language-tag stripping
  - `sh:xone`: exactly one of the given sub-shapes must be satisfied (XOR logic over the existing `sh:or` / `sh:not` primitives)
  - `sh:minExclusive` / `sh:maxExclusive` / `sh:minInclusive` / `sh:maxInclusive`: XSD-typed numeric comparison; reuse the ordering logic from `sh:lessThan` / `sh:lessThanOrEquals`
  - Target: full SHACL Core constraint coverage (35/35); W3C SHACL Core test suite must pass completely

- [x] **Complex `sh:path` expressions** (S4-6)
  - `sh:inversePath`: query `(o, s)` instead of `(s, o)` on the VP table
  - `sh:alternativePath`: union of multiple sub-paths
  - Sequence paths (`(sh:path (ex:a ex:b))`): chained joins
  - `sh:zeroOrMorePath`, `sh:oneOrMorePath`, `sh:zeroOrOnePath`: compile to `WITH RECURSIVE … CYCLE` CTEs, reusing the SPARQL property-path compiler from `src/sparql/property_path.rs`
  - Drop the TODO placeholder in `src/shacl/constraints/property_path.rs`

- [x] **SHACL violation report enhancements** (S4-7, S4-8)
  - Extend `Violation` struct with `sh_value` (the offending value node, decoded) and `sh_source_constraint_component` (W3C constraint component IRI, e.g. `sh:MinCountConstraintComponent`)
  - For `sh:rule` triples (SHACL-AF): emit a PT4xx WARNING if rules are detected but SHACL-AF compilation is not yet implemented; never silently drop the rule

- [x] **OWL 2 RL rule set completion** (S3-1)
  - `cax-sco`: full `rdfs:subClassOf` transitive closure (currently single-step only)
  - `prp-spo1`: `rdfs:subPropertyOf` chain (current binary case → full chain)
  - `prp-ifp`: inverse-functional-property derived `owl:sameAs` propagation
  - `cls-avf`: chained `owl:allValuesFrom` interaction with subclass hierarchy
  - `owl:minCardinality`, `owl:maxCardinality`, `owl:cardinality` entailment rules
  - Target: W3C OWL 2 RL CI suite ≥ 95% pass rate (upgrading the gate from informational to required)

- [x] **SPARQL Update: MOVE, COPY, ADD** (S2-2)
  - `ADD`: `INSERT { ?s ?p ?o } WHERE { GRAPH source { ?s ?p ?o } }` (source preserved)
  - `COPY`: `CLEAR target` + `ADD`
  - `MOVE`: `COPY` + `DROP source`
  - Wire into `src/sparql/mod.rs` Update arm; add pg_regress tests for all three operations

- [x] **SPARQL-star variable-inside-quoted-triple patterns** (S2-1)
  - Convert the current silent `FALSE` emission into a proper dictionary join on `qt_s`, `qt_p`, `qt_o` columns already present in `_pg_ripple.dictionary`
  - Patterns like `<< ?s ?p ?o >> :assertedBy ?who` return rows
  - Add pg_regress tests `rdfstar_variable_quoted.sql`

- [x] **Performance baselines and benchmarks** (S7-2, S7-3)
  - Record per-query p50/p95/p99 latency for all 32 WatDiv templates in `tests/watdiv/baselines.json`; CI warning gate on > 10% regression
  - Add `benchmarks/merge_throughput.sql`: 5-minute pgbench script with N writers + `merge_workers ∈ {1, 2, 4, 8}`; document the scaling curve

- [x] **Operational hardening** (S1-1, S1-3, S1-4, S1-5, S2-4, S2-6, S3-4, S6-3, S7-4, S7-5, S9-4, S9-6, S10-2, S10-3, S10-5)
  - HTAP merge cutover: add a concurrent-merge regression test (50 parallel SPARQL queries during a forced merge cycle; assert zero `relation does not exist` errors) (S1-1)
  - Merge worker backoff: replace `std::thread::sleep` with `BackgroundWorker::wait_latch` (S1-3)
  - Add `source` column integrity pg_regress test (S1-4)
  - Predicate-OID cache: add `CacheRegisterRelcacheCallback` hook (S1-5)
  - Add `pg_ripple.federation_max_response_bytes` GUC (default 100 MiB); refuse responses exceeding it with PT543 (S2-4)
  - CONSTRUCT RDF-star: emit `<< s p o >>` notation for ground quoted triples in CONSTRUCT output (S2-6)
  - SAVEPOINT helper: either wire `execute_with_savepoint()` into the parallel-strata path or gate with `#[cfg(test)]` (S3-4)
  - `pg_dump` / restore round-trip test (`tests/pg_dump_restore.sh`) (S6-3)
  - Add `pg_ripple.insert_triples(TEXT[][])` SRF for batch single-triple inserts from orchestration tools (S7-4)
  - HNSW vs IVFFlat benchmark and documentation (S7-5)
  - Mermaid architecture diagram in `docs/src/reference/architecture.md` (S9-4)
  - Migration script headers lint (`scripts/check_migration_headers.sh`) (S9-6)
  - `release-please`-style release automation workflow (S10-2)
  - `docs/src/operations/pg-upgrade.md` with supported upgrade matrix and pre-upgrade steps (S10-3)
  - Extend migration-chain test to load a representative data batch after the v0.1.0 install and verify data survives through v0.48.0 (S10-5)

### New GUC Parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.federation_max_response_bytes` | integer | `104857600` | Maximum federation response body in bytes (100 MiB); PT543 on violation |

### New Error Codes

| Code | Severity | Message |
|------|----------|---------|
| PT543 | ERROR | Federation response exceeded `federation_max_response_bytes` limit |

### Migration Script

`sql/pg_ripple--0.47.0--0.48.0.sql` — no schema changes. Comment header describing SHACL Core completion, OWL 2 RL rule additions, and SPARQL Update completions.

### Documentation

- [x] `reference/shacl-reference.md` — all 35 SHACL Core constraints marked implemented; complex path expressions documented with examples
- [x] `reference/owl2rl-results.md` — pass rate updated to reflect ≥ 95% required gate
- [x] `user-guide/best-practices/sparql-update.md` — MOVE, COPY, ADD examples
- [x] `user-guide/rdf-star.md` — variable-inside-quoted-triple patterns documented
- [x] `operations/pg-upgrade.md` — new page with supported upgrade matrix
- [x] Release notes for v0.48.0

### Exit Criteria

W3C SHACL Core test suite passes 35/35 constraints. OWL 2 RL CI gate upgraded to required at ≥ 95%. All three SPARQL Update operations (MOVE, COPY, ADD) pass the W3C SPARQL 1.1 Update test suite entries for those operations. SPARQL-star variable patterns return correct rows. WatDiv latency baselines recorded and regression gate active. `pg_upgrade` compatibility document published. `pg_dump` / restore round-trip test passes. Migration chain test passes through v0.48.0.

</details>

---

## v0.49.0 — AI & LLM Integration

**Theme**: Natural-language query generation and embedding-based entity alignment.

> **In plain language:** Two high-leverage AI features: a function that takes plain English and returns a SPARQL query (using any configured LLM endpoint — Ollama, OpenAI, Claude, or a self-hosted model); and a function that uses the existing vector embeddings to surface candidate `owl:sameAs` pairs — entities that might be the same thing expressed differently. Both build on infrastructure already in place (the SPARQL engine and the v0.27.0 pgvector integration) and require no new storage schema changes.
>
> **Effort estimate: 4–6 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **NL → SPARQL via LLM function calling** (Feature C-1)
  - New module `src/llm/mod.rs`; new SQL function `pg_ripple.sparql_from_nl(question TEXT) RETURNS TEXT`
  - Calls a configured LLM endpoint with the schema VoID description as context; returns a SPARQL SELECT query string
  - GUCs: `pg_ripple.llm_endpoint` (TEXT, default `''` = disabled), `pg_ripple.llm_model` (TEXT, default `gpt-4o`), `pg_ripple.llm_api_key_env` (TEXT, name of the env var holding the key — never stored inline)
  - Optional few-shot examples loaded from `_pg_ripple.llm_examples (question TEXT, sparql TEXT)`; seeded via `pg_ripple.add_llm_example(question TEXT, sparql TEXT)`
  - SHACL shapes included as additional semantic context when `pg_ripple.llm_include_shapes = on` (bool GUC, default `on`)
  - Error codes: PT700 (LLM endpoint unreachable), PT701 (LLM returned non-SPARQL output), PT702 (generated SPARQL failed to parse)
  - pg_regress tests run with a mock HTTP server returning a canned SPARQL response

- [x] **Embedding-based `owl:sameAs` candidate generation** (Feature C-2)
  - New SQL function `pg_ripple.suggest_sameas(threshold REAL DEFAULT 0.9) RETURNS TABLE(s1 TEXT, s2 TEXT, similarity REAL)`
  - Runs an HNSW self-join on the embedding column in `_pg_ripple.entities`; returns pairs whose cosine similarity exceeds `threshold`
  - Companion `pg_ripple.apply_sameas_candidates(min_similarity REAL DEFAULT 0.95)` inserts accepted pairs as `owl:sameAs` triples and triggers cluster merging
  - Respects `pg_ripple.sameas_max_cluster_size` (PT550) bound
  - Example: `examples/embedding_alignment.sql` — load two datasets with overlapping entities, run `suggest_sameas`, inspect candidates, apply with `apply_sameas_candidates`

### New GUC Parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.llm_endpoint` | string | `''` | LLM API base URL (empty = NL→SPARQL disabled) |
| `pg_ripple.llm_model` | string | `gpt-4o` | LLM model identifier |
| `pg_ripple.llm_api_key_env` | string | `PG_RIPPLE_LLM_API_KEY` | Name of the environment variable holding the LLM API key |
| `pg_ripple.llm_include_shapes` | bool | `on` | Include SHACL shapes as LLM context when generating SPARQL |

### New Error Codes

| Code | Severity | Message |
|------|----------|---------|
| PT700 | ERROR | LLM endpoint unreachable or returned HTTP error |
| PT701 | ERROR | LLM response did not contain a valid SPARQL query |
| PT702 | ERROR | LLM-generated SPARQL query failed to parse |

### Migration Script

`sql/pg_ripple--0.48.0--0.49.0.sql` — adds `_pg_ripple.llm_examples (question TEXT, sparql TEXT)` table.

### Documentation

- [x] `user-guide/nl-to-sparql.md` — new page: configuring the LLM endpoint, running `sparql_from_nl`, adding few-shot examples, error handling
- [x] `user-guide/entity-alignment.md` — new page: `suggest_sameas`, `apply_sameas_candidates`, tuning threshold, cluster size limits
- [x] `reference/guc-reference.md` — four new GUC parameters
- [x] `reference/error-catalog.md` — PT700–PT702
- [x] Release notes for v0.49.0

### Exit Criteria

`pg_ripple.sparql_from_nl()` returns a parseable SPARQL query against a mock LLM endpoint. `pg_ripple.suggest_sameas()` returns candidates for two overlapping test datasets with ≥ 90% recall. `apply_sameas_candidates()` does not exceed `sameas_max_cluster_size`. All GUC validators pass. PT700–PT702 are triggered by the appropriate error conditions. Migration chain test passes through v0.49.0.

</details>

---

## v0.50.0 — Developer Experience & GraphRAG Polish

**Theme**: Interactive query debugger and full RAG pipeline.

> **In plain language:** Two developer-facing features that raise the ceiling on how easy it is to work with pg_ripple day-to-day. An extended `EXPLAIN SPARQL` command surfaces the algebra tree, generated SQL, plan-cache status, and per-step row counts as an interactive JSON structure. The RAG pipeline ties together vector recall, SPARQL graph expansion, and LLM context-window assembly into a single SQL function call.
>
> **Effort estimate: 5–7 person-weeks**

<details>
<summary>Completed items (click to expand)</summary>

### Deliverables

- [x] **SPARQL query debugger** (Feature B-3)
  - Extend `pg_ripple.explain_sparql(query TEXT)` to return JSONB with: algebra tree, generated SQL, plan-cache status (`hit` / `miss` / `bypass`), per-operator estimated rows, per-operator actual rows (when `analyze := true`)
  - New overload `pg_ripple.explain_sparql(query TEXT, analyze BOOL DEFAULT FALSE) RETURNS JSONB`
  - VS Code extension renders the JSONB as a collapsible tree with operator annotations
  - pg_regress `sparql_explain_analyze.sql`: assert the JSONB schema is stable across SELECT, ASK, CONSTRUCT, and DESCRIBE query types
  - VS Code extension renders the JSONB as a collapsible tree with operator annotations (deferred to v1.10)

- [x] **RAG pipeline with graph-contextualised embeddings** (Feature C-3)
  - New SQL function `pg_ripple.rag_context(question TEXT, k INT DEFAULT 10) RETURNS TEXT`
  - Step 1: embed `question` via `pg_ripple.embed_text()` (from v0.27.0)
  - Step 2: vector recall — top-k entities by HNSW similarity
  - Step 3: SPARQL graph expansion — for each entity, fetch its 1-hop neighbourhood as JSON-LD
  - Step 4: assemble a context string from the JSON-LD fragments, formatted for LLM ingestion
  - Step 5 (optional): if `pg_ripple.llm_endpoint` is set, call `sparql_from_nl()` and execute the generated query, appending the result to the context
  - Example: `examples/graphrag_rag_pipeline.sql` — end-to-end with a Wikipedia-derived knowledge graph

### Migration Script

`sql/pg_ripple--0.49.0--0.50.0.sql` — no schema changes.

### Documentation

- [x] `user-guide/explain-sparql.md` — EXPLAIN output format, ANALYZE mode, interpreting the algebra tree
- [x] `user-guide/rag-pipeline.md` — `rag_context()` step-by-step, tuning k, combining with NL→SPARQL
- [x] Release notes for v0.50.0

### Exit Criteria

`explain_sparql(query, analyze := true)` returns JSONB with `algebra`, `sql`, `cache_status`, and per-operator `actual_rows` keys for SELECT, ASK, CONSTRUCT, and DESCRIBE queries. `rag_context()` returns non-empty context for a known question against a pre-loaded test knowledge graph. Migration chain test passes through v0.50.0.

</details>

---

## v0.51.0 — Security Hardening & Production Readiness

**Theme**: Close every blocking item for v1.0.0: non-root container, SPARQL DoS protection, HTTP streaming, OTLP observability, pg_upgrade compatibility, CDC backpressure documentation, released-image hardening, and release automation. Also addresses all High- and Medium-severity findings from [plans/PLAN_OVERALL_ASSESSMENT_4.md](plans/PLAN_OVERALL_ASSESSMENT_4.md).

> **In plain language:** This release converts pg_ripple from "release-candidate quality" to "production-certified". Every security and operational gap found in the v0.50.0 audit is closed: the container no longer runs as root, a malicious SPARQL query can no longer exhaust the server stack, the HTTP service can stream large results without buffering them in memory, real OpenTelemetry spans reach external APM tools, and every database administrator can follow a documented upgrade path. After this release, the only remaining step to v1.0.0 is a final conformance and stress run.
>
> **Effort estimate: 8–10 person-weeks**

### Deliverables

#### Security & Container Hardening (blocking for v1.0.0)

- [ ] **Non-root Docker image** (N6-1 / F-2): add `USER postgres:postgres` before `CMD` in [Dockerfile](Dockerfile); publish two image tags — `:dev` (trust auth, for local testing) and `:prod` (password-required, aliases to `:latest`) to eliminate the `trust`-auth-in-published-image risk (N6-4)
- [ ] **SPARQL algebra-tree depth limit** (N6-2 / F-4): new GUCs `pg_ripple.sparql_max_algebra_depth` (default 256) and `pg_ripple.sparql_max_triple_patterns` (default 4 096); reject over-limit queries at parse time with **PT440**; add pg_regress `sparql_depth_limit.sql`
- [ ] **Certificate-fingerprint pinning** (S5-2 / N6-3): `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` env var in `pg_ripple_http` — comma-separated SHA-256 hashes; reject TLS handshake on mismatch
- [ ] **SPDX licence check** (S6-2): add `cargo license --json` to CI; fail on any dependency not in the MIT/Apache-2.0/BSD/ISC allow-list
- [ ] **SBOM generation** (N6-6): add `cargo cyclonedx` step to the release workflow; attach `sbom.xml` as a GitHub release artefact
- [ ] **`secrets/` directory renamed** (N6-5): move `secrets/` to `tests/fixtures/sealed_secrets/`; add a top-level README explaining the contents are test stubs with no real credentials
- [ ] **SQL-injection format! lint** (N1-5): `scripts/check_no_string_format_in_sql.sh` — bans `format!` patterns that interpolate non-`i64` values into SQL strings; runs in CI alongside `check_no_security_definer.sh`
- [ ] **`cargo tree --duplicates` advisory CI gate** (N1-2): non-blocking job that records the duplicate-crate count as a CI annotation; blocks on any increase > 2

#### HTTP Streaming (blocking for v1.0.0)

- [ ] **`POST /sparql/stream` endpoint** (N2-1 / N9-1 / F-1): wire `sparql_cursor()`, `sparql_cursor_turtle()`, `sparql_cursor_jsonld()` ([src/sparql/cursor.rs](src/sparql/cursor.rs)) into a chunked-transfer-encoded HTTP handler in `pg_ripple_http`; content-type negotiation: `application/sparql-results+json` JSON-Lines (SELECT/ASK), `application/n-triples` (CONSTRUCT); add streaming section to [pg_ripple_http/README.md](pg_ripple_http/README.md)

#### Operational Excellence (blocking for v1.0.0)

- [ ] **`pg_upgrade` compatibility matrix + integration test** (S10-3 / F-12): document PG18.x → PG18.y upgrade matrix in `docs/src/operations/pg-upgrade.md`; add `tests/pg_upgrade_compat.sh` that runs `pg_upgrade` + `ALTER EXTENSION pg_ripple UPDATE` on a loaded database
- [ ] **CDC subscription backpressure documented** (S5-3): add `docs/src/operations/cdc.md` covering NOTIFY queue tuning (`max_notify_queue_pages`), recommended subscriber patterns, and a warning template for slow consumers
- [ ] **Release automation** (S10-2): GitHub Actions workflow that opens a PR with version-bump, CHANGELOG draft, and migration-script template on `git tag v*`; `just release VERSION` recipe wires it from the developer side
- [ ] **`cargo audit`/`cargo deny` blocking** (S6-1): flip existing weekly `cargo-audit.yml` and `deny.toml` checks from advisory to blocking on every PR
- [ ] **PT-code drift linter** (N3-3): `scripts/check_pt_codes.sh` diffs `grep -roh 'PT[4-7][0-9][0-9]' src/` against `docs/src/reference/error-catalog.md`; fails CI on any undocumented code
- [ ] **Migration script header linter** (S9-6): `scripts/check_migration_headers.sh` enforces the AGENTS.md header template on every `sql/pg_ripple--*.sql` file
- [ ] **Migration chain data-preservation test** (S10-5): extend `tests/test_migration_chain.sh` to insert a representative dataset after v0.1.0, apply every migration, and re-query triple counts + SPARQL patterns at v0.51.0
- [ ] **`pg_dump`/restore round-trip test** (S6-3): `tests/pg_dump_restore.sh` loads 10 k triples, dumps, drops extension, restores, verifies triple count

#### Observability

- [ ] **OTLP tracing exporter wired** (N2-2 / F-3): `opentelemetry-otlp` + `tracing-opentelemetry` dependencies; new GUC `pg_ripple.tracing_otlp_endpoint`; when `tracing_exporter = 'otlp'` spans for SPARQL parse/translate/execute are exported to the configured collector, annotated with algebra digest and VP table scans
- [ ] **`predicate_workload_stats()` SRF** (N3-1 / F-5): `pg_ripple.predicate_workload_stats() RETURNS TABLE(predicate_iri TEXT, query_count BIGINT, merge_count BIGINT, last_merged TIMESTAMPTZ)` backed by per-predicate counters in `_pg_ripple.predicate_stats`; updated atomically by the merge worker and query executor
- [ ] **`explain_sparql()` BUFFERS** (N3-2): extend `explain_sparql(analyze:=true)` JSON output with a `buffers` key (`{shared_hit, shared_read, shared_dirtied, shared_written}`) when `analyze = true`

#### Storage & SPARQL Correctness

- [ ] **Merge worker latch-driven backoff** (S1-3): replace `std::thread::sleep` at [src/worker.rs:142](src/worker.rs#L142) with `BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs)))` for correct SIGTERM response during backoff
- [ ] **Predicate-OID syscache callback** (S1-5): register `CacheRegisterRelcacheCallback` in [src/storage/catalog.rs](src/storage/catalog.rs) to invalidate the predicate-OID thread-local cache when a VP table is rebuilt by `VACUUM FULL`
- [ ] **Consolidate path-depth GUCs** (S2-5): remove deprecated `property_path_max_depth`; migrate users to `max_path_depth`; emit a WARNING if the old name is set; add `min = 1, max = 65535` check_hook
- [ ] **CONSTRUCT ground RDF-star quoted triples** (S2-6 / N5-5): emit `<< s p o >>` N-Triples-star notation for ground quoted triples in CONSTRUCT templates ([src/sparql/mod.rs:742-748](src/sparql/mod.rs#L742))
- [ ] **Wire `execute_with_savepoint()`** (S3-4): call the SAVEPOINT helper in the parallel-strata coordinator before launching worker batches, or mark `#[cfg(test)]` and document the decision
- [ ] **Complete complex `sh:path` dispatcher** (S4-6 / N4-1): remove `#![allow(dead_code)]` from [src/shacl/constraints/property_path.rs](src/shacl/constraints/property_path.rs); wire sequence, alternative, inverse, `*`, `+`, `?` paths into the SHACL constraint dispatcher; add `tests/pg_regress/sql/shacl_complex_path.sql`

#### SPARQL Standards

- [ ] **Native SPARQL CSV/TSV SRFs** (N9-4 / F-6): `pg_ripple.sparql_csv(query TEXT) RETURNS SETOF TEXT` and `sparql_tsv(query TEXT) RETURNS SETOF TEXT` per W3C SPARQL 1.1 Results CSV and TSV formats; add pg_regress `sparql_csv_tsv.sql`
- [ ] **OWL 2 RL XFAIL closure** (N5-4): fix the four known failures in [tests/owl2rl/known_failures.txt](tests/owl2rl/known_failures.txt) (`prp-spo2`, `scm-sco`, `eq-diff1`, `dt-type2`); flip OWL 2 RL conformance gate from informational to blocking at 66/66

#### Documentation

- [ ] **GUC ↔ workload-class tuning matrix** (S9-2): new section in `docs/src/operations/tuning.md` mapping each GUC to workload characteristics
- [ ] **Worked examples for new features** (S9-3 / N8-2): `examples/llm_workflow.sql` (NL→SPARQL with mock endpoint, sameAs candidates, RAG context); `examples/federation_multi_endpoint.sql`; `examples/cdc_subscription.sql`
- [ ] **AGENTS.md pgrx version** (N7-4): update tech-stack table from pgrx 0.17 → 0.18
- [ ] **`justfile` `release` and `docs` recipes** (N7-5): `just release VERSION` (bumps version, generates migration script template, opens changelog section); `just docs-serve` (starts mdBook dev server)

### Migration Script

`sql/pg_ripple--0.50.0--0.51.0.sql` — schema changes: add `_pg_ripple.predicate_stats (predicate_id BIGINT PRIMARY KEY, query_count BIGINT DEFAULT 0, merge_count BIGINT DEFAULT 0, last_merged TIMESTAMPTZ)`; deprecation notice for `property_path_max_depth` GUC.

### Exit Criteria

All v1.0.0 blocking items in [plans/PLAN_OVERALL_ASSESSMENT_4.md](plans/PLAN_OVERALL_ASSESSMENT_4.md) are closed. Docker image runs as non-root. `POST /sparql/stream` returns chunked N-Triples for a large CONSTRUCT query without full in-memory buffering. `sparql_max_algebra_depth = 5` rejects a nested query of depth 6 with PT440. OWL 2 RL gate is blocking at 66/66. Migration chain data-preservation test passes v0.1.0 → v0.51.0.

---

## v0.52.0 — DX, Extended Standards & Architecture

**Theme**: SHACL-SPARQL constraint component, `COPY rdf FROM` bulk load, RAG pipeline hardening, CDC lifecycle events, architecture module splits, and OpenAPI specification for the HTTP companion.

> **In plain language:** This release advances pg_ripple's standards completeness and developer ergonomics. SHACL-SPARQL enables sophisticated data-quality rules written in SPARQL itself. Bulk loading via PostgreSQL's `COPY` command handles massive datasets without custom client code. The LLM-based RAG pipeline gains security hardening and response caching. An OpenAPI specification makes the HTTP companion self-documenting. And the codebase gets structural maintenance that keeps it navigable for future contributors. (The VS Code extension moves to v1.10.)
>
> **Effort estimate: 6–9 person-weeks**

### Deliverables

#### Developer Tooling

- [ ] **OpenAPI specification for HTTP service** (N8-4): generate `openapi.yaml` from `utoipa` annotations in `pg_ripple_http/src/main.rs`; publish at `docs/src/reference/openapi.yaml`
- [ ] **Architecture diagram** (N8-3 / S9-4): Mermaid diagram in `docs/src/reference/architecture.md` showing: client → dictionary → VP tables → SPARQL/Datalog/SHACL engines → views/exporters → federation → HTTP companion

#### Standards Completeness

- [ ] **SHACL-SPARQL `sh:SPARQLConstraintComponent`** (N5-3 / F-9): W3C SHACL-SPARQL; `sh:SPARQLConstraintComponent` executes a user-authored SPARQL query as a constraint; results decoded into `Violation` structs; add pg_regress `shacl_sparql_constraint.sql`
- [ ] **SHACL-AF `sh:rule` warning / compile** (S4-8 / N9-3): emit **PT480** `ShAFRuleUnsupported` when `sh:rule` triples are detected; or (preferred path) compile SPARQLRules to the existing Datalog rule engine
- [ ] **`COPY rdf FROM` integration** (N9-5 / F-10): register a custom PostgreSQL `COPY` handler so `COPY pg_ripple.triples FROM '/path/to.nt' WITH (FORMAT 'ntriples')` and `(FORMAT 'turtle')` work as first-class PostgreSQL commands with batched dictionary encoding; return row count

#### RAG & LLM Hardening

- [ ] **RAG pipeline hardening** (F-11): sanitise NL input before LLM call (block prompt-injection patterns; truncate at `llm_max_input_tokens`); add response caching keyed on `(question_hash, k, schema_digest)` in `_pg_ripple.rag_cache`; add `pg_ripple_http /rag` REST endpoint; add `examples/llm_workflow.sql` if not added in v0.51.0

#### CDC & Subscriptions

- [ ] **CDC lifecycle events** (N9-2): second NOTIFY channel `pg_ripple_cdc_lifecycle_{name}` emitting merge-cycle events (`{"op":"merge","predicate_id":N,"merged":M,"tombstones":T}`) and VP promotion events (`{"op":"promote","predicate_id":N,"from":"rare","to":"vp"}`)

#### Test Coverage

- [ ] **Property-based test generator enrichment** (N4-2): enrich dictionary generator (NFC/NFD Unicode, RTL, emoji, zero-width), SPARQL round-trip generator (property paths, subqueries, aggregates), JSON-LD framing generator (nested `@context`, `@list`, `@container`)
- [ ] **RDF/XML + JSON-LD framing fuzz targets** (N4-3): `fuzz/fuzz_targets/rdfxml_parser.rs` and `fuzz/fuzz_targets/jsonld_framer.rs`; assert no panic on arbitrary input
- [ ] **HTTP companion fuzz coverage** (N4-4): `fuzz/fuzz_targets/http_request.rs` targeting the `pg_ripple_http` request-handler chain
- [ ] **WatDiv conformance gate flip** (N4-5): promote WatDiv latency baseline gate from warning to blocking after two consecutive stable releases

#### Architecture & Code Quality

- [ ] **`gucs.rs` subsystem split** (N1-1): split 1,617-line `src/gucs.rs` into `src/gucs/{sparql.rs, datalog.rs, shacl.rs, federation.rs, llm.rs, storage.rs, observability.rs}`
- [ ] **`src/datalog/mod.rs` split** (N1-4): extract semi-naïve evaluator → `seminaive.rs`, magic-set transformer → `magic.rs`, demand-filter rewrite → `demand.rs`, parallel-strata coordinator → `coordinator.rs`
- [ ] **`filter.rs` split** (N7-1): split 901-line `src/sparql/translate/filter.rs` into `filter_expr.rs` (expression compilation) and `filter_dispatch.rs` (pattern dispatch)
- [ ] **HTTP companion hot-path `unwrap()` fixes** (N1-3): convert request-handler-path `unwrap()` calls at [pg_ripple_http/src/main.rs:829,865](pg_ripple_http/src/main.rs#L829) to `?`-propagation with HTTP 400/500 responses
- [ ] **Merge-throughput baseline** (N2-3): record p50/p95 at `merge_workers ∈ {1,2,4,8}` in `benchmarks/merge_throughput_baselines.json`; add CI warning gate on >15 % regression
- [ ] **HTAP merge cutover atomic fix** (C-3): investigate and, if confirmed safe, eliminate the `CREATE OR REPLACE VIEW` step in [src/storage/merge.rs:331-346](src/storage/merge.rs#L331); add concurrent-merge stress test asserting zero `relation does not exist` errors over 50 parallel queries

### Migration Script

`sql/pg_ripple--0.51.0--0.52.0.sql` — schema changes: add `_pg_ripple.rag_cache (question_hash TEXT PRIMARY KEY, k INT, schema_digest TEXT, result TEXT, cached_at TIMESTAMPTZ)`; add PT480 to error catalog.

### Exit Criteria

`sh:SPARQLConstraintComponent` passes W3C SHACL-SPARQL smoke test. `COPY pg_ripple.triples FROM 'file.nt' WITH (FORMAT 'ntriples')` loads 1 M triples successfully. OpenAPI spec published and validated. All 9 fuzz targets run in CI. WatDiv gate is blocking.

---

## v0.53.0 — High Availability & Logical Replication

**Theme**: Production HA via PG18 logical-decoding RDF replication, Kubernetes Helm chart, and vector-index performance baselines.

> **In plain language:** For organisations running pg_ripple in production, this release provides the infrastructure they need for high availability: a second PostgreSQL instance can subscribe to the RDF graph's change stream and stay in sync with the primary in near-real-time. A Kubernetes Helm chart makes deployment in containerised environments first-class. Vector-index benchmarks give operators the data they need to choose between HNSW and IVFFlat for their specific workload.
>
> **Effort estimate: 5–7 person-weeks**

### Deliverables

#### Logical Replication (F-8)

- [ ] **Logical-decoding output plugin**: custom PG18 logical-decoding plugin (`pg_ripple_logical` crate) that decodes VP delta-table `INSERT`/`DELETE` changes into N-Triples format; plug into a `CREATE PUBLICATION pg_ripple_pub FOR ALL TABLES IN SCHEMA _pg_ripple`
- [ ] **Replica-side consumer**: `pg_ripple.logical_apply_worker` background worker that subscribes to the publication, receives N-Triples batches, and applies them via `load_ntriples()` in order; conflict resolution: `last-writer-wins` per SID, configurable via `pg_ripple.replication_conflict_strategy`
- [ ] **Replication status SRF**: `pg_ripple.replication_stats() RETURNS TABLE(slot_name TEXT, lag_bytes BIGINT, last_applied_lsn PG_LSN, last_applied_at TIMESTAMPTZ)`
- [ ] **`docs/src/operations/replication.md`**: architecture overview, setup walkthrough (primary + replica), lag monitoring, failover procedure

#### Kubernetes & Helm Chart (F-2 Helm portion)

- [ ] **Helm chart**: `charts/pg_ripple/` with values for `replicaCount`, `persistence` (PVC), `http.service` (LoadBalancer/ClusterIP), `federationEndpoints`, `shacl.shapesConfigMap`, `llm.apiKeySecret`; liveness and readiness probes via `GET /health`; published to GitHub Pages Helm repo
- [ ] **`docs/src/operations/kubernetes.md`**: deployment guide for Helm, values reference, monitoring integration with Prometheus; design stub for future Go operator using `controller-runtime`

#### Vector-Index Performance (N2-4)

- [ ] **Vector-index comparison benchmark**: `benchmarks/vector_index_compare.sql` — 100 k-embedding fixture; measure p50/p95/p99 ANN recall and latency for `embedding_index_type ∈ {hnsw, ivfflat}` at `embedding_precision ∈ {single, half, binary}`; results published in `docs/src/reference/vector-index-tradeoffs.md`

#### Documentation

- [ ] `docs/src/operations/high-availability.md` — decision tree: pg_ripple logical replication vs. standard PG streaming replication; trade-offs and supported topologies
- [ ] Update `docs/src/reference/guc-reference.md` with v0.53.0 GUCs (`replication_enabled`, `replication_conflict_strategy`)

### Migration Script

`sql/pg_ripple--0.52.0--0.53.0.sql` — schema change: create `_pg_ripple.replication_status` catalog table when `pg_ripple.replication_enabled = on`.

### Exit Criteria

A primary + replica test using the logical-decoding plugin achieves < 1 s replication lag on a 10 k-triple/s insert workload. Helm chart deploys successfully on `minikube`. Vector-index benchmark results published and linked from the GUC reference.

---

## v0.54.0 — pg-trickle Relay Integration

**Theme**: Hub-and-spoke event streaming — connect pg_ripple to external sources and consumers via pg-trickle's relay transport layer.

> **In plain language:** pg_ripple can already store, infer, and query knowledge graphs. This release makes it easy to *feed* the graph from real-world event streams (Kafka, NATS, webhooks) and *publish* enriched results back out to consumers — all inside PostgreSQL, without any application code. The key pieces are: a helper function that converts any JSON event into RDF triples, a background worker that watches for newly inferred triples and writes them to pg-trickle's outbox, and configurable triggers for latency-sensitive paths. Pre-built vocabulary alignment templates handle the common case where different data sources use different names for the same concepts.
>
> **Effort estimate: 5–7 person-weeks**
>
> **Design reference**: [plans/pg_trickle_relay_integration.md](plans/pg_trickle_relay_integration.md)

### Deliverables

#### JSON → RDF Transform Helpers

- [ ] **`pg_ripple.json_to_ntriples(payload JSONB, subject_iri TEXT, type_iri TEXT) RETURNS TEXT`** (`src/bulk_load.rs`): converts a JSON object to N-Triples; handles nested objects, arrays, and XSD-typed values; optional `context JSONB` argument maps JSON keys to vocabulary URIs
- [ ] **`pg_ripple.json_to_ntriples_trigger() RETURNS TRIGGER`**: PL/pgSQL wrapper callable directly as an `AFTER INSERT` trigger on pg-trickle inbox tables; reads `NEW.payload`, derives subject IRI from configurable trigger argument
- [ ] **`docs/src/integrations/json-to-rdf.md`**: mapping rules, supported JSON structures, trigger usage examples

#### CDC → pg-trickle Outbox Bridge Worker

- [ ] **`_pg_ripple.cdc_bridge_worker`** (`src/storage/cdc_bridge.rs`): `BackgroundWorker` that listens on the CDC `NOTIFY` channel, batches notifications by `pg_ripple.cdc_bridge_batch_size` (default: 100) or `pg_ripple.cdc_bridge_flush_ms` (default: 200 ms), bulk-decodes dictionary IDs via a single SPI call, and batch-inserts JSON-LD events into `pg_ripple.cdc_bridge_outbox_table` (configurable GUC)
- [ ] **New GUCs**: `pg_ripple.cdc_bridge_enabled` (bool, default: `off`), `pg_ripple.cdc_bridge_batch_size` (int, default: 100), `pg_ripple.cdc_bridge_flush_ms` (int, default: 200), `pg_ripple.cdc_bridge_outbox_table` (text, default: `enriched_events`), `pg_ripple.trickle_integration` (bool, default: `on` when pg-trickle detected)
- [ ] **`docs/src/integrations/cdc-bridge.md`**: architecture, GUC tuning, backpressure guidance

#### Selective CDC Bridge Triggers

- [ ] **`pg_ripple.enable_cdc_bridge_trigger(name TEXT, predicate TEXT, outbox TEXT) RETURNS VOID`** (`src/storage/cdc_bridge.rs`): installs a trigger on the VP delta table for the given predicate that writes decoded JSON-LD directly to the specified outbox table in the same transaction
- [ ] **`pg_ripple.disable_cdc_bridge_trigger(name TEXT) RETURNS VOID`**: drops the trigger
- [ ] **`pg_ripple.cdc_bridge_triggers() RETURNS TABLE(name TEXT, predicate TEXT, outbox TEXT, active BOOL)`**: catalog SRF
- [ ] **Catalog table**: `_pg_ripple.cdc_bridge_triggers (name TEXT PRIMARY KEY, predicate_id BIGINT, outbox_table TEXT, created_at TIMESTAMPTZ)`

#### JSON-LD Event Serializer

- [ ] **`pg_ripple.triple_to_jsonld(s BIGINT, p BIGINT, o BIGINT) RETURNS JSONB`** (`src/export/jsonld.rs`): decodes a single triple from dictionary IDs using the LRU cache; returns a JSON-LD object with inline `@context`
- [ ] **`pg_ripple.triples_to_jsonld(subject BIGINT) RETURNS JSONB`**: collects all triples for a subject into a single JSON-LD document (star-pattern batch)
- [ ] Both functions used internally by bridge worker and CDC triggers; directly callable from SQL

#### Outbox Dedup Key from Statement ID

- [ ] **`pg_ripple.statement_dedup_key(s BIGINT, p BIGINT, o BIGINT) RETURNS TEXT`** (`src/storage/vp_tables.rs`): looks up the `i` column for the given triple and returns `'ripple:{statement_id}'` as a relay-compatible dedup key
- [ ] Dedup key included automatically in outbox JSON-LD payloads produced by the bridge worker and CDC bridge triggers as `"_dedup_key"` field

#### Vocabulary Alignment Templates

- [ ] **`sql/vocab/schema_to_saref.pl`**: Schema.org ↔ SAREF (IoT sensor data) alignment rules
- [ ] **`sql/vocab/schema_to_fhir.pl`**: Schema.org ↔ FHIR R4 basic resources (Patient, Observation)
- [ ] **`sql/vocab/schema_to_provo.pl`**: Schema.org ↔ PROV-O (provenance, agent, activity)
- [ ] **`sql/vocab/generic_to_schema.pl`**: generic JSON key → Schema.org property heuristic rules
- [ ] **`pg_ripple.load_vocab_template(name TEXT) RETURNS INT`**: loads a named template from `sql/vocab/`; returns number of rules loaded
- [ ] **`docs/src/integrations/vocabulary-templates.md`**: template reference and customisation guide

#### pg-trickle Runtime Detection & Graceful Degradation

- [ ] **Runtime detection at `_PG_init`** (`src/lib.rs`): check for pg-trickle via `SPI_execute('SELECT 1 FROM pg_extension WHERE extname = $1', 'pg_trickle')`; set module-level flag
- [ ] **Graceful degradation**: bridge functions return `PT800` error code when pg-trickle is absent; rest of pg_ripple unaffected
- [ ] **Error code `PT800`**: `pg_trickle extension is not installed; install pg_trickle to use bridge features`
- [ ] **`pg_ripple.trickle_available() RETURNS BOOL`**: SQL-callable runtime check

#### Integration Test Suite

- [ ] **`tests/pg_regress/sql/trickle_integration.sql`**: end-to-end tests using pg-trickle mock (inbox INSERT → trigger → load_ntriples → Datalog → CDC → outbox row); asserts round-trip correctness, dedup key uniqueness, JSON-LD schema
- [ ] **`tests/pg_regress/sql/trickle_graceful_degradation.sql`**: verifies `PT800` error when pg-trickle absent, all non-bridge functions unaffected
- [ ] **CI matrix extension**: add `pg_trickle` to the Docker Compose test service; `trickle_integration` tests are `REGRESS_OPTS += --schedule=trickle` (skipped if pg-trickle unavailable)

#### Documentation

- [ ] **`docs/src/integrations/pg-trickle-overview.md`**: hub-and-spoke architecture overview, full pipeline diagram, links to sub-pages
- [ ] **`docs/src/integrations/hub-and-spoke-example.md`**: complete worked example: Kafka orders → RDF → Datalog enrichment → NATS outbound
- [ ] Update `docs/src/reference/guc-reference.md` with v0.54.0 GUCs

### Migration Script

`sql/pg_ripple--0.53.0--0.54.0.sql` — schema changes: create `_pg_ripple.cdc_bridge_triggers` catalog table; no changes to VP tables or the dictionary.

### Exit Criteria

All `trickle_integration` pg_regress tests pass when pg-trickle is installed. `trickle_graceful_degradation` tests pass when pg-trickle is absent (bridge functions return `PT800`; all other functions return expected results). CDC bridge trigger delivers a decoded JSON-LD event to the outbox table within 20 ms of a VP delta INSERT in the same transaction. Bridge worker achieves ≥ 1,000 events/s sustained throughput on a 4-core CI runner. `json_to_ntriples()` handles nested JSON objects, arrays, and XSD-typed values correctly. Vocabulary template `schema_to_saref.pl` loads without errors and aligns at least the top 10 SAREF properties. Migration chain test passes through v0.54.0.

---

## v1.0.0 — Production Release

**Theme**: Stability, conformance, and production certification.

> **In plain language:** The 1.0 release is not about new features — it's about *confidence*. We run pg_ripple against the official W3C test suites for SPARQL and SHACL to verify standards compliance. A 72-hour continuous stress test checks for memory leaks and crash recovery. A security audit reviews the code for vulnerabilities. The result is a release that organisations can rely on for production workloads with a clear API stability guarantee: the public interface will not break in future minor versions.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **SPARQL 1.1 Query conformance**
  - Pass W3C SPARQL 1.1 Query test suite (supported subset)
  - Document unsupported features (property functions)
  - Verify conformance via both SQL and HTTP interfaces
  - Federation (`SERVICE`) covered by v0.16.0
- [ ] **SPARQL 1.1 Update conformance**
  - Pass W3C SPARQL 1.1 Update test suite (supported subset)
  - Document unsupported features
- [ ] **SHACL Core conformance**
  - Pass the full W3C SHACL Core test suite
  - Any optimization strategy must preserve the same externally visible results as the reference semantics
- [ ] **Stability hardening**
  - 72-hour continuous load test (mixed read/write)
  - Memory leak detection (Valgrind via `cargo pgrx test --valgrind`)
  - Crash recovery testing (kill -9 during merge, reload, verify)
- [ ] **Security audit**
  - Review all SPI query generation for injection vectors
  - Review shared memory usage for race conditions
  - Review dictionary cache for timing side-channels
- [ ] **API stability guarantee**
  - All `pg_ripple.*` SQL functions considered stable API
  - `_pg_ripple.*` internal schema reserved for internal use
  - Semantic versioning contract: breaking changes only in major versions
- [ ] **Final benchmarks**
  - BSBM at 100M triples
  - Published performance report
- [ ] **Release artifacts**
  - Tagged release on GitHub
  - Published to PGXN
  - crates.io publication (library crate)

### Documentation

> See [plans/documentation.md](plans/documentation.md) for details. The 1.0.0 documentation milestone is a full audit: every page verified, every example tested against the release, no unresolved stubs.

- [ ] Final audit of all docs pages — every code example verified against 1.0.0, all `TODO` / stub markers resolved
- [ ] `user-guide/upgrading.md` complete — upgrade procedure from every 0.x version to 1.0.0; migration script inventory
- [ ] `reference/error-reference.md` complete — all PT001–PT799 codes documented
- [ ] `reference/faq.md` final pass — 20–30 questions covering all features
- [ ] `reference/troubleshooting.md` final pass — complete runbook for every subsystem
- [ ] All `research/` section mirrors complete

### Exit Criteria

Stable, tested, documented, and published. Ready for production workloads up to 100M+ triples on a single node.

---

## Post-1.0 Horizon

> **In plain language:** These are future directions that extend pg_ripple beyond its initial scope. Each addresses a specific real-world need — from distributing data across multiple servers, to geographic queries, to bridging with existing relational databases. They are listed roughly in order of anticipated demand; some may be reordered or combined based on community feedback after 1.0.
>
> **v1.6 Cypher/GQL** has a dedicated exploratory analysis in [plans/cypher/](plans/cypher/). The core finding: VP tables already encode all LPG structural elements; a standalone `cypher-algebra` crate (openCypher + GQL grammar, unified SQL-emitting algebra IR) is the correct architecture. Full write support requires v0.4.0 (RDF-star) for edge properties — already available. Gremlin is explicitly out of scope.

| Version | Theme | What it delivers | Key Technical Features |
|---|---|---|---|
| 1.1 | Distributed | Spread data across multiple servers for horizontal scale | Citus integration, subject-based sharding |
| 1.2 | Temporal | Track how data changes over time; query historical states | Bitstring versioning, TimescaleDB integration |
| 1.4 | Extended VP | Automatically pre-compute shortcuts for frequent query patterns | Automated workload-driven ExtVP stream tables (pg_trickle), ontology change propagation DAG |
| 1.5 | Interop | Bridge to GraphQL APIs and expose LPG views for visualization tools | GraphQL-to-SPARQL auto-generation from SHACL shapes, stable LPG view layer for visualization tooling |
| 1.6 | Cypher / GQL | Query and write data using the industry-standard graph query languages | `cypher-algebra` standalone crate (openCypher + GQL grammar, same IR); `pg_ripple.cypher()` SQL function; `CREATE`, `MERGE`, `SET`, `DELETE` via VP write path; openCypher TCK ≥80%; edge properties available since v0.4.0 (RDF-star) |
| 1.7 | GeoSPARQL + PostGIS | Answer geographic questions ("find all hospitals within 5 km of this point") | `geo:asWKT` literal type backed by PostGIS `geometry`, spatial FILTER functions, R-tree index on spatial VP tables |
| 1.8 | R2RML Virtual Graphs | Expose existing database tables as if they were RDF data — no migration needed | W3C R2RML mappings, SPARQL queries transparently join VP tables with mapped SQL tables |
| 1.9 | Quad-Level Provenance | Track where each fact came from and when it was added | Per-quad metadata table with source, timestamp, and transaction ID; integration with Datalog rule provenance (why-provenance) |
| 1.10 | VS Code Extension | Editor integration for writing and running SPARQL, SHACL, and Datalog | Separate `pg-ripple-vscode` repo; TextMate grammars for SPARQL 1.1, SHACL Turtle, and Datalog; query runner against `pg_ripple_http`; SHACL shape linter; collapsible EXPLAIN tree view; VS Code Marketplace publication |

---

## Version Timeline (Estimated Cadence)

> **In plain language:** The "Calendar" column shows how long after the previous release each version is expected to ship. The "Effort" column shows the total developer-time required. With two developers working together, the calendar durations are achievable; with one developer, roughly double the calendar time.

| Version | Calendar (pair) | Effort (person-weeks) | Cumulative effort |
|---|---|---|---|
| 0.1.0 | Week 0 (start) | 6–8 pw | 6–8 pw |
| 0.2.0 | +4 weeks | 6–8 pw | 12–16 pw |
| 0.3.0 | +4 weeks | 6–8 pw | 18–24 pw |
| 0.4.0 | +5 weeks | 8–10 pw | 26–34 pw |
| 0.5.0 | +3 weeks | 6–8 pw | 32–42 pw |
| 0.5.1 | +3 weeks | 6–8 pw | 38–50 pw |
| 0.6.0 | +4 weeks | 8–10 pw | 46–60 pw |
| 0.7.0 | +3 weeks | 4–6 pw | 50–66 pw |
| 0.8.0 | +3 weeks | 4–6 pw | 54–72 pw |
| 0.9.0 | +2 weeks | 3–4 pw | 57–76 pw |
| 0.10.0 | +5 weeks | 10–12 pw | 67–88 pw |
| 0.11.0 | +3 weeks | 5–7 pw | 72–95 pw |
| 0.12.0 | +2 weeks | 3–4 pw | 75–99 pw |
| 0.13.0 | +4 weeks | 6–8 pw | 81–107 pw |
| 0.14.0 | +3 weeks | 4–6 pw | 85–113 pw |
| 0.15.0 | +2 weeks | 3–4 pw | 88–117 pw |
| 0.16.0 | +3 weeks | 4–6 pw | 92–123 pw |
| 0.19.0 | +3 weeks | 3–5 pw | 95–128 pw |
| 0.20.0 | +3 weeks | 5–7 pw | 100–135 pw |
| 0.45.0 | +3 weeks | 4–6 pw | 104–141 pw |
| 0.46.0 | +4 weeks | 5–7 pw | 109–148 pw |
| 0.47.0 | +5 weeks | 8–10 pw | 117–158 pw |
| 0.48.0 | +4 weeks | 6–8 pw | 123–166 pw |
| 0.49.0 | +3 weeks | 4–6 pw | 127–172 pw |
| 0.50.0 | +4 weeks | 5–7 pw | 132–179 pw |
| 0.51.0 | +5 weeks | 8–10 pw | 140–189 pw |
| 0.52.0 | +5 weeks | 6–9 pw | 146–198 pw |
| 0.53.0 | +4 weeks | 5–7 pw | 151–205 pw |
| 0.54.0 | +4 weeks | 5–7 pw | 156–212 pw |
| 1.0.0 | +4 weeks | 6–8 pw | **162–220 pw** |
| 1.1–1.9 | Post-1.0 | Community-driven | — |

*Estimates assume a pair of focused developers with Rust and PostgreSQL experience. "pw" = person-weeks. Calendar durations assume pair programming; a solo developer should expect roughly double the calendar time. Actual pace depends on contributor availability and scope adjustments discovered during implementation.*
