# pg_ripple — Roadmap Feature Descriptions (v0.51.0–v0.53.0)

This document describes the three upcoming releases in plain language. The goal is to give a clear picture of what each version delivers, why it matters, and how much work is involved — without requiring deep technical knowledge.

---

## What is pg_ripple?

pg_ripple is a database extension for PostgreSQL that lets you store and query knowledge graphs — structured networks of facts expressed as subject → predicate → object triples. It supports SPARQL (the standard query language for knowledge graphs), SHACL (data validation rules), and Datalog (inference rules). It also integrates with AI/LLM tooling for question-answering over graph data.

---

## v0.51.0 — Security Hardening & Production Readiness

**Effort estimate: 8–10 person-weeks**

**Goal:** Close every gap between "ready for developers" and "ready for production". This release is about trust and safety — ensuring that organisations can run pg_ripple in real environments without worrying about security vulnerabilities, server crashes from bad queries, or operational surprises.

### Security & Container Safety

**The container no longer runs as root.**
The current Docker image runs the database process with full administrator privileges inside the container. This is a known security risk. In v0.51.0 the container runs as a restricted user (`postgres`) instead. Two image variants will be published: one for local development, one for production (with password authentication required).

**Protection against malicious SPARQL queries.**
A carefully crafted SPARQL query can be written to be deeply nested or have an enormous number of patterns — like a puzzle designed to exhaust the server. pg_ripple gains configurable limits on query complexity. If a query exceeds these limits it is rejected immediately with a clear error message, rather than hanging or crashing the server.

**Certificate pinning for outbound HTTP connections.**
When pg_ripple connects to external SPARQL endpoints (federation), it can be configured to only accept connections to servers whose TLS certificate matches a known fingerprint. This prevents man-in-the-middle attacks in controlled environments.

**Automated dependency vulnerability scanning (blocking).**
Weekly security scans of all Rust library dependencies already exist, but they are advisory only. In v0.51.0, any pull request that introduces a dependency with a known vulnerability will be blocked from merging automatically.

**Software Bill of Materials (SBOM).**
Every release will include a machine-readable inventory of all libraries pg_ripple depends on, enabling organisations to audit supply-chain risk and react quickly when a dependency vulnerability is announced.

**SQL injection prevention linter.**
An automated script runs in CI to detect patterns in the Rust code that could be unsafe when building SQL queries dynamically. Any new code matching these patterns will fail the build.

### HTTP Streaming

**Large query results are streamed, not buffered.**
Currently, when you run a CONSTRUCT or SELECT query via the HTTP service, pg_ripple collects all results in memory before sending them back. For large graphs this can exhaust available memory. In v0.51.0, the HTTP service gains a streaming endpoint (`POST /sparql/stream`) that sends results line by line as they are generated, using HTTP chunked transfer encoding — similar to how video streaming services send data progressively rather than all at once.

### Operational Readiness

**Documented upgrade path between PostgreSQL minor versions.**
When a new PostgreSQL 18.x patch is released, database administrators need to know how to upgrade without losing their data. v0.51.0 documents the supported upgrade matrix and includes an automated test that exercises the full upgrade procedure.

**Change data capture documentation.**
pg_ripple supports real-time notifications when triples change (CDC — Change Data Capture). v0.51.0 adds documentation for operators on how to tune the notification queue and what to do when a subscriber is slow.

**Automated release tooling.**
Releasing a new version involves many manual steps: bumping version numbers, writing changelog entries, creating database migration scripts. v0.51.0 automates these with a `just release VERSION` command and a GitHub Actions workflow.

**Backup and restore testing.**
An automated test verifies that a database containing pg_ripple data can be backed up with `pg_dump` and restored successfully, confirming data integrity across the full backup/restore cycle.

### Observability

**Real OpenTelemetry traces.**
pg_ripple can be configured to export detailed traces of query execution (parsing, planning, executing) to observability tools like Jaeger, Grafana Tempo, or Datadog. Previously the configuration option existed but was not actually connected to anything. In v0.51.0 it is fully wired and working.

**Per-predicate query statistics.**
A new function `pg_ripple.predicate_workload_stats()` returns a table showing how often each RDF predicate (relationship type) has been queried and merged. This helps operators understand workload distribution and tune storage.

**Extended query explain output.**
The `explain_sparql()` function gains buffer I/O statistics (cache hits, disk reads, dirty writes) when run with `analyze := true`, giving query optimizers more data to work with.

### Data Correctness

**Merge worker responds to shutdown signals correctly.**
The background process that merges new triples into the main storage currently uses a simple sleep loop for its idle backoff. This means it can take up to 30 seconds to shut down cleanly. v0.51.0 replaces this with a proper latch wait that responds immediately to shutdown signals.

**Storage cache invalidation on vacuum.**
When PostgreSQL performs a full vacuum on a vertical-partitioning table, the internal cache that maps predicate names to table identifiers can become stale. v0.51.0 registers a cache invalidation callback so this is handled automatically.

