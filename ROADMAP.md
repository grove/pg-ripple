# pg_triple — Roadmap

> From **0.1.0** (foundation) to **1.0.0** (production-ready triple store)

## How to read this roadmap

Each release below has two layers:

- **The plain-language summary** (in the coloured box) explains *what* the release delivers and *why it matters* — no programming knowledge required.
- **The technical deliverables** list the specific items developers will build. Feel free to skip these if you're reading for the big picture.

**Effort estimates** are given as *person-weeks* — e.g. "6–8 pw" means the release would take roughly 6–8 weeks for a single full-time developer, or 3–4 weeks for a pair working together. The total estimated effort from v0.1.0 to v1.0.0 is **95–122 person-weeks** (~22–28 months for one developer; ~11–14 months for a pair).

---

## Overview at a glance

| Version | Name | What it delivers (one sentence) | Effort |
|---|---|---|---|
| 0.1.0 | Foundation | Install the extension, store and retrieve facts | 6–8 pw |
| 0.2.0 | Vertical Partitioning | Fast storage layout and bulk data import | 6–8 pw |
| 0.3.0 | SPARQL Basic | Ask questions in the standard RDF query language | 6–8 pw |
| 0.4.0 | SPARQL Advanced | Follow chains of relationships, compute totals, search text | 8–10 pw |
| 0.5.0 | HTAP Architecture | Heavy reads and writes at the same time without slowdowns | 8–10 pw |
| 0.6.0 | SHACL Core | Define data quality rules; reject bad data on insert | 4–6 pw |
| 0.7.0 | SHACL Advanced | Complex data quality rules with background checking | 4–6 pw |
| 0.8.0 | Serialization | Import and export data in all standard RDF file formats | 3–4 pw |
| 0.9.0 | Datalog Reasoning | Automatically derive new facts from rules and logic | 10–12 pw |
| 0.10.0 | SPARQL Views | Live, always-up-to-date dashboards from SPARQL queries | 4–6 pw |
| 0.11.0 | SPARQL Update | Standard RDF write operations (add / change / delete) | 4–6 pw |
| 0.12.0 | Performance | Speed tuning, benchmarks, production-grade throughput | 6–8 pw |
| 0.13.0 | Admin & Security | Operations tooling, access control, docs, packaging | 4–6 pw |
| 0.14.0 | SPARQL Protocol | Standard HTTP API so web apps and tools can query directly | 3–4 pw |
| 0.15.0 | SPARQL Federation | Query remote SPARQL endpoints alongside local data | 4–6 pw |
| 0.16.0 | RDF-star / RDF 1.2 | Make statements about statements (provenance, annotations) | 8–10 pw |
| 1.0.0 | Production Release | Standards conformance, stress testing, security audit | 6–8 pw |
| | | **Total estimated effort** | **95–122 pw** |

---

## v0.1.0 — Foundation

**Theme**: Core data model, dictionary encoding, and basic triple CRUD.

> **In plain language:** This is the "hello world" release. After installing pg_triple into a PostgreSQL database, a user can store facts (called *triples* — think "subject → relationship → object", e.g. "Alice → knows → Bob") and retrieve them by pattern. No query language yet — just the basic building blocks. Internally, every piece of text (names, URLs, values) is converted to a compact number for fast storage and comparison. This release also sets up automated testing so that every future change is verified.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] pgrx 0.17 project scaffolding targeting PostgreSQL 18
- [ ] Extension bootstrap: `CREATE EXTENSION pg_triple` creates `_pg_triple` schema
- [ ] **Dictionary encoder**
  - Resource dictionary table (IRIs, blank nodes)
  - Literal dictionary table (typed values)
  - XXH3-128 hash-based dedup
  - Encode/decode SQL functions: `pg_triple.encode_iri()`, `pg_triple.decode_id()`
