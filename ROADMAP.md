# pg_triple — Roadmap

> From **0.1.0** (foundation) to **1.0.0** (production-ready triple store)

---

## v0.1.0 — Foundation

**Theme**: Core data model, dictionary encoding, and basic triple CRUD.

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

**Theme**: Per-predicate table layout for real performance.

### Deliverables

- [ ] **Dynamic VP table management**
  - Auto-create `_pg_triple.vp_{predicate_id}` tables on first triple with a new predicate
  - Predicate catalog: `_pg_triple.predicates (id BIGINT, table_oid OID, triple_count BIGINT)`
  - Dual B-tree indices per VP table: `(s, o)` and `(o, s)`
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
- [ ] **IRI prefix management**
  - `pg_triple.register_prefix(prefix TEXT, expansion TEXT)`
  - `pg_triple.prefixes() RETURNS TABLE`
  - Prefix expansion in encode/decode paths
- [ ] Benchmarks: insert throughput (1M triples)
- [ ] pg_regress test suite: `triple_crud.sql`, `named_graphs.sql`

### Exit Criteria

VP layout operational. Bulk loading >50K triples/sec on commodity hardware. Named graphs functional.

---

## v0.3.0 — SPARQL Query Engine (Basic)

**Theme**: Parse and execute SPARQL SELECT queries with basic graph patterns.

### Deliverables

- [ ] **SPARQL parser integration** (`spargebra` crate)
  - Parse SPARQL SELECT queries into algebra tree
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
  - Dictionary decoding of result set
- [ ] **Join optimizations** (phase 1)
  - Self-join elimination for star patterns
  - Filter pushdown: encode FILTER constants before SQL generation
- [ ] `pg_triple.sparql_explain(query TEXT) RETURNS TEXT` — show generated SQL
- [ ] pg_regress: `sparql_queries.sql` (20+ test queries)

### Exit Criteria

Users can run SPARQL SELECT queries with BGPs, FILTER, OPTIONAL against data loaded via bulk load. Queries return correct results.

---

## v0.4.0 — SPARQL Query Engine (Advanced)

**Theme**: Property paths, UNION, aggregates, subqueries.

### Deliverables

- [ ] **Property path compilation**
  - `+` (one or more) → `WITH RECURSIVE` CTE
  - `*` (zero or more) → `WITH RECURSIVE` CTE with zero-hop anchor
  - `?` (zero or one) → `UNION` of direct + zero-hop
  - `/` (sequence) → chained joins
  - `|` (alternative) → `UNION`
  - `^` (inverse) → swap `s`/`o`
  - Cycle detection via visited-node array
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
- [ ] Benchmark: SP2Bench subset
- [ ] pg_regress: `property_paths.sql`, `aggregates.sql`

### Exit Criteria

SPARQL 1.1 Query coverage for all major features except federated queries. Property path queries complete with cycle detection.

---

## v0.5.0 — HTAP Architecture

**Theme**: Separate read and write paths for concurrent OLTP/OLAP.

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
- [ ] **Statistics**
  - `pg_triple.stats()` JSONB: triple count, per-predicate counts, cache hit ratio, delta/main sizes
- [ ] **pg_trickle integration: live statistics** *(optional, when pg_trickle is installed)*
  - `pg_triple.enable_live_statistics()` creates `_pg_triple.predicate_stats` and `_pg_triple.graph_stats` stream tables
  - `pg_triple.stats()` reads from stream tables instead of full-scanning VP tables (100–1000× faster)
- [ ] Benchmark: concurrent read/write (pgbench custom scripts)
- [ ] pg_regress: `htap_merge.sql`

### Exit Criteria

Writes do not block reads. Merge worker operates correctly. >100K triples/sec bulk insert sustained.

---

## v0.6.0 — SHACL Validation (Core)

**Theme**: Data integrity enforcement via W3C SHACL shapes.

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

## v0.7.0 — SHACL Advanced & Query Optimization

**Theme**: Async validation, complex shapes, SHACL-driven query optimization.

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
- [ ] **SHACL-driven query optimization**
  - `sh:minCount 1` → OPTIONAL→INNER JOIN downgrade in SPARQL→SQL
  - `sh:maxCount 1` → skip DISTINCT for single-valued properties
  - `sh:class` → VP table pruning based on target class
- [ ] **pg_trickle integration: multi-shape DAG validation** *(optional)*
  - Multiple SHACL shapes as a DAG of stream tables with topologically-ordered refresh
- [ ] pg_regress: `shacl_advanced.sql`, `shacl_query_opt.sql`

### Exit Criteria

Async validation pipeline operational. Complex SHACL shapes validated correctly. Query optimizer exploits SHACL constraints.

---

## v0.8.0 — Serialization, Export & Interop

**Theme**: Full RDF I/O, multiple serialization formats, and Turtle bulk loading.

### Deliverables

- [ ] **Turtle parser for bulk load**
  - `pg_triple.load_turtle(data TEXT) RETURNS BIGINT`
  - Prefix declarations auto-registered
  - Blank node scoping per load operation
- [ ] **RDF/XML parser**
  - `pg_triple.load_rdfxml(data TEXT) RETURNS BIGINT`
