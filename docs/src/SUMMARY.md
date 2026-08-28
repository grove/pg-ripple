# Summary

[What Is pg_ripple?](landing.md)

---

# Evaluate

- [When to Use pg_ripple](evaluate/when-to-use.md)
- [Architecture at a Glance](evaluate/architecture-glance.md)
- [Comparison vs Alternatives](evaluate/comparison.md)
- [Performance & Conformance Results](evaluate/performance-results.md)
- [v0.132.0 Soak-Test Evidence](benchmarks/soak-0.132.0.md)

---

# Getting Started

- [Installation](getting-started/installation.md)
- [Hello World — Five-Minute Walkthrough](getting-started/hello-world.md)
- [Guided Tutorial — Build a Knowledge Graph in 30 Minutes](getting-started/tutorial.md)
- [Key Concepts — RDF for PostgreSQL Users](getting-started/key-concepts.md)
- [Playground — Try Without Installing](user-guide/playground.md)

---

# Feature Deep Dives

- [Storing Knowledge](features/storing-knowledge.md)
- [Loading Data](features/loading-data.md)
- [Querying with SPARQL](features/querying-with-sparql.md)
- [Validating Data Quality (SHACL)](features/validating-data-quality.md)
- [Reasoning and Inference (Datalog)](features/reasoning-and-inference.md)
- [Exporting and Sharing](features/exporting-and-sharing.md)
- [Live Views and Subscriptions](features/live-views-and-subscriptions.md)
- [Geospatial (GeoSPARQL)](features/geospatial.md)
- [Full-Text Search](features/full-text-search.md)
- [Temporal & Provenance](features/temporal-and-provenance.md)
- [Multi-Tenant Graphs](features/multi-tenant-graphs.md)
- [OWL 2 Profiles (RL / EL / QL)](features/owl-profiles.md)
- [SHACL-SPARQL Rules](features/shacl-sparql-rules.md)
- [R2RML — Relational to RDF](features/r2rml.md)
- [Lattice Datalog — When and Why](features/lattice-datalog.md)
- [Advanced Inference: WCOJ, DRed & Tabling](features/advanced-inference.md)
- [Graph Analytics (PageRank)](features/pagerank.md)
- [Probabilistic Reasoning](features/uncertain-knowledge.md)
- [CDC Subscriptions](features/cdc-subscriptions.md)
- [Live SPARQL Subscriptions](features/live-subscriptions.md)
- [JSON Mapping (Bidirectional JSON-LD)](features/json-mapping.md)
- [Cypher / LPG → RDF Mapping](features/lpg-mapping.md)

---

# AI, RAG & Record Linkage

- [AI Overview — Decision Tree](features/ai-overview.md)
- [Vector & Hybrid Search](features/vector-and-hybrid-search.md)
- [RAG Pipeline — `rag_context()`](user-guide/rag-pipeline.md)
- [Natural Language to SPARQL](features/nl-to-sparql.md)
- [Knowledge-Graph Embeddings (KGE)](features/knowledge-graph-embeddings.md)
- [Record Linkage & Entity Resolution](features/record-linkage.md)
- [GraphRAG (Microsoft)](features/graphrag.md)
- [Vector Federation](user-guide/vector-federation.md)
- [AI Agent Integration (LangChain, LlamaIndex)](features/ai-agent-integration.md)

---

# Use-Case Cookbook

