# pg_ripple — Documentation Plan

> **Date**: 2026-04-19
> **Status**: Supersedes all prior documentation plans
> **Scope**: Complete documentation strategy, site structure, content guidelines, and delivery plan

---

## 1. Executive Summary

pg_ripple's documentation exists to answer one question: *How do I use this to solve my problem?* Every page, every example, every diagram serves that purpose. The documentation is not a mirror of the source code or a record of the development process — it is a product in its own right, designed for the people who use pg_ripple to build things.

The philosophy is problem-centric. Users arrive with goals — load data, run a query, enforce quality rules, deploy to production — and the documentation meets them where they are. A database administrator with two decades of PostgreSQL experience and a data scientist who has never touched a terminal should both find their way to working code within ten minutes. We achieve this through layered explanations: plain language up front, technical depth available on demand, and worked examples that are copy-pasteable without modification.

pg_ripple is a mature system (v0.31.0, 100% W3C SPARQL 1.1 and SHACL Core conformance) with a comprehensive feature set spanning data loading, SPARQL querying, SHACL validation, Datalog reasoning, JSON-LD framing, vector-hybrid search, federation, and HTTP endpoints. The documentation must reflect that maturity. It should read like the documentation of a system that has been in production for years — confident, precise, and complete — rather than a collection of feature announcements.

Five user archetypes drive content decisions: the Data Engineer who needs reliable data pipelines, the Application Developer who wants to power features with graph queries, the Knowledge Architect who models domains and writes inference rules, the Decision-Maker who evaluates whether pg_ripple fits the architecture, and the AI/ML Engineer who builds LLM-powered retrieval pipelines. Every page has a primary audience, and every section includes signposts that guide readers to the depth they need.

---

## 2. User Archetypes

### Data Engineer

The Data Engineer manages data pipelines. They have RDF files from upstream systems — Turtle exports from an ontology tool, N-Triples dumps from a linked-data publisher, RDF/XML from a legacy system — and need to load them into pg_ripple reliably, validate them against quality rules, and serve them to downstream consumers. Their entry point is: *I have data. How do I get it in?*

They care about throughput numbers (triples per second), error handling during bulk loads, blank-node scoping across files, incremental updates without downtime, and monitoring load progress. They are comfortable with SQL, shell scripts, and CI pipelines. They want documentation that includes batch sizes, memory implications, and what happens when things go wrong.

### Application Developer

The Application Developer builds software features on top of graph data. They want to query relationships — recommendations, entity resolution, shortest paths, transitive closures — and integrate results into an application via SQL or HTTP. Their entry point is: *Can I query relationships? How fast?*

They care about query latency, result formats (JSON, CSV), the HTTP API, and how to combine SPARQL results with regular SQL queries. They want examples that look like real application code: parameterized queries, connection pooling, caching strategies, and error handling.

### Knowledge Architect

The Knowledge Architect designs the data model. They define ontologies, write SHACL shapes to enforce data quality, author Datalog rules for inference, and manage the lifecycle of named graphs. Their entry point is: *Can I define constraints and reason over facts?*

They care about expressiveness — what SHACL constraints are supported, what Datalog features exist, how inference interacts with SPARQL queries. They want documentation that explains the theoretical foundations plainly (what is stratification? what is OWL RL?) and shows how to translate domain requirements into shapes and rules.

### Decision-Maker

The Decision-Maker evaluates tools. They need to understand what pg_ripple does, when it is the right choice, and when it is not. Their entry point is: *What problems does pg_ripple solve? Should we use it?*

They care about comparison matrices, production readiness, support for standards, operational complexity, and the trajectory of the project. They want honest trade-offs, not marketing. A page that says "pg_ripple is not yet suitable for graphs exceeding 10 billion triples" earns more trust than one that avoids the question.

### AI/ML Engineer

The AI/ML Engineer builds retrieval pipelines and LLM-powered applications. They are evaluating pg_ripple as a backend for RAG (Retrieval-Augmented Generation), entity resolution, or structured prompt generation. Their entry point is: *Can I combine graph traversal with vector similarity? How do I get structured context for an LLM?*

They care about hybrid retrieval (SPARQL + embeddings in one query), JSON-LD framing for prompt construction, `owl:sameAs` entity deduplication before embedding, the `rag_retrieve()` function, and how pg_ripple compares to a pure vector store like Qdrant or pgvector alone. They are comfortable with Python and SQL but may be new to RDF and SPARQL. They want documentation that leads with the AI use case and explains the graph concepts only as they become necessary.

---

## 3. Site Structure

The documentation site is organized into four sections, each serving a distinct purpose. A reader should be able to identify which section they need within five seconds of arriving.

**Information architecture**: The site is built with mdBook. URL slugs follow the pattern `section/page-name` (e.g., `/feature-deep-dives/querying-with-sparql`). The left sidebar has two levels: sections and pages. Cross-references use relative links. mdBook does not support HTML `<details>` collapsible blocks without a preprocessor — use admonition-style blockquotes (`> **Note**:`) for optional-depth content instead of `<details>`. **Decision**: use `mdbook-admonish`. Add `mdbook-admonish` to `book.toml` and the `[preprocessor.admonish]` block before Phase 1 content work starts. All callout blocks (notes, warnings, advanced sections, version callouts) must use `mdbook-admonish` fenced syntax (` ```admonish note ``` `, ` ```admonish warning ``` `, ` ```admonish tip ``` `). Plain `> **Note**:` blockquotes are acceptable only in pages not yet restructured; all new and restructured pages use `mdbook-admonish` syntax exclusively. Each page must have a `description` in its front matter for SEO; the sitemap is auto-generated. Page titles should use the vocabulary users type into search: "Load RDF Data" not "Data Loading," "Property Paths" not "Path Expressions."

### Evaluate

A single page for readers who need to decide whether pg_ripple fits their architecture before installing anything. Link this page from the landing page and the README, but keep it outside the getting-started flow — readers who have already chosen pg_ripple should skip directly to Section 1.

#### When to Use pg_ripple

