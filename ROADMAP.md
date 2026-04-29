# pg_ripple — Roadmap

> **Audience:** Product managers, stakeholders, and technically curious readers
> who want to understand what each release delivers and why it matters —
> without needing to read Rust code or SQL specifications.

> **Authority rule**: [plans/implementation_plan.md](plans/implementation_plan.md) is the authoritative description of the **eventual target architecture**. This roadmap is the delivery sequence for that architecture.

## Versions

### Foundation (v0.1.0 – v0.5.1)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.1.0](roadmap/v0.1.0.md) | Install the extension, store and retrieve facts (VP storage from day one) | ✅ Released | Medium | [Full details](roadmap/v0.1.0-full.md) |
| [v0.2.0](roadmap/v0.2.0.md) | Bulk data import, named graphs, rare-predicate consolidation, N-Triples export | ✅ Released | Medium | [Full details](roadmap/v0.2.0-full.md) |
| [v0.3.0](roadmap/v0.3.0.md) | Ask questions in the standard RDF query language (incl. GRAPH patterns) | ✅ Released | Medium | [Full details](roadmap/v0.3.0-full.md) |
| [v0.4.0](roadmap/v0.4.0.md) | Make statements about statements; LPG-ready storage | ✅ Released | Large | [Full details](roadmap/v0.4.0-full.md) |
| [v0.5.0](roadmap/v0.5.0.md) | Property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES | ✅ Released | Medium | [Full details](roadmap/v0.5.0-full.md) |
| [v0.5.1](roadmap/v0.5.1.md) | Inline encoding, CONSTRUCT/DESCRIBE, INSERT/DELETE DATA, FTS | ✅ Released | Medium | [Full details](roadmap/v0.5.1-full.md) |

### Storage Architecture & Validation (v0.6.0 – v0.10.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.6.0](roadmap/v0.6.0.md) | Heavy reads and writes at the same time; shared-memory cache | ✅ Released | Large | [Full details](roadmap/v0.6.0-full.md) |
| [v0.7.0](roadmap/v0.7.0.md) | Define data quality rules; reject bad data on insert; on-demand and merge-time triple deduplication | ✅ Released | Medium | [Full details](roadmap/v0.7.0-full.md) |
| [v0.8.0](roadmap/v0.8.0.md) | Complex data quality rules with background checking | ✅ Released | Small | [Full details](roadmap/v0.8.0-full.md) |
| [v0.9.0](roadmap/v0.9.0.md) | Import and export data in all standard RDF file formats | ✅ Released | Small | [Full details](roadmap/v0.9.0-full.md) |
| [v0.10.0](roadmap/v0.10.0.md) | Automatically derive new facts from rules and logic | ✅ Released | Very Large | [Full details](roadmap/v0.10.0-full.md) |

### Query, Protocol & Interoperability (v0.11.0 – v0.20.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.11.0](roadmap/v0.11.0.md) | Live, always-up-to-date dashboards from SPARQL and Datalog queries | ✅ Released | Medium | [Full details](roadmap/v0.11.0-full.md) |
| [v0.12.0](roadmap/v0.12.0.md) | Pattern-based updates and graph management commands | ✅ Released | Small | [Full details](roadmap/v0.12.0-full.md) |
| [v0.13.0](roadmap/v0.13.0.md) | Speed tuning, benchmarks, production-grade throughput | ✅ Released | Medium | [Full details](roadmap/v0.13.0-full.md) |
| [v0.14.0](roadmap/v0.14.0.md) | Operations tooling, access control, docs, packaging | ✅ Released | Small | [Full details](roadmap/v0.14.0-full.md) |
| [v0.15.0](roadmap/v0.15.0.md) | Standard HTTP API, graph-aware loaders and deletes as SQL functions | ✅ Released | Small | [Full details](roadmap/v0.15.0-full.md) |
| [v0.16.0](roadmap/v0.16.0.md) | Query remote SPARQL endpoints alongside local data | ✅ Released | Small | [Full details](roadmap/v0.16.0-full.md) |
| [v0.17.0](roadmap/v0.17.0.md) | Frame-driven CONSTRUCT queries producing nested JSON-LD | ✅ Released | Small | [Full details](roadmap/v0.17.0-full.md) |
| [v0.18.0](roadmap/v0.18.0.md) | Materialize CONSTRUCT and ASK queries as live, incrementally-updated stream tables | ✅ Released | Small | [Full details](roadmap/v0.18.0-full.md) |
| [v0.19.0](roadmap/v0.19.0.md) | Connection pooling, result caching, query rewriting, and batching for remote SPARQL endpoints | ✅ Released | Small | [Full details](roadmap/v0.19.0-full.md) |
| [v0.20.0](roadmap/v0.20.0.md) | W3C SPARQL 1.1 and SHACL Core test suite compliance, crash recovery and memory safety hardening | ✅ Released | Medium | [Full details](roadmap/v0.20.0-full.md) |

