# pg_ripple Blog

> **Note:** This blog directory is an experiment. All posts were generated with
> AI assistance (GitHub Copilot / Claude) as a way to explore how well
> LLM-generated technical writing holds up for a niche systems engineering
> topic. The technical content has been reviewed for accuracy, but treat the
> posts as drafts — not as officially reviewed documentation.

---

## Posts

### Core Concepts & Architecture

| Post | Summary |
|------|---------|
| [Why RDF Inside PostgreSQL?](why-rdf-in-postgresql.md) | The case for a triple store that lives where your data already is — no ETL pipeline, no separate cluster, no impedance mismatch. Why VP storage on top of PostgreSQL beats both property-graph databases and standalone triplestores for most knowledge-graph workloads. |
| [Vertical Partitioning: One Table Per Predicate](vertical-partitioning-explained.md) | Inside pg_ripple's storage model: each predicate gets its own table with `(s, o, g)` columns. Why this beats a single wide `(s, p, o)` table by 10–100× for selective queries, and how rare-predicate consolidation keeps the catalog sane. |
| [Everything Is an Integer](dictionary-encoding-integer-joins.md) | Dictionary encoding with XXH3-128: every IRI, blank node, and literal is hashed to `BIGINT` before it touches storage. Why string comparisons in a triple store are a performance bug, and how integer-only joins change the cost model. |
| [How SPARQL Becomes a PostgreSQL Query Plan](sparql-to-sql-translation.md) | The translation pipeline from SPARQL text to `spargebra` algebra to SQL to SPI execution. Filter pushdown, self-join elimination, and why the PostgreSQL optimizer is surprisingly good at graph queries when you give it the right SQL. |

### Storage & Performance

| Post | Summary |
|------|---------|
| [HTAP for Triples: Reads and Writes at the Same Time](htap-reads-and-writes.md) | The delta/main/tombstone split that lets pg_ripple handle concurrent OLTP writes and analytical SPARQL queries without locking. Background merge workers, BRIN indexes, and the query path that unions it all together. |
| [Leapfrog Triejoin: When Triangle Queries Meet Optimal Joins](leapfrog-triejoin.md) | Standard hash joins go quadratic on cyclic patterns like triangles and cliques. Leapfrog triejoin doesn't. How pg_ripple compiles worst-case optimal joins into PostgreSQL, and what a 10–100× speedup looks like in practice. |
| [Property Paths Are Just Recursive CTEs](property-paths-recursive-ctes.md) | SPARQL property paths (`foaf:knows+`, `skos:broader*`) compiled to `WITH RECURSIVE … CYCLE`. Why PostgreSQL 18's hash-based cycle detection matters, bounded depth for safety, and early fixpoint termination for hierarchies. |

### Reasoning & Inference

| Post | Summary |
|------|---------|
| [Datalog Inside PostgreSQL](datalog-inside-postgresql.md) | Automatic fact derivation from rules — RDFS subclass inference, OWL RL reasoning, transitive closure, all running as SQL inside your database. Semi-naive evaluation, stratified negation, and why Datalog is the query language your ontology already speaks. |
| [Magic Sets: Ask a Question, Infer Only What You Need](magic-sets-goal-directed.md) | Full materialization computes every possible inference. Magic sets rewrite rules so that only facts reachable from your query are derived. The difference between 2 million inferred triples and 47. |
| [owl:sameAs Without the Explosion](owl-sameas-entity-resolution.md) | Entity canonicalization at query time: union-find over `owl:sameAs` chains, canonical ID rewriting in the dictionary, and why naive `owl:sameAs` handling turns a 1-second query into a 10-minute one. |

### Data Quality & Validation

| Post | Summary |
|------|---------|
| [SHACL: Schema Validation for the Schema-Less](shacl-data-quality.md) | RDF has no schema. SHACL gives it one — declaratively, after the fact, without migration scripts. How pg_ripple compiles SHACL shapes into DDL constraints and async validation pipelines, with hints that feed the SPARQL query planner. |

### Interoperability & Federation

| Post | Summary |
|------|---------|
| [Querying the World from Inside PostgreSQL](sparql-federation-local-remote.md) | SPARQL `SERVICE` clauses that reach out to Wikidata, DBpedia, or your company's other SPARQL endpoints — federated from inside a single query. Cost-based planning, connection pooling, result caching, and the SSRF allowlist that keeps it safe. |
| [From Flat Triples to Nested JSON](json-ld-framing-nested-json.md) | A CONSTRUCT query returns triples. JSON-LD framing turns those triples into the nested JSON your API consumers actually want. Frame-driven shaping, `@embed`, `@reverse`, and why you don't need a separate API layer for graph-to-tree conversion. |

### AI & Semantic Search

| Post | Summary |
|------|---------|
| [When Semantic Search Meets Knowledge Graphs](vector-sparql-hybrid-search.md) | pgvector finds things that *sound* similar. SPARQL finds things that *are* related. Combine them: `pg:similar()` in a SPARQL query, reciprocal rank fusion, graph-contextualized embeddings, and hybrid retrieval that outperforms either alone. |
| [GraphRAG: Feeding LLMs with Structured Knowledge](graphrag-knowledge-export.md) | LLMs hallucinate when they don't have context. Knowledge graphs have context but no language model. GraphRAG bridges the gap: Parquet export, Datalog-enriched entity graphs, community detection, and the `rag_context()` pipeline that turns a question into grounded answers. |

### Streaming & Integration