A comparison page with an honest matrix: pg_ripple vs. plain SQL, vs. standalone RDF stores (Blazegraph, Virtuoso, Fuseki), vs. LPG systems (Neo4j, Amazon Neptune), vs. embedded solutions, and vs. pure vector databases (Qdrant, Weaviate, pgvector-only). For each row, state what pg_ripple does well, where it has limitations, and when the alternative is a better fit. Dedicate a specific section to the AI/LLM comparison: when does a knowledge graph outperform flat vector retrieval for RAG? (Answer: when the query requires multi-hop reasoning, entity deduplication, or structured output that a top-k vector search cannot provide.) Include a decision flowchart: "Do you already run PostgreSQL? → Do you need SPARQL? → How large is your dataset? → Do you need graph context for LLM prompts?" This page must be scrupulously honest — it builds trust with evaluators who will check claims against reality.

*Primary audience*: Decision-Maker, Application Developer evaluating options.

---

### Section 1: Core Concepts & Getting Started

These pages answer foundational questions and get users to working code within ten minutes. They are the front door of the documentation — the pages linked from the README and shared in conference talks. Readers who are still evaluating whether to use pg_ripple should start with the Evaluate section above.

#### Landing Page — What Is pg_ripple?

A single screen that explains what pg_ripple does, what makes it different from other graph systems, and who it is for. No jargon. No installation instructions. Just the core value proposition: *turn your PostgreSQL database into a knowledge graph store, with SPARQL, validation, and reasoning built in, and no extra infrastructure for the data store itself.* (The optional `pg_ripple_http` companion service adds a SPARQL Protocol HTTP endpoint; it is not required for SQL-based access.) Include the architecture diagram from the README and a single compelling code example (load data, query it, get results). Link to the next logical step: installation or the five-minute tutorial.

*Primary audience*: Decision-Maker, everyone arriving for the first time.

#### Why Knowledge Graphs Matter *(publish as blog post, not a docs page)*

This content — a non-technical narrative explaining why knowledge graphs matter using a concrete business scenario — is better suited to a blog post or conference talk than a documentation page. Documentation users arrive with a specific task; they do not browse explainer articles in a sidebar. Publish this as an external post and link to it from the landing page under "Learn more."

*Action*: Remove from site structure. File a content item to publish as a blog post on the project website or dev.to before Phase 1 ships.

#### pg_ripple in 60 Seconds *(fold into landing page, not a separate URL)*

The content here — key numbers, a comparison matrix, a "when to use / when not to use" summary — belongs as a visually distinct summary section at the bottom of the landing page, not as a separate URL. A separate page adds navigation overhead: evaluators who share a link want the landing page, not a sub-page. If a printable one-pager is needed for conference or sales use, produce it as a PDF artifact outside the docs site.

*Action*: Remove from site structure. Incorporate the key-numbers block and comparison summary into the landing page layout.

#### Installation

Platform-specific installation instructions: from source (cargo pgrx), Docker (docker-compose with pre-built image), and Linux packages (when available). Each method is a self-contained section with prerequisites, step-by-step commands, and a verification step (`SELECT pg_ripple.triple_count()` returns 0). Include troubleshooting for common failures: wrong PostgreSQL version, missing `shared_preload_libraries`, pgrx version mismatch. The Docker path should be the recommended default — zero dependencies beyond Docker itself.

*Primary audience*: Data Engineer, Application Developer.

#### Hello World — Five-Minute Walkthrough

The fastest path from zero to working queries. Load ten triples about people and movies (inline, no external files), run three queries of increasing complexity (simple pattern, OPTIONAL, property path), and explain each result. The reader finishes this page understanding what a triple is, what SPARQL looks like, and how results come back as a regular SQL table. Every code block is numbered and annotated. Expected output is shown after every query.

*Primary audience*: Everyone, especially first-time users.

#### Guided Tutorial — Build a Knowledge Graph in 30 Minutes

Picks up where Hello World ends. Assumes the reader has loaded and queried triples. The tutorial is structured in four self-contained segments of under ten minutes each, so readers can stop at any point with a working system — each segment leaves the reader with a functional, incrementally richer knowledge graph:

1. **Load and Explore** — Import a Turtle file of bibliographic data (papers, authors, institutions); run SPARQL queries of increasing complexity (author lookup, citation chains, publication-decade aggregation); understand named graphs.
2. **Validate** — Add SHACL shapes requiring every Movie to have a title and release year; observe a violation; fix it.
3. **Reason** — Write a Datalog rule for transitive “worked-with” relationships; run `infer()`; query the derived facts.
4. **Export** — Produce a JSON-LD document shaped for a REST API using `sparql_construct_jsonld()`.

Readers who complete all four segments finish with a validated, reasoning-capable knowledge graph. Expected output is shown after every query. The bibliographic dataset established here (papers, authors, institutions, topics, citations) is reused across the feature deep-dive chapters and the AI Retrieval & Graph RAG chapter.

*Primary audience*: Data Engineer, Application Developer, Knowledge Architect.

#### Key Concepts — RDF for PostgreSQL Users

A glossary-style page that explains RDF concepts using PostgreSQL analogies. A triple is like a row with three columns. A predicate is like a table name in pg_ripple's VP storage. A named graph is like a schema. An IRI is like a globally unique identifier. A blank node is like a row with no primary key. Cover: triples, IRIs, blank nodes, literals (plain, typed, language-tagged), predicates, named graphs, RDF-star (statements about statements), and SPARQL (the query language). Use diagrams and concrete examples for every concept. This is the page that other pages link to when they mention a concept for the first time.

*Primary audience*: Everyone, especially those new to RDF.

---

### Section 2: Feature Deep Dives

Each major capability gets a self-contained chapter. These are not reference pages — they are narrative guides that span understanding, practice, and troubleshooting. Every chapter follows a consistent structure so readers always know where to find what they need:

1. **What and Why** (200–300 words): What is this feature? What problems does it solve? Real-world scenarios where you would use it.
2. **How It Works** (200–300 words): The mechanism, explained clearly. Why did pg_ripple design it this way? What trade-offs were made?
3. **Worked Examples** (multiple code blocks): Start simple, add complexity. Each example is self-contained — includes sample data, the operation, and expected output.
4. **Common Patterns** (recipe format): Real workflows users perform. "Load a Turtle file and validate it against SHACL shapes." "Run inference, then query the inferred facts."
5. **Performance and Trade-offs** (150–200 words): Realistic numbers. When to use this feature vs. an alternative. Memory and CPU implications.
6. **Gotchas and Debugging** (bullet list): Common mistakes, diagnostic steps, fixes.
7. **Next Steps**: Links to related chapters, advanced variations, reference pages.

#### 2.1 Storing Knowledge

How to model data as triples. Walk through a concrete domain (research papers, authors, institutions, topics) and show each fact becoming a triple. Explain named graphs — when you need them (multi-source data, access control, versioning) and when you don't. Cover blank nodes (anonymous entities) with honest advice about when to use them and when to avoid them. Introduce RDF-star for statements about statements (provenance, confidence scores, temporal annotations). Show how types work (`rdf:type`) and how they interact with SHACL and Datalog. The reader finishes understanding how to translate a relational schema or a domain model into an RDF graph.

*Primary audience*: Knowledge Architect, Data Engineer.

#### 2.2 Loading Data

How to get data into pg_ripple. Cover all supported formats: Turtle (human-readable, the default recommendation), N-Triples (fastest for bulk loading), N-Quads (with named graphs), TriG (Turtle with named graphs), and RDF/XML (legacy interoperability). Explain the three loading modes: inline text (`load_turtle('...')`), file path (`load_turtle_file('/path')`), and individual inserts (`insert_triple()`). Discuss bulk loading performance: batch sizes, the VP promotion threshold (rare predicates consolidate into a shared table until they exceed the threshold), when to run ANALYZE afterward, and how to monitor progress. Cover blank-node scoping (blank nodes are local to each load call — loading the same blank node label from two separate calls creates two distinct entities). Show how to generate triples from existing SQL tables using INSERT combined with encode patterns.

*Primary audience*: Data Engineer.

#### 2.3 Querying with SPARQL

The heart of pg_ripple. Start with basic graph patterns — matching triples by subject, predicate, and object. Progress to OPTIONAL (left joins), FILTER (restricting results), BIND (computed variables), and VALUES (parameterized lookup). Introduce property paths — following chains of relationships automatically (`foaf:knows+` to find friends-of-friends) — with clear explanation of the path operators (`+`, `*`, `?`, `/`, `|`, `^`). Cover aggregation (COUNT, SUM, GROUP BY, HAVING), subqueries, UNION, MINUS, and GRAPH patterns for querying specific named graphs. Dedicate a section to performance: how to read `sparql_explain()` output, what makes a query fast or slow, how filter pushdown works, and the `max_path_depth` safety limit. Include a recipe section with real-world query patterns: entity resolution, recommendation, transitive closure, temporal queries over RDF-star. Mention SPARQL Update (INSERT DATA, DELETE DATA, DELETE/INSERT WHERE) as the write counterpart.

*Primary audience*: Application Developer, Data Engineer.

#### 2.4 Validating Data Quality

How to define and enforce data quality rules using SHACL (Shapes Constraint Language). Start with simple shapes: "every Person must have exactly one name" (`sh:minCount 1`, `sh:maxCount 1`). Progress to type constraints (`sh:datatype`), value range constraints, pattern constraints (`sh:pattern`), and class constraints. Explain the two validation modes: synchronous (violations are caught on insert — immediate feedback, slight latency cost) and asynchronous (background worker checks periodically — no insert latency, but violations are detected after the fact). Show how to read a validation report, how to fix violations, and how to use the dead-letter queue for async violations. Include patterns for common quality rules: referential integrity between entities, mandatory properties for specific types, allowed value lists, and cross-property constraints using `sh:or`/`sh:and`/`sh:not`.

*Primary audience*: Knowledge Architect, Data Engineer.

#### 2.5 Reasoning and Inference

How to derive new facts from existing ones using Datalog rules. Start with the motivating example: "if Alice manages Bob and Bob manages Carol, then Alice indirectly manages Carol." Show how to write this as a Datalog rule, load it, and run inference. Explain the built-in rule sets (RDFS for subclass/subproperty hierarchies, OWL RL for richer ontological reasoning) and when to use each. Cover stratification — the requirement that negation and aggregation in rules follow a layered structure — and explain what it means in practice (which rules can depend on which). Discuss the distinction between explicit and inferred triples (the `source` column in VP tables) and how SPARQL queries interact with inferred data. Include performance guidance: how inference time scales with rule count and data size, when to use goal-directed mode (derive only what a query needs) vs. full materialization. Cover advanced features: magic sets optimization, semi-naive evaluation, demand transformation.

*Primary audience*: Knowledge Architect.

#### 2.6 Exporting and Sharing

How to get data out of pg_ripple. Cover all export formats: Turtle (human-readable, good for inspection and version control), N-Triples (line-oriented, good for streaming and large exports), JSON-LD (the bridge to web applications, REST APIs, and LLMs), and RDF/XML (legacy interoperability). Dedicate significant space to JSON-LD framing — the ability to produce nested JSON documents shaped for a specific API contract, not flat triple dumps. Show how to use `sparql_construct_jsonld()` with a frame to produce exactly the JSON structure an API consumer expects. Cover the GraphRAG export pipeline: exporting entities and relationships in Microsoft GraphRAG's BYOG Parquet format, enriching the graph with Datalog rules, and validating export quality with SHACL. **GraphRAG canonical chapter**: all other mentions of GraphRAG in the documentation cross-reference this chapter. Do not duplicate the full GraphRAG workflow elsewhere.

*Primary audience*: Data Engineer, Application Developer.

#### 2.7 AI Retrieval & Graph RAG

How to use pg_ripple as the retrieval layer for AI and LLM applications. This chapter is the canonical reference for all AI-related capabilities; §2.6 (Exporting) and §2.8 (APIs) cross-reference it for LLM-specific workflows.