### Correctness & Datalog Optimization (v0.21.0 – v0.32.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.21.0](roadmap/v0.21.0.md) | Implement all ~40 missing SPARQL 1.1 built-in functions, fix the FILTER silent-drop hazard, and close critical query-semantics bugs | ✅ Released | Medium | [Full details](roadmap/v0.21.0-full.md) |
| [v0.22.0](roadmap/v0.22.0.md) | Fix HTAP merge race conditions, dictionary cache rollback, shmem cache thrashing, rare-predicate promotion race, and HTTP service security gaps | ✅ Released | Medium | [Full details](roadmap/v0.22.0-full.md) |
| [v0.23.0](roadmap/v0.23.0.md) | Complete the SHACL constraint set, add SPARQL query introspection, and fix Datalog/JSON-LD correctness issues | ✅ Released | Medium | [Full details](roadmap/v0.23.0-full.md) |
| [v0.24.0](roadmap/v0.24.0.md) | Semi-naive Datalog evaluation, complete OWL RL rule set, batch-decode large result sets, bound property-path depth | ✅ Released | Medium | [Full details](roadmap/v0.24.0-full.md) |
| [v0.25.0](roadmap/v0.25.0.md) | GeoSPARQL 1.1 geometry primitives, stabilise internal catalog against OID drift, close remaining medium- and low-priority issues | ✅ Released | Medium | [Full details](roadmap/v0.25.0-full.md) |
| [v0.26.0](roadmap/v0.26.0.md) | Microsoft GraphRAG integration: BYOG Parquet export, Datalog-enriched entity graphs, SHACL quality enforcement, Python CLI bridge | ✅ Released | Small | [Full details](roadmap/v0.26.0-full.md) |
| [v0.27.0](roadmap/v0.27.0.md) | Core pgvector integration — embedding table, HNSW index, `pg:similar()` SPARQL function, bulk embedding, hybrid retrieval modes | ✅ Released | Medium | [Full details](roadmap/v0.27.0-full.md) |
| [v0.28.0](roadmap/v0.28.0.md) | Production-grade RRF fusion, incremental embedding worker, graph-contextualized embeddings, end-to-end RAG retrieval | ✅ Released | Medium | [Full details](roadmap/v0.28.0-full.md) |
| [v0.29.0](roadmap/v0.29.0.md) | Goal-directed inference via magic sets, cost-based body atom reordering, subsumption checking, anti-join negation, filter pushdown | ✅ Released | Medium | [Full details](roadmap/v0.29.0-full.md) |
| [v0.30.0](roadmap/v0.30.0.md) | Aggregation in rule bodies (Datalog^agg), SQL plan caching across inference runs, SPARQL on-demand query speedup | ✅ Released | Medium | [Full details](roadmap/v0.30.0-full.md) |
| [v0.31.0](roadmap/v0.31.0.md) | `owl:sameAs` entity canonicalization, demand transformation for goal-directed rule rewriting, SPARQL query planner integration | ✅ Released | Medium | [Full details](roadmap/v0.31.0-full.md) |
| [v0.32.0](roadmap/v0.32.0.md) | Three-valued semantics for cyclic ontologies, subsumptive result caching for Datalog and SPARQL repeated sub-queries | ✅ Released | Medium | [Full details](roadmap/v0.32.0-full.md) |

### Performance, Conformance & Ecosystem (v0.33.0 – v0.46.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.33.0](roadmap/v0.33.0.md) | Complete docs site rebuild — CI harness, eight feature-deep-dive chapters, operations guide, reference section, and content governance | ✅ Released | Large | [Full details](roadmap/v0.33.0-full.md) |
| [v0.34.0](roadmap/v0.34.0.md) | Early fixpoint termination for bounded hierarchies (20–50% faster SPARQL property paths); Delete-Rederive for write-correct materialized predicates | ✅ Released | Medium | [Full details](roadmap/v0.34.0-full.md) |
| [v0.35.0](roadmap/v0.35.0.md) | Background-worker parallelism for independent Datalog rules (2–5× faster materialization); add/remove rules without full recompute | ✅ Released | Medium | [Full details](roadmap/v0.35.0-full.md) |
| [v0.36.0](roadmap/v0.36.0.md) | Leapfrog Triejoin for cyclic SPARQL patterns (10×–100× speedup); Datalog^L monotone lattice aggregation | ✅ Released | Medium | [Full details](roadmap/v0.36.0-full.md) |
| [v0.37.0](roadmap/v0.37.0.md) | Fix HTAP merge race, rare-predicate promotion race, dictionary cache rollback; eliminate all hard panics; add GUC validators | ✅ Released | Large | [Full details](roadmap/v0.37.0-full.md) |
| [v0.38.0](roadmap/v0.38.0.md) | Split god-module, PredicateCatalog trait, batch encoding, SCBD, SPARQL Update completeness, SHACL hints in planner | ✅ Released | Large | [Full details](roadmap/v0.38.0-full.md) |
| [v0.39.0](roadmap/v0.39.0.md) | REST API exposing all 27 Datalog SQL functions in `pg_ripple_http`: rule management, inference, goal queries, constraints, admin | ✅ Released | Small | [Full details](roadmap/v0.39.0-full.md) |
| [v0.40.0](roadmap/v0.40.0.md) | Server-side SPARQL cursors, `explain_sparql()`, `explain_datalog()`, OpenTelemetry tracing, resource governors | ✅ Released | Large | [Full details](roadmap/v0.40.0-full.md) |
| [v0.41.0](roadmap/v0.41.0.md) | Complete W3C SPARQL 1.1 test suite harness with parallelized execution; 3,000+ tests in < 2 min CI | ✅ Released | Medium | [Full details](roadmap/v0.41.0-full.md) |
| [v0.42.0](roadmap/v0.42.0.md) | Multi-worker HTAP merge, FedX-style federation planner, parallel SERVICE, live RDF change subscriptions | ✅ Released | Very Large | [Full details](roadmap/v0.42.0-full.md) |
| [v0.43.0](roadmap/v0.43.0.md) | Apache Jena edge-case tests (~1,000) and WatDiv scale-correctness benchmark (10M+ triples, star/chain/snowflake/complex patterns) | ✅ Released | Medium | [Full details](roadmap/v0.43.0-full.md) |
| [v0.44.0](roadmap/v0.44.0.md) | LUBM OWL RL inference correctness across 14 canonical queries; Datalog API validation sub-suite | ✅ Released | Small | [Full details](roadmap/v0.44.0-full.md) |
| [v0.45.0](roadmap/v0.45.0.md) | Close remaining SHACL Core gaps, harden parallel Datalog strata rollback, add crash-recovery scenarios, standardise migration documentation | ✅ Released | Small | [Full details](roadmap/v0.45.0-full.md) |
| [v0.46.0](roadmap/v0.46.0.md) | `proptest` for SPARQL/dictionary invariants, fuzz federation result decoder, W3C OWL 2 RL test suite in CI, TopN push-down, BSBM regression gate | ✅ Released | Medium | [Full details](roadmap/v0.46.0-full.md) |