| Post | Summary |
|------|---------|
| [CDC for Knowledge Graphs: React to Every Triple Change](cdc-knowledge-graphs.md) | Triple-level change data capture: subscribe to predicate changes, receive insert/delete events, and drive downstream systems in real time. The outbox pattern, exactly-once delivery, and why your knowledge graph should be an event source. |
| [pg_ripple + pg_trickle: The Semantic Event Hub](semantic-hub-trickle-relay.md) | pg_trickle's relay delivers events from PostgreSQL to Kafka, Redis, and webhooks. pg_ripple's CDC emits triple-change events. Connect them: a semantic event bus that streams knowledge graph changes to any subscriber. Integration architecture, guaranteed delivery, and the topology that makes it work. |
| [SPARQL CONSTRUCT Views: Live Materialized Graph Transformations](construct-views-live-transformations.md) | Derived graphs that update themselves when the source data changes. Vocabulary alignment, structural reshaping, and chained transformation pipelines — all maintained incrementally with DRed retraction. |
| [R2RML: Your Relational Tables Are Already a Knowledge Graph](r2rml-relational-to-graph.md) | Map existing PostgreSQL tables to RDF without copying a single row. R2RML mappings, join conditions, SQL views as sources, and the migration path for teams that want knowledge graph features without rewriting their data layer. |

### Reasoning & Inference (continued)

| Post | Summary |
|------|---------|
| [Well-Founded Semantics: When Your Ontology Has Cycles](well-founded-semantics.md) | Three-valued logic for the real-world cases where true/false isn't enough. Negation cycles, the alternating fixpoint algorithm, and when pg_ripple automatically falls back from stratification to WFS. |
| [Probabilistic Datalog: Soft Rules for Uncertain Knowledge](probabilistic-datalog.md) | Weighted rules, confidence propagation through inference chains, noisy-OR combination, and querying by confidence threshold. For NLP extractions, sensor fusion, and any domain where 95% is good enough. |

### Observability & Query Engineering

| Post | Summary |
|------|---------|
| [EXPLAIN for SPARQL: Reading pg_ripple Query Plans](explain-sparql-query-plans.md) | `pg_ripple.explain_sparql()` shows the full translation: algebra tree, SQL plan, cardinality estimates, and execution statistics. How to read it, what to look for, and how to fix the common performance problems it reveals. |
| [Natural Language to SPARQL: When Users Don't Speak Graph](natural-language-to-sparql.md) | LLM-powered translation from English questions to SPARQL queries. Schema-grounded prompts, validation against SHACL shapes, and the feedback loop that turns failed queries into training data. |

### Governance & Compliance

| Post | Summary |
|------|---------|
| [GDPR Right-to-Erasure in a Knowledge Graph](gdpr-right-to-erasure.md) | Deleting a person across every VP table, every inference, every embedding — in one transaction. `erase_subject()`, DRed retraction of inferred triples, dictionary cleanup, and the audit tombstone that proves the erasure happened. |
| [Multi-Tenant Knowledge Graphs with Quotas](multi-tenant-knowledge-graphs.md) | Per-tenant isolation using named graphs, quota enforcement with triple limits, row-level security on VP tables, and tenant-scoped inference. 200 tenants on one PostgreSQL instance. |
| [Automatic Provenance Tracking with PROV-O](provenance-tracking-prov-o.md) | Every bulk load, every inference run, every source file — tracked as queryable RDF using the W3C PROV-O vocabulary. Data lineage for HIPAA, SOX, and GDPR compliance without application-level logging. |
| [Time-Travel Queries for Knowledge Graphs](temporal-time-travel-queries.md) | Point-in-time graph snapshots using statement timelines. Compliance auditing, temporal diffs, and monthly headcount analytics from your knowledge graph's history. |

### Spatial & Distributed

| Post | Summary |
|------|---------|
| [GeoSPARQL on PostGIS: Spatial Queries Meet RDF](geosparql-postgis-spatial.md) | GeoSPARQL functions translated to PostGIS operations with GiST index support. Distance filters, spatial containment, and why pg_ripple delegates to PostGIS instead of implementing its own spatial engine. |
| [SPARQL on Citus: Shard-Pruning for Distributed Knowledge Graphs](citus-shard-pruning-sparql.md) | Subject-based sharding of VP tables across Citus workers. Shard pruning for bound subjects, carry-forward for multi-hop traversals, and when distribution is worth the complexity. |

### AI & Semantic Search (continued)

| Post | Summary |
|------|---------|
| [Automated Ontology Mapping: Aligning Vocabularies Without Manual Labor](ontology-mapping-alignment.md) | Lexical and KGE-based alignment of vocabularies, pre-built mapping templates, and the pipeline from candidate suggestions to OWL equivalence assertions. Hours instead of weeks for vocabulary integration. |
| [Neuro-Symbolic Entity Resolution](neuro-symbolic-entity-resolution.md) | Combining ML embedding similarity with SHACL constraint vetoes and Datalog transitivity propagation. High-recall candidate generation, high-precision logical filtering, and an auditable merge pipeline inside PostgreSQL. |

### Advanced

| Post | Summary |
|------|---------|
| [RDF-star: Making Statements About Statements](rdf-star-statements-about-statements.md) | "Alice knows Bob" is a triple. "Alice knows Bob, according to the HR system, as of January 2024" is a statement about a statement. How pg_ripple stores RDF-star quoted triples in the dictionary, why reification is dead, and what this means for provenance, temporal data, and trust. |

---

## Contributing

These posts are deliberately rough-edged — they're drafts exploring how the extension works, not polished marketing copy. If you spot a technical inaccuracy, open an issue or PR. If you want to write a post, open a discussion first to avoid duplication.