Open with the motivating contrast: flat vector search finds similar documents but loses the relationships between them. A graph-augmented retrieval query can answer "find papers semantically similar to X that were authored by someone in Alice's co-author network" — combining similarity, traversal, and filtering in one operation that pure pgvector cannot express.

Cover the full retrieval stack:

- **Vector embeddings**: storing embeddings alongside graph facts in `_pg_ripple.embeddings`, building HNSW indexes, the `pg:similar()` SPARQL function.
- **Hybrid retrieval**: combining a SPARQL graph pattern with a vector similarity filter in one query; Reciprocal Rank Fusion merging the two result lists; when hybrid outperforms pure-vector and when it does not.
- **`rag_retrieve()`**: the high-level function that accepts a query embedding, a set of graph patterns, and a `top_k` parameter, and returns a JSONB context block ready for use as an LLM system prompt.
- **JSON-LD framing for prompts**: using `sparql_construct_jsonld()` with a frame template to produce structured, token-efficient prompt context; how to design frames for common LLM prompt patterns; avoiding flat triple dumps.
- **`owl:sameAs` before embedding**: why entity canonicalization matters for embedding quality; how to run the sameAs pre-pass before bulk-embedding with `infer_demand()`.
- **Full-text search**: `fts_search()` for keyword expansion and broadening vector queries.
- **RAG pipeline end to end**: a worked example using a document corpus with named entities and relationships; load data → embed entities → infer relationships → retrieve context → generate prompt.

Performance section: HNSW index sizing, embedding worker throughput, when to pre-materialize RAG contexts as SPARQL views.

Comparison with pure vector stores (Qdrant, Weaviate, pgvector-only): when graph context adds value, when it adds complexity without benefit, and when to use each.

*Primary audience*: AI/ML Engineer, Application Developer.

#### 2.8 APIs and Integration

How to expose pg_ripple to applications. The `pg_ripple_http` companion service is a separate Rust binary (see `pg_ripple_http/README.md`); its documentation belongs in this chapter, not scattered across other pages. Cover the SPARQL Protocol HTTP endpoint (`pg_ripple_http`): configuration, supported response formats (JSON, XML, CSV, Turtle, JSON-LD), authentication, and Docker Compose deployment. Show how to call pg_ripple from application code: Python with `psycopg2` or `SPARQLWrapper`, JavaScript with `pg`, Java with JDBC. Cover SPARQL federation: querying remote SPARQL endpoints alongside local data using the `SERVICE` keyword, with connection pooling, result caching, and timeout configuration. Discuss caching strategies for production: plan cache tuning, result caching at the HTTP layer, and when to use materialized SPARQL views for frequently-run queries.

*Primary audience*: Application Developer.

---

### Section 3: Operations & Production

These pages address deployment, scaling, monitoring, and operational safety. The primary audience is the Data Engineer or DBA who runs pg_ripple in production and needs to keep it healthy.

#### Architecture Overview

How pg_ripple works under the hood — written for operators, not contributors. Explain the four key components: the dictionary (every value mapped to a compact integer ID), VP tables (one table per predicate), the HTAP storage engine (delta partitions for writes, main partitions for reads, a background merge worker that combines them), and the shared-memory cache (dictionary lookups without touching disk). Include the architecture diagram. Explain how a SPARQL query flows through the system: parse → optimize → generate SQL → execute via SPI → decode results. This page helps operators understand where bottlenecks occur and which knobs affect which component.

#### Deployment Models

How to deploy pg_ripple in different environments: standalone PostgreSQL on bare metal or a VM, containerized with Docker (single container or Compose with `pg_ripple_http`), and managed PostgreSQL services (which support custom extensions and which do not). For each model, explain the trade-offs: operational complexity, performance characteristics, backup strategy, and upgrade path. Include a recommendation: start with Docker for evaluation, move to a dedicated PostgreSQL instance for production.

#### Configuration and Tuning

All GUC parameters, grouped by subsystem: storage (`vp_promotion_threshold`, `merge_threshold`, `latch_trigger_threshold`), query engine (`plan_cache_size`, `max_path_depth`, `describe_strategy`), inference (`inference_mode`), validation (`shacl_mode`), caching (`dictionary_cache_size`, `cache_budget`), and system (`shared_memory_size`). For each parameter: type, default value, range, restart requirement, and a sentence explaining when and why to change it. Include a "quick-start production config" section with recommended values for three deployment sizes: small (under 1M triples), medium (1M–100M), and large (100M+).

#### Monitoring and Observability

What to monitor: `pg_ripple.stats()` output (cache hit rates, merge worker status, delta sizes), `pg_stat_statements` for SPARQL-generated SQL, slow-query logging via `sparql_explain(analyze := true)`, and the Prometheus metrics endpoint on `pg_ripple_http`. Include a Grafana dashboard template or describe the key panels. Define health checks: what does "healthy" look like? What thresholds trigger alerts?

#### Performance Tuning

How to identify and resolve bottlenecks. Cover the three common bottleneck areas: query performance (read `sparql_explain()`, check for missing filter pushdown, tune `plan_cache_size`), write throughput (merge worker lag, delta bloat, `merge_threshold` tuning), and cache pressure (dictionary cache hit rate below 95%, increase `cache_budget` or `dictionary_cache_size`). Include realistic benchmark numbers from BSBM and internal tests. Provide tuning recipes for specific workloads: read-heavy analytics, write-heavy ingestion, mixed HTAP.

#### Backup and Disaster Recovery

How to back up and restore a pg_ripple database. Explain that `pg_dump`/`pg_restore` capture everything (VP tables, dictionary, predicates catalog) because it is all standard PostgreSQL tables. Cover point-in-time recovery with WAL. Address the common concern: "do VP tables survive a dump/restore?" (yes — they are regular tables in the `_pg_ripple` schema). Include a tested backup/restore procedure with exact commands.

#### Upgrading Safely

