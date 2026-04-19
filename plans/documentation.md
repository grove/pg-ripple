# pg_ripple — Documentation Plan

> **Date**: 2026-04-19
> **Status**: Supersedes all prior documentation plans
> **Scope**: Complete documentation strategy, site structure, content guidelines, and delivery plan

---

## 1. Executive Summary

pg_ripple's documentation exists to answer one question: *How do I use this to solve my problem?* Every page, every example, every diagram serves that purpose. The documentation is not a mirror of the source code or a record of the development process — it is a product in its own right, designed for the people who use pg_ripple to build things.

The philosophy is problem-centric. Users arrive with goals — load data, run a query, enforce quality rules, deploy to production — and the documentation meets them where they are. A database administrator with two decades of PostgreSQL experience and a data scientist who has never touched a terminal should both find their way to working code within ten minutes. We achieve this through layered explanations: plain language up front, technical depth available on demand, and worked examples that are copy-pasteable without modification.

pg_ripple is a mature system (v0.31.0, 100% W3C SPARQL 1.1 and SHACL Core conformance) with a comprehensive feature set spanning data loading, SPARQL querying, SHACL validation, Datalog reasoning, JSON-LD framing, vector-hybrid search, federation, and HTTP endpoints. The documentation must reflect that maturity. It should read like the documentation of a system that has been in production for years — confident, precise, and complete — rather than a collection of feature announcements.

Four user archetypes drive content decisions: the Data Engineer who needs reliable data pipelines, the Application Developer who wants to power features with graph queries, the Knowledge Architect who models domains and writes inference rules, and the Decision-Maker who evaluates whether pg_ripple fits the architecture. Every page has a primary audience, and every section includes signposts that guide readers to the depth they need.

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

---

## 3. Site Structure

The documentation site is organized into four sections, each serving a distinct purpose. A reader should be able to identify which section they need within five seconds of arriving.

**Information architecture**: The site is built with mdBook. URL slugs follow the pattern `section/page-name` (e.g., `/feature-deep-dives/querying-with-sparql`). The left sidebar has two levels: sections and pages. Cross-references use relative links. mdBook does not support HTML `<details>` collapsible blocks without a preprocessor — use admonition blocks (`> **Note**`) for optional-depth content instead of `<details>`. Each page must have a `description` in its front matter for SEO; the sitemap is auto-generated. Page titles should use the vocabulary users type into search: "Load RDF Data" not "Data Loading," "Property Paths" not "Path Expressions."

### Section 1: Core Concepts & Getting Started

These pages answer foundational questions and get users to working code within ten minutes. They are the front door of the documentation — the pages linked from the README, shared in conference talks, and bookmarked by evaluators.

#### Landing Page — What Is pg_ripple?

A single screen that explains what pg_ripple does, what makes it different from other graph systems, and who it is for. No jargon. No installation instructions. Just the core value proposition: *turn your PostgreSQL database into a knowledge graph store, with SPARQL, validation, and reasoning built in, and no extra infrastructure.* Include the architecture diagram from the README and a single compelling code example (load data, query it, get results). Link to the next logical step: installation or the five-minute tutorial.

*Primary audience*: Decision-Maker, everyone arriving for the first time.

#### Why Knowledge Graphs Matter

A non-technical page (no code) explaining the problem pg_ripple solves in business terms. Use a concrete scenario: a company has customer data in one table, product data in another, interaction logs in a third, and a partner's product catalog in an external system. A knowledge graph connects all of these into a single queryable web of relationships. Show what questions become easy ("Which customers bought products similar to ones their connections recommended?") and what would be painful to answer with pure SQL. This page is for the Decision-Maker who needs to justify the technology to stakeholders.

*Primary audience*: Decision-Maker, non-technical stakeholders.

#### pg_ripple in 60 Seconds — One-Page Summary