### Architecture, Observability & Production (v0.47.0 – v0.54.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.47.0](roadmap/v0.47.0.md) | Fix parsed-but-not-checked SHACL constraints, wire `preallocate_sid_ranges()`, finish `sparql/translate/` module split, add fuzz targets, GUC validators, security hygiene | ✅ Released | Large | [Full details](roadmap/v0.47.0-full.md) |
| [v0.48.0](roadmap/v0.48.0.md) | Complete all 35 SHACL Core constraints, close OWL 2 RL rule set, add SPARQL Update MOVE/COPY/ADD, fix SPARQL-star variable patterns, WatDiv baselines | ✅ Released | Medium | [Full details](roadmap/v0.48.0-full.md) |
| [v0.49.0](roadmap/v0.49.0.md) | `sparql_from_nl()` NL-to-SPARQL via configurable LLM endpoint; embedding-based entity alignment with `suggest_sameas()` | ✅ Released | Small | [Full details](roadmap/v0.49.0-full.md) |
| [v0.50.0](roadmap/v0.50.0.md) | `explain_sparql(analyze:=true)` interactive query debugger; `rag_context()` RAG pipeline | ✅ Released | Small | [Full details](roadmap/v0.50.0-full.md) |
| [v0.51.0](roadmap/v0.51.0.md) | Non-root container, SPARQL DoS protection, HTTP streaming, OTLP, pg_upgrade compat, CDC docs, conformance gate flips | ✅ Released | Large | [Full details](roadmap/v0.51.0-full.md) |
| [v0.52.0](roadmap/v0.52.0.md) | JSON→RDF helpers, CDC→outbox bridge worker, CDC bridge triggers, JSON-LD event serializer, dedup keys, vocabulary templates, pg-trickle runtime detection | ✅ Released | Medium | [Full details](roadmap/v0.52.0-full.md) |
| [v0.53.0](roadmap/v0.53.0.md) | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, CDC lifecycle events, architecture module splits, OpenAPI spec | ✅ Released | Medium | [Full details](roadmap/v0.53.0-full.md) |
| [v0.54.0](roadmap/v0.54.0.md) | PG18 logical-decoding RDF replication, Helm chart, CloudNativePG image volume, merge/vector-index performance baselines | ✅ Released | Medium | [Full details](roadmap/v0.54.0-full.md) |

### Quality, Security & Ecosystem (v0.55.0 – v0.59.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.55.0](roadmap/v0.55.0.md) | Security hardening (SSRF allowlist, HTAP race fix), error-catalog reconciliation, tombstone GC, named-graph RLS, read-replica routing, VoID, SPARQL Service Description, OpenAPI spec | ✅ Released | Large | [Full details](roadmap/v0.55.0-full.md) |
| [v0.56.0](roadmap/v0.56.0.md) | GeoSPARQL 1.1, SPARQL Entailment Regime tests, Arrow/Flight export, federation circuit breaker, SPARQL audit log, dead-code audit, deprecated GUC removal | ✅ Released | Medium | [Full details](roadmap/v0.56.0-full.md) |
| [v0.57.0](roadmap/v0.57.0.md) | OWL 2 EL/QL reasoning profiles, KG embeddings (TransE/RotatE), entity alignment, LLM SPARQL repair, ontology mapping, multi-tenant graph isolation, columnar VP, adaptive indexing | ✅ Released | Very Large | [Full details](roadmap/v0.57.0-full.md) |
| [v0.58.0](roadmap/v0.58.0.md) | Temporal RDF queries (`point_in_time`), SPARQL-DL, Citus horizontal sharding, PROV-O graph provenance, v1.0.0 readiness integration suite | ✅ Released | Large | [Full details](roadmap/v0.58.0-full.md) |
| [v0.59.0](roadmap/v0.59.0.md) | Citus SPARQL shard-pruning for bound subjects (10–100× speedup), rebalance NOTIFY coordination, `explain_sparql()` Citus section | ✅ Released | Medium | [Full details](roadmap/v0.59.0-full.md) |