How to upgrade from one pg_ripple version to the next. Explain the migration path: `ALTER EXTENSION pg_ripple UPDATE` walks through the chain of migration scripts (`pg_ripple--0.30.0--0.31.0.sql`, etc.). Cover pre-upgrade checks, the upgrade command, post-upgrade verification, and rollback strategy. Note explicitly that pg_ripple does not currently support zero-downtime upgrades: `ALTER EXTENSION pg_ripple UPDATE` acquires a brief exclusive lock while migration scripts execute. For systems that cannot tolerate downtime, document the recommended approach: schedule during a maintenance window, or route read traffic to a streaming replica while the primary is upgraded and then fail back once the upgrade completes.

#### Scaling

When and how to scale. Explain vertical scaling (more memory for dictionary cache, more CPU for merge worker, faster storage for VP tables) and the HTAP architecture's role in handling mixed workloads. Cover the merge worker in detail: what it does, how to tune it, and how to monitor its progress. For horizontal scaling, explain the current options (read replicas via PostgreSQL streaming replication) and what is not yet supported (sharding across multiple nodes).

#### Troubleshooting

A runbook-format page. Each entry follows the structure: symptom → likely cause → diagnostic steps → fix. Cover the most common issues across all subsystems: SPARQL returns zero rows (wrong prefix, unregistered IRI, case mismatch), merge worker not running (missing `shared_preload_libraries`), slow queries (unbounded property path, missing filter pushdown), SHACL validation not triggering (wrong `shacl_mode`), inference producing no results (rules not loaded, `inference_mode` off), shared memory errors after upgrade, and cache eviction pressure.

#### Security

Access control: named-graph-level row-level security backed by PostgreSQL roles. Input validation: how dictionary encoding prevents SQL injection in VP table queries. Secure deployment: TLS configuration for `pg_ripple_http`, authentication options, network isolation. File-path loaders (`load_turtle_file`, etc.) require superuser — explain why and how to delegate safely.

---

### Section 4: Reference & Community

Quick-lookup resources. These pages are not for reading front-to-back — they are for finding a specific answer fast.

#### SQL Function Reference

All pg_ripple SQL functions, organized by use case rather than alphabetically:

- **Loading**: `insert_triple`, `load_turtle`, `load_turtle_file`, `load_ntriples`, `load_ntriples_file`, `load_nquads`, `load_nquads_file`, `load_trig`, `load_trig_file`, `load_rdfxml`, `load_rdfxml_file`, `load_graph_turtle`, `load_graph_ntriples`, `sparql_update`
- **Querying**: `sparql`, `sparql_ask`, `sparql_explain`, `sparql_update`, `find_triples`, `triple_count`, `fts_search`
- **Validating**: `load_shacl`, `validate`, `list_shapes`, `drop_shape`
- **Reasoning**: `load_rules`, `load_rules_builtin`, `infer`, `list_rules`, `drop_rules`
- **Exporting**: `export_turtle`, `export_ntriples`, `export_jsonld`, `sparql_construct_jsonld`
- **Administration**: `stats`, `vacuum`, `reindex`, `promote_rare_predicates`, `register_prefix`, `prefixes`, `create_graph`, `drop_graph`, `list_graphs`, `encode_term`, `decode_id`

For each function: one-sentence description, full signature with parameter types and defaults, parameter table, one working example (with sample data and expected output), and edge-case notes. This reference supplements, not replaces, doc comments in the Rust source (`src/lib.rs` and related files). The Rust comments are for contributors; this page is for users. Keep them in sync when functions change signatures.

#### SPARQL Compliance Matrix

A table of every SPARQL 1.1 Query, Update, and Protocol feature, with status (Supported, Partial, Not Supported) and notes. Link to the W3C test suite results. For features with partial support, explain what works and what does not. For unsupported features, suggest workarounds.

#### Error Message Catalog

Every error code (PT001–PT799) with: error code, subsystem, message template, explanation of the cause, and how to fix it. Auto-generated from `src/error.rs` where possible, with curated explanations for errors that require context.

#### FAQ

25–30 questions organized by topic: Getting Started ("What PostgreSQL version do I need?"), Data Modeling ("When should I use named graphs?"), Querying ("Does pg_ripple support SPARQL 1.1 property paths?"), Performance ("How fast is bulk loading?"), Operations ("Do I need `shared_preload_libraries`?"), and Comparisons ("How does pg_ripple compare to Neo4j?"). Each answer is 50–150 words with links to the relevant deep-dive page.

#### Glossary

Plain-language definitions of every term used in the documentation: triple, predicate, IRI, blank node, literal, named graph, property path, SPARQL, SHACL, Datalog, RDF-star, VP table, dictionary encoding, HTAP, merge worker. Each definition is one to three sentences, with a concrete example.

#### Release Notes and Roadmap

The CHANGELOG.md and ROADMAP.md mirrored into the docs site. Each release entry includes: what changed, which migration script to run, and links to the relevant documentation pages that were added or updated.

#### Contributing

How to improve pg_ripple: development environment setup, running tests (`cargo pgrx test pg18`, `cargo pgrx regress pg18`), PR workflow, code conventions (from AGENTS.md), and how to contribute documentation. Include a section on reporting issues and requesting documentation improvements.

**Visibility**: “Contributing” buried at the end of Section 4 sends the wrong signal for a project seeking community growth. Add a persistent “Contribute” entry in the top-level mdBook navigation alongside the main section links, and include a brief callout card (“Want to improve pg_ripple? Start here →”) on the landing page. The full contributing guide remains here, but entry points must be visible from the landing page.

#### Research and Foundations *(move to contributor documentation, not user-facing reference)*

Academic citations (Abadi et al., VP storage model, Datalog evaluation techniques, SPARQL-to-SQL compilation strategy) are relevant to contributors, not users. This content belongs in `CONTRIBUTING.md` under an “Architecture and Background” heading, or as `docs/src/contributing/architecture.md` linked from the contributor guide. Do not include it in the user-facing Reference & Community section. Users who want to understand *why* pg_ripple is designed a certain way are served by the Operations § Architecture Overview chapter.

*Action*: Move content to `CONTRIBUTING.md`. Remove this entry from the Section 4 site structure.

---

## 4. Content Guidelines

### Language and Style