- [ ] **Single VP table** (non-partitioned, flat triple table as stepping stone)
  - `_pg_triple.triples (s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
  - Composite B-tree indices: `(s, p, o)`, `(p, o, s)`, `(o, s, p)`
- [ ] **Basic triple CRUD**
  - `pg_triple.insert_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_triple.delete_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_triple.triple_count() RETURNS BIGINT`
- [ ] **Basic querying** (SQL-level, no SPARQL yet)
  - `pg_triple.find_triples(s TEXT, p TEXT, o TEXT) RETURNS TABLE` — any param can be NULL for wildcard
- [ ] Unit tests for dictionary encode/decode round-trips
- [ ] Integration test: insert + query cycle
- [ ] CI pipeline (GitHub Actions)

### Exit Criteria

A user can install the extension, insert triples, and query them back by pattern.

---

## v0.2.0 — Vertical Partitioning

**Theme**: Per-predicate table layout for real performance, with Turtle and N-Triples bulk loading.

> **In plain language:** This release reorganises how data is stored internally so that queries run much faster — instead of one giant table, each type of relationship (e.g. "knows", "worksAt", "hasEmail") gets its own optimised table. It also adds *bulk import*: users can load large RDF data files (in Turtle and N-Triples formats) in one go, rather than inserting facts one at a time. Named graphs (the ability to group facts into labelled collections) are introduced here too.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Dynamic VP table management**
  - Auto-create `_pg_triple.vp_{predicate_id}` tables on first triple with a new predicate
  - Predicate catalog: `_pg_triple.predicates (id BIGINT, table_oid OID, triple_count BIGINT)`
  - Dual B-tree indices per VP table: `(s, o)` and `(o, s)`
- [ ] **Rare-predicate consolidation table**
  - Predicates with fewer than `pg_triple.vp_promotion_threshold` triples (default: 1,000) are stored in a shared `_pg_triple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT)` table with a composite index on `(p, s, o)`
  - Once a predicate crosses the threshold, its rows are migrated to a dedicated VP table and the catalog updated — transparent to callers
  - Prevents catalog bloat for predicate-rich datasets (DBpedia ≈60K predicates, Wikidata ≈10K) — avoids hundreds of thousands of PG objects, reduces planner overhead, and cuts VACUUM cost
- [ ] **Migrate CRUD to VP storage**
  - `insert_triple()` routes to correct VP table
  - `delete_triple()` targets correct VP table
  - `find_triples()` queries one or all VP tables depending on whether predicate is bound
- [ ] **Named graph support** (basic)
  - `g` column in VP tables
  - `pg_triple.create_graph()`, `pg_triple.drop_graph()`, `pg_triple.list_graphs()`
- [ ] **Bulk loader** (N-Triples)
  - `pg_triple.load_ntriples(data TEXT) RETURNS BIGINT`
  - Streaming parser via `rio_turtle` crate
  - Batch encoding + COPY for throughput
- [ ] **Bulk loader** (Turtle)
  - `pg_triple.load_turtle(data TEXT) RETURNS BIGINT`
  - Prefix declarations auto-registered
  - Blank node scoping per load operation
  - `rio_turtle` crate already handles both formats — incremental parser work
- [ ] **IRI prefix management**
  - `pg_triple.register_prefix(prefix TEXT, expansion TEXT)`
  - `pg_triple.prefixes() RETURNS TABLE`
  - Prefix expansion in encode/decode paths
- [ ] Benchmarks: insert throughput (1M triples)
- [ ] pg_regress test suite: `triple_crud.sql`, `named_graphs.sql`

### Exit Criteria

VP layout operational. Rare-predicate consolidation table absorbs low-frequency predicates. Bulk loading >50K triples/sec on commodity hardware. Named graphs functional. Both N-Triples and Turtle data can be loaded.

---

## v0.3.0 — SPARQL Query Engine (Basic)

**Theme**: Parse and execute SPARQL SELECT and ASK queries with basic graph patterns. N-Triples export for test verification.

> **In plain language:** SPARQL is the standard language for asking questions over linked data — the same way SQL is for relational databases. This release makes pg_triple understand SPARQL, so users can write queries like *"find all people who know someone who works at Acme Corp"* using the official W3C syntax. It also adds the ability to export data back out as a file (N-Triples format), which is essential for checking results and sharing data with other tools.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **SPARQL parser integration** (`spargebra` crate)
  - Parse SPARQL SELECT and ASK queries into algebra tree
  - Support: Basic Graph Patterns (BGP), FILTER, OPTIONAL, LIMIT, OFFSET, ORDER BY, DISTINCT
- [ ] **SQL generator** (initial)
  - BGP → JOIN across VP tables (integer equality)
  - FILTER → WHERE clause on integer-encoded values
  - OPTIONAL → LEFT JOIN
  - LIMIT/OFFSET/ORDER BY passthrough
  - DISTINCT → SQL DISTINCT
- [ ] **Query executor**
  - `pg_triple.sparql(query TEXT) RETURNS SETOF JSONB`
  - SPI execution of generated SQL
  - **Batch dictionary decode**: collect all output i64 IDs from the result set, decode in a single `WHERE id = ANY(...)` query, build an in-memory lookup map, then emit human-readable rows — avoids per-row dictionary round-trips
- [ ] **SPARQL ASK**
  - ASK → `SELECT EXISTS(...)` → returns BOOLEAN
  - `pg_triple.sparql_ask(query TEXT) RETURNS BOOLEAN`
- [ ] **N-Triples export** (basic)
  - `pg_triple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - Streaming variant returning `SETOF TEXT` for large graphs
  - Essential for test verification and debugging
- [ ] **Join optimizations** (phase 1)
  - Self-join elimination for star patterns
  - Filter pushdown: encode FILTER constants before SQL generation
- [ ] `pg_triple.sparql_explain(query TEXT) RETURNS TEXT` — show generated SQL
- [ ] pg_regress: `sparql_queries.sql` (20+ test queries)

### Exit Criteria

Users can run SPARQL SELECT and ASK queries with BGPs, FILTER, OPTIONAL against data loaded via bulk load. Queries return correct results. Data can be exported as N-Triples for verification.

---

## v0.4.0 — SPARQL Query Engine (Advanced)

**Theme**: Property paths, UNION, aggregates, subqueries.

> **In plain language:** This release teaches the query engine to handle more powerful questions. *Property paths* let you follow chains of relationships — e.g. "find everyone reachable through any number of 'knows' links" (like a social network friend-of-a-friend search). *Aggregates* let you compute totals and averages ("how many people work in each department?"). *Full-text search* lets you search through text values efficiently — e.g. "find all articles whose title contains 'climate change'". Together these cover the vast majority of real-world SPARQL queries.
>
> **Effort estimate: 8–10 person-weeks**

### Deliverables

- [ ] **Property path compilation**
  - `+` (one or more) → `WITH RECURSIVE` CTE
  - `*` (zero or more) → `WITH RECURSIVE` CTE with zero-hop anchor
  - `?` (zero or one) → `UNION` of direct + zero-hop
  - `/` (sequence) → chained joins
  - `|` (alternative) → `UNION`
  - `^` (inverse) → swap `s`/`o`
  - Cycle detection via PG18 `CYCLE` clause (hash-based, replaces array-based visited tracking for $O(1)$ membership checks instead of $O(n)$ array scans)
  - `pg_triple.max_path_depth` GUC
- [ ] **UNION / MINUS**
  - UNION → SQL `UNION`
  - MINUS → SQL `EXCEPT`
- [ ] **Aggregates**
  - COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT
  - GROUP BY → SQL GROUP BY
  - HAVING → SQL HAVING
- [ ] **Subqueries**
  - Nested SELECT in WHERE / FROM clause
- [ ] **BIND / VALUES**
  - BIND → SQL column alias
  - VALUES → SQL VALUES clause
- [ ] **Advanced join optimizations**
  - Optional-self-join elimination
  - Self-union elimination (UNION → WHERE IN)
  - Projection pushing for DISTINCT queries
- [ ] **Full-text search on literals**
  - `pg_triple.fts_index(predicate TEXT)` — create a GIN `tsvector` index on the literal dictionary for a predicate
  - SPARQL `CONTAINS()` and `REGEX()` FILTERs on indexed predicates rewrite to `@@` / `LIKE` against the GIN index
  - `pg_triple.fts_search(query TEXT, predicate TEXT) RETURNS TABLE` — direct full-text search API
  - Index is maintained incrementally on `insert_triple()` for indexed predicates
- [ ] Benchmark: SP2Bench subset
- [ ] pg_regress: `property_paths.sql`, `aggregates.sql`, `fts_search.sql`

### Exit Criteria

SPARQL 1.1 Query coverage for all major features except federated queries. Property path queries complete with hash-based cycle detection via PG18 `CYCLE` clause. Full-text search on indexed literal predicates is functional.