### Pre-1.0 Hardening & Ecosystem (v0.60.0 – v0.63.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.60.0](roadmap/v0.60.0.md) | Close all v1.0.0 blockers: HTAP cutover atomic swap, Actions SHA pinning, SECURITY DEFINER CI lint, new fuzz targets (GeoSPARQL WKT, R2RML, LLM prompt), `/ready` endpoint, `geof:distance`, merge-throughput trend artifact, pg_dump round-trip CI test, LangChain tool package | Released ✅ | Large | [Full details](roadmap/v0.60.0-full.md) |
| [v0.61.0](roadmap/v0.61.0.md) | Ecosystem depth: per-named-graph RLS, `explain_inference()` derivation tree, GDPR `erase_subject()`, dbt adapter, SHACL-AF rule execution, OTLP traceparent propagation, richer federation call stats; Citus object-based shard pruning and direct-shard bulk-load path | Released ✅ | Large | [Full details](roadmap/v0.61.0-full.md) |
| [v0.62.0](roadmap/v0.62.0.md) | Query frontier: Apache Arrow Flight bulk export, WCOJ planner integration, visual graph explorer in `pg_ripple_http`, `clippy --deny warnings` CI gate; Citus property-path push-down, `vp_rare` cold-entry archival, tiered dictionary cache, distributed inference dispatch, live shard rebalance, multi-hop pruning carry-forward | Released ✅ | Very Large | [Full details](roadmap/v0.62.0-full.md) |
| [v0.63.0](roadmap/v0.63.0.md) | SPARQL CONSTRUCT writeback rules (raw-to-canonical pipelines, incremental delta maintenance, Delete-Rederive, pipeline stratification); Citus scalability: SERVICE result shard pruning, streaming fan-out cursor, HyperLogLog `COUNT(DISTINCT)`, batched dictionary encoding, per-worker SID tables, non-blocking VP promotion, per-graph RLS CI gate, per-worker BRIN summarise | Released ✅ | Large | [Full details](roadmap/v0.63.0-full.md) |

### Assessment Remediation & Release Trust (v0.64.0 – v0.69.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|--------------|
| [v0.64.0](roadmap/v0.64.0.md) | Release truth and safety freeze: feature-status API, deep readiness, immutable GitHub Actions, digest-scanned Docker releases, documentation truth pass, release evidence dashboard foundation | Released ✅ | Large | [Full details](roadmap/v0.64.0-full.md) |
| [v0.65.0](roadmap/v0.65.0.md) | CONSTRUCT writeback correctness closure: real delta maintenance, HTAP-aware retraction, exact provenance capture, parameterized rule catalog writes, full CWB behavior test matrix | ✅ Released | Very Large | [Full details](roadmap/v0.65.0-full.md) |
| [v0.66.0](roadmap/v0.66.0.md) | Streaming and distributed reality: true SPARQL cursors, signed Arrow IPC export, explainable WCOJ mode, integrated Citus pruning/HLL/BRIN/RLS/promotion paths | ✅ Released | Very Large | [Full details](roadmap/v0.66.0-full.md) |
| [v0.67.0](roadmap/v0.67.0.md) | Assessment 9 critical remediation and production evidence: storage mutation journal, VP table RLS coverage, Arrow Flight security/correctness, fail-closed release-truth gates, soak tests, benchmark baselines, security audit | Released ✅ | Very Large | [Full details](roadmap/v0.67.0-full.md) |
| [v0.68.0](roadmap/v0.68.0.md) | Distributed scalability, streaming completion and fuzz hardening: CONSTRUCT cursor streaming, Citus HLL translation, SERVICE pruning, nonblocking VP promotion, scheduled fuzz CI | Released ✅ | Large | [Full details](roadmap/v0.68.0-full.md) |
| [v0.69.0](roadmap/v0.69.0.md) | Module architecture restructuring: split sparql/mod.rs, pg_ripple_http/main.rs, construct_rules.rs, and storage/mod.rs along single-responsibility boundaries | Released ✅ | Large | [Full details](roadmap/v0.69.0-full.md) |

### Assessment 10 Remediation & Production Hardening (v0.70.0 – v0.73.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|-------------- |
| [v0.70.0](roadmap/v0.70.0.md) | Assessment 10 critical remediation: bulk-load mutation journal, per-statement flush, fail-closed evidence gate, SHACL doc truth, README versioning, RLS SQL quoting, SBOM currency | Released ✅ | Large | [Full details](roadmap/v0.70.0-full.md) |
| [v0.71.0](roadmap/v0.71.0.md) | Arrow Flight streaming validation, Citus multi-node integration test, pg_ripple_http/pg_ripple compatibility matrix, HLL accuracy docs, SERVICE shard benchmark | Released ✅ | Large | [Full details](roadmap/v0.71.0-full.md) |
| [v0.72.0](roadmap/v0.72.0.md) | Architecture and protocol hardening: mutation journal SAVEPOINT safety, plan cache docs, continued module split, ConstructTemplate proptest, SPARQL Update fuzz, conformance gate promotion, Arrow Flight replay protection | Planned | Large | [Full details](roadmap/v0.72.0-full.md) |
| [v0.73.0](roadmap/v0.73.0.md) | SPARQL 1.2 tracking, live SPARQL subscription API (WebSocket/SSE), feature status taxonomy, CONTRIBUTING.md, Helm chart SHA pin, R2RML scope docs | Planned | Large | [Full details](roadmap/v0.73.0-full.md) |

#### PLAN_OVERALL_ASSESSMENT_10 coverage map

Every finding and recommendation from [plans/PLAN_OVERALL_ASSESSMENT_10.md](plans/PLAN_OVERALL_ASSESSMENT_10.md) is assigned to one or more post-v0.69.0 roadmap milestones:

| Assessment finding | Roadmap coverage |
|---|---|
| CF-1: Bulk load bypasses mutation journal | v0.70.0 BULK-01 |
| CF-2: Per-triple `flush()` runs CWB pipeline O(quads × rules) | v0.70.0 FLUSH-01 |
| CF-3: SHACL-SPARQL documentation overclaim (third assessment) | v0.70.0 SHACL-DOC-01 |
| CF-4: `feature_status()` cites three non-existent evidence files | v0.70.0 GATE-03 |
| HF-1: Citus integration test claimed but missing | v0.71.0 CITUS-INT-01 |
| HF-2: README two releases stale | v0.70.0 README-01, README-02 |
| HF-3: Arrow Flight streaming behavior unverified | v0.71.0 FLIGHT-STREAM-01 |
| HF-4: RLS DDL interpolates role names without quoting | v0.70.0 RLS-SQL-01 |
| HF-5: SBOM 18 releases stale | v0.70.0 SBOM-02 |
| HF-6: Plan cache key omits graph/security context | v0.72.0 CACHE-01 |
| HF-7: Mutation journal not safe under SAVEPOINT/ROLLBACK | v0.72.0 XACT-01 |
| HF-8: pg_ripple_http independently versioned, no compatibility matrix | v0.71.0 COMPAT-01 |
| MF-1: cwb_write_path_equivalence.sql cannot prove bulk-load arm | v0.70.0 BULK-01 (test extension) |
| MF-2: Legacy .sh gate scripts coexist with Python replacements | v0.70.0 GATE-04 |
| MF-4: No regression test for `recover_interrupted_promotions()` | v0.70.0 TEST-03 |
| MF-5: merge_throughput_history.csv has only one row | v0.72.0 BENCH-03 |
| MF-6: v067_features.sql and v069_features.sql missing | v0.70.0 TEST-01, TEST-02 |
| MF-7: Citus HLL accuracy bounds undocumented | v0.71.0 HLL-DOC-01 |
| MF-8: Citus SERVICE annotation effectiveness not benchmarked | v0.71.0 CITUS-BENCH-01 |
| MF-10: src/lib.rs and src/storage/mod.rs still >500 lines | v0.72.0 MOD-01 |
| MF-11: No proptest for ConstructTemplate | v0.72.0 PROPTEST-01 |
| MF-12: Fuzz corpus may not cover SPARQL Update | v0.72.0 FUZZ-02 |
| MF-13: Streaming metrics not exposed via /metrics | v0.72.0 OBS-02 |
| MF-14: Three batch-size GUCs undocumented relative to each other | v0.72.0 GUC-DOC-01 |
| MF-15: [Unreleased] section lacks contributor guidance | v0.73.0 CONTRIB-01 |
| MF-16: roadmap/v0.67.0.md still marked Planned | v0.70.0 DOC-01 |
| MF-17: Datalog/CWB interaction undocumented and untested | v0.72.0 CWB-DATALOG-01 |
| MF-18: is_citus_worker_endpoint() URL parsing untested | v0.72.0 CITUS-URL-01 |
| MF-19: Arrow Flight ticket has no replay protection | v0.72.0 FLIGHT-NONCE-01 |
| MF-20: feature_status() taxonomy has no promotion criteria | v0.73.0 TAXONOMY-01 |
| Dimension 12 / item 17: SPARQL 1.2 tracking | v0.73.0 SPARQL12-01 |
| Dimension 12 / item 18: WebSocket/SSE live subscription API | v0.73.0 SUB-01 |
| Low: CONTRIBUTING.md missing | v0.73.0 CONTRIB-01 |
| Low: Helm chart uses latest tag | v0.73.0 HELM-01 |
| Low: src/llm/ and src/kge.rs not in feature_status() | v0.73.0 FEATURE-STATUS-02 |
| Low: src/r2rml.rs scope unclear | v0.73.0 R2RML-DOC-01 |
| Low: pg_ripple.control comment stale | v0.73.0 CONTROL-01 |
| v1.0.0 readiness: Jena ≥95%, security audit, threat model, clean-package install | v1.0.0 PROD-01 through PROD-05 |

#### PLAN_OVERALL_ASSESSMENT_8 coverage map

Every finding and recommendation from [plans/PLAN_OVERALL_ASSESSMENT_8.md](plans/PLAN_OVERALL_ASSESSMENT_8.md) is assigned to one or more post-v0.63.0 roadmap milestones:

| Assessment area | Roadmap coverage |
|---|---|
| C1: CONSTRUCT writeback is not incremental | v0.65.0 CWB-FIX-01, CWB-FIX-02, CWB-FIX-08 |
| C2: promoted HTAP predicate retraction can fail | v0.65.0 CWB-FIX-03, CWB-FIX-06, TEST-3 |
| C3 and S1: mutable GitHub Actions / missing SHA pinning | v0.64.0 TRUTH-03, TEST-1 |
| C4, S2, P5: Arrow Flight is stubbed and tickets are unsigned | v0.66.0 FLIGHT-01, FLIGHT-02, OBS-01 |
| H1, P4, SC4: WCOJ is planner guidance, not true triejoin | v0.64.0 TRUTH-05, v0.66.0 WCOJ-01 |
| H2, P2, P3, S4: v0.63 Citus claims are not fully wired | v0.64.0 TRUTH-01/TRUTH-05, v0.66.0 CITUS-01 through CITUS-06 |
| H3: SPARQL cursors materialize full result sets | v0.66.0 STREAM-01, STREAM-02, TEST-1 |
| H4 and SC1: SHACL-SPARQL rule support is overstated | v0.64.0 TRUTH-05, v0.65.0 CWB-FIX-09 |
| H5: Docker release continues after build/push failure | v0.64.0 TRUTH-04, TEST-4 |
| P1: construct-rule provenance over-attributes target triples | v0.65.0 CWB-FIX-04, PERF-1 |
| S3 and A2: construct-rule catalog SQL uses manual escaping | v0.65.0 CWB-FIX-05 |
| SC2: v0.63 CWB test matrix is not implemented | v0.65.0 CWB-FIX-08, TEST-1 |
| SC3: version/date/API narrative drift | v0.64.0 TRUTH-05, TRUTH-07 |
| A1, A3, A4: release claims need evidence, degradation semantics, and no dead-helper claims | v0.64.0 TRUTH-01, TRUTH-06, TRUTH-10 |
| Operational and observability gaps | v0.65.0 CWB-FIX-07, CWB-FIX-10; v0.66.0 OBS-01; v0.67.0 PROD-07 |
| Documentation and developer-experience gaps | v0.64.0 TRUTH-05, TRUTH-07, TRUTH-08, UX-1 through UX-4 |
| Test coverage and validation gaps | v0.64.0 TEST-1 through TEST-4; v0.65.0 TEST-1 through TEST-4; v0.66.0 TEST-1 through TEST-4; v0.67.0 TEST-1 through TEST-4 |
| Roadmap gap: Release Truth Gate | v0.64.0 TRUTH-06, TRUTH-09 |
| Roadmap gap: Feature Status SQL API and deep readiness | v0.64.0 TRUTH-01, TRUTH-02 |
| Roadmap gap: Incremental Maintenance Unification | v0.65.0 CWB-FIX-01, CWB-FIX-09 |
| Roadmap gap: Distributed Execution Contract | v0.66.0 CITUS-01 through CITUS-06, OBS-01 |
| Roadmap gap: Streaming Contract | v0.66.0 STREAM-01, STREAM-02, FLIGHT-02 |
| Roadmap gap: Security Hardening Track | v0.64.0 TRUTH-03/TRUTH-04, v0.66.0 FLIGHT-01, v0.67.0 PROD-02 |
| Recommended feature: Release Evidence Dashboard | v0.64.0 TRUTH-09, v0.67.0 PROD-05 |
| Recommended feature: Canonical Graph Pipeline UI/API foundation | v0.65.0 CWB-FIX-07, CWB-FIX-10 |