Write in active voice with short sentences (average 15 words). Lead with what the user can do, not how the implementation works: "Load a Turtle file in one call" before any mention of VP tables or dictionary encoding. Avoid jargon without explanation — the first use of any term links to the Glossary page. Use parenthetical asides for optional depth: "pg_ripple compiles this to a recursive CTE (a technique for following chains in SQL) with hash-based cycle detection." Put advanced implementation details in admonition blocks (`> **Advanced**`) rather than inline, to keep the main path readable.

Each paragraph conveys a complete thought in 150–250 words. Resist the urge to break prose into bullet lists unless the content is genuinely a checklist, a comparison matrix, or a sequence of steps. Bullet-point avalanches feel comprehensive but communicate poorly — readers skim them without absorbing the relationships between ideas.

### Voice and Person

Write in second person, addressing the reader directly: "you can load a Turtle file with one call" not "the user can load a Turtle file" and not "one can load a Turtle file." pg_ripple is referred to as "pg_ripple" (never "we" in user-facing pages; "the system" is acceptable in architecture and operations pages when describing system behavior). Instructions use imperative mood: "Run the following command," not "You should run the following command." Use American English spelling throughout ("color" not "colour," "initialize" not "initialise"). Contractions are acceptable in informal sections (FAQ, tutorials) but should be avoided in reference pages.

### pg_ripple-Specific Anti-Patterns

Avoid these patterns that appear repeatedly in first-draft documentation for this project:

- **Explaining VP tables before explaining triples.** Implementation details belong in "How It Works," never the opening paragraph. Lead with what the user does.
- **Conflating SPARQL-the-language with SPARQL-the-protocol.** "SPARQL" in query context is the W3C query language. The HTTP interface is the SPARQL Protocol. The `pg_ripple_http` service implements the Protocol. Keep these distinct.
- **Using `INSERT DATA` examples when `load_turtle()` is the natural entry point.** Lead bulk-loading examples with `load_turtle()`; `INSERT DATA` belongs only in the SPARQL Update section.
- **Omitting the `SELECT pg_ripple.sparql(...)` wrapper.** Every SPARQL example must show the full SQL call, not just the bare SPARQL string.
- **Presenting integer IDs as user-visible.** Dictionary IDs are internal. Never show raw `i64` values as if users interact with them; use `decode_id()` in any example that must mention an ID.
- **Asserting blank-node identity across loads.** Blank nodes are scoped per `load_turtle()` call. Examples that imply `_:x` inserted in one call is the same entity as `_:x` in a second call are incorrect.

### Examples

Every code example is copy-pasteable and runnable against a real pg_ripple instance. No pseudocode, no illustrative snippets with `...` elisions. Each example includes: (1) the sample data (INSERT or LOAD statement), (2) the operation, and (3) the expected output. Examples are tested in CI — a broken example fails the build.

Use consistent sample data across related examples. The guided tutorial establishes a bibliographic dataset (papers, authors, institutions, topics, citations); feature deep-dives reuse it where possible so readers build familiarity. When a feature needs different data, introduce it explicitly.

### Organization

One problem per page. If a user searches "how do I bulk-load a Turtle file?", they find exactly that workflow on the Loading Data page — not scattered across a function reference, a best-practices guide, and a configuration page.

Mark sections with difficulty levels: **Beginner**, **Intermediate**, **Advanced**. Each page lists prerequisites ("read this first") and related pages ("go here next"). Feature deep-dive pages follow the consistent seven-part structure described in Section 2 so readers always know where to find what they need.

### Versioned Callouts

Features introduced in specific versions get a callout: `> **Available since v0.10.0**`. This helps users on older versions understand which sections apply to them.

### Documentation Versioning

The documentation tracks the current release. When a feature changes incompatibly, add a versioned callout explaining both old and new behaviour. Do not maintain separate documentation trees for old versions — migration guides and CHANGELOG provide backward-compatibility information. When a deprecation is introduced, add `> **Deprecated since vX.Y.Z**` alongside the existing `> **Available since**` callout. Remove deprecated content only after two minor releases have passed.

when a single page accumulates more than three `> **Available since**` callouts anywhere in the document — the threshold is three total, regardless of how far apart they appear in the page — the prose has grown too fragmented for versioned callouts to carry alone. Refactor the page around current behavior and move earlier-version details to a dedicated migration note in `docs/src/reference/migration-notes.md` or to the CHANGELOG, rather than adding another inline callout.

**URL versioning**: The documentation site always reflects the current release. There are no per-version URL snapshots (e.g., no `/v0.31/`). This is a deliberate decision: maintaining parallel documentation trees is expensive, and pg_ripple's migration scripts plus CHANGELOG provide the backward-compatibility story. Any content that describes behavior specific to an older version must be placed in `docs/src/reference/migration-notes.md` or the CHANGELOG — not in main documentation pages — so that users on older versions can find it even when the current page no longer describes their behavior.

### Term Formatting

Follow project conventions: IRIs in `<angle brackets>`, literals in `"double quotes"^^xsd:type`, blank nodes as `_:label`, function names as `function_name()`, GUC parameters as `parameter_name`.

### Discoverability

Each page sets a `title` and `description` in mdBook front matter. Titles follow the pattern `{Task} — pg_ripple` (e.g., "Load RDF Data — pg_ripple"). Descriptions are one sentence of 100–160 characters phrased as a concrete capability ("Load Turtle, N-Triples, or RDF/XML files into pg_ripple at over 100,000 triples per second."). The sitemap is auto-generated by mdBook. Heading text in feature chapters must use the vocabulary users type into search engines: "property paths" not "path expressions," "bulk loading" not "batch insertion." The SQL Function Reference groups functions by task to match how users search, not alphabetically.

---

## 5. Delivery Strategy

### Phase 0: CI Test Harness (Prerequisite)

