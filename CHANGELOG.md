# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

Development towards [v0.8.0 (SHACL Advanced)](ROADMAP.md).

---

## [0.7.0] — 2026-04-15 — SHACL Validation (Core) + Deduplication

This release adds **SHACL Core** data quality enforcement and explicit deduplication utilities. SHACL shapes are loaded from Turtle and stored in the database; they can be enforced inline at insert time (`sync` mode) or evaluated on demand via `validate()`. Two new deduplication functions provide on-demand cleanup for datasets with duplicate triples.

### What you can do

- **Load SHACL shapes** — `pg_ripple.load_shacl(data TEXT)` parses W3C SHACL Turtle and stores each `sh:NodeShape` / `sh:PropertyShape` in `_pg_ripple.shacl_shapes`
- **Validate data** — `pg_ripple.validate(graph TEXT DEFAULT NULL)` runs a full SHACL validation report against all active shapes; returns `{"conforms": bool, "violations": [...]}` as JSONB
- **Inline rejection** — set `pg_ripple.shacl_mode = 'sync'` to have `insert_triple()` reject any triple that violates an active `sh:maxCount`, `sh:datatype`, `sh:in`, or `sh:pattern` constraint
- **Manage shapes** — `list_shapes()` enumerates all loaded shapes; `drop_shape(uri)` removes one
- **Deduplicate triples** — `deduplicate_predicate(p_iri)` removes duplicate `(s, o, g)` rows for one predicate, keeping the lowest-SID row; `deduplicate_all()` deduplicates everything
- **Merge-time dedup** — `pg_ripple.dedup_on_merge = true` makes the HTAP merge worker use `DISTINCT ON` to eliminate duplicates during each generation merge cycle

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_shacl(data TEXT)` | `INTEGER` | Parse Turtle, store shapes, return count loaded |
| `pg_ripple.validate(graph TEXT DEFAULT NULL)` | `JSONB` | Full SHACL validation report |
| `pg_ripple.list_shapes()` | `TABLE(shape_iri TEXT, active BOOLEAN)` | All shapes in the catalog |
| `pg_ripple.drop_shape(shape_uri TEXT)` | `INTEGER` | Remove a shape by IRI |
| `pg_ripple.deduplicate_predicate(p_iri TEXT)` | `BIGINT` | Remove duplicate triples for one predicate |
| `pg_ripple.deduplicate_all()` | `BIGINT` | Deduplicate all predicates and vp_rare |

### New GUCs

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.shacl_mode` | `'off'` | Validation mode: `'off'`, `'sync'`, `'async'` (async: v0.8.0) |
| `pg_ripple.dedup_on_merge` | `false` | Enable merge-time deduplication via `DISTINCT ON` |

### New schema objects

| Object | Description |
|--------|-------------|
| `_pg_ripple.shacl_shapes` | Shape catalog: `shape_iri`, `shape_json` (JSONB IR), `active`, timestamps |
| `_pg_ripple.validation_queue` | Async validation inbox (populated when `shacl_mode = 'async'`) |
| `_pg_ripple.dead_letter_queue` | Async violations with JSONB violation report |

### New regression tests

| Test | Description |
|------|-------------|
| `shacl_validation.sql` | load_shacl, validate, list_shapes, drop_shape, sync mode enforcement |
| `shacl_malformed.sql` | Malformed shapes, missing sh:path, unknown prefix, circular sh:node |
| `deduplication.sql` | Explicit dedup functions, idempotency, merge-time dedup with compact() |

### Documentation

- `user-guide/sql-reference/shacl.md` — new: `load_shacl`, `validate`, `list_shapes`, `drop_shape`; validation report JSON structure; `shacl_mode` GUC
- `user-guide/best-practices/shacl-patterns.md` — new: NodeShape vs PropertyShape, `sh:datatype`/`sh:minCount`/`sh:maxCount`, sync mode latency impact
- `user-guide/pre-deployment.md` — expanded: SHACL mode selection, load shapes before bulk import
- `reference/troubleshooting.md` — expanded: insert rejected by SHACL, shape parsing failures
- `user-guide/sql-reference/admin.md` — expanded: `deduplicate_predicate`, `deduplicate_all`, `dedup_on_merge` GUC

### Supported SHACL constraints (v0.7.0 Core)

`sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, `sh:targetClass`, `sh:targetNode`, `sh:targetSubjectsOf`, `sh:targetObjectsOf`. `sh:or`/`sh:and`/`sh:not` and qualified constraints are v0.8.0.

### Migration

Users upgrading from v0.6.0 must run:

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration script (`sql/pg_ripple--0.6.0--0.7.0.sql`) creates the three new tables (`shacl_shapes`, `validation_queue`, `dead_letter_queue`) and their indexes. No existing tables are modified.

---

## [0.6.0] — 2026-04-15 — HTAP Architecture

This release introduces a full HTAP (Hybrid Transactional/Analytical Processing) storage architecture, separating write traffic from read traffic so both can proceed at full speed simultaneously. A background merge worker periodically promotes delta rows into a read-optimised main partition. Change Data Capture (CDC) enables event-driven subscription to triple changes via PostgreSQL `LISTEN/NOTIFY`.

### What you can do

- **Concurrent reads and writes** — all writes now land in a small `_delta` table (B-tree indexed); the read path sees both the BRIN-indexed `_main` table and `_delta` via `UNION ALL`, so queries never block writers
- **Background merge worker** — when `pg_ripple` is loaded via `shared_preload_libraries`, a background worker periodically compacts delta tables into `_main` using a fresh-table generation merge (sort-ordered insertion, BRIN-optimal), then runs `ANALYZE`
- **Tombstone-based cross-partition deletes** — deleting a triple that lives in `_main` inserts a row in `_pg_ripple.vp_{id}_tombstones`; the query view filters it out immediately and the merge worker eliminates it on next compaction
- **`pg_ripple.compact()` RETURNS BIGINT** — trigger an immediate full merge of all HTAP VP tables; rebuilds `subject_patterns` and `object_patterns` in the same pass; returns the total row count after compaction
- **Subject/object pattern tables** — `_pg_ripple.subject_patterns(s BIGINT, pattern BIGINT[])` and `_pg_ripple.object_patterns(o BIGINT, pattern BIGINT[])` are rebuilt by the merge worker after each generation; GIN-indexed for O(1) predicate lookup per subject/object
- **Change notification / CDC** — `pg_ripple.subscribe(pattern TEXT, channel TEXT) RETURNS BIGINT` subscribes to triple changes matching a predicate pattern (use `'*'` for all); `pg_ripple.unsubscribe(channel TEXT) RETURNS BIGINT` removes subscriptions; notifications fire as `pg_notify(channel, '{"op":"insert|delete","s":...,"p":...,"o":...,"g":...}')` after each delta insert or delete
- **Statistics** — `pg_ripple.stats() RETURNS JSONB` reports `total_triples`, `dedicated_predicates`, `htap_predicates`, `rare_triples`, `unmerged_delta_rows`, and `merge_worker_pid`
- **`ExecutorEnd_hook` latch-poke** — when `shared_preload_libraries` is set, an `ExecutorEnd_hook` monitors `TOTAL_DELTA_ROWS`; once it reaches `latch_trigger_threshold`, the hook pokes the merge worker latch to trigger an early merge without waiting for the next poll cycle

### New GUCs

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.merge_threshold` | `10000` | Minimum delta rows before background merge triggers |
| `pg_ripple.merge_interval_secs` | `60` | Max seconds between merge worker polling cycles |
| `pg_ripple.merge_retention_seconds` | `60` | Seconds to keep previous main table before dropping |
| `pg_ripple.latch_trigger_threshold` | `10000` | Batch rows before poking merge worker latch (ExecutorEnd hook) |
| `pg_ripple.worker_database` | `postgres` | Database the merge worker connects to |
| `pg_ripple.merge_watchdog_timeout` | `300` | Seconds of worker inactivity before a WARNING is logged |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.compact()` | `BIGINT` | Immediate full merge + pattern table rebuild |
| `pg_ripple.htap_migrate_predicate(pred_id BIGINT)` | `void` | Migrate a flat VP table to HTAP split |
| `pg_ripple.stats()` | `JSONB` | Storage and worker statistics |
| `pg_ripple.subscribe(pattern TEXT, channel TEXT)` | `BIGINT` | Subscribe to CDC notifications |
| `pg_ripple.unsubscribe(channel TEXT)` | `BIGINT` | Remove a CDC subscription |
| `pg_ripple.subject_predicates(subject_id BIGINT)` | `BIGINT[]` | Predicates for a subject (from pattern table) |
| `pg_ripple.object_predicates(object_id BIGINT)` | `BIGINT[]` | Predicates for an object (from pattern table) |

### New regression tests

| Test | Description |
|------|-------------|
| `htap_merge.sql` | Delta→main promotion, tombstone-based deletes, compact idempotency |
| `change_notification.sql` | CDC subscribe/notify, wildcard patterns, payload validation |
| `merge_edge_cases.sql` | Edge cases: empty-delta compact, idempotency, delta-resident delete, non-existent delete |

### Documentation

- `user-guide/configuration.md` — expanded with all HTAP GUCs and descriptions
- `user-guide/scaling.md` — new: HTAP architecture diagram, merge lifecycle, tuning guide
- `user-guide/pre-deployment.md` — new: production checklist (`shared_preload_libraries`, memory sizing, monitoring)
- `user-guide/sql-reference/admin.md` — new: `compact()`, `stats()`, `subscribe()`, `unsubscribe()`, `htap_migrate_predicate()`, `predicate_stats` view
- `user-guide/best-practices/bulk-loading.md` — expanded with HTAP delta-growth strategies
- `reference/troubleshooting.md` — expanded with merge worker not running, delta bloat, CDC not firing
- `reference/faq.md` — expanded with `shared_preload_libraries`, merge worker, change notifications

### Migration

Users upgrading from v0.5.1 must run:

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration script (`sql/pg_ripple--0.5.1--0.6.0.sql`) adds the `htap` column to `_pg_ripple.predicates`, creates the pattern tables and CDC infrastructure, and migrates every existing dedicated VP table to the delta/main/tombstones architecture in a single transaction. After migration, existing triples reside in delta tables; call `pg_ripple.compact()` to promote them to `_main` immediately.

### Bug fixes

- **`shmem::init()` race condition** — `SHMEM_READY` is now set inside a final `shmem_startup_hook` rather than immediately in `_PG_init`, eliminating the window where `SHMEM_READY = true` but `PgAtomic` inner pointers were still null
- **Postmaster GUC registration crash** — `dictionary_cache_size` and `cache_budget` GUCs (both `GucContext::Postmaster`) are now only registered when `process_shared_preload_libraries_in_progress` is true, preventing `FATAL: cannot create PGC_POSTMASTER variables after startup` when `CREATE EXTENSION pg_ripple` runs without `shared_preload_libraries`
- **SPARQL aggregate decode bug** — aggregate results (COUNT, SUM, etc.) were incorrectly dictionary-decoded instead of being emitted as raw numbers; the `raw_numeric_vars` set is now propagated through `Extend` nodes to `execute_select`
- **Merge worker DROP TABLE without CASCADE** — the fresh-table generation merge failed when the old `_main` table had dependent views; `DROP TABLE` now uses `CASCADE` and the view is recreated afterward
- **Merge worker stale BRIN index** — repeated `compact()` calls failed with "relation already exists" because the BRIN index name survived table renames; the merge now drops the stale index before creating a new one

### Benchmarks & CI

- **Insert throughput benchmark** — `benchmarks/insert_throughput.sql` measures 1M-triple insert throughput and query latency
- **CI performance regression baseline** — `benchmarks/ci_benchmark.sh` records insert throughput and point-query latency; CI `benchmark` job uploads results as artifacts and can gate on >10% regression

### New regression tests

| Test | Description |
|------|-------------|
| `htap_merge.sql` | Delta→main promotion, tombstone-based deletes, compact idempotency |
| `change_notification.sql` | CDC subscribe/notify, wildcard patterns, payload validation |
| `merge_edge_cases.sql` | Edge cases: empty-delta compact, idempotency, delta-resident delete, non-existent delete |
| `sparql_star_conformance.sql` | W3C SPARQL-star conformance gate: N-Triples-star parsing, dictionary round-trips, SID lifecycle, annotation patterns, ground triple patterns, data integrity, known-limitation documentation |

---

## [0.5.1] — 2025-04-15 — SPARQL Advanced (Storage, Serialization & Write)

This release introduces inline value encoding for numeric and date literals, completes all four SPARQL query forms with CONSTRUCT and DESCRIBE, adds basic SPARQL Update (INSERT DATA / DELETE DATA), and delivers full-text search on literal objects.

### What you can do

- **Inline value encoding** — `xsd:integer`, `xsd:boolean`, `xsd:date`, and `xsd:dateTime` literals are now stored as bit-packed `BIGINT` IDs in VP tables. FILTER comparisons on these types (`>`, `<`, `<=`, `>=`, `=`) require zero dictionary round-trips and execute as plain SQL integer comparisons
- **SPARQL CONSTRUCT** — `pg_ripple.sparql_construct(query TEXT) RETURNS SETOF JSONB` — constructs new triples from a template and returns them as JSONB objects `{s, p, o}`; supports both explicit-template and bare CONSTRUCT WHERE forms
- **SPARQL DESCRIBE** — `pg_ripple.sparql_describe(query TEXT, strategy TEXT) RETURNS SETOF JSONB` — returns Concise Bounded Description (CBD) or Symmetric CBD (SCBD) of named resources as JSONB triples; `pg_ripple.describe_strategy` GUC selects the default algorithm
- **SPARQL UPDATE** — `pg_ripple.sparql_update(query TEXT) RETURNS BIGINT` — executes `INSERT DATA { … }` and `DELETE DATA { … }` statements; returns count of affected triples; supports typed literals including inline-encoded types
- **Full-text search** — `pg_ripple.fts_index(predicate TEXT)` creates a GIN `tsvector` index on string-literal objects of a predicate; `pg_ripple.fts_search(query TEXT, predicate TEXT) RETURNS TABLE(s TEXT, p TEXT, o TEXT)` searches using PostgreSQL `tsquery` syntax

### Bug fixes

- `fts_index`: accepts N-Triples notation (`<IRI>`) for predicate — angle brackets are stripped before dictionary lookup
- `fts_index`: index predicate changed from subquery-based partial index (unsupported by PostgreSQL) to a `WHERE kind = 2` partial index on all plain string literals — search correctness is ensured by the VP-table JOIN in `fts_search`
- `batch_decode`: inline IDs are now decoded locally without a DB round-trip, fixing incorrect NULL returns for inline-encoded values in SPARQL SELECT results

### New GUCs

- `pg_ripple.describe_strategy` (TEXT, default `'cbd'`) — DESCRIBE expansion algorithm: `'cbd'` (Concise Bounded Description), `'scbd'` (Symmetric CBD), `'simple'` (single subject only)

### Test infrastructure

- 5 new pg_regress test files: `inline_encoding.sql`, `sparql_construct.sql`, `sparql_insert_data.sql`, `sparql_delete_data.sql`, `fts_search.sql`
- Total: 19 pg_regress tests, all passing

### Documentation

- `user-guide/sql-reference/sparql-query.md` — CONSTRUCT / DESCRIBE, `describe_strategy` GUC
- `user-guide/sql-reference/sparql-update.md` — `sparql_update()`, INSERT DATA / DELETE DATA
- `user-guide/sql-reference/fts.md` — `fts_index`, `fts_search`
- `user-guide/best-practices/update-patterns.md` — INSERT DATA vs bulk load, idempotent patterns

## [0.5.0] — 2026-04-15 — SPARQL Advanced Query Engine

This release completes the SPARQL query engine with property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES, and OPTIONAL. All standard SPARQL 1.1 graph pattern forms are now supported.

### What you can do

- **Property paths** — `+`, `*`, `?`, `/`, `|`, `^` all compile to PostgreSQL `WITH RECURSIVE` CTEs with PG18 `CYCLE` clause for O(1) hash-based cycle detection; unbounded traversal on cyclic graphs terminates safely
- **UNION / MINUS** — `UNION { ... } UNION { ... }` and `MINUS { ... }` compile to SQL `UNION` / `LEFT JOIN … WHERE NULL` anti-join
- **Aggregates** — `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT` with `GROUP BY` and `HAVING` compile to SQL aggregates
- **Subqueries** — nested `{ SELECT … WHERE { … } }` patterns supported at any depth
- **BIND / VALUES** — `BIND(<expr> AS ?var)` and `VALUES ?x { … }` compile to SQL expressions and inline data rows
- **OPTIONAL** — `OPTIONAL { … }` compiles to a `LEFT JOIN` correctly preserving unbound variables
- **Resource exhaustion safety** — `pg_ripple.max_path_depth` GUC limits recursive CTE depth; plan cache keyed on the GUC value so cache invalidation works correctly

### Bug fixes

- **Sequence paths (`p/q`)**: spargebra encodes intermediate variables as anonymous `BlankNode`s; `bind_term` now correctly applies equi-join constraints on blank nodes instead of treating them as wildcards (Cartesian product bug)
- **ZeroOrMore (`p*`) CYCLE error**: PostgreSQL 18 requires exactly one non-recursive anchor in a `WITH RECURSIVE` CTE to use `CYCLE`; combined one-hop and zero-hop anchors into a single subquery
- **LeftJoin alias bug**: `build_from` appended `AS alias` to an already-aliased expression; redesigned to use subquery projections with distinct `_lc_`/`_rc_` prefixes
- **GROUP BY column refs**: inner query now uses explicit `_gi_{v}` column aliases instead of `SELECT *` to prevent `_t0.o` style references from going out of scope
- **MINUS ON clause**: fixed `ON _lminus.{v}` → `ON _lminus._m_{v}` matching the alias assigned by `translate_minus`
- **VALUES double alias**: removed duplicate `AS alias` from `(VALUES …) AS t AS t` pattern
- **BIND/Extend expression**: `translate_extend` now uses `translate_expr_value` (raw column reference) instead of `translate_expr` (`IS NOT NULL` boolean predicate) — critical for `SELECT (COUNT(?p) AS ?cnt)` subquery patterns
- **Numeric literal comparison**: xsd:integer/decimal/float/double literals in FILTER expressions now produce raw numeric SQL values instead of dictionary IDs — fixes `FILTER(?cnt >= 2)` and similar
- **Plan cache staleness**: cache key now includes current `pg_ripple.max_path_depth` GUC value; changing depth mid-session no longer returns stale cached results

### Documentation

- **Docs site launched** — mdBook-based site at `docs/` with GitHub Pages auto-publish workflow (`.github/workflows/docs.yml`)
- Catch-up pages for v0.1.0–v0.4.0: introduction, installation, getting-started, SQL reference (triple CRUD, bulk load, named graphs, SPARQL queries, RDF-star, dictionary, prefix), configuration (all GUCs), best-practices (data modeling, bulk loading, SPARQL patterns), FAQ (14 questions), troubleshooting, playground, security policy stub, research index
- v0.5.0 pages: SPARQL query reference expanded (property paths, aggregates, UNION/MINUS, subqueries, BIND/VALUES), SPARQL patterns guide expanded (property path recipes, resource exhaustion safeguards), configuration expanded (`max_path_depth` GUC)

### Test infrastructure

- All pg_regress test files are now fully idempotent via namespace-scoped `DELETE FROM _pg_ripple.vp_rare` cleanup blocks at the start of each file — safe to run multiple times against the same pgrx-managed database
- `setup.sql` drops and recreates the extension before each run (`DROP EXTENSION IF EXISTS … CASCADE`) for full isolation
- New tests: `property_paths.sql`, `aggregates.sql`, `resource_limits.sql`
- All 12 tests pass on consecutive runs (12/12)

---

## [0.4.0] — 2026-04-14 — RDF-star / Statement Identifiers

This release adds RDF-star support: quoted triples, statement identifiers, and SPARQL-star query patterns. You can now make statements about statements — essential for provenance, temporal annotations, and LPG-style edge properties.

### What you can do

- **Load N-Triples-star data** — `pg_ripple.load_ntriples()` now accepts N-Triples-star input including subject-position and object-position quoted triples, and nested quoted triples
- **Encode and decode quoted triples** — `pg_ripple.encode_triple(s, p, o) RETURNS BIGINT` encodes a quoted triple to its dictionary ID; `pg_ripple.decode_triple(id) RETURNS JSONB` reverses the lookup
- **Use statement identifiers (SIDs)** — `pg_ripple.insert_triple()` now accepts an optional named graph and returns the statement SID; SIDs are stable `BIGINT` values that can appear as subjects or objects in other triples
- **Look up statements by SID** — `pg_ripple.get_statement(i BIGINT) RETURNS JSONB` returns `{"s":...,"p":...,"o":...,"g":...}` for the statement with that identifier
- **Query with SPARQL-star** — Ground (all-constant) triple term patterns in SPARQL WHERE clauses are supported; e.g. `WHERE { << :Alice :knows :Bob >> :assertedBy ?who }`

### Technical highlights

- `KIND_QUOTED_TRIPLE = 5` added to the dictionary; quoted triples stored with `qt_s`, `qt_p`, `qt_o` columns via non-destructive `ALTER TABLE … ADD COLUMN IF NOT EXISTS`
- Custom recursive-descent N-Triples-star line parser — avoids the `oxrdf/rdf-12` + `spargebra` feature conflict by using only a pure-Rust parser with no new crate dependencies
- `spargebra` and `sparopt` now use the `sparql-12` feature, which properly enables `TermPattern::Triple` with correct exhaustiveness guards
- SPARQL-star ground patterns compile to a dictionary lookup + SQL equality condition; variable-inside-quoted-triple patterns emit a warning and match nothing (deferred to v0.5.x)

### Known limitations

- Turtle-star is not yet supported; use N-Triples-star for RDF-star bulk loading
- Variable-inside-quoted-triple SPARQL patterns (e.g. `<< ?s :knows ?o >> :assertedBy ?who`) are deferred to v0.5.x
- W3C SPARQL-star conformance test suite not yet run (deferred to v0.5.x)

---

## [0.3.0] — 2026-04-14 — SPARQL Query Engine (Basic)

This release introduces SPARQL SELECT and ASK queries. You can now ask questions over stored RDF data using the standard W3C query language, with results returned as JSONB rows.

### What you can do

- **Run SPARQL SELECT queries** — `pg_ripple.sparql(query TEXT) RETURNS SETOF JSONB` parses and executes a SPARQL SELECT, returning one JSONB object per result row with variable names as keys and N-Triples–formatted term strings as values
- **Run SPARQL ASK queries** — `pg_ripple.sparql_ask(query TEXT) RETURNS BOOLEAN` returns `TRUE` if any results exist
- **Inspect generated SQL** — `pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false) RETURNS TEXT` shows the SQL generated from a SPARQL query; pass `analyze := true` to run EXPLAIN ANALYZE on it
- **Tune plan cache size** — `pg_ripple.plan_cache_size` GUC (default: 256) controls how many SPARQL→SQL translations are cached per backend; set to `0` to disable

### Supported SPARQL features

- Basic Graph Patterns (BGP) with bound subjects, predicates, and objects
- `FILTER` expressions: `=`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `!`, `BOUND()`
- `OPTIONAL` (LEFT JOIN)
- `GRAPH <iri> { ... }` and `GRAPH ?g { ... }` named-graph patterns
- `PROJECT` (SELECT variable list), `DISTINCT`, `REDUCED`
- `LIMIT`, `OFFSET`
- `ORDER BY` (single variable, ASC/DESC)
- Result pagination via `LIMIT`/`OFFSET`

### How it works behind the scenes

- SPARQL text is parsed by `spargebra 0.4` into a `GraphPattern` algebra tree
- The SPARQL algebra is translated to SQL by `src/sparql/sqlgen.rs`: each BGP triple pattern maps to a VP-table join (integer equality on encoded IDs); every IRI and literal constant is encoded to an `i64` before appearing in SQL — SQL injection via SPARQL constants is structurally impossible
- A per-query encoding cache (`Ctx.per_query`) avoids redundant SPI dictionary lookups for constants that appear multiple times in one query
- Self-join elimination: BGP patterns that share a subject but use different predicates are compiled into a single scan with multiple predicates joined, not separate subqueries
- Batch decode: all `i64` result columns are collected, deduplicated, and decoded in a single `SELECT ... WHERE id IN (...)` round-trip; no per-row dictionary queries
- `sparopt 0.3` is in `Cargo.toml`; direct algebra-tree conversion between sparopt and spargebra types is not yet available (distinct type systems), so filter-pushdown and constant-folding are implemented inline in the SQL generator
- A `vp_source` lookup bug was discovered and fixed: pgrx 0.17 returns `Err(InvalidPosition)` for zero-row queries rather than `Ok(None)`, so querying with `AND table_oid IS NOT NULL` incorrectly returned `VpSource::Empty` for rare-predicate tables

### Technical Details

<details>
<summary>Click to expand implementation details</summary>

- **New module `src/sparql/`**: `mod.rs` (public functions), `sqlgen.rs` (SPARQL algebra → SQL), `plan_cache.rs` (LRU translation cache)
- **New functions**: `pg_ripple.sparql`, `pg_ripple.sparql_ask`, `pg_ripple.sparql_explain`
- **New GUC**: `pg_ripple.plan_cache_size` (i32, default 256, range 0–65536)
- **Dictionary additions** (`src/dictionary/mod.rs`): `lookup(term, kind)` and `lookup_iri(iri)` — read-only dictionary lookups that return `None` for unknown terms, used by the SPARQL translator to handle unresolvable IRIs without polluting the dictionary
- **Dependencies added**: `spargebra = "0.4"`, `sparopt = "0.3"`
- **Test deadlock fix**: added `RUST_TEST_THREADS = "1"` to `.cargo/config.toml` to serialize `cargo pgrx test` execution and prevent concurrent dictionary upsert deadlocks
- **pg_test tests** (8 new, in `src/lib.rs`): `pg_test_sparql_select_empty`, `pg_test_sparql_select_one_triple`, `pg_test_sparql_ask_empty`, `pg_test_sparql_ask_match`, `pg_test_sparql_explain_returns_sql`, `pg_test_sparql_limit`, `pg_test_sparql_distinct`, `pg_test_sparql_filter_bound`
- **pg_regress tests**: `sparql_queries.sql` (10 queries), `sparql_injection.sql` (7 adversarial inputs)

</details>

---

## [0.2.0] — 2026-04-14 — Bulk Loading & Named Graphs

This release adds bulk data import, named graph management, N-Triples/N-Quads export, and rare-predicate consolidation. You can now load large RDF datasets in standard formats without inserting triples one at a time.

### What you can do

- **Load RDF data in bulk** — `pg_ripple.load_ntriples(data TEXT)`, `load_nquads(data TEXT)`, `load_turtle(data TEXT)`, `load_trig(data TEXT)` accept standard RDF text and return the number of triples loaded
- **Load from a server-side file** — `load_ntriples_file(path TEXT)` and its siblings read a file via `pg_read_file()` (superuser required); essential for datasets larger than ~1 GB
- **Use named graphs** — group related triples into labelled collections with `pg_ripple.create_graph('<iri>')`, drop them with `drop_graph('<iri>')`, and list all graphs with `list_graphs()`
- **Export data** — `pg_ripple.export_ntriples(graph)` and `export_nquads(graph)` serialise stored triples back to standard text formats; pass `NULL` to export all triples
- **Register IRI prefixes** — `pg_ripple.register_prefix('ex', 'https://example.org/')` records abbreviations for use in future query features; `prefixes()` lists all registered mappings
- **Promote rare predicates manually** — `pg_ripple.promote_rare_predicates()` moves any predicate that has accumulated enough triples into its own dedicated table

### What happens behind the scenes

- Predicates with fewer than 1,000 triples (configurable via `pg_ripple.vp_promotion_threshold`) are held in a shared `vp_rare` table rather than creating a separate table for each. Once a predicate crosses the threshold, its triples are automatically migrated to a dedicated table
- Blank node identifiers from different load operations are isolated by a generation counter — `_:b0` from two separate load calls always produces two distinct dictionary entries, preventing unintended merging of blank nodes across files
- After each bulk load, `ANALYZE` is run on the affected tables so the query planner has accurate row-count estimates ready for the SPARQL engine (v0.3.0)
- The `_pg_ripple.statements` range-mapping catalog is created in this release; it maps statement-ID ranges to the VP tables they belong to. This table is populated by the merge worker in v0.6.0+ and is required by RDF-star in v0.4.0
- Literals (plain, language-tagged, and typed) are now properly encoded in both the SQL API and the bulk loaders — `insert_triple('<s>', '<p>', '"hello"@en')` stores a language-tagged literal, and `insert_triple('<s>', '<p>', '"42"^^<xsd:integer>')` stores a typed literal

### Technical Details

<details>
<summary>Click to expand implementation details</summary>

- **rio_turtle 0.8 / rio_api 0.8** added as dependencies for N-Triples, N-Quads, Turtle, and TriG parsing
- **Blank node scoping** (`_pg_ripple.load_generation_seq`): each load call advances a shared sequence; blank node hashes are prefixed with `"{generation}:"` so cross-load merging is impossible
- **Rare-predicate routing** (`src/storage/mod.rs`): `insert_triple` checks `_pg_ripple.predicates.table_oid IS NOT NULL` before routing to vp_rare. `batch_insert_encoded` in the bulk loader groups triples by predicate and issues a single multi-row INSERT per predicate group, reducing SPI round-trips. Promotion to a dedicated VP table is deferred to the end of each bulk load; `promote_rare_predicates()` can also be called manually
- **Named graph support** (`src/storage/mod.rs`): `create_graph`, `drop_graph`, `list_graphs` operate on the `g` column already present in every VP table and vp_rare. A `(g, p, s, o)` index on vp_rare supports efficient graph-drop bulk-delete
- **`pg_ripple.named_graph_optimized` GUC**: when enabled at table creation time, adds a `(g, s, o)` index to each dedicated VP table for fast graph-scoped queries
- **`_pg_ripple.statements` catalog**: lightweight range-mapping table `(sid_min, sid_max, predicate_id, table_oid)` created now; populated in v0.6.0
- **`_pg_ripple.prefixes` table**: `(prefix TEXT PRIMARY KEY, expansion TEXT)` for IRI prefix abbreviations
- **Literal encoding** (`src/dictionary/mod.rs`): `encode_typed_literal`, `encode_lang_literal`, `encode_plain_literal`, `decode_full`, `format_ntriples` added to support proper RDF term types throughout the storage and export paths
- **N-Triples / N-Quads export** (`src/export.rs`): `export_ntriples` and `export_nquads` decode i64 IDs in bulk via `format_ntriples` and assemble the output string
- **GUCs added**: `pg_ripple.vp_promotion_threshold` (i32, default 1000), `pg_ripple.named_graph_optimized` (bool, default off)
- **pg_regress tests**: `triple_crud.sql`, `named_graphs.sql`, `export_ntriples.sql`, `nquads_trig.sql`

</details>

---

## [0.1.0] — 2026-04-14 — Foundation

pg_ripple can now be installed into a PostgreSQL 18 database. After installation, you can store facts (triples like "Alice knows Bob") and retrieve them by pattern. This is the first working release — no query language yet, just the basic building blocks.

### What you can do

- **Install the extension** into any PostgreSQL 18 database with `CREATE EXTENSION pg_ripple` (requires database superuser)
- **Store facts** — `pg_ripple.insert_triple('<Alice>', '<knows>', '<Bob>')` saves a fact and returns a unique identifier for it
- **Find facts by pattern** — `pg_ripple.find_triples('<Alice>', NULL, NULL)` finds everything about Alice; use NULL as a wildcard for any position
- **Delete facts** — `pg_ripple.delete_triple(...)` removes a specific fact
- **Count facts** — `pg_ripple.triple_count()` returns how many facts are stored
- **Encode and decode terms** — `pg_ripple.encode_term(...)` converts a text term to its internal numeric ID; `pg_ripple.decode_id(...)` converts it back

### What happens behind the scenes

- Every piece of text (names, URLs, values) is converted to a compact number before storage, so lookups and joins are fast
- Facts are automatically organized into one table per relationship type (called "vertical partitioning") — this makes pattern queries efficient
- Rarely-used relationship types share a single table to avoid creating thousands of small tables
- Every fact gets a globally unique identifier, which will be used in future versions for making statements about statements (RDF-star)
- A continuous integration pipeline automatically checks code quality and runs all tests on every change

### Technical Details

<details>
<summary>Click to expand implementation details</summary>

- pgrx 0.17 project scaffolding targeting PostgreSQL 18
- Extension bootstrap creates `pg_ripple` (user-visible) and `_pg_ripple` (internal) schemas
  - `pg_ripple` schema requires `superuser = true` and a bootstrap `SET LOCAL allow_system_table_mods = on` due to PostgreSQL's `pg_` prefix restriction
- **Dictionary encoder** (`src/dictionary/mod.rs`): unified `_pg_ripple.dictionary` table with hash-backed-sequence encoding (XXH3-128 full hash stored in BYTEA; dense IDENTITY sequence id as join key). Backend-local LRU caches for encode and decode paths. CTE-based upsert pattern avoids pgrx 0.17 `InvalidPosition` error on empty `RETURNING` results.
- **Vertical partitioning** (`src/storage/mod.rs`): auto-created `_pg_ripple.vp_{predicate_id}` tables with dual B-tree indices on `(s,o)` and `(o,s)`. `_pg_ripple.predicates` catalog tracks table OIDs and triple counts. `_pg_ripple.vp_rare` consolidation table for low-frequency predicates. `_pg_ripple.statement_id_seq` shared sequence for globally-unique statement IDs.
- **Error taxonomy** (`src/error.rs`): `thiserror`-based error types — PT001–PT099 (dictionary), PT100–PT199 (storage)
- GUC parameter: `pg_ripple.default_graph`
- GUC-gated lazy initialization: future subsystems (merge worker, SHACL, reasoning) start only when enabled
- `pg_ripple.predicate_stats` view for human-readable statistics
- pg_regress tests: `setup.sql`, `dictionary.sql`, `basic_crud.sql`
- CI pipeline: fmt, clippy, pg_test, pg_regress (`.github/workflows/ci.yml`)

</details>



