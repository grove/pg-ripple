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

For a plain-language explanation of what each release delivers and why it matters, see the individual per-version plain-language files in the `roadmap/` directory (e.g. [roadmap/v0.54.0.md](roadmap/v0.54.0.md)).

---

## Overview at a glance

| Version | Name | What it delivers (one sentence) | Full details | Effort |
|---|---|---|---|---|
| [v0.1.0](roadmap/v0.1.0.md) | Foundation | Install the extension, store and retrieve facts (VP storage from day one) | [Full details](roadmap/v0.1.0-full.md) | 6–8 pw |
| [v0.2.0](roadmap/v0.2.0.md) | Bulk Loading & Named Graphs | Bulk data import, named graphs, rare-predicate consolidation, N-Triples export | [Full details](roadmap/v0.2.0-full.md) | 6–8 pw |
| [v0.3.0](roadmap/v0.3.0.md) | SPARQL Basic | Ask questions in the standard RDF query language (incl. GRAPH patterns) | [Full details](roadmap/v0.3.0-full.md) | 6–8 pw |
| [v0.4.0](roadmap/v0.4.0.md) | RDF-star / Statement IDs | Make statements about statements; LPG-ready storage | [Full details](roadmap/v0.4.0-full.md) | 8–10 pw |
| [v0.5.0](roadmap/v0.5.0.md) | SPARQL Advanced (Query) | Property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES | [Full details](roadmap/v0.5.0-full.md) | 6–8 pw |
| [v0.5.1](roadmap/v0.5.1.md) | SPARQL Advanced (Storage & Write) | Inline encoding, CONSTRUCT/DESCRIBE, INSERT/DELETE DATA, FTS | [Full details](roadmap/v0.5.1-full.md) | 6–8 pw |
| [v0.6.0](roadmap/v0.6.0.md) | HTAP Architecture | Heavy reads and writes at the same time; shared-memory cache | [Full details](roadmap/v0.6.0-full.md) | 8–10 pw |
| [v0.7.0](roadmap/v0.7.0.md) | SHACL Core + Deduplication | Define data quality rules; reject bad data on insert; on-demand and merge-time triple deduplication | [Full details](roadmap/v0.7.0-full.md) | 5–7 pw |
| [v0.8.0](roadmap/v0.8.0.md) | SHACL Advanced | Complex data quality rules with background checking | [Full details](roadmap/v0.8.0-full.md) | 4–6 pw |
| [v0.9.0](roadmap/v0.9.0.md) | Serialization | Import and export data in all standard RDF file formats | [Full details](roadmap/v0.9.0-full.md) | 3–4 pw |
| [v0.10.0](roadmap/v0.10.0.md) | Datalog Reasoning | Automatically derive new facts from rules and logic | [Full details](roadmap/v0.10.0-full.md) | 10–12 pw |
| [v0.11.0](roadmap/v0.11.0.md) | SPARQL & Datalog Views | Live, always-up-to-date dashboards from SPARQL and Datalog queries | [Full details](roadmap/v0.11.0-full.md) | 5–7 pw |
| [v0.12.0](roadmap/v0.12.0.md) | SPARQL Update (Advanced) | Pattern-based updates and graph management commands | [Full details](roadmap/v0.12.0-full.md) | 3–4 pw |
| [v0.13.0](roadmap/v0.13.0.md) | Performance | Speed tuning, benchmarks, production-grade throughput | [Full details](roadmap/v0.13.0-full.md) | 6–8 pw |
| [v0.14.0](roadmap/v0.14.0.md) | Admin & Security | Operations tooling, access control, docs, packaging | [Full details](roadmap/v0.14.0-full.md) | 4–6 pw |
| [v0.15.0](roadmap/v0.15.0.md) | SPARQL Protocol | Standard HTTP API, graph-aware loaders and deletes as SQL functions | [Full details](roadmap/v0.15.0-full.md) | 3–4 pw |
| [v0.16.0](roadmap/v0.16.0.md) | SPARQL Federation | Query remote SPARQL endpoints alongside local data | [Full details](roadmap/v0.16.0-full.md) | 4–6 pw |
| [v0.17.0](roadmap/v0.17.0.md) | JSON-LD Framing | Frame-driven CONSTRUCT queries producing nested JSON-LD | [Full details](roadmap/v0.17.0-full.md) | 3–4 pw |
| [v0.18.0](roadmap/v0.18.0.md) | SPARQL CONSTRUCT & ASK Views | Materialize CONSTRUCT and ASK queries as live, incrementally-updated stream tables | [Full details](roadmap/v0.18.0-full.md) | 2–3 pw |
| [v0.19.0](roadmap/v0.19.0.md) | Federation Performance | Connection pooling, result caching, query rewriting, and batching for remote SPARQL endpoints | [Full details](roadmap/v0.19.0-full.md) | 3–5 pw |
| [v0.20.0](roadmap/v0.20.0.md) | W3C Conformance & Stability | W3C SPARQL 1.1 and SHACL Core test suite compliance, crash recovery and memory safety hardening, security audit initiation | [Full details](roadmap/v0.20.0-full.md) | 5–7 pw |
| [v0.21.0](roadmap/v0.21.0.md) | SPARQL Built-in Functions & Query Correctness | Implement all ~40 missing SPARQL 1.1 built-in functions, fix the FILTER silent-drop hazard, and close critical query-semantics bugs | [Full details](roadmap/v0.21.0-full.md) | 6–8 pw |
| [v0.22.0](roadmap/v0.22.0.md) | Storage Correctness & Security Hardening | Fix HTAP merge race conditions, dictionary cache rollback, shmem cache thrashing, rare-predicate promotion race, and HTTP service security gaps | [Full details](roadmap/v0.22.0-full.md) | 6–8 pw |
| [v0.23.0](roadmap/v0.23.0.md) | SHACL Core Completion & SPARQL Diagnostics | Complete the SHACL constraint set, add SPARQL query introspection, and fix Datalog/JSON-LD correctness issues | [Full details](roadmap/v0.23.0-full.md) | 6–8 pw |
| [v0.24.0](roadmap/v0.24.0.md) | Semi-naive Datalog & Performance Hardening | Implement semi-naive evaluation for Datalog rules, complete the OWL RL rule set, batch-decode large result sets, and bound property-path depth | [Full details](roadmap/v0.24.0-full.md) | 6–8 pw |
| [v0.25.0](roadmap/v0.25.0.md) | GeoSPARQL & Architectural Polish | Add GeoSPARQL 1.1 geometry primitives, stabilise the internal catalog against OID drift, and close remaining medium- and low-priority issues | [Full details](roadmap/v0.25.0-full.md) | 6–8 pw |
| [v0.26.0](roadmap/v0.26.0.md) | GraphRAG Integration | First-class integration with Microsoft GraphRAG: BYOG Parquet export, Datalog-enriched entity graphs, SHACL quality enforcement, and a Python CLI bridge | [Full details](roadmap/v0.26.0-full.md) | 4–6 pw |
| [v0.27.0](roadmap/v0.27.0.md) | Vector + SPARQL Hybrid: Foundation | Core pgvector integration — embedding table, HNSW index, `pg:similar()` SPARQL function, bulk embedding, and hybrid retrieval modes | [Full details](roadmap/v0.27.0-full.md) | 5–7 pw |
| [v0.28.0](roadmap/v0.28.0.md) | Advanced Hybrid Search & RAG Pipeline | Production-grade RRF fusion, incremental embedding worker, graph-contextualized embeddings, and end-to-end RAG retrieval | [Full details](roadmap/v0.28.0-full.md) | 5–8 pw |
| [v0.29.0](roadmap/v0.29.0.md) | Datalog Optimization: Magic Sets & Cost-Based Compilation | Goal-directed inference via magic sets, cost-based body atom reordering, subsumption checking, anti-join negation, filter pushdown, delta table indexing | [Full details](roadmap/v0.29.0-full.md) | 5–7 pw |
| [v0.30.0](roadmap/v0.30.0.md) | Datalog Aggregation & Compiled Rule Plans | Aggregation in rule bodies (Datalog^agg), SQL plan caching across inference runs, SPARQL on-demand query speedup | [Full details](roadmap/v0.30.0-full.md) | 5–7 pw |
| [v0.31.0](roadmap/v0.31.0.md) | Entity Resolution & Demand Transformation | `owl:sameAs` entity canonicalization, demand transformation for goal-directed rule rewriting, SPARQL query planner integration | [Full details](roadmap/v0.31.0-full.md) | 5–7 pw |
| [v0.32.0](roadmap/v0.32.0.md) | Well-Founded Semantics & Tabling | Three-valued semantics for cyclic ontologies, subsumptive result caching for Datalog and SPARQL repeated sub-queries | [Full details](roadmap/v0.32.0-full.md) | 5–7 pw |
| [v0.33.0](roadmap/v0.33.0.md) | Documentation Site & Content Overhaul | Complete docs site rebuild — CI harness, eight feature-deep-dive chapters, operations guide, reference section, and content governance | [Full details](roadmap/v0.33.0-full.md) | 8–12 pw |
| [v0.34.0](roadmap/v0.34.0.md) | Bounded-Depth Termination & Incremental Retraction (DRed) | Early fixpoint termination for bounded hierarchies (20–50% faster SPARQL property paths); Delete-Rederive for write-correct materialized predicates | [Full details](roadmap/v0.34.0-full.md) | 5–7 pw |
| [v0.35.0](roadmap/v0.35.0.md) | Parallel Stratum Evaluation & Incremental Rule Updates | Background-worker parallelism for independent rules (2–5× faster materialization); add/remove rules without full recompute | [Full details](roadmap/v0.35.0-full.md) | 5–7 pw |
| [v0.36.0](roadmap/v0.36.0.md) | Worst-Case Optimal Joins & Lattice-Based Datalog | Leapfrog Triejoin for cyclic SPARQL patterns (10×–100× speedup); Datalog^L monotone lattice aggregation | [Full details](roadmap/v0.36.0-full.md) | 6–9 pw |
| [v0.37.0](roadmap/v0.37.0.md) | Storage Concurrency Hardening & Error Safety | Fix HTAP merge race, rare-predicate promotion race, dictionary cache rollback; eliminate all hard panics; add GUC validators | [Full details](roadmap/v0.37.0-full.md) | 9–11 pw |
| [v0.38.0](roadmap/v0.38.0.md) | Architecture Refactoring & Query Completeness | Split god-module, PredicateCatalog trait, batch encoding, SCBD, SPARQL Update completeness, SHACL hints in planner | [Full details](roadmap/v0.38.0-full.md) | 9–11 pw |
| [v0.39.0](roadmap/v0.39.0.md) | Datalog HTTP API | REST API exposing all 27 Datalog SQL functions in `pg_ripple_http`: rule management, inference, goal queries, constraints, admin | [Full details](roadmap/v0.39.0-full.md) | 3–5 pw |
| [v0.40.0](roadmap/v0.40.0.md) | Streaming Results, Explain & Observability | Server-side SPARQL cursors, `explain_sparql()`, `explain_datalog()`, OpenTelemetry tracing, resource governors | [Full details](roadmap/v0.40.0-full.md) | 9–11 pw |
| [v0.41.0](roadmap/v0.41.0.md) | Full W3C SPARQL 1.1 Test Suite | Complete W3C SPARQL 1.1 Query + Update + Graph Patterns + Aggregates test suite harness with parallelized execution; 3,000+ tests in < 2 min CI | [Full details](roadmap/v0.41.0-full.md) | 5–7 pw |
| [v0.42.0](roadmap/v0.42.0.md) | Parallel Merge, Cost-Based Federation & Live CDC | Multi-worker HTAP merge, FedX-style federation planner, parallel SERVICE, live RDF change subscriptions | [Full details](roadmap/v0.42.0-full.md) | 10–12 pw |
| [v0.43.0](roadmap/v0.43.0.md) | WatDiv + Jena Conformance Suite | Apache Jena edge-case tests (~1,000) and WatDiv scale-correctness benchmark (10M+ triples, star/chain/snowflake/complex patterns); 90% harness reuse from v0.41.0 | [Full details](roadmap/v0.43.0-full.md) | 5–7 pw |
| [v0.44.0](roadmap/v0.44.0.md) | LUBM Conformance Suite | Lehigh University Benchmark — OWL RL inference correctness across 14 canonical queries on 1K–8M triple datasets; includes Datalog API validation sub-suite for rule compilation, iteration tracking, inferred triples, goal queries, and performance baseline | [Full details](roadmap/v0.44.0-full.md) | 3–5 pw |
| [v0.45.0](roadmap/v0.45.0.md) | SHACL Completion, Datalog Robustness & Crash Recovery | Close remaining SHACL Core gaps (`sh:equals`/`sh:disjoint`, decoded violation IRIs, async load test), harden parallel Datalog strata rollback, add missing crash-recovery scenarios, and standardise migration documentation | [Full details](roadmap/v0.45.0-full.md) | 4–6 pw |
| [v0.46.0](roadmap/v0.46.0.md) | Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance | `proptest` for SPARQL and dictionary invariants, fuzz the federation result decoder, W3C OWL 2 RL test suite in CI, TopN push-down, BSBM regression gate, sequence pre-allocation for Datalog workers, rustdoc coverage enforcement, and HTTP certificate pinning | [Full details](roadmap/v0.46.0-full.md) | 5–7 pw |
| [v0.47.0](roadmap/v0.47.0.md) | SHACL Truthfulness, Dead-Code Activation & Architecture Refactor | Fix parsed-but-not-checked SHACL constraints, wire `preallocate_sid_ranges()`, finish the `sparql/translate/` module split, add 5 fuzz targets, 4 crash-recovery scenarios, cache hit-rate SRFs, GUC validators, and security hygiene | [Full details](roadmap/v0.47.0-full.md) | 8–10 pw |
| [v0.48.0](roadmap/v0.48.0.md) | SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness | Complete all 35 SHACL Core constraints and complex `sh:path` expressions, close the OWL 2 RL rule set, add SPARQL Update MOVE/COPY/ADD, fix SPARQL-star variable patterns, WatDiv baselines, and operational hardening | [Full details](roadmap/v0.48.0-full.md) | 6–8 pw |
| [v0.49.0](roadmap/v0.49.0.md) | AI & LLM Integration | `sparql_from_nl()` NL-to-SPARQL via configurable LLM endpoint; `suggest_sameas()` and `apply_sameas_candidates()` for embedding-based entity alignment | [Full details](roadmap/v0.49.0-full.md) | 4–6 pw |
| [v0.50.0](roadmap/v0.50.0.md) | Developer Experience & GraphRAG Polish | `explain_sparql(analyze:=true)` interactive query debugger; `rag_context()` RAG pipeline | [Full details](roadmap/v0.50.0-full.md) | 3–5 pw |
| [v0.51.0](roadmap/v0.51.0.md) | Security Hardening & Production Readiness | Non-root container, SPARQL DoS protection, HTTP streaming, OTLP, pg_upgrade compat, CDC docs, conformance gate flips | [Full details](roadmap/v0.51.0-full.md) | 8–10 pw |
| [v0.52.0](roadmap/v0.52.0.md) | pg-trickle Relay Integration | JSON→RDF helpers, CDC→outbox bridge worker, CDC bridge triggers, JSON-LD event serializer, dedup keys, vocabulary templates, pg-trickle runtime detection, integration test suite | [Full details](roadmap/v0.52.0-full.md) | 5–7 pw |
| [v0.53.0](roadmap/v0.53.0.md) | DX, Extended Standards & Architecture | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, CDC lifecycle events, architecture module splits, OpenAPI spec | [Full details](roadmap/v0.53.0-full.md) | 6–9 pw |
| [v0.54.0](roadmap/v0.54.0.md) | High Availability & Logical Replication | PG18 logical-decoding RDF replication, Helm chart, CloudNativePG image volume, merge/vector-index performance baselines | [Full details](roadmap/v0.54.0-full.md) | 5–7 pw |
| [v1.0.0](roadmap/v1.0.0-full.md) | Production Release | Standards conformance, stress testing, security audit | [Full details](roadmap/v1.0.0-full.md) | 6–8 pw |
| | | **Total estimated effort** | | **275–376 pw** |