Build the infrastructure that keeps examples honest before writing any new pages. The harness is a script that: (1) spins up a local pg_ripple instance via `cargo pgrx run pg18` or Docker, (2) extracts fenced SQL code blocks from markdown files under `docs/src/`, (3) executes them in document order with per-file setup and teardown, and (4) compares stdout against expected output embedded in the markdown as a comment block directly below each code block. Fixture data lives in `docs/fixtures/` and is loaded once per test run. One shared fixture dataset is required: a **bibliographic dataset** (papers, authors, institutions, topics, citations, and pre-computed embeddings). This dataset is intentionally academic: papers, authors, and citations are well-understood, license-clear, and supported by public data sources (Semantic Scholar Open Graph, OpenAlex), which simplifies fixture generation. The tradeoff against a product-catalog or supply-chain dataset is domain specificity — acknowledge this honestly in the tutorial introduction so that Data Engineers from non-academic industries can map the patterns to their own domains. The coverage remains strong — entity relationships (authors/papers/institutions), typed literals (publication dates, DOIs), named graphs (conference proceedings as separate graphs), inference (co-authorship transitivity), SHACL validation (mandatory fields), and hybrid retrieval (vector similarity over paper embeddings). Where a feature chapter benefits from a more commercial scenario, supplement the shared dataset with a brief inline example rather than replacing the dataset. A single shared dataset across all chapters is simpler to maintain than the separate “movie/person dataset for features” + “document corpus for AI” split. The CI job runs the harness on every PR that touches `docs/`. Without this infrastructure, Phase 1 examples will rot immediately. Estimated scope: approximately one week (harness script + Docker integration + fixture datasets + CI job wiring + first end-to-end green run). Phase 0 is considered done only when the CI job has passed on a real PR, not merely when the script runs locally.

### Phase 1: Foundation (Immediate)

**Prerequisite**: Phase 0 (CI harness) must be complete and passing in CI before Phase 1 *new code examples* are committed. Prose corrections, restructuring, and rewrites that contain no runnable SQL blocks may land before Phase 0 is complete. Writing new examples before the harness is in place creates untested content that rots immediately.

Rewrite the landing page, installation, hello-world walkthrough, and key-concepts pages. These are the front door — they determine first impressions. Restructure the existing SQL reference pages into the feature deep-dive format, converting function-by-function listings into narrative guides organized around user goals. Estimated scope: **M** (2–3 weeks).

### Phase 2: Core Features

Write the eight feature deep-dive chapters: Storing Knowledge, Loading Data, Querying with SPARQL, Validating Data Quality, Reasoning and Inference, Exporting and Sharing, AI Retrieval & Graph RAG, and APIs and Integration. (There is no separate "Search and Discovery" chapter — that content lives in §2.7 AI Retrieval & Graph RAG.) Each chapter is complete and publishable independently. Prioritize by user traffic: Loading Data and Querying with SPARQL first (the most common entry points), then Validating and Reasoning (differentiation features), then Export, AI Retrieval, and APIs. Estimated scope: **L** (4–6 weeks for all eight chapters at production quality).

### Phase 3: Operations

Write the full operations section: Architecture, Deployment, Configuration, Monitoring, Performance, Backup, Upgrading, Scaling, Troubleshooting, Security. The configuration and troubleshooting pages are highest priority — they are the pages operators reach for first. Estimated scope: **M** (2–3 weeks).

### Phase 4: Reference and Polish

Complete the SQL function reference, SPARQL compliance matrix, error catalog, FAQ, and glossary. Audit all cross-references. Review every code example against the current release. Solicit feedback from early users and fill gaps. Estimated scope: **M** (2–3 weeks).

### Testing

Every code example runs against a real pg_ripple instance in CI. The test harness extracts SQL blocks from markdown files, executes them in order, and verifies that output matches expectations. Broken examples fail the build. This is non-negotiable — untested examples rot within weeks.

### Iteration

User questions on GitHub drive documentation improvements. Every question that takes more than five minutes to answer becomes a candidate for a new page or an expansion of an existing one. Track documentation gaps as GitHub issues with a `docs` label.

### Content Currency Policy

CI catches broken examples; nothing automatically catches outdated prose. As the project moves fast (32 releases to date), prose rot is the primary long-term documentation risk. To mitigate it:

- Any PR that changes a user-visible API (function signature, GUC parameter name or default, behavior change, new format supported) must include either a `docs/` update or a `docs-gap` issue in the same commit. Enforced by a CI job (`scripts/check_docs_coverage.sh`) that diffs exported function signatures in `src/lib.rs` against the SQL Function Reference and fails the build when a changed signature has no corresponding `docs/` touch in the same PR. A PR template checkbox is included as a human-readable reminder but is not the primary enforcement mechanism.
- Run a CI broken-link check (`mdbook-linkcheck` or equivalent) on every PR that touches `docs/`. Whenever a page is moved or removed, update a redirect map (`docs/redirects.toml` or equivalent) in the same commit; the CI job verifies the redirect map is current.
- The Phase 4 audit repeats at every minor release: run a script that diffs the function signatures in `src/lib.rs` against the SQL Function Reference and flags mismatches.
- The `> **Available since**` callout system (§4, Documentation Versioning) provides per-feature version tracking; the three-callout density limit prevents fragmentation.

---

## 6. Success Criteria

Documentation succeeds when:

1. **Five-minute evaluation**: A new user answers "What is pg_ripple? Should I use it?" within five minutes of arriving at the documentation site.

2. **Thirty-minute onboarding**: A developer loads data, runs a query, and understands the core concepts within thirty minutes, following the guided tutorial.

3. **Self-service problem solving**: At least 80% of "Was this helpful?" widget responses on any given page are positive, measured monthly. A supplementary metric is the ratio of `docs-gap`-labelled GitHub Discussions to total questions per quarter, with a target of below 20%. (The widget measures satisfaction, not self-service rate directly — the `docs-gap` ratio provides the self-service signal.) Tracked via: a "Was this helpful?" two-button widget on each page (results posted to a dedicated GitHub Discussion thread monthly), GitHub Discussions tagged with a `docs-gap` label in a dedicated category, and a quarterly review of site search queries that return zero results. **Tooling required** (mdBook provides none of this natively): select and configure (1) a privacy-respecting analytics provider (Plausible, Umami, or GoatCounter) for page-view and zero-result search tracking, (2) a custom mdBook preprocessor or lightweight JavaScript snippet for the two-button widget, and (3) a GitHub Discussion category with a `docs-gap` label and triage workflow. Tooling selection and integration must be completed and merged before Phase 2 ships; without it, this success criterion is unmeasurable. This work is tracked independently of Phase 0 — the CI harness must not be blocked on analytics procurement or tooling decisions.

