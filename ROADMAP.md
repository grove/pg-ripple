# pg_ripple — Roadmap

> From **0.1.0** (foundation) to **1.0.0** (production-ready triple store)

## How to read this roadmap

Each release below has two layers:

- **The plain-language summary** (in the coloured box) explains *what* the release delivers and *why it matters* — no programming knowledge required.
- **The technical deliverables** list the specific items developers will build. Feel free to skip these if you're reading for the big picture.

**Effort estimates** are given as *person-weeks* — e.g. "6–8 pw" means the release would take roughly 6–8 weeks for a single full-time developer, or 3–4 weeks for a pair working together. The total estimated effort from v0.1.0 to v1.0.0 is **98–131 person-weeks** (~23–30 months for one developer; ~11–15 months for a pair).

---

## Overview at a glance

| Version | Name | What it delivers (one sentence) | Effort |
|---|---|---|---|
| 0.1.0 | Foundation | Install the extension, store and retrieve facts (VP storage from day one) | 6–8 pw |
| 0.2.0 | Bulk Loading & Named Graphs | Bulk data import, named graphs, rare-predicate consolidation, N-Triples export | 6–8 pw |
| 0.3.0 | SPARQL Basic | Ask questions in the standard RDF query language (incl. GRAPH patterns) | 6–8 pw |
| 0.4.0 | RDF-star / Statement IDs | Make statements about statements; LPG-ready storage | 8–10 pw |
| 0.5.0 | SPARQL Advanced (Query) | Property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES | 6–8 pw |
| 0.5.1 | SPARQL Advanced (Storage & Write) | Inline encoding, CONSTRUCT/DESCRIBE, INSERT/DELETE DATA, FTS | 6–8 pw |
| 0.6.0 | HTAP Architecture | Heavy reads and writes at the same time; shared-memory cache | 8–10 pw |
| 0.7.0 | SHACL Core | Define data quality rules; reject bad data on insert | 4–6 pw |
| 0.8.0 | SHACL Advanced | Complex data quality rules with background checking | 4–6 pw |
| 0.9.0 | Serialization | Import and export data in all standard RDF file formats | 3–4 pw |
| 0.10.0 | Datalog Reasoning | Automatically derive new facts from rules and logic | 10–12 pw |
| 0.11.0 | SPARQL & Datalog Views | Live, always-up-to-date dashboards from SPARQL and Datalog queries | 5–7 pw |
| 0.12.0 | SPARQL Update (Advanced) | Pattern-based updates and graph management commands | 3–4 pw |
| 0.13.0 | Performance | Speed tuning, benchmarks, production-grade throughput | 6–8 pw |
| 0.14.0 | Admin & Security | Operations tooling, access control, docs, packaging | 4–6 pw |
| 0.15.0 | SPARQL Protocol | Standard HTTP API so web apps and tools can query directly | 3–4 pw |
| 0.16.0 | SPARQL Federation | Query remote SPARQL endpoints alongside local data | 4–6 pw |
| 1.0.0 | Production Release | Standards conformance, stress testing, security audit | 6–8 pw |
| | | **Total estimated effort** | **98–131 pw** |

---

## v0.1.0 — Foundation

**Theme**: Core data model, dictionary encoding, and basic triple CRUD.

> **In plain language:** This is the "hello world" release. After installing pg_ripple into a PostgreSQL database, a user can store facts (called *triples* — think "subject → relationship → object", e.g. "Alice → knows → Bob") and retrieve them by pattern. No query language yet — just the basic building blocks. Internally, every piece of text (names, URLs, values) is converted to a compact number for fast storage and comparison. This release also sets up automated testing so that every future change is verified.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] pgrx 0.17 project scaffolding targeting PostgreSQL 18
- [ ] Extension bootstrap: `CREATE EXTENSION pg_ripple` creates `_pg_ripple` schema
- [ ] **Dictionary encoder**
  - Unified dictionary table (IRIs, blank nodes, literals in a single table with `kind` discriminator — avoids ID space collision between separate resource/literal tables)
  - XXH3-128 hash-based dedup
  - Encode/decode SQL functions: `pg_ripple.encode_iri()`, `pg_ripple.decode_id()`
- [ ] **Vertical Partitioning from day one**
  - Dynamic VP table management: auto-create `_pg_ripple.vp_{predicate_id}` tables on first triple with a new predicate
  - Predicate catalog: `_pg_ripple.predicates (id BIGINT, table_oid OID, triple_count BIGINT)`
  - Dual B-tree indices per VP table: `(s, o)` and `(o, s)`
  - Global statement identifier sequence: `_pg_ripple.statement_id_seq` — every VP table row gets a globally-unique SID via `i BIGINT NOT NULL DEFAULT nextval('statement_id_seq')`
  - SIDs are not exposed to users in v0.1.0 but are available for internal use from the start (prerequisite for RDF-star in v0.4.0)
- [ ] **Basic triple CRUD**
  - `pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_ripple.delete_triple(s TEXT, p TEXT, o TEXT)`
  - `pg_ripple.triple_count() RETURNS BIGINT`
- [ ] **Basic querying** (SQL-level, no SPARQL yet)
  - `pg_ripple.find_triples(s TEXT, p TEXT, o TEXT) RETURNS TABLE (s TEXT, p TEXT, o TEXT, g TEXT)` — any param can be NULL for wildcard; returns decoded string values
- [ ] Unit tests for dictionary encode/decode round-trips
- [ ] Integration test: insert + query cycle
- [ ] pg_regress: `dictionary.sql` (encode/decode, prefix expansion, hash collision behaviour), `basic_crud.sql` (insert, delete, find_triples, triple_count)
- [ ] CI pipeline (GitHub Actions)
- [ ] **GUC-gated lazy initialization**
  - Merge worker, SHACL engine, and reasoning engine only start when their respective GUCs are enabled (`pg_ripple.merge_threshold > 0`, `pg_ripple.shacl_mode != 'off'`, `pg_ripple.inference_mode != 'off'`)
  - Reduces resource overhead for deployments that use only a subset of features
- [ ] **Error taxonomy module** (`src/error.rs`)
  - `thiserror`-based error types with PT error code constants
  - Initial ranges: dictionary errors (PT001–PT099) and storage errors (PT100–PT199)
  - PostgreSQL-style formatting: lowercase first word, no trailing period
  - Extended in subsequent milestones as new subsystems are added (see §13.6 of the [Implementation Plan](plans/implementation_plan.md) for the complete PT001–PT799 range table)

> **Shared memory note**: v0.1.0 through v0.5.1 use a **backend-local** `lru::LruCache` for the dictionary cache. This avoids requiring `shared_preload_libraries` for the "hello world" release and defers the pgrx shared-memory complexity to v0.6.0 when the HTAP architecture actually needs it. The shared-memory dictionary cache, bloom filters, slot versioning, and `pg_ripple.shared_memory_size` startup GUC are all introduced in v0.6.0.

### Exit Criteria

A user can install the extension, insert triples (routed to per-predicate VP tables), and query them back by pattern. No `shared_preload_libraries` configuration required. VP tables are created dynamically on first encounter of a new predicate.

---

## v0.2.0 — Bulk Loading & Named Graphs

**Theme**: Bulk data import, rare-predicate consolidation, named graphs, and prefix management.

> **In plain language:** This release adds *bulk import*: users can load large RDF data files (in Turtle and N-Triples formats) in one go, rather than inserting facts one at a time. Named graphs (the ability to group facts into labelled collections) are introduced here too. A "rare predicate" consolidation table prevents catalog bloat when datasets have thousands of distinct predicates. N-Triples export is included for test verification and round-trip checking.
>
> **Storage partition note**: In v0.2.0 through v0.5.0, each VP table is a *single flat table* — there is no delta/main split yet. All reads and writes target the same table. The HTAP dual-partition architecture (separate `_delta` and `_main` tables with a background merge worker) is introduced in v0.6.0 via an explicit schema migration that renames existing VP tables and creates the initial `_main` partition.
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Rare-predicate consolidation table**
  - Predicates with fewer than `pg_ripple.vp_promotion_threshold` triples (default: 1,000) are stored in a shared `_pg_ripple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT, i BIGINT)` table with a primary composite index on `(p, s, o)` and two secondary indices: `(s, p)` for DESCRIBE queries and `(g, p, s, o)` for efficient graph-drop bulk-delete
  - Promotion is **deferred to end-of-statement** (not mid-batch): during a bulk load, triples accumulate in `vp_rare`; after the load completes, predicates exceeding the threshold are promoted in a single `INSERT … SELECT` + `DELETE` transaction — avoids disrupting in-flight COPY streams
  - `pg_ripple.promote_rare_predicates()` can also be called manually or by the background merge worker
  - Prevents catalog bloat for predicate-rich datasets (DBpedia ≈60K predicates, Wikidata ≈10K) — avoids hundreds of thousands of PG objects, reduces planner overhead, and cuts VACUUM cost