---

## v0.5.0 — HTAP Architecture

**Theme**: Separate read and write paths for concurrent OLTP/OLAP.

> **In plain language:** In a real production system, people are loading new data and running complex queries at the same time. Without special care, these two activities interfere with each other — writes block reads and vice versa. This release splits the storage into a "write inbox" and a "read-optimised archive" so both can happen simultaneously at full speed. It also adds a *change notification* system: applications can subscribe to be told whenever specific facts change (useful for triggering workflows, updating caches, or feeding dashboards). An in-memory cache makes repeated lookups much faster. Optionally, the companion pg_trickle extension enables automatically-updating live statistics.
>
> **Effort estimate: 8–10 person-weeks**

### Deliverables

- [ ] **Delta/Main partition split**
  - Each VP table gets `_delta` and `_main` suffixes
  - All writes target `_delta`; `_main` is append-only / read-optimized
  - Query path: `UNION ALL` of `_main` and `_delta`
- [ ] **Background merge worker**
  - pgrx `BackgroundWorker` implementation
  - Configurable merge threshold via `pg_triple.merge_threshold` GUC
  - SPI-based merge: INSERT from delta → main, TRUNCATE delta
  - BRIN index rebuild on main post-merge
  - Shared-memory latch signaling
- [ ] **Bloom filter for delta existence checks**
  - In shared memory, per VP table
  - Queries against main-only data skip delta scan
- [ ] **Dictionary LRU cache in shared memory**
  - `pg_triple.dictionary_cache_size` GUC
  - Shared across all backends via pgrx `PgSharedMem`
  - **Sharded lock design**: partition the hash map into N shards (default: 64), each with its own lightweight lock — eliminates global lock contention under concurrent encode/decode workloads
- [ ] **Shared-memory budget & back-pressure**
  - `pg_triple.shared_memory_limit` GUC — total memory budget for dictionary cache + bloom filters + merge worker buffers
  - Automatic eviction priority: bloom filters reclaimed first, then oldest LRU dictionary entries
  - Back-pressure on bulk loads when shared memory is >90% utilised — throttle batch size to prevent OOM
- [ ] **Statistics**
  - `pg_triple.stats()` JSONB: triple count, per-predicate counts, cache hit ratio, delta/main sizes
- [ ] **pg_trickle integration: live statistics** *(optional, when pg_trickle is installed)*
  - `pg_triple.enable_live_statistics()` creates `_pg_triple.predicate_stats` and `_pg_triple.graph_stats` stream tables
  - `pg_triple.stats()` reads from stream tables instead of full-scanning VP tables (100–1000× faster)
- [ ] **Change notification / CDC**
  - `pg_triple.subscribe(pattern TEXT, channel TEXT)` — emit `NOTIFY` on triple changes matching a predicate/graph pattern
  - Thin trigger-based CDC on VP delta tables; fires on INSERT/DELETE
  - Payload: JSON with `{"op": "insert"|"delete", "s": ..., "p": ..., "o": ..., "g": ...}` (integer IDs)
  - `pg_triple.unsubscribe(channel TEXT)` to remove subscriptions
  - Enables downstream event-driven architectures (CDC consumers, webhooks, cache invalidation)
- [ ] Benchmark: concurrent read/write (pgbench custom scripts)
- [ ] pg_regress: `htap_merge.sql`, `change_notification.sql`

### Exit Criteria

Writes do not block reads. Merge worker operates correctly. >100K triples/sec bulk insert sustained. Change notifications fire correctly for matching patterns.

---

## v0.6.0 — SHACL Validation (Core)

**Theme**: Data integrity enforcement via W3C SHACL shapes.

> **In plain language:** SHACL is a standard way to define *data quality rules* — for example, "every Person must have exactly one email address" or "an age must be a number". When these rules are loaded, pg_triple can automatically reject data that violates them the moment it is inserted, rather than discovering errors later. This is similar to how a spreadsheet can reject invalid entries in a cell. A validation report function lets you check existing data against the rules at any time.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **SHACL parser** (Turtle-based shapes)
  - `pg_triple.load_shacl(data TEXT)` — parse and store shapes
  - Internal shape IR stored in `_pg_triple.shacl_shapes`
- [ ] **Static constraint compilation**
  - `sh:minCount` → NOT NULL / CHECK trigger
  - `sh:maxCount` → UNIQUE index
  - `sh:datatype` → CHECK on literal datatype
  - `sh:in` → CHECK with allowed values
  - `sh:pattern` → regex CHECK
- [ ] **Synchronous validation mode**
  - Triggered on `insert_triple()` when `pg_triple.enable_shacl = 'sync'`
  - Returns validation error immediately on constraint violation
- [ ] **Validation report**
  - `pg_triple.validate(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Full SHACL validation report as JSON
- [ ] **SHACL management**
  - `pg_triple.list_shapes() RETURNS TABLE`
  - `pg_triple.drop_shape(shape_uri TEXT)`
- [ ] **pg_trickle integration: SHACL violation monitors** *(optional)*
  - Simple cardinality/datatype constraints modeled as `IMMEDIATE` mode stream tables
  - Violations detected within the same transaction as the DML
- [ ] pg_regress: `shacl_validation.sql`

### Exit Criteria

Core SHACL constraints are enforced at insert time. Validation reports conform to SHACL spec.

---

## v0.7.0 — SHACL Advanced

**Theme**: Async validation pipeline and complex shapes.

> **In plain language:** Builds on v0.6.0 by supporting more sophisticated data quality rules — for instance, "a person's address must be either a US address or a EU address (but not both)", or "if a company has more than 50 employees, it must have a compliance officer". It also adds a *background validation mode* so that checking complex rules doesn't slow down data loading — violations are flagged asynchronously and collected in a report queue.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **Asynchronous validation pipeline**
  - Validation queue table: `_pg_triple.validation_queue`
  - Background worker processes queue in batches
  - Dead letter queue for invalid triples with violation reports
  - `pg_triple.enable_shacl = 'async'` GUC mode
- [ ] **Complex shape support**
  - `sh:class` — type constraint via `rdf:type` lookup
  - `sh:node` — nested shape references
  - `sh:or` / `sh:and` / `sh:not` — logical constraint combinators
  - `sh:qualifiedValueShape` — qualified cardinality
- [ ] **pg_trickle integration: multi-shape DAG validation** *(optional)*
  - Multiple SHACL shapes as a DAG of stream tables with topologically-ordered refresh
- [ ] pg_regress: `shacl_advanced.sql`

### Exit Criteria

Async validation pipeline operational. Complex SHACL shapes validated correctly.

---

## v0.8.0 — Serialization, Export & Interop

**Theme**: Full RDF I/O, remaining serialization formats, and SPARQL CONSTRUCT/DESCRIBE.

> **In plain language:** RDF data comes in several standard file formats (Turtle, RDF/XML, JSON-LD). This release completes the set so that pg_triple can import from and export to *all* of them — making it easy to exchange data with other tools and systems. It also adds SPARQL CONSTRUCT (generate new triples from a query) and DESCRIBE (get everything known about a given entity), completing the four standard SPARQL query forms.
>
> **Effort estimate: 3–4 person-weeks** *(the hardest parts — Turtle import and N-Triples export — were already delivered in v0.2.0 and v0.3.0)*

*Note: Turtle import and N-Triples export were delivered in v0.2.0 and v0.3.0 respectively.*

### Deliverables

- [ ] **RDF/XML parser**
  - `pg_triple.load_rdfxml(data TEXT) RETURNS BIGINT`
- [ ] **Export functions**
  - `pg_triple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_triple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Streaming variants returning `SETOF TEXT` for large graphs
