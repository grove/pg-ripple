# pg_ripple — Roadmap Feature Descriptions

This file contains plain-language descriptions of every pg_ripple release from v0.1.0 through v0.54.0 and v1.0.0. The goal is to give a clear picture of what each version delivers, why it matters, and how much work is involved — without requiring deep technical knowledge.

---

## What is pg_ripple?

pg_ripple is a database extension for PostgreSQL that lets you store and query knowledge graphs — structured networks of facts expressed as subject → predicate → object triples. It supports SPARQL (the standard query language for knowledge graphs), SHACL (data validation rules), and Datalog (inference rules). It also integrates with AI/LLM tooling for question-answering over graph data.

---

## Roadmap Overview

| Version | Theme | Key Deliverables | Estimated Effort | Full Technical Details |
|---|---|---|---|---|
| **[v0.1.0](roadmap/v0.1.0.md)** | Foundation | VP tables, dictionary encoding (XXH3-128), basic CRUD, SPARQL placeholder | 3–5 pw | [full](roadmap/v0.1.0-full.md) |
| **[v0.2.0](roadmap/v0.2.0.md)** | Bulk Loading & Named Graphs | N-Triples/Turtle/RDF-XML bulk load, named graphs, 50K+ triples/sec | 3–5 pw | [full](roadmap/v0.2.0-full.md) |
| **[v0.3.0](roadmap/v0.3.0.md)** | SPARQL Query Engine (Basic) | BGP, FILTER, OPTIONAL, star-pattern optimisation, query plan cache | 4–6 pw | [full](roadmap/v0.3.0-full.md) |
| **[v0.4.0](roadmap/v0.4.0.md)** | RDF-star / Statement Identifiers | Quoted triples, N-Triples-star parsing, provenance annotations | 3–4 pw | [full](roadmap/v0.4.0-full.md) |
| **[v0.5.0](roadmap/v0.5.0.md)** | SPARQL Advanced (Query Completeness) | Property paths, UNION/MINUS, aggregates, subqueries, VALUES | 4–6 pw | [full](roadmap/v0.5.0-full.md) |
| **[v0.5.1](roadmap/v0.5.1.md)** | SPARQL Advanced (Storage, Serialisation & Write) | INSERT/DELETE DATA, CONSTRUCT/DESCRIBE, inline term encoding, full-text search | 3–5 pw | [full](roadmap/v0.5.1-full.md) |
| **[v0.6.0](roadmap/v0.6.0.md)** | HTAP Architecture | Delta/main split, background merge worker, CDC, shared-memory cache, bloom filter | 5–7 pw | [full](roadmap/v0.6.0-full.md) |
| **[v0.7.0](roadmap/v0.7.0.md)** | SHACL Validation (Core) | Load SHACL shapes, synchronous enforcement, validate(), deduplication | 4–6 pw | [full](roadmap/v0.7.0-full.md) |
| **[v0.8.0](roadmap/v0.8.0.md)** | SHACL Advanced | sh:or/and/not, async validation pipeline, dead-letter queue | 4–6 pw | [full](roadmap/v0.8.0-full.md) |
| **[v0.9.0](roadmap/v0.9.0.md)** | Serialisation, Export & Interop | RDF-XML import, Turtle/JSON-LD export, RDF-star serialisation | 3–4 pw | [full](roadmap/v0.9.0-full.md) |
| **[v0.10.0](roadmap/v0.10.0.md)** | Datalog Reasoning Engine | load_rules(), infer(), RDFS/OWL-RL rules, named rule sets, Datalog constraints | 8–12 pw | [full](roadmap/v0.10.0-full.md) |
| **[v0.11.0](roadmap/v0.11.0.md)** | Incremental SPARQL/Datalog Views | Live stream tables, ExtVP, auto-refresh views, pg-trickle integration | 5–7 pw | [full](roadmap/v0.11.0-full.md) |
| **[v0.12.0](roadmap/v0.12.0.md)** | SPARQL Update Advanced | DELETE/INSERT WHERE, LOAD from URL, CLEAR/DROP/CREATE graphs | 3–4 pw | [full](roadmap/v0.12.0-full.md) |
| **[v0.13.0](roadmap/v0.13.0.md)** | Performance Hardening | BGP join reordering, parallel query, SHACL hints, BSBM benchmark, fuzz testing | 4–6 pw | [full](roadmap/v0.13.0-full.md) |
| **[v0.14.0](roadmap/v0.14.0.md)** | Administrative & Operational Readiness | vacuum/reindex/compact, graph-level RLS, upgrade scripts, operator docs | 4–6 pw | [full](roadmap/v0.14.0-full.md) |
| **[v0.15.0](roadmap/v0.15.0.md)** | SPARQL Protocol (HTTP Endpoint) | pg_ripple_http service, SPARQL 1.1 Protocol, content negotiation, auth, Prometheus | 4–6 pw | [full](roadmap/v0.15.0-full.md) |
| **[v0.16.0](roadmap/v0.16.0.md)** | SPARQL Federation | SERVICE keyword, parallel execution, endpoint allowlist, bind-join optimisation | 4–6 pw | [full](roadmap/v0.16.0-full.md) |
| **[v0.17.0](roadmap/v0.17.0.md)** | JSON-LD Framing | W3C JSON-LD Framing, frame-to-SPARQL translation, export_jsonld_framed(), live framing views | 4–6 pw | [full](roadmap/v0.17.0-full.md) |
| **[v0.18.0](roadmap/v0.18.0.md)** | SPARQL CONSTRUCT/DESCRIBE/ASK Views | Live derived triple stores, boolean monitors, CONSTRUCT/DESCRIBE/ASK views | 4–6 pw | [full](roadmap/v0.18.0-full.md) |
| **[v0.19.0](roadmap/v0.19.0.md)** | Federation Performance | Connection pooling, result caching with TTL, variable projection, adaptive timeouts | 4–5 pw | [full](roadmap/v0.19.0-full.md) |
| **[v0.20.0](roadmap/v0.20.0.md)** | W3C Conformance & Stability Foundation | W3C test suites in CI, crash recovery testing, memory leak detection, BSBM at 100M triples | 6–8 pw | [full](roadmap/v0.20.0-full.md) |
| **[v0.21.0](roadmap/v0.21.0.md)** | SPARQL Built-in Functions & Query Correctness | 40 built-in functions, NULL sort order, reflexive path fix, named error codes | 4–6 pw | [full](roadmap/v0.21.0-full.md) |
| **[v0.22.0](roadmap/v0.22.0.md)** | Storage Correctness & Security Hardening | Dictionary cache rollback, HTAP race fixes, shmem redesign, HTTP security, privilege lockdown | 6–8 pw | [full](roadmap/v0.22.0-full.md) |
| **[v0.23.0](roadmap/v0.23.0.md)** | SHACL Core Completion & SPARQL Diagnostics | 8 new SHACL constraints, explain_sparql(), SHACL query hints, Datalog fixes | 6–8 pw | [full](roadmap/v0.23.0-full.md) |
| **[v0.24.0](roadmap/v0.24.0.md)** | Semi-naive Datalog & Performance Hardening | Semi-naive evaluation (5×–100× faster), 4 OWL RL rules, batch decoding, BRIN on SID | 6–8 pw | [full](roadmap/v0.24.0-full.md) |
| **[v0.25.0](roadmap/v0.25.0.md)** | GeoSPARQL & Architectural Polish | PostGIS integration, geo:sfIntersects/contains/distance, catalog stability, canary() | 6–8 pw | [full](roadmap/v0.25.0-full.md) |
| **[v0.26.0](roadmap/v0.26.0.md)** | GraphRAG Integration | gr: ontology, BYOG Parquet export, Datalog enrichment rules, Python CLI bridge | 4–6 pw | [full](roadmap/v0.26.0-full.md) |
| **[v0.27.0](roadmap/v0.27.0.md)** | Vector + SPARQL Hybrid: Foundation | Embeddings table, HNSW index, embed_entities(), similar_entities(), pg:similar() | 5–7 pw | [full](roadmap/v0.27.0-full.md) |
| **[v0.28.0](roadmap/v0.28.0.md)** | Advanced Hybrid Search & RAG Pipeline | RRF hybrid_search(), incremental embedding worker, rag_retrieve(), POST /rag | 5–8 pw | [full](roadmap/v0.28.0-full.md) |
| **[v0.29.0](roadmap/v0.29.0.md)** | Datalog Optimization: Magic Sets & Cost-Based Compilation | infer_goal(), magic sets, cost-based body reordering, subsumption, anti-join negation | 5–7 pw | [full](roadmap/v0.29.0-full.md) |
| **[v0.30.0](roadmap/v0.30.0.md)** | Datalog Aggregation & Compiled Rule Plans | COUNT/SUM/AVG/MIN/MAX in rules, infer_agg(), aggregation stratification, rule plan cache | 5–7 pw | [full](roadmap/v0.30.0-full.md) |
| **[v0.31.0](roadmap/v0.31.0.md)** | Entity Resolution & Demand Transformation | owl:sameAs canonicalisation, SPARQL alias rewriting, infer_demand() | 5–7 pw | [full](roadmap/v0.31.0-full.md) |
| **[v0.32.0](roadmap/v0.32.0.md)** | Well-Founded Semantics & Tabling | infer_wfs(), alternating fixpoint, certainty annotations, session tabling cache | 5–7 pw | [full](roadmap/v0.32.0-full.md) |
| **[v0.33.0](roadmap/v0.33.0.md)** | Documentation Site & Content Overhaul | 5 user archetypes, 8 feature deep-dives, operations guide, SQL reference, CI doc tests | 8–12 pw | [full](roadmap/v0.33.0-full.md) |
| **[v0.34.0](roadmap/v0.34.0.md)** | Bounded-Depth Termination & DRed | sh:maxDepth early termination, DRed incremental retraction, add_rule()/remove_rule() | 5–7 pw | [full](roadmap/v0.34.0-full.md) |
| **[v0.35.0](roadmap/v0.35.0.md)** | Parallel Stratum Evaluation | Concurrent rule groups via background workers, configurable worker count (1–16) | 5–7 pw | [full](roadmap/v0.35.0-full.md) |
| **[v0.36.0](roadmap/v0.36.0.md)** | Worst-Case Optimal Joins & Lattice-Based Datalog | Leapfrog Triejoin for cyclic patterns, lattice Datalog (Min/Max/Set/Interval) | 6–9 pw | [full](roadmap/v0.36.0-full.md) |
| **[v0.37.0](roadmap/v0.37.0.md)** | Storage Concurrency Hardening & Error Safety | HTAP merge race fix, panic elimination, GUC validators, diagnostic_report(), schema_version | 9–11 pw | [full](roadmap/v0.37.0-full.md) |
| **[v0.38.0](roadmap/v0.38.0.md)** | Architecture Refactoring & Query Completeness | src/lib.rs module split, PredicateCatalog trait, SPARQL Update completion, SHACL hints wired | 9–11 pw | [full](roadmap/v0.38.0-full.md) |
| **[v0.39.0](roadmap/v0.39.0.md)** | Datalog HTTP API | 24 REST endpoints for Datalog rule management, inference, query, constraints, and monitoring | 3–5 pw | [full](roadmap/v0.39.0-full.md) |
| **[v0.40.0](roadmap/v0.40.0.md)** | Streaming Results, Explain & Observability | Streaming cursor API, enhanced explain_sparql(), OPTIONAL-inside-GRAPH fix, OTLP tracing | 9–11 pw | [full](roadmap/v0.40.0-full.md) |
| **[v0.41.0](roadmap/v0.41.0.md)** | Full W3C SPARQL 1.1 Test Suite | 3,000+ tests, 8-way parallel runner, 180-test smoke subset, XFAIL manifest | 5–7 pw | [full](roadmap/v0.41.0-full.md) |
| **[v0.42.0](roadmap/v0.42.0.md)** | Parallel Merge, Cost-Based Federation & Live CDC | Multi-worker merge pool, VoID source selection, parallel SERVICE, WebSocket CDC subscriptions | 9–11 pw | [full](roadmap/v0.42.0-full.md) |
| **[v0.43.0](roadmap/v0.43.0.md)** | WatDiv & Apache Jena Conformance Suites | Jena adapter (~1,000 tests), WatDiv 100-template harness, unified conformance runner | 5–7 pw | [full](roadmap/v0.43.0-full.md) |
| **[v0.44.0](roadmap/v0.44.0.md)** | LUBM Conformance Suite | LUBM data generator, 14 OWL RL queries, exact reference cardinalities, required CI check | 4–6 pw | [full](roadmap/v0.44.0-full.md) |
| **[v0.45.0](roadmap/v0.45.0.md)** | SHACL Completion, Datalog Robustness & Crash Recovery | sh:equals & sh:disjoint, decoded IRIs in violations, rollback coordination, crash-recovery tests | 6–8 pw | [full](roadmap/v0.45.0-full.md) |
| **[v0.46.0](roadmap/v0.46.0.md)** | Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance | proptest round-trips, cargo-fuzz federation decoder, OWL 2 RL suite, TopN push-down, dependency gates | 5–7 pw | [full](roadmap/v0.46.0-full.md) |
| **[v0.47.0](roadmap/v0.47.0.md)** | SHACL Truthfulness, Dead-Code Activation & Architecture Refactor | sh:closed/uniqueLang/pattern fixed, SID pre-alloc wired, sqlgen.rs split, 5 new fuzz targets | 8–11 pw | [full](roadmap/v0.47.0-full.md) |
| **[v0.48.0](roadmap/v0.48.0.md)** | SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness | 7 SHACL constraints, complex sh:path, 5 OWL RL rules, MOVE/COPY/ADD, SPARQL-star variables | 9–11 pw | [full](roadmap/v0.48.0-full.md) |
| **[v0.49.0](roadmap/v0.49.0.md)** | AI & LLM Integration | sparql_from_nl(), few-shot examples, LLM GUCs, suggest_sameas(), apply_sameas_candidates() | 5–7 pw | [full](roadmap/v0.49.0-full.md) |
| **[v0.50.0](roadmap/v0.50.0.md)** | Developer Experience & GraphRAG Polish | Enhanced explain_sparql() with decoded IRIs, rag_context() full RAG pipeline | 4–6 pw | [full](roadmap/v0.50.0-full.md) |
| **[v0.51.0](roadmap/v0.51.0.md)** | Security Hardening & Production Readiness | Non-root container, SPARQL DoS limits, HTTP streaming, OTLP tracing, pg_upgrade docs, OWL 2 RL completion | 8–10 pw | [full](roadmap/v0.51.0-full.md) |
| **[v0.52.0](roadmap/v0.52.0.md)** | pg-trickle Relay Integration | JSON→RDF helpers, CDC→outbox bridge worker, CDC bridge triggers, JSON-LD event serializer, dedup keys, vocabulary alignment templates, pg-trickle runtime detection, integration test suite | 5–7 pw | [full](roadmap/v0.52.0-full.md) |
| **[v0.53.0](roadmap/v0.53.0.md)** | DX, Extended Standards & Architecture | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, OpenAPI spec, CDC lifecycle events, code quality splits | 6–9 pw | [full](roadmap/v0.53.0-full.md) |
| **[v0.54.0](roadmap/v0.54.0.md)** | High Availability & Logical Replication | RDF logical replication, Helm chart, vector index benchmarks | 5–7 pw | [full](roadmap/v0.54.0-full.md) |
| **v1.0.0** | Production Release | Final conformance, stress test, security audit, API stability guarantee | 6–8 pw | [full](roadmap/v1.0.0-full.md) |

**Total estimated effort to v1.0.0 from the current state (v0.51.0): 30–41 person-weeks**