**Complete SHACL property path validation.**
SHACL supports complex property paths for validation (sequences, alternatives, wildcards like `*` and `+`). The code for handling these exists but is currently disabled. v0.51.0 enables and fully tests this functionality.

**Correct handling of RDF-star quoted triples in CONSTRUCT.**
A known edge case in SPARQL CONSTRUCT queries: ground RDF-star quoted triples (nested statements) were silently dropped from the output. v0.51.0 emits them correctly in N-Triples-star notation.

### Standards & Compliance

**Complete OWL 2 RL conformance.**
Four known failures remain in the OWL 2 RL reasoning test suite. v0.51.0 fixes them and promotes the conformance gate from advisory to blocking — meaning CI fails if the suite no longer passes at 100%.

**SPARQL CSV and TSV output formats.**
The W3C SPARQL specification defines four result formats: JSON, XML, CSV, and TSV. pg_ripple already supports the first two. v0.51.0 adds CSV and TSV, making it straightforward to pipe SPARQL query results directly into spreadsheet tools or data pipelines.

### Documentation

- Tuning guide: maps each configuration parameter to workload characteristics
- Worked examples: LLM-to-SPARQL workflow, multi-endpoint federation, CDC subscription patterns
- Updated developer guide to reflect the correct pgrx version (0.18, not 0.17)
- New `just docs-serve` recipe for the documentation site

---

## v0.52.0 — Developer Experience, Extended Standards & Architecture

**Effort estimate: 6–9 person-weeks**

**Goal:** Advance standards completeness, improve the developer experience with richer tooling, and keep the codebase maintainable as it grows. No security blockers remain after v0.51.0; this release is about quality, completeness, and long-term health.

### Developer Tooling

**Self-documenting HTTP API.**
The pg_ripple HTTP companion service gains an OpenAPI specification — a machine-readable description of every endpoint, its parameters, and its response format. This enables tools like Postman, Swagger UI, or code generators to work with the API automatically, and makes integration straightforward for teams that prefer a REST interface over direct SQL.

**Visual architecture diagram.**
A Mermaid diagram will be published in the documentation showing the complete data flow: from a client submitting a query, through the dictionary encoder, to the vertical-partitioning tables, through the SPARQL/Datalog/SHACL engines, to the serializers and federation layer. This makes onboarding new contributors significantly faster.

### Standards Completeness

**SHACL-SPARQL custom constraint rules.**
Standard SHACL constraints cover common cases (required fields, value types, cardinality), but cannot express complex business rules. SHACL-SPARQL extends this with `sh:SPARQLConstraintComponent`, allowing users to write a full SPARQL query as a validation rule. If the query returns results, the constraint is violated. Examples: "employees must have an email address if their employment date is before 2020", or "no two people in the same department may share a manager". This is a significant step up in data quality expressiveness.

**SHACL rule engine integration or clear error messages.**
SHACL also defines `sh:rule` (SHACL-AF), which lets users write inference rules. v0.52.0 either compiles these rules into pg_ripple's existing Datalog engine (preferred), or emits a clear, documented error code (`PT480`) when such rules are encountered, so users know what is and is not supported.

**`COPY rdf FROM` bulk loading.**
PostgreSQL's `COPY` command is the fastest way to load large datasets. v0.52.0 registers a custom handler so that standard PostgreSQL `COPY` syntax works directly with RDF files:

```sql
COPY pg_ripple.triples FROM '/path/to/data.nt' WITH (FORMAT 'ntriples');
COPY pg_ripple.triples FROM '/path/to/data.ttl' WITH (FORMAT 'turtle');
```

This is especially useful for initial data loads of hundreds of millions of triples.

### RAG & AI Pipeline Hardening

**Security and caching for the LLM question-answering pipeline.**
The `rag_context()` function (natural language → SPARQL → results for LLM context) gains two hardening features: input sanitisation to block prompt-injection attacks before the text reaches the language model, and a response cache so that repeated questions with the same schema don't incur redundant LLM API calls. A new `/rag` REST endpoint in the HTTP service exposes this capability to applications that prefer HTTP over SQL.

### Change Data Capture Improvements

**Richer CDC lifecycle event notifications.**
pg_ripple's change notification system currently sends an event when triples change. v0.52.0 adds a second notification channel for infrastructure events: when the background merge worker completes a merge cycle, or when a predicate is promoted from the rare-predicate store to its own vertical-partition table. This gives monitoring systems visibility into storage lifecycle, not just data changes.

### Robustness & Testing

**Broader automated test coverage.**
Three areas of the codebase gain automated fuzz testing (feeding random/malformed inputs to verify no crashes or data corruption):
- The RDF/XML parser and JSON-LD framing module
- The HTTP service request handler