4. **Working examples**: Every code example works without modification against the current release. CI enforces this.

5. **Operator confidence**: A DBA can deploy pg_ripple to production, configure it for their workload, and set up monitoring by following the operations section — without reading the source code or asking the development team.

6. **Honest comparisons**: The “When to Use pg_ripple” page contains zero factually incorrect comparison claims. Verified by a scheduled team review within 30 days of any major competing product release and within 30 days of each pg_ripple minor release. Claims that cannot be verified against current third-party documentation are replaced with links to that documentation rather than maintained as inline assertions.

7. **Community contribution**: At least two documentation PRs from external contributors are merged within six months of Phase 2 completion. This is a lagging signal that the structure is navigable enough for others to extend without guidance from the core team. Track via a `docs-contributor` GitHub PR label; declare the criterion failed if no external docs PRs arrive and investigate whether onboarding friction (missing contributor guide, complex toolchain) is the cause.

---

## Appendix: Existing Documentation Inventory

The following pages exist in `docs/src/` and should be restructured or rewritten to match this plan.

**Quality ratings**: **K** = Keep (light restructuring, content is largely sound), **R** = Rewrite (existing content is a useful starting point but needs substantial revision — typically because it is in function-listing reference format and must become a narrative guide), **X** = Replace from scratch (current content is inadequate as a starting point).

### Pages to restructure into Feature Deep Dives
- `user-guide/sql-reference/triple-crud.md` → Storing Knowledge chapter — **R**
- `user-guide/sql-reference/bulk-load.md` → Loading Data chapter — **R**
- `user-guide/sql-reference/sparql-query.md` → Querying with SPARQL chapter — **R**
- `user-guide/sql-reference/sparql-update.md` → Querying with SPARQL chapter (Update section) — **R**
- `user-guide/sql-reference/shacl.md` → Validating Data Quality chapter — **R**
- `user-guide/sql-reference/datalog.md` → Reasoning and Inference chapter — **R**
- `user-guide/sql-reference/serialization.md` → Exporting and Sharing chapter — **R**
- `user-guide/sql-reference/fts.md` → AI Retrieval & Graph RAG chapter (§2.7, full-text search section) — **R**
- `user-guide/sql-reference/http-endpoint.md` → APIs and Integration chapter — **R**
- `user-guide/sql-reference/federation.md` → APIs and Integration chapter — **R**
- `user-guide/sql-reference/framing-views.md` → Exporting and Sharing chapter — **R**

### Pages to restructure into Operations
- `user-guide/configuration.md` → Configuration and Tuning — **K**
- `user-guide/scaling.md` → Scaling — **K**
- `user-guide/operations.md` → Monitoring and Observability — **K**
- `user-guide/pre-deployment.md` → Deployment Models — **K**
- `user-guide/backup-restore.md` → Backup and Disaster Recovery — **K**
- `user-guide/upgrading.md` → Upgrading Safely — **K**

### Pages to preserve and enhance
- `user-guide/introduction.md` → Landing Page (rewrite for non-technical audience) — **R**
- `user-guide/installation.md` → Installation (expand with Docker-first approach) — **K**
- `user-guide/getting-started.md` → Hello World walkthrough (expand with expected output) — **K**
- `user-guide/playground.md` → Part of Installation (Docker section) — **K**
- `user-guide/contributing.md` → Contributing (Reference section) — **K**
- `user-guide/graphrag.md` → Exporting and Sharing chapter (§2.6, GraphRAG section) — **K**
- `user-guide/hybrid-search.md` → AI Retrieval & Graph RAG chapter (§2.7, hybrid retrieval section) — **K**
- `user-guide/rag.md` → AI Retrieval & Graph RAG chapter (§2.7, RAG pipeline section) — **K**
- `user-guide/geospatial.md` → **Deprecated**: geospatial is out of scope for §2.1–2.8 and has no current roadmap item. Replace the page content with a deprecation notice during Phase 1 and remove the page entirely in Phase 4. No `docs-gap` issue required. — **X**
- `user-guide/vector-federation.md` → AI Retrieval & Graph RAG chapter (§2.7, hybrid retrieval section) — **K**
- `user-guide/graphrag-enrichment.md` → Exporting and Sharing chapter (§2.6, alongside `graphrag.md`) — **K**

### Best-practices pages to fold into Feature Deep Dives
- `user-guide/best-practices/bulk-loading.md` → Loading Data (Performance section) — **K**
- `user-guide/best-practices/sparql-patterns.md` → Querying with SPARQL (Performance section) — **K**
- `user-guide/best-practices/shacl-patterns.md` → Validating Data Quality (Common Patterns) — **K**
- `user-guide/best-practices/datalog-optimization.md` → Reasoning and Inference (Performance) — **K**
- `user-guide/best-practices/federation-performance.md` → APIs and Integration (Performance) — **K**
- `user-guide/best-practices/update-patterns.md` → Querying with SPARQL (Update Patterns) — **K**
- `user-guide/best-practices/data-modeling.md` → Storing Knowledge (Common Patterns) — **K**

### Reference pages to preserve
- `reference/faq.md` → FAQ (expand to 25–30 questions) — **K**
- `reference/troubleshooting.md` → Troubleshooting (operations section) — **K**
- `reference/error-reference.md` → Error Message Catalog — **K**
- `reference/changelog.md` → Release Notes (mirror) — **K**
- `reference/roadmap.md` → Roadmap (mirror) — **K**
- `reference/security.md` → Security (operations section) — **K**
- `reference/w3c-conformance.md` → SPARQL Compliance Matrix — **K**
- `reference/guc-reference.md` → Fold into Configuration and Tuning — **K**
- `reference/api-stability.md` → Preserve in Reference section — **K**