#### PLAN_OVERALL_ASSESSMENT_9 coverage map

Every finding and recommendation from [plans/PLAN_OVERALL_ASSESSMENT_9.md](plans/PLAN_OVERALL_ASSESSMENT_9.md) is assigned to one or more post-v0.66.0 roadmap milestones:

| Assessment finding | Roadmap coverage |
|---|---|
| CF-1: CONSTRUCT writeback bypasses SPARQL Update and bulk-load paths | v0.67.0 MJOURNAL-01, MJOURNAL-02, MJOURNAL-03 |
| CF-2: Graph-level RLS protects only `_pg_ripple.vp_rare` | v0.67.0 RLS-01, RLS-02 |
| CF-3: Arrow Flight accepts unsigned tickets and exports stale buffered data | v0.67.0 FLIGHT-SEC-01, FLIGHT-SEC-02, FLIGHT-SEC-03 |
| CF-4: Release-truth gates can pass while checking nothing | v0.67.0 GATE-01, GATE-02 |
| HF-1: Feature-status evidence contains stale or nonexistent references | v0.67.0 GATE-02, GATE-03 |
| HF-2: Documentation and version narrative inconsistent at v0.66.0 | v0.67.0 GATE-03 |
| HF-3: Benchmark workflow invocation broken, failures suppressed | v0.67.0 BENCH-01, BENCH-02 |
| HF-4: Scheduled fuzz workflow absent | v0.68.0 FUZZ-01 |
| HF-5: Streaming observability counters wired to dead-code stubs | v0.67.0 MJOURNAL-02 (stats wiring), FLIGHT-SEC-02 (HTTP Arrow metrics) |
| HF-6: SBOM metadata stale by 15 releases | v0.67.0 SBOM-01 |
| HF-7: `construct_rules.rs` contains a production `panic!` | v0.67.0 PANIC-01 |
| HF-8: Architecture — side effects attached to wrappers, not storage contracts | v0.67.0 MJOURNAL-01/02; v0.69.0 ARCH-01 through ARCH-04 |
| Action item 11 (S): README/CHANGELOG/ROADMAP/implementation_plan truth pass | v0.67.0 GATE-03 |
| Action item 12 (M): SBOM regeneration in release CI | v0.67.0 SBOM-01 |
| Action item 13 (M): Fix benchmark workflow | v0.67.0 BENCH-01 |
| Action item 14 (L): Scheduled performance workflow with trend artifacts | v0.67.0 BENCH-02 |
| Action item 15 (M): Replace `construct_rules.rs` panic | v0.67.0 PANIC-01 |
| Action item 16 (L): Streaming CONSTRUCT Turtle/JSON-LD iterators | v0.68.0 STREAM-01 |
| Action item 17 (L): Citus HLL aggregate translation with exact fallback | v0.68.0 CITUS-HLL-01 |
| Action item 18 (L): Citus SERVICE and multihop pruning in SPARQL translator | v0.68.0 CITUS-SVC-01 |
| Action item 19 (XL): Nonblocking VP promotion with shadow tables | v0.68.0 PROMO-01 |
| Action item 20 (M): Scheduled fuzz workflows for all 12 targets | v0.68.0 FUZZ-01 |
| Action item 21 (L): Split large modules along contract boundaries | v0.69.0 ARCH-01 through ARCH-05 |
| Action item 22 (XL): Production release checklist artifacts | v1.0.0 PROD-01 through PROD-05 |

### Stable Release & Ecosystem (v1.0.0 – v1.1.0)

| Version | Theme | Status | Scope | Full details |
|---------|-------|--------|-------|-------------- |
| [v1.0.0](roadmap/v1.0.0-full.md) | Production hardening: 72-hour continuous load test, third-party security audit, API stability guarantee, documentation final audit and freeze, public BSBM/WatDiv benchmark results published | Planned | Medium | [Full details](roadmap/v1.0.0-full.md) |
| [v1.1.0](roadmap/v1.1.0.md) | Post-1.0 ecosystem: Cypher/GQL read-only transpiler (`MATCH … RETURN`) + write operations (`CREATE`/`SET`/`DELETE`), Jupyter SPARQL kernel, LangChain/LlamaIndex tool packages, Kafka CDC sink, materialized SPARQL views, dbt adapter | Planned | Large | [Full details](roadmap/v1.1.0-full.md) |