- [ ] **Export functions**
  - `pg_triple.export_turtle(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_triple.export_ntriples(graph TEXT DEFAULT NULL) RETURNS TEXT`
  - `pg_triple.export_jsonld(graph TEXT DEFAULT NULL) RETURNS JSONB`
  - Streaming variants returning `SETOF TEXT` for large graphs
- [ ] **SPARQL CONSTRUCT / DESCRIBE**
  - CONSTRUCT → returns triples as Turtle or JSONB
  - DESCRIBE → concentric bounded description
- [ ] **SPARQL ASK**
  - ASK → returns BOOLEAN
- [ ] **pg_trickle integration: inference materialization** *(optional — superseded by v0.8.5 Datalog engine)*
  - Hard-coded RDFS closures from this milestone are replaced by the general Datalog engine in v0.8.5
  - If v0.8.5 is not yet complete, the minimal `rdfs:subClassOf` / `rdfs:subPropertyOf` stream tables serve as a stepping stone
  - See [plans/ecosystem/datalog.md](plans/ecosystem/datalog.md)
- [ ] pg_regress: `serialization.sql`, `sparql_construct.sql`

### Exit Criteria

Round-trip: load Turtle → query → export Turtle. All major RDF serialization formats supported for both import and export.

---

## v0.8.5 — Datalog Reasoning Engine

**Theme**: General-purpose rule-based inference over the triple store.

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
- [ ] pg_regress: `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_custom.sql`, `datalog_negation.sql`

### Exit Criteria

Users can load RDFS or OWL RL rule sets (or custom rules), and SPARQL queries return inferred triples. Both on-demand and materialized modes operational. Stratified negation correctly validated and compiled.

---

## v0.9.0 — Performance Hardening

**Theme**: Optimize for production-scale workloads. Benchmark-driven improvements.

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
- [ ] **pg_trickle integration: ExtVP and SPARQL view caching** *(optional)*
  - `pg_triple.create_sparql_view(name, sparql, schedule, decode)` — compile a SPARQL SELECT query into an always-fresh, incrementally-maintained stream table; `decode => FALSE` (recommended) keeps integer IDs in the stream table with a thin decoding view on top, minimising CDC surface
  - `pg_triple.drop_sparql_view(name)` and `pg_triple.list_sparql_views()` for lifecycle management
  - `_pg_triple.sparql_views` catalog table: records original SPARQL text, generated SQL, schedule, decode mode, and stream table OID
  - Refresh mode heuristics: `IMMEDIATE` for constraint-style queries, `DIFFERENTIAL` + schedule for dashboards, `FULL` + long schedule for heavy analytics and transitive-closure property paths
  - Manual ExtVP semi-join stream tables for high-frequency predicate pairs
  - See detailed design in [plans/ecosystem/pg_trickle.md § 2.2](plans/ecosystem/pg_trickle.md)
- [ ] Performance regression test suite (pgbench custom scripts)

### Exit Criteria

BSBM results documented. >100K triples/sec sustained bulk load. <10ms for simple BGP queries at 10M triples.

---

## v0.10.0 — Administrative & Operational Readiness

**Theme**: Production operations tooling, upgrade paths, documentation.

### Deliverables

- [ ] **Extension upgrade scripts**
  - Tested upgrade path `0.1.0 → ... → 0.10.0`
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
- [ ] **Packaging**
  - `cargo pgrx package` produces installable `.deb` and `.rpm`
  - Docker image with extension pre-installed
  - PGXN metadata

### Exit Criteria

Extension is installable, upgradable, and documented. Operational tooling sufficient for production use.

---

## v1.0.0 — Production Release

**Theme**: Stability, conformance, and production certification.

### Deliverables

- [ ] **SPARQL 1.1 Query conformance**
  - Pass W3C SPARQL 1.1 Query test suite (supported subset)
  - Document unsupported features (federation, property functions)
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

| Version | Theme | Key Features |
|---|---|---|
| 1.1 | Distributed | Citus integration, subject-based sharding |
| 1.2 | Vector + Graph | pgvector integration, hybrid semantic search |
| 1.3 | Temporal | Bitstring versioning, TimescaleDB integration |
| 1.4 | Extended VP | Automated workload-driven ExtVP stream tables (pg_trickle), ontology change propagation DAG |
| 1.5 | Interop | Apache AGE bridge, GraphQL-to-SPARQL |
| 1.6 | Update | SPARQL 1.1 Update (INSERT/DELETE DATA) |
| 1.7 | Federation | SPARQL SERVICE keyword for remote endpoints |

---

## Version Timeline (Estimated Cadence)

| Version | Target |
|---|---|
| 0.1.0 | Foundation |
| 0.2.0 | +4 weeks |
| 0.3.0 | +4 weeks |
| 0.4.0 | +4 weeks |
| 0.5.0 | +4 weeks |
| 0.6.0 | +3 weeks |
| 0.7.0 | +3 weeks |
| 0.8.0 | +3 weeks |
| 0.8.5 | +3 weeks |
| 0.9.0 | +4 weeks |
| 0.10.0 | +3 weeks |
| 1.0.0 | +4 weeks |

*Estimates assume a small focused team (1–3 developers). Actual pace depends on contributor availability and scope adjustments discovered during implementation.*