- [ ] **`_pg_ripple.statements` range-mapping catalog**
  - Maintained by the merge worker; stores `(sid_min, sid_max, predicate_id, table_oid)` range rows rather than one row per statement — resolved via binary search in *O(log n)* with no full-table scans
  - After each merge cycle the worker inserts one range row per VP table covering the SIDs allocated since the last merge; because SIDs are drawn from a monotonically-increasing sequence, ranges are non-overlapping
  - Required for v0.4.0 RDF-star where SIDs appear as subjects/objects in other VP tables and must be unambiguously resolved to their owning VP table
- [ ] **Named graph support** (basic)
  - `g` column in VP tables
  - `pg_ripple.create_graph()`, `pg_ripple.drop_graph()`, `pg_ripple.list_graphs()`
- [ ] **`pg_ripple.named_graph_optimized` GUC** (default: `off`)
  - When enabled, adds an optional `(g, s, o)` index per dedicated VP table (and equivalent coverage on `vp_rare`) to accelerate graph-scoped queries (e.g. list all triples in graph G, drop a named graph)
  - Off by default to avoid index bloat for workloads that do not use named graphs heavily
- [ ] **Blank node document-scoping**
  - Each bulk load operation is assigned a monotonically-increasing `load_generation` counter from a shared sequence
  - Blank nodes are hashed as `"{generation}:{label}"` — so `_:b0` from two different load calls yields two distinct dictionary IDs
  - Prevents incorrect merging of blank nodes across document boundaries, which would corrupt data in multi-file loads
  - Also applies to `INSERT DATA` (SPARQL Update, v0.5.1+) which always gets its own generation
- [ ] **Bulk loader** (N-Triples)
  - `pg_ripple.load_ntriples(data TEXT) RETURNS BIGINT`
  - Streaming parser via `rio_turtle` crate
  - Batch encoding + COPY for throughput
- [ ] **Bulk loader** (N-Quads)
  - `pg_ripple.load_nquads(data TEXT) RETURNS BIGINT`
  - Standard format for named-graph quads (`<s> <p> <o> <g> .`); same `rio_turtle` parser path as N-Triples
  - Route quads to the appropriate named graph (`g` column) automatically
- [ ] **Bulk loader** (Turtle)
  - `pg_ripple.load_turtle(data TEXT) RETURNS BIGINT`
  - Prefix declarations auto-registered
  - Blank node scoping per load operation
  - `rio_turtle` crate already handles both formats — incremental parser work
- [ ] **Bulk loader** (TriG)
  - `pg_ripple.load_trig(data TEXT) RETURNS BIGINT`
  - Turtle with named graph blocks (`GRAPH <g> { … }`) — the standard interchange format for named-graph Turtle data
  - Uses the same `rio_turtle` streaming parser; named graph IRI is dictionary-encoded and stored in the `g` column
- [ ] **File-path bulk load variants**
  - `pg_ripple.load_turtle_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_ntriples_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_nquads_file(path TEXT) RETURNS BIGINT`
  - `pg_ripple.load_trig_file(path TEXT) RETURNS BIGINT`
  - Reads via `pg_read_file()` with superuser privilege check — prevents unauthorized file access
  - Essential for datasets larger than ~1 GB where passing data as a TEXT parameter exceeds PostgreSQL's TEXT size limit and imposes significant memory overhead
  - Returns count of loaded triples; otherwise identical behaviour to the inline TEXT variants
- [ ] **IRI prefix management**
  - `pg_ripple.register_prefix(prefix TEXT, expansion TEXT)`
  - `pg_ripple.prefixes() RETURNS TABLE`
  - Prefix expansion in encode/decode paths
- [ ] **ANALYZE after bulk loads**
  - All inline and file-path load functions run `ANALYZE` on affected VP tables after load completes
  - Ensures the PostgreSQL planner has accurate selectivity estimates for generated SQL — critical for good join plans in v0.3.0+
- [ ] Benchmarks: insert throughput (1M triples)
- [ ] **Performance regression baseline**: record insert throughput and point-query latency as CI benchmark baselines; fail CI if a commit regresses throughput by >10% (maintained and extended in every subsequent milestone)
- [ ] **N-Triples / N-Quads export** (basic)
  - `pg_ripple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_ripple.export_nquads(graph TEXT DEFAULT NULL) RETURNS TEXT` — exports all named graphs as NQuads when `graph` is NULL; a single graph when specified
  - Streaming variants returning `SETOF TEXT` for large graphs
  - Essential for verifying bulk load round-trips in v0.2.0 testing
- [ ] pg_regress test suite: `triple_crud.sql`, `named_graphs.sql`, `export_ntriples.sql`, `nquads_trig.sql` (N-Quads round-trip, TriG named-graph import, file-path loaders)

### Exit Criteria

Rare-predicate consolidation table absorbs low-frequency predicates. Bulk loading >50K triples/sec on commodity hardware. Named graphs functional. All four inline formats (N-Triples, N-Quads, Turtle, TriG) and their file-path counterparts load correctly. Multi-graph data can be loaded via N-Quads/TriG and round-tripped via N-Quads export. VP tables have current planner statistics after bulk load.

---

## v0.3.0 — SPARQL Query Engine (Basic)

**Theme**: Parse and execute SPARQL SELECT and ASK queries with basic graph patterns, named graph querying, and initial join optimizations.

> **In plain language:** SPARQL is the standard language for asking questions over linked data — the same way SQL is for relational databases. This release makes pg_ripple understand SPARQL, so users can write queries like *"find all people who know someone who works at Acme Corp"* using the official W3C syntax. It also enables querying across named graphs (created in v0.2.0) using the standard SPARQL `GRAPH` keyword.
>
> **Effort estimate: 6–8 person-weeks**

### Prerequisites

- **`sparopt` availability check** *(must be resolved before beginning v0.3.0)*: verify that `sparopt` is published to crates.io with a stable, usable API and pin the version. If unavailable or API-unstable, absorb its filter-pushdown and constant-folding work directly into pg_ripple's own algebra optimizer pass (`src/sparql/algebra.rs`) before starting v0.3.0 — do not begin v0.3.0 development without resolving this gate.

### Deliverables

- [ ] **`sparopt` first-pass algebra optimizer** (`sparopt` crate)
  - Sits between the `spargebra` parse tree and pg_ripple's own algebra pass
  - Performs filter pushdown, constant folding, and empty-pattern elimination before SQL generation — reduces the surface area that pg_ripple's pass needs to handle
- [ ] **SPARQL parser integration** (`spargebra` crate)
  - Parse SPARQL SELECT and ASK queries into algebra tree
  - Support: Basic Graph Patterns (BGP), FILTER, OPTIONAL, LIMIT, OFFSET, ORDER BY, DISTINCT
  - `GRAPH ?g { ... }` patterns and `FROM` / `FROM NAMED` dataset clauses — map to `WHERE g = encode(uri)` filters on VP tables
- [ ] **Per-query `EncodingCache`** (`src/dictionary/query_cache.rs`)
  - Short-lived `HashMap` for IRIs and literals seen within a single SPARQL query
  - Avoids repeated SPI dictionary look-ups for constants that appear multiple times in one query
- [ ] **SQL generator** (initial)
  - BGP → JOIN across VP tables (integer equality)
  - FILTER → WHERE clause on integer-encoded values (dictionary-join decode for type comparisons; inline encoding deferred to v0.5.0)
  - OPTIONAL → LEFT JOIN
  - LIMIT/OFFSET/ORDER BY passthrough
  - DISTINCT → SQL DISTINCT
- [ ] **Query executor**
  - `pg_ripple.sparql(query TEXT) RETURNS SETOF JSONB`
  - SPI execution of generated SQL
  - **Batch dictionary decode**: collect all output i64 IDs from the result set, decode in a single `WHERE id = ANY(...)` query, build an in-memory lookup map, then emit human-readable rows — avoids per-row dictionary round-trips
  - **`projector.rs`** as a distinct final pipeline stage: variable mapping, SELECT expressions, BIND computations, and computed values are resolved here before SQL emission — keeps the SQL generator focused on join structure
- [ ] **SPARQL ASK**
  - ASK → `SELECT EXISTS(...)` → returns BOOLEAN
  - `pg_ripple.sparql_ask(query TEXT) RETURNS BOOLEAN`
- [ ] **Join optimizations** (phase 1)
  - Self-join elimination for star patterns
  - Filter pushdown: encode FILTER constants before SQL generation
