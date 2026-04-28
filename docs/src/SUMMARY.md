# Summary

[What Is pg_ripple?](landing.md)

---

# Evaluate

- [When to Use pg_ripple](evaluate/when-to-use.md)
- [Architecture at a Glance](evaluate/architecture-glance.md)
- [Comparison vs Alternatives](evaluate/comparison.md)
- [Performance & Conformance Results](evaluate/performance-results.md)

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
- [Audit Trail with PROV-O & Temporal Queries](cookbook/audit-trail.md)
- [CDC → Kafka via JSON-LD Outbox](cookbook/cdc-to-kafka.md)
- [Probabilistic Rules for Soft Constraints](cookbook/probabilistic-rules.md)
- [SPARQL Repair Workflow](cookbook/sparql-repair.md)
- [Ontology Mapping and Alignment](cookbook/ontology-mapping.md)
- [LLM Workflow — NL to Knowledge Graph Answer](cookbook/llm-workflow.md)
- [Federation with Wikidata and DBpedia](cookbook/federation-wikidata.md)
- [SHACL + Datalog Data Quality Pipeline](cookbook/shacl-datalog-quality.md)

---

# APIs & Integration

- [APIs and Integration Overview](features/apis-and-integration.md)
- [SPARQL Query Debugger — `EXPLAIN SPARQL`](user-guide/explain-sparql.md)

---

# Operations

- [Architecture Overview](operations/architecture.md)
- [Deployment Models](operations/deployment.md)
- [Docker (Batteries-Included)](operations/docker.md)
- [Kubernetes & Helm](operations/kubernetes.md)
- [CloudNativePG](operations/cloudnativepg.md)
- [High Availability](operations/high-availability.md)
- [Logical Replication](operations/replication.md)
- [CDC Operations](operations/cdc.md)
- [Citus + pg-trickle Integration](operations/citus-integration.md)
- [pg-trickle Relay: Hub-and-Spoke](operations/pg-trickle-relay.md)
- [Configuration](operations/configuration.md)
- [Tuning](operations/tuning.md)
- [Monitoring and Observability](operations/monitoring.md)
- [Performance Tuning](operations/performance.md)
- [Parallel Merge Workers](operations/merge-workers.md)
- [Backup and Disaster Recovery](operations/backup-recovery.md)
- [Upgrading Safely](operations/upgrading.md)
- [Scaling](operations/scaling.md)
- [Troubleshooting](operations/troubleshooting.md)
- [Security](operations/security.md)

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
- [GUC Reference](reference/guc-reference.md)
- [Audit Log](reference/audit-log.md)
- [Embedding Functions](reference/embedding-functions.md)
- [GraphRAG Functions](reference/graphrag-functions.md)
- [GraphRAG Ontology](reference/graphrag-ontology.md)
- [GeoSPARQL Functions](reference/geosparql.md)
- [Lattice-Based Datalog](reference/lattice-datalog.md)
- [HTTP API Reference](reference/http-api.md)
- [API Stability Guarantees](reference/api-stability.md)
- [SPARQL Compliance Matrix](reference/sparql-compliance.md)
- [W3C Conformance](reference/w3c-conformance.md)
- [OWL 2 RL Results](reference/owl2rl-results.md)
- [WatDiv Results](reference/watdiv-results.md)
- [LUBM Results](reference/lubm-results.md)
- [Vector Index Trade-offs](reference/vector-index-tradeoffs.md)
- [Running Conformance Tests](reference/running-conformance-tests.md)
- [Error Catalog](reference/error-catalog.md)
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
- [Why RDF Inside PostgreSQL?](blog/why-rdf-in-postgresql.md)
- [Vertical Partitioning: One Table Per Predicate](blog/vertical-partitioning-explained.md)
- [Everything Is an Integer](blog/dictionary-encoding-integer-joins.md)
- [How SPARQL Becomes a PostgreSQL Query Plan](blog/sparql-to-sql-translation.md)
- [HTAP for Triples](blog/htap-reads-and-writes.md)
- [Leapfrog Triejoin](blog/leapfrog-triejoin.md)
- [Property Paths Are Just Recursive CTEs](blog/property-paths-recursive-ctes.md)
- [Datalog Inside PostgreSQL](blog/datalog-inside-postgresql.md)
- [Magic Sets: Ask a Question, Infer Only What You Need](blog/magic-sets-goal-directed.md)
- [owl:sameAs Without the Explosion](blog/owl-sameas-entity-resolution.md)
- [SHACL: Schema Validation for the Schema-Less](blog/shacl-data-quality.md)
- [Querying the World from Inside PostgreSQL](blog/sparql-federation-local-remote.md)
- [From Flat Triples to Nested JSON](blog/json-ld-framing-nested-json.md)
- [When Semantic Search Meets Knowledge Graphs](blog/vector-sparql-hybrid-search.md)
- [GraphRAG: Feeding LLMs with Structured Knowledge](blog/graphrag-knowledge-export.md)
- [RDF-star: Making Statements About Statements](blog/rdf-star-statements-about-statements.md)
- [CDC for Knowledge Graphs](blog/cdc-knowledge-graphs.md)
- [pg_ripple + pg_trickle: The Semantic Event Hub](blog/semantic-hub-trickle-relay.md)
- [Natural Language to SPARQL](blog/natural-language-to-sparql.md)
- [EXPLAIN for SPARQL](blog/explain-sparql-query-plans.md)
- [GeoSPARQL on PostGIS](blog/geosparql-postgis-spatial.md)
- [Well-Founded Semantics](blog/well-founded-semantics.md)
- [Probabilistic Datalog](blog/probabilistic-datalog.md)
- [SPARQL CONSTRUCT Views](blog/construct-views-live-transformations.md)
- [R2RML: Your Relational Tables Are Already a Knowledge Graph](blog/r2rml-relational-to-graph.md)
- [GDPR Right-to-Erasure in a Knowledge Graph](blog/gdpr-right-to-erasure.md)
- [Multi-Tenant Knowledge Graphs with Quotas](blog/multi-tenant-knowledge-graphs.md)
- [Automated Ontology Mapping](blog/ontology-mapping-alignment.md)
- [Time-Travel Queries for Knowledge Graphs](blog/temporal-time-travel-queries.md)
- [Automatic Provenance Tracking with PROV-O](blog/provenance-tracking-prov-o.md)
- [SPARQL on Citus: Shard-Pruning](blog/citus-shard-pruning-sparql.md)
- [Neuro-Symbolic Entity Resolution](blog/neuro-symbolic-entity-resolution.md)
- [Incremental View Maintenance with pg_trickle](blog/ivm-pg-trickle-integration.md)