Additionally, property-based tests (which generate thousands of structured test cases automatically) are enriched to cover Unicode edge cases, RTL text, emoji, zero-width characters, and complex SPARQL patterns.

**WatDiv conformance gate becomes blocking.**
The WatDiv benchmark (100 query templates over a synthetic e-commerce knowledge graph) has been running in CI in advisory mode. After two consecutive stable releases, v0.52.0 promotes this gate to blocking — any performance regression beyond the allowed threshold will fail the CI build.

### Code Quality & Architecture

**Breaking apart large source files.**
Three source files have grown to the point where they are difficult to navigate and review:
- `src/gucs.rs` (1,617 lines): all configuration parameters in one file — will be split into subsystem-specific files
- `src/datalog/mod.rs` (1,681 lines): the Datalog engine in one file — will be split into evaluator, transformer, and coordinator modules
- `src/sparql/translate/filter.rs` (901 lines): SPARQL filter handling — will be split into expression compilation and pattern dispatch

These are purely internal structural improvements; no behaviour changes.

**HTTP companion error handling fixes.**
Two code locations in the HTTP service currently use Rust's `unwrap()` — which crashes the process if something unexpected happens. v0.52.0 converts these to proper error propagation, returning HTTP 400/500 responses instead of panicking.

**Merge-throughput baseline recorded.**
The merge worker's throughput (how fast it moves data from the write-optimised delta store to the read-optimised main store) is benchmarked at 1, 2, 4, and 8 parallel workers. Results are saved as a baseline, and CI will warn if a future change causes more than a 15% regression.

**HTAP merge view safety investigation.**
During the merge cycle, there is a brief window where the query view is replaced. If a query arrives at exactly this moment, it could see a "relation does not exist" error. v0.52.0 investigates whether this can be eliminated and adds a concurrent stress test to detect it.

---

## v0.53.0 — High Availability & Logical Replication

**Effort estimate: 5–7 person-weeks**

**Goal:** Give organisations the infrastructure they need to run pg_ripple in high-availability production environments — including a second replica that stays in sync in near-real-time, a Kubernetes deployment package, and benchmark data to guide vector index configuration.

### RDF Logical Replication

**Streaming RDF changes to a replica database.**
PostgreSQL supports logical replication: a primary database streams a log of all changes to one or more replica databases. v0.53.0 builds a custom logical-decoding plugin for pg_ripple that translates internal storage changes into standard RDF triples (N-Triples format), which are then applied to the replica's pg_ripple instance.

This means:
- A second database stays in sync with the primary within ~1 second under normal load
- If the primary fails, the replica can take over with minimal data loss
- Read-heavy workloads can be distributed across replicas

The replica worker handles conflicts using a "last writer wins" strategy, configurable per deployment. A `pg_ripple.replication_stats()` function shows replication lag and progress in real time.

**Documentation** covers the full setup (primary + replica configuration), lag monitoring, and failover procedure.

### Kubernetes Helm Chart

**First-class deployment on Kubernetes.**
A Helm chart packages pg_ripple for deployment on Kubernetes clusters with configurable options for:
- Number of read replicas
- Persistent storage (PVC configuration)
- HTTP service type (LoadBalancer or ClusterIP)
- Federation endpoints
- SHACL shapes configuration
- LLM API key secret reference

The chart includes health check probes and will be published to a GitHub Pages Helm repository, making `helm install pg-ripple` a single command.

Documentation covers deployment on standard clusters, values reference, Prometheus monitoring integration, and a design outline for a future Kubernetes operator.

### Vector Index Performance Benchmarks

**Data to guide the choice between HNSW and IVFFlat indexes.**
pg_ripple supports vector similarity search (via pgvector) for hybrid SPARQL + semantic search queries. Two index types are available — HNSW and IVFFlat — as well as three precision levels (full, half, binary). The trade-offs between recall accuracy, index build time, and query latency depend heavily on the dataset.

v0.53.0 publishes a benchmark comparing all six combinations on a 100,000-embedding fixture, measuring recall at p50/p95/p99 and query latency. The results are published as a reference document so operators can make an informed decision for their specific workload.

---

## Summary Table

| Version | Theme | Key Deliverables | Estimated Effort |
|---|---|---|---|
| **v0.51.0** | Security Hardening & Production Readiness | Non-root container, SPARQL DoS limits, HTTP streaming, OTLP tracing, pg_upgrade docs, OWL 2 RL completion | 8–10 pw |
| **v0.52.0** | DX, Extended Standards & Architecture | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, OpenAPI spec, CDC lifecycle events, code quality splits | 6–9 pw |
| **v0.53.0** | High Availability & Logical Replication | RDF logical replication, Helm chart, vector index benchmarks | 5–7 pw |
| **v1.0.0** | Production Release | Final conformance, stress test, security audit, API stability guarantee | 6–8 pw |

Total estimated effort to v1.0.0 from the current state (v0.50.0): **25–34 person-weeks**.
