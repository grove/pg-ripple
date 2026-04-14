# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

Nothing yet — next milestone is [v0.3.0 (SPARQL Basic)](ROADMAP.md).

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