A single printable or shareable page for evaluators who will not read the full documentation. Top half: three sentences on what pg_ripple does, one architecture diagram, and the three most important numbers (triples per second, conformance level, PostgreSQL version required). Bottom half: a comparison matrix against the three most common alternatives, a "when to use" / "when not to use" table, and a link to the installation page. This page is what gets shared in a Slack message or attached to an evaluation report. It must be accurate, unadorned, and current with every release.

*Primary audience*: Decision-Maker.

#### When to Use pg_ripple

A comparison page with an honest matrix: pg_ripple vs. plain SQL, vs. standalone RDF stores (Blazegraph, Virtuoso, Fuseki), vs. LPG systems (Neo4j, Amazon Neptune), vs. embedded solutions. For each row, state what pg_ripple does well, where it has limitations, and when the alternative is a better fit. Include a decision flowchart: "Do you already run PostgreSQL? → Do you need SPARQL? → How large is your dataset?" This page must be scrupulously honest — it builds trust with evaluators who will check claims against reality.

*Primary audience*: Decision-Maker, Application Developer evaluating options.

#### Installation

Platform-specific installation instructions: from source (cargo pgrx), Docker (docker-compose with pre-built image), and Linux packages (when available). Each method is a self-contained section with prerequisites, step-by-step commands, and a verification step (`SELECT pg_ripple.triple_count()` returns 0). Include troubleshooting for common failures: wrong PostgreSQL version, missing `shared_preload_libraries`, pgrx version mismatch. The Docker path should be the recommended default — zero dependencies beyond Docker itself.

*Primary audience*: Data Engineer, Application Developer.

#### Hello World — Five-Minute Walkthrough

The fastest path from zero to working queries. Load ten triples about people and movies (inline, no external files), run three queries of increasing complexity (simple pattern, OPTIONAL, property path), and explain each result. The reader finishes this page understanding what a triple is, what SPARQL looks like, and how results come back as a regular SQL table. Every code block is numbered and annotated. Expected output is shown after every query.

*Primary audience*: Everyone, especially first-time users.

#### Guided Tutorial — Build a Knowledge Graph in 30 Minutes

Picks up where Hello World ends. Assumes the reader has loaded and queried triples. The tutorial is structured in four independent segments of under ten minutes each, so readers can stop at any point with a working system:

1. **Load and Explore** — Import a Turtle file of movie/person/organization data; run SPARQL queries of increasing complexity (genre lookup, co-starring chains, decade aggregation); understand named graphs.
2. **Validate** — Add SHACL shapes requiring every Movie to have a title and release year; observe a violation; fix it.
3. **Reason** — Write a Datalog rule for transitive “worked-with” relationships; run `infer()`; query the derived facts.
4. **Export** — Produce a JSON-LD document shaped for a REST API using `sparql_construct_jsonld()`.

Readers who complete all four segments finish with a validated, reasoning-capable knowledge graph. Expected output is shown after every query. The movie/person/organization dataset established here is reused across the feature deep-dive chapters.

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

How to model data as triples. Walk through a concrete domain (movies, people, organizations) and show each fact becoming a triple. Explain named graphs — when you need them (multi-source data, access control, versioning) and when you don't. Cover blank nodes (anonymous entities) with honest advice about when to use them and when to avoid them. Introduce RDF-star for statements about statements (provenance, confidence scores, temporal annotations). Show how types work (`rdf:type`) and how they interact with SHACL and Datalog. The reader finishes understanding how to translate a relational schema or a domain model into an RDF graph.

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

#### 2.7 Search and Discovery

How to find knowledge when you don't know the exact IRIs. Cover full-text search: indexing literal values (names, descriptions, notes) with PostgreSQL's GIN indexes, and searching them with `fts_search()`. Introduce vector embeddings: storing embeddings alongside graph facts, building HNSW indexes, and using `pg:similar()` in SPARQL to find semantically similar entities. Show hybrid retrieval: combining a SPARQL graph pattern with a vector similarity search in a single query, using Reciprocal Rank Fusion to merge results. Cover the RAG pipeline: using pg_ripple to retrieve graph-contextualized context for language model prompts. Include the keyword expansion feature for broadening search queries automatically.