## How these versions fit together

```
v0.1.0–v0.5.1  ─── Foundation: VP storage, dictionary encoding, SPARQL engine, RDF-star, bulk loading
       │
v0.6–v0.10     ─── Storage architecture: HTAP delta/main split, SHACL validation, Datalog reasoning engine
       │
v0.11–v0.20    ─── Query completeness: SPARQL views, Update, Protocol, federation, JSON-LD, W3C conformance baseline
       │
v0.21–v0.32    ─── Correctness & Datalog: built-in functions, SHACL completion, magic sets, well-founded semantics, entity resolution
       │
v0.33–v0.46    ─── Scale & ecosystem: docs site, parallel stratum eval, WCO joins, full conformance suites (SPARQL 1.1, WatDiv, Jena, LUBM, OWL 2 RL)
       │
v0.47–v0.51    ─── Architecture hardening: dead-code wiring, SHACL completeness, streaming results, AI/LLM integration, security hardening
       │
v0.52–v0.54    ─── Integration: pg-trickle relay, OpenAPI, logical replication, Helm chart
       │
v0.55–v0.56    ─── Quality & security: SSRF allowlist, error-catalog, GeoSPARQL, Arrow/Flight, audit log
       │
v0.57–v0.59    ─── Reasoning & sharding: OWL 2 EL/QL, KG embeddings, temporal queries, Citus sharding & shard-pruning, PROV-O
       │
v0.60          ─── Production hardening sprint: HTAP atomic swap, Actions SHA pinning, SECURITY DEFINER lint,
               │   new fuzz targets, geof:distance, pg_dump round-trip CI test
       │
v0.61          ─── Ecosystem depth: per-graph RLS, explain_inference, GDPR erasure, dbt,
               │   SHACL-AF execution, OTLP traceparent, richer federation call stats;
               │   Citus object shard pruning, direct bulk-load
       │
v0.62          ─── Query frontier: Arrow Flight export, WCOJ planner integration, visual graph explorer;
               │   Citus property-path push-down, vp_rare archival, tiered dict cache,
               │   distributed inference dispatch, live shard rebalance, multi-hop pruning
       │
v0.63          ─── SPARQL CONSTRUCT writeback rules: raw-to-canonical pipelines,
               │   incremental delta maintenance, Delete-Rederive, pipeline stratification;
               │   Citus: SERVICE shard pruning, streaming fan-out, HyperLogLog COUNT(DISTINCT),
               │   batched dict encoding, per-worker SID tables, non-blocking VP promotion
       │
v0.64          ─── Release truth and safety freeze: feature_status, deep readiness,
               │   immutable GitHub Actions, digest-scanned Docker release, documentation truth pass
       │
v0.65          ─── CONSTRUCT writeback correctness closure: real delta maintenance,
               │   HTAP-aware retraction, exact provenance, full behavior test matrix
       │
v0.66          ─── Streaming and distributed reality: true cursors, signed Arrow IPC export,
               │   explainable WCOJ mode, integrated Citus pruning/HLL/BRIN/RLS/promotion paths
       │
v0.67          ─── Assessment 9 critical remediation: storage mutation journal,
               │   VP table RLS coverage, Arrow Flight security/correctness,
               │   fail-closed release-truth gates, soak tests, benchmark baselines
       │
v0.68          ─── Distributed scalability and streaming completion: CONSTRUCT cursor
               │   streaming, Citus HLL translation, SERVICE pruning, nonblocking VP
               │   promotion, scheduled fuzz CI for all 12 targets
       │
v0.69          ─── Module architecture restructuring: split sparql/mod.rs,
               │   pg_ripple_http/main.rs, construct_rules.rs, storage/mod.rs
       │
v0.70          ─── Assessment 10 critical remediation: bulk-load mutation journal,
               │   per-statement flush, fail-closed evidence gate, SHACL doc truth,
               │   README versioning, RLS SQL quoting, SBOM currency
       │
v0.71          ─── Arrow Flight streaming validation, Citus multi-node integration,
               │   compatibility matrix, HLL accuracy docs, SERVICE benchmark
       │
v0.72          ─── Architecture hardening: mutation journal SAVEPOINT safety,
               │   plan cache docs, module split, ConstructTemplate proptest,
               │   SPARQL Update fuzz, conformance gate promotion, replay protection
       │
v0.73          ─── SPARQL 1.2 tracking, live subscription API (SSE/WebSocket),
               │   feature taxonomy, CONTRIBUTING.md, Helm chart SHA, R2RML docs
       │
v1.0.0         ─── Stable release: 72-hour continuous load test, third-party security audit, documentation freeze, public benchmarks
       │
v1.1           ─── Post-stable: Cypher/GQL transpiler (read-only + write ops), Jupyter kernel, LangChain/LlamaIndex tools, Kafka CDC sink, materialized SPARQL views, dbt adapter
```

## Summary

v0.1.0 through v0.5.1 build the complete core storage and query engine.
v0.6.0 through v0.10.0 add the HTAP architecture, SHACL validation, and the full
Datalog reasoning engine.

v0.11.0 through v0.20.0 complete the SPARQL query and update surfaces and establish
the W3C conformance baseline.

v0.21.0 through v0.32.0 harden correctness and deliver production-grade Datalog
optimizations including magic sets, semi-naive evaluation, well-founded semantics,
and entity resolution.

v0.33.0 through v0.46.0 deliver the documentation site, parallel evaluation,
worst-case optimal joins, full conformance suites, and the AI/LLM integration layer.