- [ ] `pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false) RETURNS TEXT` — show generated SQL; `analyze := true` executes the query and augments the output with actual row counts
- [ ] **SQL injection / adversarial tests**: verify that SPARQL queries containing SQL metacharacters in IRIs, literals, and prefixed names (`'; DROP TABLE --`, Unicode escapes, null bytes) are safely dictionary-encoded and never reach generated SQL as raw strings
- [ ] **Malformed input tests**: invalid Turtle, truncated N-Triples, malformed SPARQL — verify clean error messages (no panics, no partial state)
- [ ] **W3C SPARQL conformance gate**: run the applicable subset of the W3C SPARQL 1.1 Query manifest tests against the features delivered so far; extend this gate in every subsequent SPARQL milestone (v0.4.0, v0.5.0, v0.5.1, v0.9.0, v0.12.0, v0.16.0) until full conformance at v1.0.0
- [ ] pg_regress: `sparql_queries.sql` (20+ test queries), `sparql_injection.sql` (adversarial inputs)

### Exit Criteria

Users can run SPARQL SELECT and ASK queries with BGPs, FILTER, OPTIONAL, and GRAPH patterns against data loaded via bulk load. Named graph queries work correctly. Queries return correct results.

---

## v0.4.0 — RDF-star / Statement Identifiers

**Theme**: Quoted triples, statement-level metadata, and LPG-ready storage — make statements about statements.

> **In plain language:** Standard RDF can say "Alice knows Bob". But it can't directly say *"Alice said that she knows Bob"* or *"The fact that Alice knows Bob was recorded on January 5th"*. RDF-star (now part of the RDF 1.2 standard) solves this by allowing triples to be embedded inside other triples — called *quoted triples*. This is essential for provenance ("where did this fact come from?"), temporal annotations ("when was this true?"), and trust ("who asserted this?"). By delivering this immediately after basic SPARQL, pg_ripple becomes **LPG-ready from the start**: Labeled Property Graph edges with properties (e.g. `[:KNOWS {since: 2020}]`) map directly to RDF-star annotations over statement identifiers already present in the VP tables since v0.1.0. This is a cross-cutting change that touches parsing, storage, dictionary encoding, and the SPARQL engine.
>
> **Effort estimate: 8–10 person-weeks**

### Design rationale — why so early?

The OneGraph (1G) research initiative (Lassila et al., 2023; Poseidon engine, AWS Neptune Analytics) demonstrates that a unified SPOI (Subject, Predicate, Object, statement-Identifier) storage model is the foundation for breaking the "graph model lock-in" between RDF and LPG. By introducing statement identifiers in v0.1.0 (storage) and RDF-star in v0.4.0 (query), pg_ripple achieves 1G-compatible storage before any advanced features are built on top. Every subsequent milestone (SHACL, Datalog, SPARQL Update, Cypher/GQL) benefits from statement IDs being available from the start.