*Primary audience*: Application Developer, Knowledge Architect.

#### 2.8 APIs and Integration

How to expose pg_ripple to applications. Cover the SPARQL Protocol HTTP endpoint (`pg_ripple_http`): configuration, supported response formats (JSON, XML, CSV, Turtle, JSON-LD), authentication, and Docker Compose deployment. Show how to call pg_ripple from application code: Python with `psycopg2` or `SPARQLWrapper`, JavaScript with `pg`, Java with JDBC. Cover SPARQL federation: querying remote SPARQL endpoints alongside local data using the `SERVICE` keyword, with connection pooling, result caching, and timeout configuration. Discuss caching strategies for production: plan cache tuning, result caching at the HTTP layer, and when to use materialized SPARQL views for frequently-run queries.

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

How to upgrade from one pg_ripple version to the next. Explain the migration path: `ALTER EXTENSION pg_ripple UPDATE` walks through the chain of migration scripts (`pg_ripple--0.30.0--0.31.0.sql`, etc.). Cover pre-upgrade checks, the upgrade command, post-upgrade verification, and rollback strategy. Address zero-downtime upgrades for systems that cannot afford downtime.

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
- **Querying**: `sparql`, `sparql_ask`, `sparql_explain`, `find_triples`, `triple_count`, `fts_search`
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

#### Research and Foundations

Academic background: the VP storage model (Abadi et al.), dictionary encoding, HTAP architecture rationale, SPARQL-to-SQL compilation strategy, and Datalog evaluation techniques. Links to relevant papers. This section is for readers who want to understand why pg_ripple is designed the way it is, not how to use it.

---

## 4. Content Guidelines

### Language and Style

Write in active voice with short sentences (average 15 words). Lead with what the user can do, not how the implementation works: "Load a Turtle file in one call" before any mention of VP tables or dictionary encoding. Avoid jargon without explanation — the first use of any term links to the Glossary page. Use parenthetical asides for optional depth: "pg_ripple compiles this to a recursive CTE (a technique for following chains in SQL) with hash-based cycle detection." Put advanced implementation details in admonition blocks (`> **Advanced**`) rather than inline, to keep the main path readable.

Each paragraph conveys a complete thought in 150–250 words. Resist the urge to break prose into bullet lists unless the content is genuinely a checklist, a comparison matrix, or a sequence of steps. Bullet-point avalanches feel comprehensive but communicate poorly — readers skim them without absorbing the relationships between ideas.

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

Use consistent sample data across related examples. The guided tutorial establishes a movie/person/organization dataset; feature deep-dives reuse it where possible so readers build familiarity. When a feature needs different data, introduce it explicitly.

### Organization

One problem per page. If a user searches "how do I bulk-load a Turtle file?", they find exactly that workflow on the Loading Data page — not scattered across a function reference, a best-practices guide, and a configuration page.

Mark sections with difficulty levels: **Beginner**, **Intermediate**, **Advanced**. Each page lists prerequisites ("read this first") and related pages ("go here next"). Feature deep-dive pages follow the consistent seven-part structure described in Section 3 so readers always know where to find what they need.

### Versioned Callouts

Features introduced in specific versions get a callout: `> **Available since v0.10.0**`. This helps users on older versions understand which sections apply to them.

### Documentation Versioning

The documentation tracks the current release. When a feature changes incompatibly, add a versioned callout explaining both old and new behaviour. Do not maintain separate documentation trees for old versions — migration guides and CHANGELOG provide backward-compatibility information. When a deprecation is introduced, add `> **Deprecated since vX.Y.Z**` alongside the existing `> **Available since**` callout. Remove deprecated content only after two minor releases have passed.

### Term Formatting