- [Index of Recipes](cookbook/index.md)
- [Knowledge Graph from a Relational Catalogue](cookbook/relational-to-rdf.md)
- [Chatbot Grounded in a Knowledge Graph](cookbook/grounded-chatbot.md)
- [Deduplicate Customer Records](cookbook/dedupe-customers.md)
- [Privacy-Preserving Record Linkage (PPRL)](cookbook/pprl.md)
- [Audit Trail with PROV-O & Temporal Queries](cookbook/audit-trail.md)
- [CDC → Kafka via JSON-LD Outbox](cookbook/cdc-to-kafka.md)
- [Probabilistic Rules for Soft Constraints](cookbook/probabilistic-rules.md)
- [Rule Libraries](cookbook/rule-libraries.md)
- [SPARQL Repair Workflow](cookbook/sparql-repair.md)
- [Ontology Mapping and Alignment](cookbook/ontology-mapping.md)
- [LLM Workflow — NL to Knowledge Graph Answer](cookbook/llm-workflow.md)
- [Federation with Wikidata and DBpedia](cookbook/federation-wikidata.md)
- [SHACL + Datalog Data Quality Pipeline](cookbook/shacl-datalog-quality.md)
- [SKOS Thesaurus Management](cookbook/skos-thesaurus.md)
- [Common Vocabulary Bundles: DCTERMS, Schema.org, FOAF](cookbook/common-vocabularies.md)
- [Migrate from Neo4j](cookbook/migrate-from-neo4j.md)
- [Build a RAG Pipeline from Scratch](cookbook/rag-pipeline-from-scratch.md)
- [Rule-Library Federation](cookbook/rule-federation.md)

---

# APIs & Integration

- [APIs and Integration Overview](features/apis-and-integration.md)
- [SPARQL Query Debugger — `EXPLAIN SPARQL`](user-guide/explain-sparql.md)
- [Rule Library Federation Guide](guides/rule-library-federation.md)

---

# Operations

- [Architecture Overview](operations/architecture.md)
- [Production Readiness Checklist](operations/production-checklist.md)
- [Deployment Models](operations/deployment.md)
- [Docker (Batteries-Included)](operations/docker.md)
- [Kubernetes & Helm](operations/kubernetes.md)
- [CloudNativePG](operations/cloudnativepg.md)
- [High Availability](operations/high-availability.md)
- [Logical Replication](operations/replication.md)
- [CDC Operations](operations/cdc.md)
- [Citus + pg-trickle Integration](operations/citus-integration.md)
- [pg-tide Relay: Hub-and-Spoke](operations/pg-tide-relay.md)
- [pg-trickle Relay (Legacy Redirect)](operations/pg-trickle-relay.md)
- [Bidi Operations Runbook](operations/bidi-runbook.md)
- [Bidi Production Checklist](operations/bidi-production-checklist.md)
- [Configuration](operations/configuration.md)
- [Tuning](operations/tuning.md)
- [Monitoring and Observability](operations/monitoring.md)
- [OpenTelemetry Observability Guide](operations/observability-otel.md)
- [Performance Tuning](operations/performance.md)
- [Parallel Merge Workers](operations/merge-workers.md)
- [Backup and Disaster Recovery](operations/backup-recovery.md)
- [Upgrading Safely](operations/upgrading.md)
- [Scaling](operations/scaling.md)
- [Read Replicas](operations/read-replicas.md)
- [Troubleshooting](operations/troubleshooting.md)
- [Security](operations/security.md)
- [Compatibility Matrix](operations/compatibility.md)
- [HTTP Companion Operations](operations/http-companion.md)

---

# Best Practices

- [Index](user-guide/best-practices/index.md)
- [Bulk Loading](user-guide/best-practices/bulk-loading.md)
- [Data Modeling](user-guide/best-practices/data-modeling.md)
- [SPARQL Patterns](user-guide/best-practices/sparql-patterns.md)
- [SPARQL Performance](user-guide/best-practices/sparql-performance.md)
- [SHACL Patterns](user-guide/best-practices/shacl-patterns.md)
- [Datalog Optimization](user-guide/best-practices/datalog-optimization.md)
- [Update Patterns](user-guide/best-practices/update-patterns.md)
- [Federation Performance](user-guide/best-practices/federation-performance.md)
- [Query Planning](user-guide/performance/query-planning.md)

---

# Reference