- [ ] **SPARQL CONSTRUCT / DESCRIBE**
  - CONSTRUCT → returns triples as Turtle or JSONB
  - DESCRIBE → concentric bounded description
- [ ] pg_regress: `serialization.sql`, `sparql_construct.sql`

### Exit Criteria

Round-trip: load Turtle → query → export Turtle. All major RDF serialization formats supported for both import and export.

---

## v0.9.0 — Datalog Reasoning Engine

**Theme**: General-purpose rule-based inference over the triple store.

> **In plain language:** This is the "intelligence layer". Users can define logical rules like *"if A manages B and B manages C, then A indirectly manages C"* — and the system will automatically figure out all the indirect management chains. It ships with two built-in rule sets covering the standard RDF and OWL vocabularies (the common language of the Semantic Web), so it can automatically derive facts like "if a Dog is a subclass of Animal, and Rex is a Dog, then Rex is also an Animal". Rules can also express *things that must never be true* — for example, "no one can be their own manager" — acting as logical integrity constraints. This is the largest single release in the roadmap.
>
> **Effort estimate: 10–12 person-weeks**

See [plans/ecosystem/datalog.md](plans/ecosystem/datalog.md) for the full design.

### Deliverables

- [ ] **Rule parser** (`src/datalog/parser.rs`)
  - Turtle-flavoured Datalog syntax: `head :- body₁, body₂, … .`
  - Variables (`?x`), prefixed IRIs, literals, named graph scoping (`GRAPH`)
  - Stratified negation via `NOT` keyword
- [ ] **Stratification engine** (`src/datalog/stratify.rs`)
  - Predicate dependency graph with positive/negative edges
  - SCC-based stratification with clear error messages for unstratifiable programs
- [ ] **SQL compiler** (`src/datalog/compiler.rs`)
  - Non-recursive rules → `INSERT … SELECT … ON CONFLICT DO NOTHING`
  - Recursive rules → `WITH RECURSIVE … CYCLE`
  - Negation → `NOT EXISTS` (higher strata only)
  - All constants dictionary-encoded before SQL generation (integer joins everywhere)
- [ ] **Arithmetic built-ins**
  - Comparison operators (`>`, `>=`, `<`, `<=`, `=`, `!=`) → SQL `WHERE` clause expressions
  - Arithmetic expressions (`?z IS ?x + ?y`) → SQL computed columns
  - String functions (`STRLEN`, `REGEX`) → SQL `LENGTH`, `~` with dictionary decode join
- [ ] **Constraint rules (integrity constraints)**
  - Empty-head rules (`:- body .`) express patterns that must never hold
  - Compile to existence checks; materialized mode → pg_trickle IMMEDIATE stream tables for in-transaction validation
  - `pg_triple.check_constraints()` returns violations as JSONB
  - `pg_triple.enforce_constraints` GUC: `'error'` / `'warn'` / `'off'`
  - Directly complements and extends SHACL validation
- [ ] **Built-in rule sets** (`src/datalog/builtins.rs`)
  - `pg_triple.load_rules_builtin('rdfs')` — W3C RDFS entailment (13 rules)
  - `pg_triple.load_rules_builtin('owl-rl')` — W3C OWL 2 RL profile (~80 rules)
- [ ] **On-demand execution mode** (no pg_trickle needed)
  - Derived predicates compiled to inline CTEs injected into SPARQL→SQL at query time
  - `SET pg_triple.inference_mode = 'on_demand'`
- [ ] **Materialized execution mode** *(optional, requires pg_trickle)*
  - `pg_triple.materialize_rules(schedule => '10s')` — derived predicates as stream tables
  - pg_trickle DAG scheduler respects stratum ordering automatically
- [ ] **Catalog and management**
  - `_pg_triple.rules` catalog table
  - Derived predicates registered in `_pg_triple.predicates` with `derived = TRUE`
  - `pg_triple.load_rules()`, `pg_triple.list_rules()`, `pg_triple.drop_rules()`
- [ ] **SPARQL engine integration**
  - Derived VP tables transparent to query planner (same lookup path as base VP tables)
  - On-demand mode prepends CTEs to generated SQL
- [ ] **SHACL-AF `sh:rule` bridge**
  - Detect `sh:rule` entries in loaded SHACL shapes that contain Datalog-compatible triple rules
  - Compile `sh:rule` bodies to Datalog IR and register in `_pg_triple.rules`
  - Bidirectional: SHACL shapes inform Datalog constraints; Datalog-derived triples are visible to SHACL validation
  - `pg_triple.load_shacl()` auto-registers any `sh:rule` triples as Datalog rules when `pg_triple.inference_mode != 'off'`
- [ ] pg_regress: `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_custom.sql`, `datalog_negation.sql`, `datalog_arithmetic.sql`, `datalog_constraints.sql`, `shacl_af_rule.sql`

### Exit Criteria