Follow project conventions: IRIs in `<angle brackets>`, literals in `"double quotes"^^xsd:type`, blank nodes as `_:label`, function names as `function_name()`, GUC parameters as `parameter_name`.

### Discoverability

Each page sets a `title` and `description` in mdBook front matter. Titles follow the pattern `{Task} — pg_ripple` (e.g., "Load RDF Data — pg_ripple"). Descriptions are one sentence of 100–160 characters phrased as a concrete capability ("Load Turtle, N-Triples, or RDF/XML files into pg_ripple at over 100,000 triples per second."). The sitemap is auto-generated by mdBook. Heading text in feature chapters must use the vocabulary users type into search engines: "property paths" not "path expressions," "bulk loading" not "batch insertion." The SQL Function Reference groups functions by task to match how users search, not alphabetically.

---

## 5. Delivery Strategy

### Phase 0: CI Test Harness (Prerequisite)

Build the infrastructure that keeps examples honest before writing any new pages. The harness is a script that: (1) spins up a local pg_ripple instance via `cargo pgrx run pg18` or Docker, (2) extracts fenced SQL code blocks from markdown files under `docs/src/`, (3) executes them in document order with per-file setup and teardown, and (4) compares stdout against expected output embedded in the markdown as a comment block directly below each code block. Fixture data lives in `docs/fixtures/` and is loaded once per test run. The CI job runs the harness on every PR that touches `docs/`. Without this infrastructure, Phase 1 examples will rot immediately. Estimated scope: one or two days.

### Phase 1: Foundation (Immediate)

Rewrite the landing page, installation, hello-world walkthrough, and key-concepts pages. These are the front door — they determine first impressions. Restructure the existing SQL reference pages into the feature deep-dive format, converting function-by-function listings into narrative guides organized around user goals. Establish the CI pipeline that tests every code example.

### Phase 2: Core Features

Write the eight feature deep-dive chapters: Storing Knowledge, Loading Data, Querying with SPARQL, Validating Data Quality, Reasoning and Inference, Exporting and Sharing, Search and Discovery, APIs and Integration. Each chapter is complete and publishable independently. Prioritize by user traffic: Loading Data and Querying with SPARQL first (the most common entry points), then Validating and Reasoning (differentiation features), then Export, Search, and APIs.

### Phase 3: Operations

Write the full operations section: Architecture, Deployment, Configuration, Monitoring, Performance, Backup, Upgrading, Scaling, Troubleshooting, Security. The configuration and troubleshooting pages are highest priority — they are the pages operators reach for first.

### Phase 4: Reference and Polish

Complete the SQL function reference, SPARQL compliance matrix, error catalog, FAQ, and glossary. Audit all cross-references. Review every code example against the current release. Solicit feedback from early users and fill gaps.

### Testing

Every code example runs against a real pg_ripple instance in CI. The test harness extracts SQL blocks from markdown files, executes them in order, and verifies that output matches expectations. Broken examples fail the build. This is non-negotiable — untested examples rot within weeks.

### Iteration

User questions on GitHub drive documentation improvements. Every question that takes more than five minutes to answer becomes a candidate for a new page or an expansion of an existing one. Track documentation gaps as GitHub issues with a `docs` label.

---

## 6. Success Criteria

Documentation succeeds when:

1. **Five-minute evaluation**: A new user answers "What is pg_ripple? Should I use it?" within five minutes of arriving at the documentation site.

2. **Thirty-minute onboarding**: A developer loads data, runs a query, and understands the core concepts within thirty minutes, following the guided tutorial.

3. **Self-service problem solving**: Users resolve 80% of their problems by searching the documentation. The remaining 20% require asking on GitHub — and each of those questions becomes a documentation improvement. Tracked via: a “Was this helpful?” two-button widget on each page (results posted to a dedicated GitHub Discussion thread monthly), GitHub Discussions tagged with a `docs-gap` label in a dedicated category, and a quarterly review of site search queries that return zero results.