- [SQL Function Reference](reference/sql-functions.md)
- [SQL API Reference](reference/sql-api.md)
- [SQL Reference Index](user-guide/sql-reference/index.md)
- [Triple CRUD](user-guide/sql-reference/triple-crud.md)
- [Bulk Load](user-guide/sql-reference/bulk-load.md)
- [SPARQL Query](user-guide/sql-reference/sparql-query.md)
- [SPARQL Update](user-guide/sql-reference/sparql-update.md)
- [Datalog](user-guide/sql-reference/datalog.md)
- [SHACL](user-guide/sql-reference/shacl.md)
- [RDF-star](user-guide/sql-reference/rdf-star.md)
- [Named Graphs](user-guide/sql-reference/named-graphs.md)
- [Federation](user-guide/sql-reference/federation.md)
- [Full-Text Search](user-guide/sql-reference/fts.md)
- [Cursor API](user-guide/sql-reference/cursor-api.md)
- [Views](user-guide/sql-reference/views.md)
- [Framing Views](user-guide/sql-reference/framing-views.md)
- [Serialization](user-guide/sql-reference/serialization.md)
- [HTTP Endpoint](user-guide/sql-reference/http-endpoint.md)
- [Dictionary](user-guide/sql-reference/dictionary.md)
- [Prefixes](user-guide/sql-reference/prefix.md)
- [Admin](user-guide/sql-reference/admin.md)
- [Explain](user-guide/sql-reference/explain.md)
- [Architecture (Internals)](reference/architecture.md)
- [Subsystem Dependency Graph](architecture.md)
- [GUC Reference](reference/guc-reference.md)
- [GUCs](reference/gucs.md)
- [Deprecated GUCs](reference/deprecated-gucs.md)
- [Feature Status Taxonomy](reference/feature-status-taxonomy.md)
- [Plan Cache](reference/plan-cache.md)
- [Audit Log](reference/audit-log.md)
- [Embedding Functions](reference/embedding-functions.md)
- [GraphRAG Functions](reference/graphrag-functions.md)
- [GraphRAG Ontology](reference/graphrag-ontology.md)
- [GeoSPARQL Functions](reference/geosparql.md)
- [GeoSPARQL Function Index](reference/geosparql-functions.md)
- [Lattice-Based Datalog](reference/lattice-datalog.md)
- [HTTP API Reference](reference/http-api.md)
- [Limits and Quotas](reference/limits.md)
- [Approximate Aggregates (HLL)](reference/approximate-aggregates.md)
- [Arrow Flight Reference](reference/arrow-flight.md)
- [Citus SERVICE Shard Pruning](reference/citus-service-pruning.md)
- [API Stability Guarantees](reference/api-stability.md)
- [SPARQL Reference](reference/sparql.md)
- [Datalog Reference](reference/datalog.md)
- [SHACL Reference](reference/shacl.md)
- [Storage Reference](reference/storage.md)
- [CONSTRUCT Writeback Rules](reference/construct-rules.md)
- [IVM Boundary: CWB vs. PageRank](reference/ivm.md)
- [Federation Reference](reference/federation.md)
- [CDC Reference](reference/cdc.md)
- [GraphRAG Reference](reference/graphrag.md)
- [Observability Reference](reference/observability.md)
- [Query Optimization](reference/query-optimization.md)
- [Vector Search Reference](reference/vector-search.md)
- [Development Reference](reference/development.md)
- [SPARQL Compliance Matrix](reference/sparql-compliance.md)
- [SPARQL 1.2 Property Path Status](reference/sparql12-status.md)
- [W3C Conformance](reference/w3c-conformance.md)
- [Conformance Trends](reference/conformance-trends.md)
- [Jena Results](reference/jena-results.md)
- [OWL 2 RL Results](reference/owl2rl-results.md)
- [WatDiv Results](reference/watdiv-results.md)
- [LUBM Results](reference/lubm-results.md)
- [Vector Index Trade-offs](reference/vector-index-tradeoffs.md)
- [Running Conformance Tests](reference/running-conformance-tests.md)
- [Error Codes](reference/error-codes.md)
- [Error Catalog](reference/error-catalog.md)
- [Scalability](reference/scalability.md)
- [FAQ](reference/faq.md)
- [Optional-Feature Degradation](reference/degradation.md)
- [Glossary](reference/glossary.md)
- [Release Notes](reference/changelog.md)
- [Roadmap](reference/roadmap.md)

---

# Research

- [Research Index](research/index.md)
- [PostgreSQL Triple-Store Deep Dive](research/postgresql-deepdive.md)

---

# Contributing

- [How to Contribute](reference/contributing.md)
- [Release Process](reference/release-process.md)

---

# Blog

- [Blog Index](blog/README.md)