Users can load RDFS or OWL RL rule sets (or custom rules), and SPARQL queries return inferred triples. Arithmetic built-ins filter correctly in rule bodies. Constraint rules detect and report violations (optionally rejecting transactions). Both on-demand and materialized modes operational. Stratified negation correctly validated and compiled. SHACL shapes with `sh:rule` entries are auto-compiled to Datalog rules.

---

## v0.10.0 — Incremental SPARQL Views & ExtVP

**Theme**: Always-fresh materialized SPARQL queries and extended vertical partitioning via pg_trickle stream tables.

> **In plain language:** Imagine pinning a SPARQL query to a dashboard and having the results update automatically whenever the underlying data changes — without re-running the query. That's what SPARQL views deliver. Under the hood, only the *changed* rows are reprocessed (not the entire dataset), so updates are nearly instantaneous. This release also adds precomputed "shortcut" tables for frequently-combined queries, making common access patterns dramatically faster. Requires the companion pg_trickle extension.
>
> **Effort estimate: 4–6 person-weeks**

See [plans/ecosystem/pg_trickle.md § 2.2](plans/ecosystem/pg_trickle.md) for the full design.

### Deliverables

- [ ] **SPARQL views** *(requires pg_trickle)*
  - `pg_triple.create_sparql_view(name, sparql, schedule, decode)` — compile a SPARQL SELECT query into an always-fresh, incrementally-maintained stream table
  - `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top, minimising CDC surface
  - `pg_triple.drop_sparql_view(name)` and `pg_triple.list_sparql_views()` for lifecycle management
  - `_pg_triple.sparql_views` catalog table: records original SPARQL text, generated SQL, schedule, decode mode, and stream table OID
  - Refresh mode heuristics: `IMMEDIATE` for constraint-style queries, `DIFFERENTIAL` + schedule for dashboards, `FULL` + long schedule for heavy analytics and transitive-closure property paths
- [ ] **ExtVP semi-join stream tables** *(requires pg_trickle)*
  - Manual creation of pre-computed semi-joins between frequently co-joined predicate pairs
  - SPARQL→SQL translator rewrites queries to target ExtVP tables when available
- [ ] **SPARQL views over derived predicates**
  - SPARQL views can reference Datalog-derived VP tables; pg_trickle DAG handles refresh ordering
- [ ] pg_regress: `sparql_views.sql`, `extvp.sql`

### Exit Criteria

Users can create SPARQL views that stay incrementally up-to-date. SPARQL view queries are sub-millisecond table scans. ExtVP semi-joins improve multi-predicate star-pattern performance.

---

## v0.11.0 — SPARQL Update

**Theme**: W3C SPARQL 1.1 Update support for standard-compliant write operations.

> **In plain language:** Up to this point, data is loaded via bulk import or the pg_triple-specific insert functions. This release adds the *standard* way to add, change, and delete data using the SPARQL Update language — the same syntax that every other RDF tool understands. This means tools like Protégé (an ontology editor), TopBraid, and SPARQL workbenches can write data directly to pg_triple without a custom adapter. It also adds commands for managing named graphs (create, clear, drop) and loading data from a URL.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **INSERT DATA**
  - Parse and execute `INSERT DATA { … }` statements
  - Route through dictionary encoder + VP table insert path
  - Named graph support: `INSERT DATA { GRAPH <g> { … } }`
- [ ] **DELETE DATA**
  - Parse and execute `DELETE DATA { … }` statements
  - Exact-match triple deletion from VP tables
  - Named graph support
- [ ] **DELETE/INSERT WHERE** (graph update)
  - Pattern-based update: `DELETE { … } INSERT { … } WHERE { … }`
  - Compile WHERE clause via existing SPARQL→SQL engine
  - Transactional: delete + insert in single statement
- [ ] **LOAD / CLEAR / DROP / CREATE**
  - `LOAD <url>` — fetch and load remote RDF data (HTTP GET + parser)
  - `CLEAR GRAPH <g>` — delete all triples in a named graph
  - `DROP GRAPH <g>` — clear + remove graph from registry
  - `CREATE GRAPH <g>` — register a new empty named graph
- [ ] **SPARQL Update executor**
  - `pg_triple.sparql_update(query TEXT) RETURNS BIGINT` — returns count of affected triples
  - Reuse existing SPARQL parser (`spargebra` supports SPARQL Update algebra)
- [ ] pg_regress: `sparql_insert_data.sql`, `sparql_delete_data.sql`, `sparql_update_where.sql`, `sparql_graph_management.sql`

### Exit Criteria

Standard SPARQL 1.1 Update operations work correctly. RDF tools that use SPARQL Update (Protégé, TopBraid, SPARQL workbenches) can interact with pg_triple without a custom adapter.

---

## v0.12.0 — Performance Hardening

**Theme**: Optimize for production-scale workloads. Benchmark-driven improvements.

> **In plain language:** This release is about *speed*. Using the Berlin SPARQL Benchmark (a standard test suite used by the RDF industry), we measure pg_triple's performance against known baselines and then tune it. Improvements include caching query plans so repeated queries skip redundant work, loading data in parallel, and teaching the system to use data quality rules (from v0.6.0/v0.7.0) as hints to avoid unnecessary work during queries. The target is simple queries answering in under 10 milliseconds on a dataset of 10 million facts, and bulk loading sustained at over 100,000 facts per second.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Berlin SPARQL Benchmark (BSBM)** integration
  - Data generator adapted for pg_triple bulk load
  - Full query mix execution with timing
  - Comparison baselines documented
- [ ] **Query plan caching**
  - Cache SPARQL→SQL translations keyed by query structure hash
  - `pg_triple.plan_cache_size` GUC
- [ ] **Parallel query exploitation**
  - Ensure VP table queries are parallel-safe
  - Mark SQL functions as `PARALLEL SAFE` where applicable
  - Generate SQL that triggers PostgreSQL parallel workers for multi-VP-table star patterns (e.g. parallel hash joins across VP tables)
  - Verify `EXPLAIN` output shows parallel plans for queries touching 3+ VP tables
- [ ] **Custom statistics for the PostgreSQL planner**
  - Run `ANALYZE` on VP tables after merge operations so the planner has accurate selectivity estimates for generated SQL
  - Provide per-predicate ndistinct and MCV statistics to guide join ordering
  - Evaluate custom statistics objects (PG18 extended statistics) on `(s, o)` pairs for correlation-aware planning
  - Consider prepared statements with parameter binding (instead of literal interpolation) so the planner can cache generic plans
- [ ] **PG18 async I/O exploitation**
  - Verify BRIN scans on main partition leverage AIO
  - Tune `io_combine_limit` recommendations
- [ ] **Memory optimization**
  - Profile and reduce per-query allocations
  - Optimize dictionary cache eviction strategy
- [ ] **Index tuning**
  - Evaluate PG18 skip scan benefits on `(s, o)` indices
  - Add covering indices where beneficial
- [ ] **Bulk load optimization**
  - Parallel dictionary encoding
  - Deferred index build with `CREATE INDEX CONCURRENTLY` post-load
- [ ] **SHACL-driven query optimization**
  - `sh:minCount 1` → OPTIONAL→INNER JOIN downgrade in SPARQL→SQL
  - `sh:maxCount 1` → skip DISTINCT for single-valued properties
  - `sh:class` → VP table pruning based on target class
- [ ] Performance regression test suite (pgbench custom scripts)
- [ ] pg_regress: `shacl_query_opt.sql`

### Exit Criteria

BSBM results documented. >100K triples/sec sustained bulk load. <10ms for simple BGP queries at 10M triples. <5ms for cached repeat queries. SHACL constraints exploited by query optimizer. PostgreSQL parallel plans verified for multi-VP-table joins.

---

## v0.13.0 — Administrative & Operational Readiness

**Theme**: Production operations tooling, upgrade paths, documentation.

> **In plain language:** Everything a system administrator needs to run pg_triple in production. This includes maintenance commands (clean up, rebuild indexes), monitoring and diagnostics, comprehensive documentation (quickstart guide, function reference, tuning guide), and *graph-level access control* — the ability to control which database users can see or modify which named graphs. It also covers packaging (Linux packages, Docker images) so the extension is easy to install in real environments. Think of this as the "operations manual" release.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **Extension upgrade scripts**
  - Tested upgrade path `0.1.0 → ... → 0.16.0`
  - `ALTER EXTENSION pg_triple UPDATE` works for all version transitions
- [ ] **Administrative functions**
  - `pg_triple.vacuum()` — force merge + VACUUM on VP tables
  - `pg_triple.reindex()` — rebuild all VP table indices
  - `pg_triple.dictionary_stats()` — detailed cache metrics
  - `pg_triple.predicate_stats()` — per-predicate triple count, table sizes
- [ ] **Logging & diagnostics**
  - Structured logging for merge operations, validation results
  - Custom `EXPLAIN` option showing SPARQL→SQL mapping (PG18 extension EXPLAIN)
- [ ] **Documentation**
  - README with quickstart
  - SQL function reference
  - SPARQL feature matrix
  - Performance tuning guide
  - SHACL constraint mapping reference
  - Datalog rule authoring guide
- [ ] **Graph-level Row-Level Security (RLS)**
  - `pg_triple.enable_graph_rls()` — activate RLS policies on VP tables using the `g` column
  - Policy driven by a mapping table: `_pg_triple.graph_access (role_name TEXT, graph_id BIGINT, permission TEXT)` — `'read'` / `'write'` / `'admin'`
  - `pg_triple.grant_graph(role TEXT, graph TEXT, permission TEXT)` / `pg_triple.revoke_graph()`
  - SPARQL queries automatically filter results to graphs the current role can read
  - Write operations (`insert_triple`, SPARQL UPDATE) enforce write permission
  - Superuser bypass via `pg_triple.rls_bypass` GUC for admin operations
- [ ] **Packaging**
  - `cargo pgrx package` produces installable `.deb` and `.rpm`
  - Docker image with extension pre-installed
  - PGXN metadata

### Exit Criteria

Extension is installable, upgradable, and documented. Operational tooling sufficient for production use. Graph-level RLS enforces access control per named graph.

---

## v0.14.0 — SPARQL Protocol (HTTP Endpoint)

**Theme**: Standard HTTP API for SPARQL queries and updates.

> **In plain language:** Without this, the only way to talk to pg_triple is through a PostgreSQL database connection (SQL). But the entire RDF ecosystem — SPARQL notebooks, visualization tools, ontology editors, web applications — expects to query a triple store over HTTP at a `/sparql` URL. This release adds a lightweight companion service that accepts standard SPARQL HTTP requests, forwards them to pg_triple inside PostgreSQL, and returns results in all the standard formats (JSON, XML, CSV, Turtle). This is the single biggest adoption enabler: it lets pg_triple drop in as a replacement for tools like Blazegraph, Virtuoso, or Apache Fuseki without requiring any client-side changes.
>
> **Effort estimate: 3–4 person-weeks**

### Deliverables

- [ ] **Companion HTTP service** (`pg_triple_http` binary)
  - Standalone Rust binary (not a PG background worker — avoids binding TCP ports inside PostgreSQL)
  - Connects to PostgreSQL via standard `libpq` / `tokio-postgres`
  - Configurable via environment variables or config file: `PG_TRIPLE_HTTP_PORT`, `PG_TRIPLE_HTTP_PG_URL`
- [ ] **W3C SPARQL 1.1 Protocol compliance**
  - `GET /sparql?query=...` — URL-encoded query
  - `POST /sparql` with `application/sparql-query` body
  - `POST /sparql` with `application/x-www-form-urlencoded` body (`query=...` / `update=...`)
  - SPARQL Update via `POST /sparql` with `application/sparql-update` body
- [ ] **Content negotiation**
  - `application/sparql-results+json` (default for SELECT/ASK)
  - `application/sparql-results+xml`
  - `text/csv` / `text/tab-separated-values`
  - `text/turtle` / `application/n-triples` (for CONSTRUCT/DESCRIBE)
  - `application/ld+json` (JSON-LD, for CONSTRUCT/DESCRIBE)
- [ ] **Connection pooling**
  - Built-in connection pool (e.g. `deadpool-postgres`) to handle concurrent HTTP requests
  - `PG_TRIPLE_HTTP_POOL_SIZE` configuration
- [ ] **Security**
  - Optional bearer token or Basic auth for access control
  - CORS configuration for browser-based SPARQL clients
  - Rate limiting GUC
- [ ] **Health and metrics**
  - `GET /health` endpoint for load balancer probes
  - Prometheus-compatible `/metrics` endpoint (query count, latency histogram, error rate)
- [ ] **Docker integration**
  - Docker image bundles both PostgreSQL (with pg_triple) and the HTTP service
  - Docker Compose example with separate PG and HTTP containers
- [ ] pg_regress: `sparql_protocol.sql` (protocol-level tests via `curl`)

### Exit Criteria

Standard SPARQL clients (YASGUI, Postman, RDF4J workbench, `curl`) can query and update pg_triple over HTTP without any pg_triple-specific configuration. Content negotiation returns correct formats.

---

## v0.15.0 — SPARQL Federation

**Theme**: Query remote SPARQL endpoints from within pg_triple queries.

> **In plain language:** Federation lets a single SPARQL query combine data from pg_triple with data from external SPARQL endpoints on the web. For example, you could ask "find all my local employees and enrich their records with data from Wikidata" — and the system will automatically fetch the remote portion, join it with local results, and return a unified answer. This is part of the SPARQL 1.1 standard (`SERVICE` keyword) and is expected by many enterprise knowledge graph workflows that integrate multiple data sources. Multiple remote calls execute in parallel when possible to minimise latency.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **SPARQL `SERVICE` keyword parsing**
  - Parse `SERVICE <url> { ... }` clauses in SPARQL queries via `spargebra`
  - Support both inline service IRIs and `SERVICE ?var` (variable endpoints, with VALUES binding)
- [ ] **Remote endpoint execution**
  - HTTP GET/POST to remote SPARQL endpoints using `reqwest` (async HTTP client)
  - Parse `application/sparql-results+json` and `application/sparql-results+xml` responses
  - Dictionary-encode remote results into local `i64` IDs for join compatibility
- [ ] **Join integration**
  - Remote result sets injected as inline `VALUES` clauses in the generated SQL
  - **Async parallel execution**: multiple `SERVICE` clauses in a single query execute concurrently (via `tokio::join!` in pg_triple_http, or sequential fallback in SPI context) — prevents a single slow endpoint from blocking the entire query
  - Bind-join optimisation: push bound variables from local results into remote queries to reduce remote result size
- [ ] **Error handling and timeouts**
  - `pg_triple.federation_timeout` GUC (default: 30s per SERVICE call)
  - `pg_triple.federation_max_results` GUC (default: 10,000 rows per remote call)
  - Graceful degradation: failed SERVICE calls return empty results with a WARNING (configurable to ERROR via `pg_triple.federation_on_error` GUC)
- [ ] **Security**
  - Allowlist of permitted remote endpoints: `_pg_triple.federation_endpoints (url TEXT, enabled BOOLEAN)`
  - `pg_triple.register_endpoint()` / `pg_triple.remove_endpoint()` management API
  - No outbound HTTP calls unless the endpoint is explicitly registered (defence against SSRF)
- [ ] **HTTP endpoint integration**
  - Federation works via both SQL (`pg_triple.sparql()`) and HTTP (`/sparql`) interfaces
- [ ] pg_regress: `sparql_federation.sql`, `federation_timeout.sql`

### Exit Criteria

SPARQL queries with `SERVICE` clauses correctly fetch and join data from registered remote endpoints. Multiple SERVICE calls execute in parallel. Timeouts and error handling work as configured. No SSRF risk — only allowlisted endpoints are contacted.

---

## v0.16.0 — RDF-star / RDF 1.2

**Theme**: Quoted triples — make statements about statements.

> **In plain language:** Standard RDF can say "Alice knows Bob". But it can't directly say *"Alice said that she knows Bob"* or *"The fact that Alice knows Bob was recorded on January 5th"*. RDF-star (now part of the RDF 1.2 standard, finalised by W3C in 2024) solves this by allowing triples to be embedded inside other triples — called *quoted triples*. This is essential for provenance ("where did this fact come from?"), temporal annotations ("when was this true?"), and trust ("who asserted this?"). This is a cross-cutting change that touches parsing, storage, dictionary encoding, and the SPARQL engine, making it the largest single feature addition in the roadmap.
>
> **Effort estimate: 8–10 person-weeks**

### Deliverables

- [ ] **Quoted triple syntax in parsers**
  - Turtle-star: `<< :Alice :knows :Bob >> :assertedBy :Carol .`
  - N-Triples-star: `<< <http://...Alice> <http://...knows> <http://...Bob> >> <http://...assertedBy> <http://...Carol> .`
  - Extend `rio_turtle` / `rio_xml` parsing (or use `oxrdf` crate for RDF-star support)
