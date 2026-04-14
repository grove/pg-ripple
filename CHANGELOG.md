# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

Nothing yet — next milestone is [v0.2.0 (Bulk Loading & Named Graphs)](ROADMAP.md).

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