4. **Working examples**: Every code example works without modification against the current release. CI enforces this.

5. **Operator confidence**: A DBA can deploy pg_ripple to production, configure it for their workload, and set up monitoring by following the operations section — without reading the source code or asking the development team.

6. **Honest comparisons**: The "When to Use pg_ripple" page is cited by third-party reviewers as a model of honest, useful comparison documentation.

7. **Community contribution**: Documentation PRs from external contributors indicate that the structure is clear enough for others to extend.

---

## Appendix: Existing Documentation Inventory

The following pages exist in `docs/src/` and should be restructured or rewritten to match this plan:

### Pages to restructure into Feature Deep Dives
- `user-guide/sql-reference/triple-crud.md` → Storing Knowledge chapter
- `user-guide/sql-reference/bulk-load.md` → Loading Data chapter
- `user-guide/sql-reference/sparql-query.md` → Querying with SPARQL chapter
- `user-guide/sql-reference/sparql-update.md` → Querying with SPARQL chapter (Update section)
- `user-guide/sql-reference/shacl.md` → Validating Data Quality chapter
- `user-guide/sql-reference/datalog.md` → Reasoning and Inference chapter
- `user-guide/sql-reference/serialization.md` → Exporting and Sharing chapter
- `user-guide/sql-reference/fts.md` → Search and Discovery chapter
- `user-guide/sql-reference/http-endpoint.md` → APIs and Integration chapter
- `user-guide/sql-reference/federation.md` → APIs and Integration chapter
- `user-guide/sql-reference/framing-views.md` → Exporting and Sharing chapter

### Pages to restructure into Operations
- `user-guide/configuration.md` → Configuration and Tuning
- `user-guide/scaling.md` → Scaling
- `user-guide/operations.md` → Monitoring and Observability
- `user-guide/pre-deployment.md` → Deployment Models
- `user-guide/backup-restore.md` → Backup and Disaster Recovery
- `user-guide/upgrading.md` → Upgrading Safely

### Pages to preserve and enhance
- `user-guide/introduction.md` → Landing Page (rewrite for non-technical audience)
- `user-guide/installation.md` → Installation (expand with Docker-first approach)
- `user-guide/getting-started.md` → Hello World walkthrough (expand with expected output)
- `user-guide/playground.md` → Part of Installation (Docker section)
- `user-guide/contributing.md` → Contributing (Reference section)
- `user-guide/graphrag.md` → Exporting and Sharing chapter (GraphRAG section)
- `user-guide/hybrid-search.md` → Search and Discovery chapter
- `user-guide/rag.md` → Search and Discovery chapter (RAG pipeline section)

### Best-practices pages to fold into Feature Deep Dives
- `user-guide/best-practices/bulk-loading.md` → Loading Data (Performance section)
- `user-guide/best-practices/sparql-patterns.md` → Querying with SPARQL (Performance section)
- `user-guide/best-practices/shacl-patterns.md` → Validating Data Quality (Common Patterns)
- `user-guide/best-practices/datalog-optimization.md` → Reasoning and Inference (Performance)
- `user-guide/best-practices/federation-performance.md` → APIs and Integration (Performance)
- `user-guide/best-practices/update-patterns.md` → Querying with SPARQL (Update Patterns)
- `user-guide/best-practices/data-modeling.md` → Storing Knowledge (Common Patterns)

### Reference pages to preserve
- `reference/faq.md` → FAQ (expand to 25–30 questions)
- `reference/troubleshooting.md` → Troubleshooting (operations section)
- `reference/error-reference.md` → Error Message Catalog
- `reference/changelog.md` → Release Notes (mirror)
- `reference/roadmap.md` → Roadmap (mirror)
- `reference/security.md` → Security (operations section)
- `reference/w3c-conformance.md` → SPARQL Compliance Matrix
- `reference/guc-reference.md` → Fold into Configuration and Tuning
- `reference/api-stability.md` → Preserve in Reference section