- [ ] **Dictionary encoding for quoted triples**
  - New term type in dictionary: `QUOTED_TRIPLE` — stores the triple `(s, p, o)` as a composite key
  - XXH3-128 hash of the triple tuple for dedup
  - `pg_triple.encode_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT` — returns the dictionary ID of the quoted triple
  - `pg_triple.decode_triple(id BIGINT) RETURNS JSONB` — returns `{"s": ..., "p": ..., "o": ...}`
- [ ] **Storage**
  - Quoted triples can appear in `s` or `o` positions of VP tables (the ID references a quoted triple in the dictionary)
  - No structural change to VP tables — quoted triple IDs are regular `BIGINT` values
  - Nested quoted triples supported (a quoted triple whose subject or object is itself a quoted triple)
- [ ] **SPARQL-star query support**
  - Parse `<< ?s ?p ?o >>` triple term patterns in SPARQL queries
  - `BIND(<< :Alice :knows :Bob >> AS ?t)` — inline quoted triple construction
  - Triple term patterns in WHERE clauses: `<< ?s :knows ?o >> :assertedBy ?who .`
  - Compile to dictionary joins: look up the quoted triple ID, then join against VP tables
  - **Batch recursive decode for nested quoted triples**: collect all quoted-triple IDs from the result set, recursively resolve inner components in bulk via `WITH RECURSIVE` dictionary lookup, build decode map before emitting rows — avoids per-row recursive dictionary round-trips