v0.47.0 through v0.51.0 complete the architecture refactor and shipping hardening
required for a production release. v0.52.0 through v0.54.0 deliver the pg-trickle
relay integration and high-availability story.

v0.55.0 through v0.56.0 address all open security findings from
PLAN_OVERALL_ASSESSMENT_6 (SSRF allowlist, error-catalog drift) and add GeoSPARQL
1.1, federation circuit breaker, and the SPARQL audit log.

v0.57.0 through v0.59.0 extend the reasoning platform to OWL 2 EL/QL, add KG
embeddings, entity alignment, temporal RDF queries, Citus sharding with shard-pruning,
and PROV-O provenance.

v0.60.0 through v0.62.0 are the pre-1.0 hardening and ecosystem sprint: v0.60.0
closes the remaining v1.0.0 blockers identified in PLAN_OVERALL_ASSESSMENT_7 (HTAP
atomic swap, CI supply-chain hardening, fuzz target gaps, `geof:distance`);
v0.61.0 delivers ecosystem depth (per-graph RLS, inference explainability, GDPR
erasure, dbt adapter, SHACL-AF execution, richer federation call stats); v0.62.0
delivers the query frontier (Arrow Flight bulk export, WCOJ planner integration,
visual graph explorer) plus six Citus scalability improvements (property-path
push-down, `vp_rare` cold-entry archival, tiered dictionary cache, distributed
inference dispatch, live shard rebalance, multi-hop pruning carry-forward).

v0.63.0 introduces SPARQL CONSTRUCT writeback rules: any CONSTRUCT query can be
registered as a persistent rule that writes its derived triples directly into a target
named graph inside the VP storage layer and maintains them incrementally — inserts
trigger a delta derivation path, deletes trigger Delete-Rederive retraction —
enabling raw-to-canonical model pipelines where the canonical graph is always
consistent with the latest raw data. v0.63.0 also delivers eight Citus scalability
improvements (CITUS-30–37): SERVICE result shard pruning, streaming coordinator
fan-out via SPARQL cursor, approximate `COUNT(DISTINCT)` via HyperLogLog, batched
dictionary encoding, per-worker statement-ID local tables, non-blocking VP promotion
via shadow-table pattern, per-graph RLS propagation CI gate, and per-worker BRIN
summarise after merge.

v0.64.0 through v0.69.0 convert the findings from PLAN_OVERALL_ASSESSMENT_8 and
PLAN_OVERALL_ASSESSMENT_9 into explicit roadmap work: v0.64.0 adds the
truth-in-release guardrails (feature status, deep readiness, immutable CI actions,
release digest scanning, and documentation correction); v0.65.0 closes CONSTRUCT
writeback correctness (delta maintenance, HTAP-aware retraction, exact provenance,
and the full behavior test matrix); v0.66.0 makes the streaming and distributed
claims real or explicitly labels them as planner hints/stubs/helpers (true SPARQL
cursors, signed Arrow IPC export, explainable WCOJ mode, and integrated Citus
pruning/HLL/BRIN/RLS/promotion paths); v0.67.0 addresses all four Critical findings
from Assessment 9 (storage mutation journal closing all CONSTRUCT writeback bypass
paths, VP table RLS coverage, Arrow Flight ticket security and tombstone-aware
export, fail-closed release-truth scripts) and gathers production evidence (soak
tests, audit or threat-model closure, public benchmarks, upgrade/backup acceptance,
and mandatory release evidence artifacts); v0.68.0 completes the distributed
execution and streaming contracts that were labelled partial or planned in v0.62–v0.66
(true CONSTRUCT streaming, Citus HLL aggregate translation, Citus SERVICE pruning,
nonblocking VP promotion, and scheduled fuzz CI for all twelve targets); and v0.69.0
restructures the large source modules along single-responsibility boundaries to make
the codebase maintainable for a v1.0.0 API freeze.

v0.70.0 through v0.73.0 address the findings from PLAN_OVERALL_ASSESSMENT_10:
v0.70.0 closes all four Critical findings (bulk-load mutation journal bypass,
per-triple flush overhead, missing evidence file citations, SHACL-SPARQL docs)
and six High/Medium items (README stale, RLS DDL quoting, SBOM currency, missing
test files, legacy script cleanup, roadmap status correction); v0.71.0 validates
the Arrow Flight streaming contract with an RSS-bounded 10 M-row integration test,
implements the previously-missing Citus RLS propagation integration test, adds an
extension/HTTP companion compatibility matrix, and documents HLL accuracy bounds;
v0.72.0 hardens the mutation journal against PostgreSQL SAVEPOINT/ROLLBACK via
xact callbacks, continues the v0.69.0 module split for the three largest remaining
files, adds a ConstructTemplate proptest suite and a SPARQL Update fuzz target,
promotes W3C conformance and BSBM gates to required CI, adds Arrow Flight replay
protection, and tests the Datalog→CWB interaction chain; v0.73.0 tracks SPARQL 1.2,
delivers a live SPARQL subscription API prototype via SSE, and completes the
ecosystem hardening items (CONTRIBUTING.md, Helm chart SHA pinning, feature status
taxonomy, and R2RML scope documentation).

v1.0.0 is the stable release: a 72-hour continuous load test, a third-party security
audit, documentation final audit and freeze, an API stability guarantee, and public
BSBM/WatDiv benchmark results.

v1.1.0 delivers post-stable improvements: Cypher/GQL transpiler (read-only and write
operations), Jupyter SPARQL kernel, LangChain/LlamaIndex tool packages, Kafka CDC
sink, materialized SPARQL views, and a dbt adapter.