**Patent clearance**: RDF-star is a W3C standard developed under the [W3C Patent Policy](https://www.w3.org/Consortium/Patent-Policy/) (Royalty-Free). Statement identifiers are well-established prior art (RDF reification, 2004; Named Graphs, 2005; RDF-star Community Group, 2014). The 1G abstract data model is published academic research (Semantic Web Journal, doi:10.3233/SW-223273), not patented technology. Poseidon's proprietary implementation details (P8APL, PAX pages, lock-free adjacency lists) are specific to Amazon's in-memory engine and are not replicated here — pg_ripple uses PostgreSQL's native heap/WAL/MVCC storage.

### Deliverables

- [ ] **Quoted triple syntax in parsers**
  - Turtle-star: `<< :Alice :knows :Bob >> :assertedBy :Carol .`
  - N-Triples-star: `<< <http://...Alice> <http://...knows> <http://...Bob> >> <http://...assertedBy> <http://...Carol> .`
  - Use `oxttl` / `oxrdf` crates for RDF-star support (complement existing `rio_turtle` / `rio_xml`)
- [ ] **Dictionary encoding for quoted triples**
  - New term type in dictionary: `QUOTED_TRIPLE` — stores the triple `(s, p, o)` as a composite key
  - XXH3-128 hash of the triple tuple for dedup
  - `pg_ripple.encode_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT` — returns the dictionary ID of the quoted triple
  - `pg_ripple.decode_triple(id BIGINT) RETURNS JSONB` — returns `{"s": ..., "p": ..., "o": ...}`
- [ ] **Statement identifier activation**
  - The `i` column (introduced in v0.2.0 VP tables) is now actively used: `insert_triple()` returns the SID
  - `pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL) RETURNS BIGINT` — returns the statement identifier
  - `pg_ripple.get_statement(i BIGINT) RETURNS JSONB` — look up a statement by its SID
  - SIDs can appear in `s` or `o` positions of VP tables (the ID references a statement, enabling edge properties and meta-statements)
- [ ] **Storage for edge properties via SIDs**
  - Annotation triples use the SID of the annotated statement as their subject: `vp_since(s=SID_of(alice,knows,bob), o=2020_id)`
  - No structural change to VP tables — SIDs and quoted triple IDs are regular `BIGINT` values
  - Nested quoted triples supported (a quoted triple whose subject or object is itself a quoted triple)
- [ ] **SPARQL-star query support**
  - Parse `<< ?s ?p ?o >>` triple term patterns in SPARQL queries
  - `BIND(<< :Alice :knows :Bob >> AS ?t)` — inline quoted triple construction
  - Triple term patterns in WHERE clauses: `<< ?s :knows ?o >> :assertedBy ?who .`
  - Compile to dictionary joins: look up the quoted triple ID, then join against VP tables
  - **Batch recursive decode for nested quoted triples**: collect all quoted-triple IDs from the result set, recursively resolve inner components in bulk via `WITH RECURSIVE` dictionary lookup, build decode map before emitting rows — avoids per-row recursive dictionary round-trips
- [ ] **Bulk load support for RDF-star data**
  - `pg_ripple.load_turtle()` and `pg_ripple.load_ntriples()` now accept Turtle-star / N-Triples-star input
  - Quoted triples in bulk load data are dictionary-encoded and stored with stable SIDs
- [ ] **W3C SPARQL-star conformance gate**: run the applicable subset of SPARQL-star tests; extend in subsequent milestones
- [ ] pg_regress: `rdf_star_load.sql`, `sparql_star_query.sql`, `statement_identifiers.sql` (SID lifecycle, edge property patterns, nested quoted triples)

### Exit Criteria

Users can load RDF-star data (Turtle-star, N-Triples-star), query it with SPARQL-star triple term patterns, and use statement identifiers to model edge properties. SIDs are returned from insert operations and can be used as subjects/objects in subsequent triples. The storage layer is LPG-ready.

---

## v0.5.0 — SPARQL Query Engine (Advanced — Query Completeness)

**Theme**: Property paths, UNION, aggregates, subqueries, and advanced join optimizations.

> **In plain language:** This release teaches the query engine to handle more powerful questions. *Property paths* let you follow chains of relationships — e.g. "find everyone reachable through any number of 'knows' links" (like a social network friend-of-a-friend search). *Aggregates* let you compute totals and averages ("how many people work in each department?"). This is a pure query-engine release with no storage changes, isolating query completeness from the inline encoding and write-path work in v0.5.1.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Property path compilation**
  - `+` (one or more) → `WITH RECURSIVE` CTE
  - `*` (zero or more) → `WITH RECURSIVE` CTE with zero-hop anchor
  - `?` (zero or one) → `UNION` of direct + zero-hop
  - `/` (sequence) → chained joins
  - `|` (alternative) → `UNION`
  - `^` (inverse) → swap `s`/`o`
  - Cycle detection via PG18 `CYCLE` clause (hash-based, replaces array-based visited tracking for $O(1)$ membership checks instead of $O(n)$ array scans)
  - `pg_ripple.max_path_depth` GUC
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
- [ ] Benchmark: SP2Bench subset
- [ ] **Resource exhaustion tests**: Cartesian-product queries, unbounded property paths on cyclic graphs, deeply nested subqueries — verify that `max_path_depth`, `statement_timeout`, and memory limits prevent runaway resource consumption
- [ ] **Fuzz testing** (`cargo-fuzz`): continuous fuzzing of the SPARQL→SQL pipeline — feed random/mutated SPARQL strings through the parser and SQL generator; verify no panics, no invalid SQL emitted, no memory safety violations
- [ ] pg_regress: `property_paths.sql`, `aggregates.sql`, `resource_limits.sql` (exhaustion tests)

### Exit Criteria

SPARQL 1.1 Query coverage for property paths, UNION/MINUS, aggregates, subqueries, BIND/VALUES. Property path queries complete with hash-based cycle detection via PG18 `CYCLE` clause. Fuzz testing runs without panics or invalid SQL.

---

## v0.5.1 — SPARQL Advanced (Storage, Serialization & Write)

**Theme**: Inline value encoding, CONSTRUCT/DESCRIBE, INSERT DATA/DELETE DATA, and full-text search.

> **In plain language:** This release introduces *inline value encoding* — a performance optimization that eliminates dictionary lookups for numeric and date comparisons. It changes the fundamental ID space model (introducing a dual-space interpretation), which is why it is separated from the pure query-engine work in v0.5.0. It also adds the two simplest SPARQL Update forms (`INSERT DATA` / `DELETE DATA`) so standard RDF tools can write to pg_ripple, *CONSTRUCT* and *DESCRIBE* to complete the four standard SPARQL query forms, and *full-text search* for efficient text matching.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Inline value encoding** (`src/dictionary/inline.rs`)
  - Type-tagged `i64` encoding for xsd:integer, xsd:boolean, xsd:dateTime, xsd:date — FILTER comparisons on these types require zero dictionary round-trips
  - IDs allocated in monotonically increasing semantic order so range FILTERs (`>`, `<`, `BETWEEN`) compile directly to SQL numeric comparisons on the raw `i64` column
  - Deferred from v0.3.0 to keep the initial SPARQL engine focused on a single ID space; now that the query engine is stable, the dual-space (inline + dictionary) model can be introduced safely
  - **Note**: `xsd:double` is stored in the dictionary rather than inline-encoded — truncating IEEE 754 doubles to 56 bits produces undefined precision/range behaviour; dictionary storage is safe and range comparisons on doubles are uncommon in SPARQL
- [ ] **SPARQL CONSTRUCT / DESCRIBE** (JSONB output)
  - CONSTRUCT → returns triples as JSONB (Turtle/JSON-LD serialization deferred to v0.9.0)
  - DESCRIBE → Concise Bounded Description (CBD) as default algorithm
  - `pg_ripple.describe_strategy` GUC (values: `'cbd'` / `'scbd'` / `'simple'`): selects the DESCRIBE expansion algorithm. Introduced here alongside DESCRIBE so the GUC is available from the first release that uses it.
  - Completes the four standard SPARQL query forms, making pg_ripple usable as an entity browser
- [ ] **Basic SPARQL Update** (`INSERT DATA` / `DELETE DATA`)
  - Parse and execute `INSERT DATA { … }` statements via `spargebra` (already supports Update algebra)
  - Route through dictionary encoder + VP table insert path
  - Named graph support: `INSERT DATA { GRAPH <g> { … } }`
  - Parse and execute `DELETE DATA { … }` statements — exact-match triple deletion from VP tables
  - `pg_ripple.sparql_update(query TEXT) RETURNS BIGINT` — returns count of affected triples
  - Pattern-based updates (`DELETE/INSERT WHERE`), `LOAD`, `CLEAR`, `DROP`, `CREATE` deferred to v0.12.0
  - Enables standard RDF tools (Protégé, TopBraid, SPARQL workbenches) to write to pg_ripple without a custom adapter
- [ ] **Full-text search on literals**
  - `pg_ripple.fts_index(predicate TEXT)` — create a GIN `tsvector` index on the dictionary for a predicate
  - SPARQL `CONTAINS()` and `REGEX()` FILTERs on indexed predicates rewrite to `@@` / `LIKE` against the GIN index
  - `pg_ripple.fts_search(query TEXT, predicate TEXT) RETURNS TABLE` — direct full-text search API
  - Index is maintained incrementally on `insert_triple()` for indexed predicates
- [ ] pg_regress: `fts_search.sql`, `sparql_construct.sql`, `sparql_insert_data.sql`, `sparql_delete_data.sql`, `inline_encoding.sql`

### Exit Criteria

Inline value encoding eliminates dictionary lookups for numeric and date FILTER comparisons. SPARQL CONSTRUCT and DESCRIBE return correct JSONB results. `INSERT DATA` / `DELETE DATA` work for standard-compliant write operations. Full-text search on indexed literal predicates is functional.

---

## v0.6.0 — HTAP Architecture

**Theme**: Separate read and write paths for concurrent OLTP/OLAP. Shared-memory dictionary cache. Subject pattern index.

> **In plain language:** In a real production system, people are loading new data and running complex queries at the same time. Without special care, these two activities interfere with each other — writes block reads and vice versa. This release splits the storage into a "write inbox" and a "read-optimised archive" so both can happen simultaneously at full speed. It also adds a *change notification* system: applications can subscribe to be told whenever specific facts change (useful for triggering workflows, updating caches, or feeding dashboards). An in-memory cache shared across all database connections makes repeated lookups much faster. Optionally, the companion pg_trickle extension enables automatically-updating live statistics.
>
> **Note**: This release introduces `shared_preload_libraries` as a requirement — v0.1.0–v0.5.1 do not require it because they use a backend-local dictionary cache. The `pg_ripple.shared_memory_size` startup GUC must be set in `postgresql.conf` before starting PostgreSQL.
>
> **Effort estimate: 8–10 person-weeks**

### Deliverables

- [ ] **Delta/Main partition split — schema migration**
  - Each VP table is migrated from its flat single-table form (v0.1.0–v0.5.1) to a dual-partition form:
    1. `CREATE TABLE _pg_ripple.vp_{id}_delta AS SELECT * FROM _pg_ripple.vp_{id}` (copy existing rows to delta)
    2. `CREATE TABLE _pg_ripple.vp_{id}_main (LIKE _pg_ripple.vp_{id})` (empty main, BRIN-indexed)
    3. `ALTER TABLE _pg_ripple.vp_{id} RENAME TO vp_{id}_pre_htap` (keep old table as backup)
    4. Update `_pg_ripple.predicates` catalog with new table OIDs
    5. Run an immediate merge cycle to promote rows from delta to main in sorted order
    6. Drop `vp_{id}_pre_htap` after merge completes successfully
  - The migration runs inside the `ALTER EXTENSION pg_ripple UPDATE` upgrade script — zero downtime during migration because rows still exist in delta until the merge completes and the query path immediately switches to `UNION ALL` of `_main` and `_delta`
  - `vp_rare` is **not** split (see vp_rare HTAP exemption below); all reads and writes target the single `vp_rare` table throughout
  - All writes target `_delta`; `_main` is append-only / read-optimized
  - Query path: `UNION ALL` of `_main` and `_delta`
- [ ] **Tombstone table for cross-partition deletes**
  - When deleting a triple that may exist in `_main`, the delete is recorded in `_pg_ripple.vp_{id}_tombstones (s BIGINT, o BIGINT, g BIGINT)`
  - Query path becomes: `(main EXCEPT tombstones) UNION ALL delta`
  - The merge worker applies tombstones against main during each generation merge, then truncates the tombstone table
  - Necessary because `_main` is read-only between merges — a DELETE targeting a main-resident triple cannot modify `_main` directly
- [ ] **`vp_rare` HTAP exemption**
  - `vp_rare` is **not** given a delta/main split — it remains a single flat table
  - Rare predicates see few writes by definition; delta/main overhead would exceed the benefit
  - Concurrent reads and writes on `vp_rare` are safe via PostgreSQL standard heap row-level locking
  - The bloom filter treats `vp_rare` conservatively (always queries it, no delta-skip shortcut)
- [ ] **Background merge worker**
  - pgrx `BackgroundWorker` implementation
  - Configurable merge threshold via `pg_ripple.merge_threshold` GUC
  - **Fresh-table generation merge**: rather than inserting into an existing `_main` table, create `vp_{id}_main_new`, insert *all* rows from both `_main` and `_delta` (minus tombstones) in sort order (ensuring BRIN pages are physically ordered), then atomically rename it to replace `_main` and TRUNCATE both `_delta` and `_tombstones` — writes to delta are never blocked during the merge and BRIN indexing is maximally effective because rows arrive in sorted order at table-creation time
  - BRIN index rebuild on main post-merge (concurrent where possible)
  - Shared-memory latch signaling
  - Also triggers `pg_ripple.promote_rare_predicates()` for any rare predicates that crossed the promotion threshold since the last merge
  - Runs `ANALYZE` on merged VP tables so the PostgreSQL planner has fresh selectivity estimates
  - **Watchdog**: if the merge worker heartbeat stalls for longer than `pg_ripple.merge_watchdog_timeout` (default: 300 s), `_PG_init` on the next backend connection logs a WARNING and attempts a restart
- [ ] **`ExecutorEnd_hook` latch-poke**
  - When a write transaction commits more than `pg_ripple.latch_trigger_threshold` rows (default: 10,000), the hook immediately pokes the merge worker's latch to trigger an early merge
  - Prevents unbounded delta growth during bursty write workloads without requiring a polling loop
- [ ] **Bloom filter for delta existence checks**
  - In shared memory, per VP table
  - Queries against main-only data skip delta scan
- [ ] **Dictionary LRU cache in shared memory**
  - `pg_ripple.dictionary_cache_size` GUC
  - Shared across all backends via pgrx `PgSharedMem`
  - **Sharded lock design**: partition the hash map into N shards (default: 64), each with its own lightweight lock — eliminates global lock contention under concurrent encode/decode workloads
- [ ] **Shared-memory budget & back-pressure**
  - `pg_ripple.cache_budget` GUC — utilization cap for the pre-allocated shared memory block (dictionary cache + bloom filters + merge worker buffers)
  - Automatic eviction priority: bloom filters reclaimed first, then oldest LRU dictionary entries
  - Back-pressure on bulk loads when shared memory is >90% of `cache_budget` — throttle batch size to prevent OOM
- [ ] **Shared-memory slot versioning**
  - Each shared memory slot (declared via pgrx 0.17's `pg_shmem_init!` macro) carries a `[u8; 8]` magic constant (e.g. `*b"pg_tripl"`) followed by a `u32` layout version at its head
  - Version mismatch at `_PG_init` triggers a controlled re-initialization of the slot rather than corrupting state — essential for safe in-place upgrades
  - **pgrx 0.17 API note**: all shared memory sizes must be declared statically in `_PG_init`. The `pg_ripple.shared_memory_size` startup GUC determines the block size; it cannot be changed at runtime. Use the pgrx 0.17 `PgSharedObject` / `PgSharedMem::new_object` API (not the old `PgSharedMem` from ≤0.14) — verify against the [pgrx 0.17 shmem examples](https://github.com/pgcentralfoundation/pgrx/tree/develop/pgrx-examples/shmem)
- [ ] **`subject_patterns` lookup table**
  - `_pg_ripple.subject_patterns(s BIGINT, predicates BIGINT[])` with a GIN index on `predicates`
  - Maintained by the merge worker after each generation merge (not on individual INSERTs — amortized cost)
  - Enables fast "which predicates does subject X have?" look-up for DESCRIBE queries and star-pattern rewriting in the algebra optimizer
- [ ] **Statistics**
  - `pg_ripple.stats()` JSONB: triple count, per-predicate counts, cache hit ratio, delta/main sizes
- [ ] **pg_trickle integration: live statistics** *(optional, when pg_trickle is installed)*
  - `pg_ripple.enable_live_statistics()` creates `_pg_ripple.predicate_stats` and `_pg_ripple.graph_stats` stream tables
  - `pg_ripple.stats()` reads from stream tables instead of full-scanning VP tables (100–1000× faster)
- [ ] **Change notification / CDC**
  - `pg_ripple.subscribe(pattern TEXT, channel TEXT)` — emit `NOTIFY` on triple changes matching a predicate/graph pattern
  - Thin trigger-based CDC on VP delta tables; fires on INSERT/DELETE
  - Payload: JSON with `{"op": "insert"|"delete", "s": ..., "p": ..., "o": ..., "g": ...}` (integer IDs)
  - `pg_ripple.unsubscribe(channel TEXT)` to remove subscriptions
  - Enables downstream event-driven architectures (CDC consumers, webhooks, cache invalidation)
- [ ] Benchmark: concurrent read/write (pgbench custom scripts)
- [ ] **Concurrency correctness tests**
  - Concurrent merge + write: verify no data loss when bulk insert and merge worker run simultaneously
  - Concurrent dictionary encode: two backends encoding the same IRI must return the same i64 (shard lock correctness)
  - Dictionary cache eviction: verify decode correctness after cache entries are evicted under memory pressure
- [ ] **Merge worker edge-case tests**
  - Merge when delta is empty (no-op, no crash)
  - Merge interrupted by `kill -9` (crash recovery: data consistent after restart)
  - Merge when `cache_budget` is near capacity (back-pressure path exercised)
- [ ] pg_regress: `htap_merge.sql`, `change_notification.sql`, `concurrent_write_merge.sql`

### Exit Criteria

Writes do not block reads. Merge worker operates correctly under concurrent writes and crash scenarios. >100K triples/sec bulk insert sustained. Change notifications fire correctly for matching patterns.

---

## v0.7.0 — SHACL Validation (Core)

**Theme**: Data integrity enforcement via W3C SHACL shapes.

> **In plain language:** SHACL is a standard way to define *data quality rules* — for example, "every Person must have exactly one email address" or "an age must be a number". When these rules are loaded, pg_ripple can automatically reject data that violates them the moment it is inserted, rather than discovering errors later. This is similar to how a spreadsheet can reject invalid entries in a cell. A validation report function lets you check existing data against the rules at any time.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **SHACL parser** (Turtle-based shapes)
  - `pg_ripple.load_shacl(data TEXT)` — parse and store shapes
  - Internal shape IR stored in `_pg_ripple.shacl_shapes`
- [ ] **Static constraint compilation**
  - `sh:minCount` → NOT NULL / CHECK trigger
  - `sh:maxCount` → UNIQUE index
  - `sh:datatype` → CHECK on literal datatype
  - `sh:in` → CHECK with allowed values
  - `sh:pattern` → regex CHECK
- [ ] **Synchronous validation mode**
  - Triggered on `insert_triple()` when `pg_ripple.shacl_mode = 'sync'`
  - Returns validation error immediately on constraint violation
- [ ] **Validation report**
  - `pg_ripple.validate(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Full SHACL validation report as JSON
- [ ] **SHACL management**
  - `pg_ripple.list_shapes() RETURNS TABLE`
  - `pg_ripple.drop_shape(shape_uri TEXT)`
- [ ] **pg_trickle integration: SHACL violation monitors** *(optional)*
  - Simple cardinality/datatype constraints modeled as `IMMEDIATE` mode stream tables
  - Violations detected within the same transaction as the DML
- [ ] pg_regress: `shacl_validation.sql`, `shacl_malformed.sql` (invalid shape definitions, circular references, undefined target classes — verify clean error messages)

### Exit Criteria

Core SHACL constraints are enforced at insert time. Validation reports conform to SHACL spec. Malformed shapes are rejected with actionable error messages.

---

## v0.8.0 — SHACL Advanced

**Theme**: Async validation pipeline and complex shapes.

> **In plain language:** Builds on v0.7.0 by supporting more sophisticated data quality rules — for instance, "a person's address must be either a US address or a EU address (but not both)", or "if a company has more than 50 employees, it must have a compliance officer". It also adds a *background validation mode* so that checking complex rules doesn't slow down data loading — violations are flagged asynchronously and collected in a report queue.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **Asynchronous validation pipeline**
  - Validation queue table: `_pg_ripple.validation_queue`
  - Background worker processes queue in batches
  - Dead letter queue for invalid triples with violation reports
  - `pg_ripple.shacl_mode = 'async'` GUC mode
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

## v0.9.0 — Serialization, Export & Interop

**Theme**: Full RDF I/O, remaining serialization formats, and Turtle/JSON-LD serialization for CONSTRUCT/DESCRIBE.

> **In plain language:** RDF data comes in several standard file formats (Turtle, RDF/XML, JSON-LD). This release completes the set so that pg_ripple can import from and export to *all* of them — making it easy to exchange data with other tools and systems. It also adds Turtle and JSON-LD output formats for SPARQL CONSTRUCT and DESCRIBE queries (which returned JSONB since v0.5.1), and RDF-star serialization support.
>
> **Effort estimate: 3–4 person-weeks** *(the hardest parts — Turtle import, N-Triples export, and CONSTRUCT/DESCRIBE JSONB — were already delivered in v0.2.0, v0.3.0, and v0.5.0)*

*Note: Turtle import and N-Triples export were delivered in v0.2.0. CONSTRUCT/DESCRIBE (JSONB output) were delivered in v0.5.1.*

### Deliverables

- [ ] **RDF/XML parser**
  - `pg_ripple.load_rdfxml(data TEXT) RETURNS BIGINT`
- [ ] **Export functions**
  - `pg_ripple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_ripple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Streaming variants returning `SETOF TEXT` for large graphs
- [ ] **SPARQL CONSTRUCT / DESCRIBE serialization formats**
  - CONSTRUCT → returns triples as Turtle or JSON-LD (in addition to JSONB from v0.5.1)
  - DESCRIBE → Turtle and JSON-LD output options
- [ ] **SPARQL-star in CONSTRUCT / DESCRIBE** *(builds on v0.4.0 RDF-star)*
  - CONSTRUCT can produce quoted triples in output
  - Turtle-star and N-Triples-star serialization in export functions
- [ ] pg_regress: `serialization.sql`, `sparql_construct.sql`, `rdf_star_construct.sql`

### Exit Criteria

Round-trip: load Turtle → query → export Turtle. All major RDF serialization formats supported for both import and export.

---

## v0.10.0 — Datalog Reasoning Engine

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
  - Multi-head rules (`h₁, h₂ :- body .`) compiled to separate `INSERT … SELECT` statements within the same stratum
- [ ] **`source` column in VP tables and `vp_rare`**
  - `source SMALLINT DEFAULT 0` added to every dedicated VP table **and to `_pg_ripple.vp_rare`** in the v0.10.0 migration
  - `0` = explicitly asserted; `1` = derived (inferred by Datalog rules)
  - Enables filtering out inferred triples at scan time without a join
  - Migration script uses `ALTER TABLE … ADD COLUMN source SMALLINT NOT NULL DEFAULT 0` for each VP table and for `vp_rare`; zero-downtime because PostgreSQL fast-path adds the column with the stored default without rewriting the table
- [ ] **Tiered hot/cold dictionary** (`src/dictionary/hot.rs`)
  - `_pg_ripple.resources_hot` (UNLOGGED) holds IRIs ≤512B and all predicate/prefix IRIs — the working set that fits in shared buffers
  - Full `resources` table unchanged; encoder checks hot table first
  - `pg_prewarm` warms the hot table at server start via `_PG_init`
  - Dramatically reduces random I/O for the most-accessed terms at large scale (100M+ triples)
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
  - `pg_ripple.check_constraints()` returns violations as JSONB
  - `pg_ripple.enforce_constraints` GUC: `'error'` / `'warn'` / `'off'`
  - Directly complements and extends SHACL validation
- [ ] **Built-in rule sets** (`src/datalog/builtins.rs`)
  - `pg_ripple.load_rules_builtin('rdfs')` — W3C RDFS entailment (13 rules)
  - `pg_ripple.load_rules_builtin('owl-rl')` — W3C OWL 2 RL profile (~80 rules)
- [ ] **On-demand execution mode** (no pg_trickle needed)
  - Derived predicates compiled to inline CTEs injected into SPARQL→SQL at query time
  - `SET pg_ripple.inference_mode = 'on_demand'`
- [ ] **Materialized execution mode** *(optional, requires pg_trickle)*
  - `pg_ripple.materialize_rules(schedule => '10s')` — derived predicates as stream tables
  - pg_trickle DAG scheduler respects stratum ordering automatically
- [ ] **Catalog and management**
  - `_pg_ripple.rules` catalog table
  - `_pg_ripple.rule_sets` catalog: groups named rules with a `rule_hash BYTEA` (XXH3-64) for cache invalidation — re-activating a rule set with an unchanged hash resumes from prior derived state without re-derivation
  - Derived predicates registered in `_pg_ripple.predicates` with `derived = TRUE`
  - `pg_ripple.load_rules()`, `pg_ripple.list_rules()`, `pg_ripple.drop_rules()`
  - `pg_ripple.enable_rule_set(name TEXT)` / `pg_ripple.disable_rule_set(name TEXT)` — activate or deactivate a named rule set without dropping it
- [ ] **SPARQL engine integration**
  - Derived VP tables transparent to query planner (same look-up path as base VP tables)
  - On-demand mode prepends CTEs to generated SQL
  - `pg_ripple.sparql(query TEXT, include_derived BOOL DEFAULT true)` — when `false`, appends `AND source = 0` to all VP table scans to exclude inferred triples (no-inference mode)
- [ ] **SHACL-AF `sh:rule` bridge**
  - Detect `sh:rule` entries in loaded SHACL shapes that contain Datalog-compatible triple rules
  - Compile `sh:rule` bodies to Datalog IR and register in `_pg_ripple.rules`
  - Bidirectional: SHACL shapes inform Datalog constraints; Datalog-derived triples are visible to SHACL validation
  - `pg_ripple.load_shacl()` auto-registers any `sh:rule` triples as Datalog rules when `pg_ripple.inference_mode != 'off'`
- [ ] **RDF-star integration in Datalog** *(builds on v0.4.0 RDF-star)*
  - Quoted triples can appear in Datalog rule heads and bodies
  - Enables provenance rules: `<< ?s ?p ?o >> ex:derivedBy ex:rule1 :- ?s ?p ?o, RULE(ex:rule1) .`
  - Statement identifiers (SIDs) can be used in rule bodies to annotate derived triples
- [ ] pg_regress: `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_custom.sql`, `datalog_negation.sql`, `datalog_arithmetic.sql`, `datalog_constraints.sql`, `shacl_af_rule.sql`, `datalog_malformed.sql` (syntax errors, unstratifiable programs, unbound variables, cyclic rule dependencies — verify clear error messages), `rdf_star_datalog.sql`

### Exit Criteria

Users can load RDFS or OWL RL rule sets (or custom rules), and SPARQL queries return inferred triples. Arithmetic built-ins filter correctly in rule bodies. Constraint rules detect and report violations (optionally rejecting transactions). Both on-demand and materialized modes operational. Stratified negation correctly validated and compiled. SHACL shapes with `sh:rule` entries are auto-compiled to Datalog rules.

---

## v0.11.0 — Incremental SPARQL Views, Datalog Views & ExtVP

**Theme**: Always-fresh materialized SPARQL and Datalog queries, plus extended vertical partitioning, via pg_trickle stream tables.

> **In plain language:** Imagine pinning a SPARQL query — or a set of Datalog reasoning rules — to a dashboard and having the results update automatically whenever the underlying data changes, without re-running the query. That's what SPARQL views and Datalog views deliver. Under the hood, only the *changed* rows are reprocessed (not the entire dataset), so updates are nearly instantaneous. Datalog views go one step further: they bundle rules and a goal pattern into a single self-contained artifact, materializing only the facts relevant to the goal. This release also adds precomputed "shortcut" tables for frequently-combined queries, making common access patterns dramatically faster. Requires the companion pg_trickle extension.
>
> **Effort estimate: 5–7 person-weeks**
>
> ⚠️ **Dependency risk**: This entire release depends on [pg_trickle](https://github.com/grove/pg-trickle) being production-ready. If pg_trickle has not reached a stable release by the time v0.10.0 ships, v0.11.0 should be **deferred** and v0.12.0 (SPARQL Update Advanced) brought forward in its place. A simpler fallback for incrementally-maintained views — using standard PostgreSQL `MATERIALIZED VIEW` with a `pg_ripple.refresh_sparql_view(name)` helper and a pg_cron schedule — can be offered as a non-IVM interim. Evaluate pg_trickle maturity at v0.10.0 exit and decide before committing to v0.11.0 scope.

See [plans/ecosystem/pg_trickle.md § 2.2](plans/ecosystem/pg_trickle.md) for the SPARQL views design and [plans/ecosystem/datalog.md § 15](plans/ecosystem/datalog.md) for the Datalog views design.

### Deliverables

- [ ] **SPARQL views** *(requires pg_trickle)*
  - `pg_ripple.create_sparql_view(name, sparql, schedule, decode)` — compile a SPARQL SELECT query into an always-fresh, incrementally-maintained stream table
  - `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top, minimising CDC surface
  - `pg_ripple.drop_sparql_view(name)` and `pg_ripple.list_sparql_views()` for lifecycle management
  - `_pg_ripple.sparql_views` catalog table: records original SPARQL text, generated SQL, schedule, decode mode, and stream table OID
  - Refresh mode heuristics: `IMMEDIATE` for constraint-style queries, `DIFFERENTIAL` + schedule for dashboards, `FULL` + long schedule for heavy analytics and transitive-closure property paths
- [ ] **Datalog views** *(requires pg_trickle)*
  - `pg_ripple.create_datalog_view(name, rules, goal, schedule, decode)` — bundle a Datalog rule set with a goal pattern into an always-fresh, incrementally-maintained stream table
  - Alternative: `pg_ripple.create_datalog_view(name, rule_set, goal, schedule, decode)` — reference a loaded rule set by name instead of inline rules
  - `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top
  - `pg_ripple.drop_datalog_view(name)` and `pg_ripple.list_datalog_views()` for lifecycle management
  - `_pg_ripple.datalog_views` catalog table: records original rule text, goal pattern, generated SQL, schedule, decode mode, and stream table OID
  - Constraint monitoring: constraint rules (empty-head) automatically synthesize a goal; any row in the stream table is a violation. `IMMEDIATE` mode catches violations within the same transaction
  - Goal-filtered materialization: only facts relevant to the goal pattern are derived and stored, reducing write amplification compared to full-closure materialized rules
- [ ] **ExtVP semi-join stream tables** *(requires pg_trickle)*
  - Manual creation of pre-computed semi-joins between frequently co-joined predicate pairs
  - SPARQL→SQL translator rewrites queries to target ExtVP tables when available
- [ ] **Views over derived predicates**
  - Both SPARQL views and Datalog views can reference Datalog-derived VP tables; pg_trickle DAG handles refresh ordering
- [ ] pg_regress: `sparql_views.sql`, `datalog_views.sql`, `extvp.sql`

### Exit Criteria

Users can create SPARQL views and Datalog views that stay incrementally up-to-date. View queries are sub-millisecond table scans. Datalog views with goal patterns materialize only goal-relevant facts. Constraint monitoring views detect violations in real time. ExtVP semi-joins improve multi-predicate star-pattern performance.

---

## v0.12.0 — SPARQL Update (Advanced)

**Theme**: W3C SPARQL 1.1 Update — pattern-based updates and graph management commands.

> **In plain language:** Building on the basic `INSERT DATA` / `DELETE DATA` support from v0.5.1, this release adds *pattern-based updates* — the ability to find-and-replace data using SPARQL patterns (e.g. "for every person without an email, add a placeholder email"). It also adds commands for managing named graphs (create, clear, drop) and loading data from a URL. This completes the full SPARQL 1.1 Update specification.
>
> **Effort estimate: 3–4 person-weeks** *(simpler than originally estimated since INSERT DATA / DELETE DATA and the Update executor were delivered in v0.5.1)*

### Deliverables

- [ ] **DELETE/INSERT WHERE** (graph update)
  - Pattern-based update: `DELETE { … } INSERT { … } WHERE { … }`
  - Compile WHERE clause via existing SPARQL→SQL engine
  - Transactional: delete + insert in single statement
- [ ] **LOAD / CLEAR / DROP / CREATE**
  - `LOAD <url>` — fetch and load remote RDF data (HTTP GET + parser)
  - `CLEAR GRAPH <g>` — delete all triples in a named graph
  - `DROP GRAPH <g>` — clear + remove graph from registry
  - `CREATE GRAPH <g>` — register a new empty named graph
- [ ] pg_regress: `sparql_update_where.sql`, `sparql_graph_management.sql`

### Exit Criteria

Full SPARQL 1.1 Update operations work correctly. Pattern-based updates compile WHERE clauses via the existing SPARQL→SQL engine.

---

## v0.13.0 — Performance Hardening

**Theme**: Optimize for production-scale workloads. Benchmark-driven improvements.

> **In plain language:** This release is about *speed*. Using the Berlin SPARQL Benchmark (a standard test suite used by the RDF industry), we measure pg_ripple's performance against known baselines and then tune it. Improvements include caching query plans so repeated queries skip redundant work, loading data in parallel, and teaching the system to use data quality rules (from v0.7.0/v0.8.0) as hints to avoid unnecessary work during queries. The target is simple queries answering in under 10 milliseconds on a dataset of 10 million facts, and bulk loading sustained at over 100,000 facts per second.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **Berlin SPARQL Benchmark (BSBM)** integration
  - Data generator adapted for pg_ripple bulk load
  - Full query mix execution with timing
  - Comparison baselines documented
- [ ] **BGP join reordering**
  - At plan time, read `pg_stats.n_distinct` and `pg_class.reltuples` for the target VP tables to estimate the selectivity of each triple pattern
  - Place the most selective pattern first in the join tree to minimize intermediate result sizes
  - Emit `SET LOCAL join_collapse_limit = 1` before the generated SQL to lock the PostgreSQL planner into the computed join order
  - When join columns are already sorted (e.g. after a range scan on an ordered `i64` column), emit `SET LOCAL enable_mergejoin = on` to exploit merge-join (strategy #6)
- [ ] **Query plan caching**
  - Cache SPARQL→SQL translations keyed by query structure hash
  - `pg_ripple.plan_cache_size` GUC
  - Note: SPI-executed dynamic SQL does not benefit from PostgreSQL's built-in prepared-statement cache — the SPARQL-layer plan cache compensates for this by avoiding repeated SPARQL→SQL translation for structurally identical queries
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
  - The algebrizer reads loaded SHACL shapes and the predicate catalog before building the join tree, making shape constraints available as plan-time hints
  - `sh:minCount 1` → OPTIONAL→INNER JOIN downgrade in SPARQL→SQL
  - `sh:maxCount 1` → skip DISTINCT for single-valued properties
  - `sh:class` → VP table pruning based on target class
- [ ] Performance regression test suite (pgbench custom scripts)
- [ ] pg_regress: `shacl_query_opt.sql`

### Exit Criteria

BSBM results documented. >100K triples/sec sustained bulk load. <10ms for simple BGP queries at 10M triples. <5ms for cached repeat queries. SHACL constraints exploited by query optimizer. PostgreSQL parallel plans verified for multi-VP-table joins.

---

## v0.14.0 — Administrative & Operational Readiness

**Theme**: Production operations tooling, upgrade paths, documentation.

> **In plain language:** Everything a system administrator needs to run pg_ripple in production. This includes maintenance commands (clean up, rebuild indexes), monitoring and diagnostics, comprehensive documentation (quickstart guide, function reference, tuning guide), and *graph-level access control* — the ability to control which database users can see or modify which named graphs. It also covers packaging (Linux packages, Docker images) so the extension is easy to install in real environments. Think of this as the "operations manual" release.
>
> **Effort estimate: 4–6 person-weeks**

### Deliverables

- [ ] **Extension upgrade scripts**
  - Tested upgrade path `0.1.0 → ... → 0.16.0`
  - `ALTER EXTENSION pg_ripple UPDATE` works for all version transitions
- [ ] **Administrative functions**
  - `pg_ripple.vacuum()` — force merge + VACUUM on VP tables
  - `pg_ripple.reindex()` — rebuild all VP table indices
  - `pg_ripple.compact(keep_old BOOL DEFAULT false)` — trigger an immediate full merge across all VP tables; `keep_old := false` drops the previous generation's `_main` table immediately after the atomic rename
  - `pg_ripple.vacuum_dictionary()` — remove dictionary entries for IRIs and literals no longer referenced by any VP table row (orphaned after bulk deletes)
  - `pg_ripple.dictionary_stats()` — detailed cache metrics
  - `pg_ripple.predicate_stats()` — per-predicate triple count, table sizes
- [ ] **Logging & diagnostics**
  - Structured logging for merge operations, validation results
  - Custom `EXPLAIN` option showing SPARQL→SQL mapping (PG18 extension EXPLAIN)
- [ ] **Documentation**
  - README with quickstart
  - SQL function reference
  - SPARQL feature matrix
  - **Performance tuning guide** — includes dictionary cache sizing guidance (`dictionary_cache_size` should be large enough to hold the working set of frequently-accessed terms; at 100M+ triples with ~50M unique terms, cache hit ratio is the dominant performance factor), `cache_budget` budgeting, `merge_threshold` tuning, and `vp_promotion_threshold` adjustment for predicate-rich vs predicate-sparse datasets
  - SHACL constraint mapping reference
  - Datalog rule authoring guide
- [ ] **Graph-level Row-Level Security (RLS)**
  - `pg_ripple.enable_graph_rls()` — activate RLS policies on VP tables using the `g` column
  - Policy driven by a mapping table: `_pg_ripple.graph_access (role_name TEXT, graph_id BIGINT, permission TEXT)` — `'read'` / `'write'` / `'admin'`
  - `pg_ripple.grant_graph(role TEXT, graph TEXT, permission TEXT)` / `pg_ripple.revoke_graph()`
  - SPARQL queries automatically filter results to graphs the current role can read
  - Write operations (`insert_triple`, SPARQL UPDATE) enforce write permission
  - Superuser bypass via `pg_ripple.rls_bypass` GUC for admin operations
- [ ] **Packaging**
  - `cargo pgrx package` produces installable `.deb` and `.rpm`
  - Docker image with extension pre-installed
  - PGXN metadata
- [ ] pg_regress: `admin_functions.sql` (vacuum, reindex, dictionary_stats, predicate_stats), `graph_rls.sql` (RLS policy enforcement, cross-role isolation, superuser bypass), `upgrade_path.sql` (install v0.1.0 → load data → sequential upgrade to current version → verify data integrity and query correctness at each step)

### Exit Criteria

Extension is installable, upgradable, and documented. Operational tooling sufficient for production use. Graph-level RLS enforces access control per named graph.

---

## v0.15.0 — SPARQL Protocol (HTTP Endpoint)

**Theme**: Standard HTTP API for SPARQL queries and updates.

> **In plain language:** Without this, the only way to talk to pg_ripple is through a PostgreSQL database connection (SQL). But the entire RDF ecosystem — SPARQL notebooks, visualization tools, ontology editors, web applications — expects to query a triple store over HTTP at a `/sparql` URL. This release adds a lightweight companion service that accepts standard SPARQL HTTP requests, forwards them to pg_ripple inside PostgreSQL, and returns results in all the standard formats (JSON, XML, CSV, Turtle). This is the single biggest adoption enabler: it lets pg_ripple drop in as a replacement for tools like Blazegraph, Virtuoso, or Apache Fuseki without requiring any client-side changes.
>
> **Effort estimate: 3–4 person-weeks**

### Deliverables

- [ ] **Companion HTTP service** (`pg_ripple_http` binary)
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
  - **RDF-star content types** *(builds on v0.4.0 RDF-star)*: Turtle-star and JSON-LD-star for CONSTRUCT/DESCRIBE results containing quoted triples
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
  - Docker image bundles both PostgreSQL (with pg_ripple) and the HTTP service
  - Docker Compose example with separate PG and HTTP containers
- [ ] pg_regress: `sparql_protocol.sql` (protocol-level tests via `curl`)

### Exit Criteria

Standard SPARQL clients (YASGUI, Postman, RDF4J workbench, `curl`) can query and update pg_ripple over HTTP without any pg_ripple-specific configuration. Content negotiation returns correct formats.

---

## v0.16.0 — SPARQL Federation

**Theme**: Query remote SPARQL endpoints from within pg_ripple queries.

> **In plain language:** Federation lets a single SPARQL query combine data from pg_ripple with data from external SPARQL endpoints on the web. For example, you could ask "find all my local employees and enrich their records with data from Wikidata" — and the system will automatically fetch the remote portion, join it with local results, and return a unified answer. This is part of the SPARQL 1.1 standard (`SERVICE` keyword) and is expected by many enterprise knowledge graph workflows that integrate multiple data sources. Multiple remote calls execute in parallel when possible to minimise latency.
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
  - **Async parallel execution**: multiple `SERVICE` clauses in a single query execute concurrently (via `tokio::join!` in pg_ripple_http, or sequential fallback in SPI context) — prevents a single slow endpoint from blocking the entire query
  - Bind-join optimisation: push bound variables from local results into remote queries to reduce remote result size
- [ ] **Error handling and timeouts**
  - `pg_ripple.federation_timeout` GUC (default: 30s per SERVICE call)
  - `pg_ripple.federation_max_results` GUC (default: 10,000 rows per remote call)
  - Graceful degradation: failed SERVICE calls return empty results with a WARNING (configurable to ERROR via `pg_ripple.federation_on_error` GUC)
- [ ] **Security**
  - Allowlist of permitted remote endpoints: `_pg_ripple.federation_endpoints (url TEXT, enabled BOOLEAN)`
  - `pg_ripple.register_endpoint()` / `pg_ripple.remove_endpoint()` management API
  - No outbound HTTP calls unless the endpoint is explicitly registered (defence against SSRF)
- [ ] **`SERVICE` → Materialized View rewrite**
  - When a `SERVICE <url>` clause references an endpoint backed by a local SPARQL view (created via `pg_ripple.create_sparql_view()`), rewrite the remote call to a direct scan of the pre-materialized stream table
  - Registered via a `local_view_name` column on `_pg_ripple.federation_endpoints` — set automatically when a SPARQL view is also registered as an endpoint
  - Eliminates HTTP overhead and enables the PostgreSQL planner to optimize the join with accurate statistics from the stream table
- [ ] **HTTP endpoint integration**
  - Federation works via both SQL (`pg_ripple.sparql()`) and HTTP (`/sparql`) interfaces
- [ ] pg_regress: `sparql_federation.sql`, `federation_timeout.sql`

### Exit Criteria

SPARQL queries with `SERVICE` clauses correctly fetch and join data from registered remote endpoints. Multiple SERVICE calls execute in parallel. Timeouts and error handling work as configured. No SSRF risk — only allowlisted endpoints are contacted.

---

## v1.0.0 — Production Release

**Theme**: Stability, conformance, and production certification.

> **In plain language:** The 1.0 release is not about new features — it's about *confidence*. We run pg_ripple against the official W3C test suites for SPARQL and SHACL to verify standards compliance. A 72-hour continuous stress test checks for memory leaks and crash recovery. A security audit reviews the code for vulnerabilities. The result is a release that organisations can rely on for production workloads with a clear API stability guarantee: the public interface will not break in future minor versions.
>
> **Effort estimate: 6–8 person-weeks**

### Deliverables

- [ ] **SPARQL 1.1 Query conformance**
  - Pass W3C SPARQL 1.1 Query test suite (supported subset)
  - Document unsupported features (property functions)
  - Verify conformance via both SQL and HTTP interfaces
  - Federation (`SERVICE`) covered by v0.16.0
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
  - All `pg_ripple.*` SQL functions considered stable API
  - `_pg_ripple.*` internal schema reserved for internal use
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

> **In plain language:** These are future directions that extend pg_ripple beyond its initial scope. Each addresses a specific real-world need — from distributing data across multiple servers, to geographic queries, to bridging with existing relational databases. They are listed roughly in order of anticipated demand; some may be reordered or combined based on community feedback after 1.0.
>
> **v1.6 Cypher/GQL** has a dedicated exploratory analysis in [plans/cypher/](plans/cypher/). The core finding: VP tables already encode all LPG structural elements; a standalone `cypher-algebra` crate (openCypher + GQL grammar, unified SQL-emitting algebra IR) is the correct architecture. Full write support requires v0.4.0 (RDF-star) for edge properties — already available. Gremlin is explicitly out of scope.

| Version | Theme | What it delivers | Key Technical Features |
|---|---|---|---|
| 1.1 | Distributed | Spread data across multiple servers for horizontal scale | Citus integration, subject-based sharding |
| 1.2 | Vector + Graph | Combine knowledge graphs with AI-style similarity search | pgvector integration, hybrid semantic search |
| 1.3 | Temporal | Track how data changes over time; query historical states | Bitstring versioning, TimescaleDB integration |
| 1.4 | Extended VP | Automatically pre-compute shortcuts for frequent query patterns | Automated workload-driven ExtVP stream tables (pg_trickle), ontology change propagation DAG |
| 1.5 | Interop | Bridge to GraphQL APIs and expose LPG views for visualization tools | GraphQL-to-SPARQL auto-generation from SHACL shapes, stable LPG view layer for visualization tooling |
| 1.6 | Cypher / GQL | Query and write data using the industry-standard graph query languages | `cypher-algebra` standalone crate (openCypher + GQL grammar, same IR); `pg_ripple.cypher()` SQL function; `CREATE`, `MERGE`, `SET`, `DELETE` via VP write path; openCypher TCK ≥80%; edge properties available since v0.4.0 (RDF-star) |
| 1.7 | GeoSPARQL + PostGIS | Answer geographic questions ("find all hospitals within 5 km of this point") | `geo:asWKT` literal type backed by PostGIS `geometry`, spatial FILTER functions, R-tree index on spatial VP tables |
| 1.8 | R2RML Virtual Graphs | Expose existing database tables as if they were RDF data — no migration needed | W3C R2RML mappings, SPARQL queries transparently join VP tables with mapped SQL tables |
| 1.9 | Quad-Level Provenance | Track where each fact came from and when it was added | Per-quad metadata table with source, timestamp, and transaction ID; integration with Datalog rule provenance (why-provenance) |

---

## Version Timeline (Estimated Cadence)

> **In plain language:** The "Calendar" column shows how long after the previous release each version is expected to ship. The "Effort" column shows the total developer-time required. With two developers working together, the calendar durations are achievable; with one developer, roughly double the calendar time.

| Version | Calendar (pair) | Effort (person-weeks) | Cumulative effort |
|---|---|---|---|
| 0.1.0 | Week 0 (start) | 6–8 pw | 6–8 pw |
| 0.2.0 | +4 weeks | 6–8 pw | 12–16 pw |
| 0.3.0 | +4 weeks | 6–8 pw | 18–24 pw |
| 0.4.0 | +5 weeks | 8–10 pw | 26–34 pw |
| 0.5.0 | +3 weeks | 6–8 pw | 32–42 pw |
| 0.5.1 | +3 weeks | 6–8 pw | 38–50 pw |
| 0.6.0 | +4 weeks | 8–10 pw | 46–60 pw |
| 0.7.0 | +3 weeks | 4–6 pw | 50–66 pw |
| 0.8.0 | +3 weeks | 4–6 pw | 54–72 pw |
| 0.9.0 | +2 weeks | 3–4 pw | 57–76 pw |
| 0.10.0 | +5 weeks | 10–12 pw | 67–88 pw |
| 0.11.0 | +3 weeks | 5–7 pw | 72–95 pw |
| 0.12.0 | +2 weeks | 3–4 pw | 75–99 pw |
| 0.13.0 | +4 weeks | 6–8 pw | 81–107 pw |
| 0.14.0 | +3 weeks | 4–6 pw | 85–113 pw |
| 0.15.0 | +2 weeks | 3–4 pw | 88–117 pw |
| 0.16.0 | +3 weeks | 4–6 pw | 92–123 pw |
| 1.0.0 | +4 weeks | 6–8 pw | **98–131 pw** |
| 1.1–1.9 | Post-1.0 | Community-driven | — |

*Estimates assume a pair of focused developers with Rust and PostgreSQL experience. "pw" = person-weeks. Calendar durations assume pair programming; a solo developer should expect roughly double the calendar time. Actual pace depends on contributor availability and scope adjustments discovered during implementation.*