- [ ] **SPARQL-star in CONSTRUCT / DESCRIBE**
  - CONSTRUCT can produce quoted triples in output
  - Turtle-star and N-Triples-star serialization in export functions
- [ ] **Datalog integration**
  - Quoted triples can appear in Datalog rule heads and bodies
  - Enables provenance rules: `<< ?s ?p ?o >> ex:derivedBy ex:rule1 :- ?s ?p ?o, RULE(ex:rule1) .`
- [ ] **Content negotiation updates**
  - HTTP endpoint serves Turtle-star and JSON-LD-star for CONSTRUCT/DESCRIBE results containing quoted triples
- [ ] pg_regress: `rdf_star_load.sql`, `sparql_star_query.sql`, `rdf_star_construct.sql`, `rdf_star_datalog.sql`

### Exit Criteria

Users can load RDF-star data (Turtle-star, N-Triples-star), query it with SPARQL-star triple term patterns, and export results in RDF-star formats. Quoted triples work as subjects and objects in VP tables. Datalog rules can reason over and produce quoted triples.

---

## v1.0.0 — Production Release

**Theme**: Stability, conformance, and production certification.

> **In plain language:** The 1.0 release is not about new features — it's about *confidence*. We run pg_triple against the official W3C test suites for SPARQL and SHACL to verify standards compliance. A 72-hour continuous stress test checks for memory leaks and crash recovery. A security audit reviews the code for vulnerabilities. The result is a release that organisations can rely on for production workloads with a clear API stability guarantee: the public interface will not break in future minor versions.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **SPARQL 1.1 Query conformance**
  - Pass W3C SPARQL 1.1 Query test suite (supported subset)
  - Document unsupported features (property functions)
  - Verify conformance via both SQL and HTTP interfaces
  - Federation (`SERVICE`) covered by v0.15.0
- [ ] **SPARQL 1.1 Update conformance**
  - Pass W3C SPARQL 1.1 Update test suite (supported subset)
  - Document unsupported features
- [ ] **SHACL Core conformance**
  - Pass W3C SHACL Core test suite (supported subset)
  - Document unsupported constraints
- [ ] **Stability hardening**
  - 72-hour continuous load test (mixed read/write)
  - Memory leak detection (Valgrind via `cargo pgrx test --valgrind`)
  - Crash recovery testing (kill -9 during merge, reload, verify)
- [ ] **Security audit**
  - Review all SPI query generation for injection vectors
  - Review shared memory usage for race conditions
  - Review dictionary cache for timing side-channels
- [ ] **API stability guarantee**
  - All `pg_triple.*` SQL functions considered stable API
  - `_pg_triple.*` internal schema reserved for internal use
  - Semantic versioning contract: breaking changes only in major versions
- [ ] **Final benchmarks**
  - BSBM at 100M triples
  - Published performance report
- [ ] **Release artifacts**
  - Tagged release on GitHub
  - Published to PGXN
  - crates.io publication (library crate)

### Exit Criteria

Stable, tested, documented, and published. Ready for production workloads up to 100M+ triples on a single node.

---

## Post-1.0 Horizon

> **In plain language:** These are future directions that extend pg_triple beyond its initial scope. Each addresses a specific real-world need — from distributing data across multiple servers, to geographic queries, to bridging with existing relational databases. They are listed roughly in order of anticipated demand; some may be reordered or combined based on community feedback after 1.0.

| Version | Theme | What it delivers | Key Technical Features |
|---|---|---|---|
| 1.1 | Distributed | Spread data across multiple servers for horizontal scale | Citus integration, subject-based sharding |
| 1.2 | Vector + Graph | Combine knowledge graphs with AI-style similarity search | pgvector integration, hybrid semantic search |
| 1.3 | Temporal | Track how data changes over time; query historical states | Bitstring versioning, TimescaleDB integration |
| 1.4 | Extended VP | Automatically pre-compute shortcuts for frequent query patterns | Automated workload-driven ExtVP stream tables (pg_trickle), ontology change propagation DAG |
| 1.5 | Interop | Bridge to other graph databases and GraphQL APIs | Apache AGE bridge, GraphQL-to-SPARQL |
| 1.6 | GeoSPARQL + PostGIS | Answer geographic questions ("find all hospitals within 5 km of this point") | `geo:asWKT` literal type backed by PostGIS `geometry`, spatial FILTER functions, R-tree index on spatial VP tables |
| 1.7 | R2RML Virtual Graphs | Expose existing database tables as if they were RDF data — no migration needed | W3C R2RML mappings, SPARQL queries transparently join VP tables with mapped SQL tables |
| 1.8 | Quad-Level Provenance | Track where each fact came from and when it was added | Per-quad metadata table with source, timestamp, and transaction ID; integration with Datalog rule provenance (why-provenance) |

---

## Version Timeline (Estimated Cadence)

> **In plain language:** The "Calendar" column shows how long after the previous release each version is expected to ship. The "Effort" column shows the total developer-time required. With two developers working together, the calendar durations are achievable; with one developer, roughly double the calendar time.

| Version | Calendar (pair) | Effort (person-weeks) | Cumulative effort |
|---|---|---|---|
| 0.1.0 | Week 0 (start) | 6–8 pw | 6–8 pw |
| 0.2.0 | +4 weeks | 6–8 pw | 12–16 pw |
| 0.3.0 | +4 weeks | 6–8 pw | 18–24 pw |
| 0.4.0 | +4 weeks | 8–10 pw | 26–34 pw |
| 0.5.0 | +4 weeks | 8–10 pw | 34–44 pw |
| 0.6.0 | +3 weeks | 4–6 pw | 38–50 pw |
| 0.7.0 | +3 weeks | 4–6 pw | 42–56 pw |
| 0.8.0 | +2 weeks | 3–4 pw | 45–60 pw |
| 0.9.0 | +5 weeks | 10–12 pw | 55–72 pw |
| 0.10.0 | +3 weeks | 4–6 pw | 59–78 pw |
| 0.11.0 | +3 weeks | 4–6 pw | 63–84 pw |
| 0.12.0 | +4 weeks | 6–8 pw | 69–92 pw |
| 0.13.0 | +3 weeks | 4–6 pw | 73–98 pw |
| 0.14.0 | +2 weeks | 3–4 pw | 76–102 pw |
| 0.15.0 | +3 weeks | 4–6 pw | 80–108 pw |
| 0.16.0 | +5 weeks | 8–10 pw | 88–118 pw |
| 1.0.0 | +4 weeks | 6–8 pw | **95–122 pw** |
| 1.1–1.8 | Post-1.0 | Community-driven | — |

*Estimates assume a pair of focused developers with Rust and PostgreSQL experience. "pw" = person-weeks. Calendar durations assume pair programming; a solo developer should expect roughly double the calendar time. Actual pace depends on contributor availability and scope adjustments discovered during implementation.*
