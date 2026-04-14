# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

### Changed

- **Dictionary: switch to hash-backed sequence encoding (Route 2)**
  - `id` column changed from a bare `BIGINT` (holding the truncated upper 64 bits of XXH3-128) to `BIGINT GENERATED ALWAYS AS IDENTITY` — a dense sequential join key independent of the hash.
  - `hash` column changed from `BIGINT` (lower 64 bits) to `BYTEA` (full 16-byte XXH3-128) so the complete 128-bit fingerprint is preserved. A `UNIQUE` index on `hash` is the collision-detection key.
  - The `kind` discriminant is now mixed into the hash input as two little-endian bytes (`kind_le_bytes || term_utf8`) so that the same string encoded as different term types (IRI vs. blank node, etc.) always maps to distinct dictionary rows.
  - Added a backend-local encode cache (`LruCache<u128, i64>`, keyed on the full 128-bit hash) alongside the existing decode cache (`LruCache<i64, String>`).

  **Rationale**: The previous scheme truncated XXH3-128 to 64 bits and used that as the dictionary key directly. At Wikidata scale (~3 billion vocabulary terms) the birthday-problem collision probability in 64-bit space is non-negligible (~1 collision expected per ~4.3 billion terms). The hash-backed sequence preserves collision freedom — the 128-bit hash is stored in full and collisions within 128-bit space are computationally implausible — while keeping all VP-table joins on dense sequential integers.

- **Query plan caching moved from v0.13.0 to v0.3.0**
  - SPI re-parses and re-plans the generated SQL on every call. Caching the SPARQL→SQL translation result keyed on the normalized algebra tree hash avoids this overhead from the first SPARQL-capable release.
  - `pg_ripple.plan_cache_size` GUC introduced in v0.3.0.

- **`WITH RECURSIVE` property path performance caveat added to plan and ROADMAP**
  - PostgreSQL materializes each CTE level. The <100 ms benchmark target applies to depth ≤ 10 on typical datasets; unbounded paths on dense graphs will exceed it. `max_path_depth` GUC and `statement_timeout` are the mitigations.

- **AGENTS.md**: added implementation-status note so automated reviewers know working code exists as of 2026-04-14.

---

## [0.1.0] — in development

### Added

- pgrx 0.17 project scaffolding targeting PostgreSQL 18.
- Extension bootstrap: `CREATE EXTENSION pg_ripple` creates `_pg_ripple` (internal) and `pg_ripple` (user-visible) schemas.
- **Dictionary encoder** (`src/dictionary/mod.rs`)
  - Unified `_pg_ripple.dictionary` table for IRIs, blank nodes, and literals with a `kind` discriminant.
  - `pg_ripple.encode_term(term TEXT, kind SMALLINT) RETURNS BIGINT`
  - `pg_ripple.decode_id(id BIGINT) RETURNS TEXT`
  - Backend-local LRU caches for encode (hash → id) and decode (id → value) paths.
- **Vertical partitioning** (`src/storage/mod.rs`)
  - Auto-creation of `_pg_ripple.vp_{predicate_id}` tables on first encounter of a new predicate.
  - Predicate catalog: `_pg_ripple.predicates (id, table_oid, triple_count)`.
  - Dual B-tree indices per VP table: `(s, o)` and `(o, s)`.
  - Rare-predicate consolidation table: `_pg_ripple.vp_rare`.
  - Global statement-identifier sequence: `_pg_ripple.statement_id_seq`.
- **Basic triple CRUD**
  - `pg_ripple.insert_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT`
  - `pg_ripple.delete_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT`
  - `pg_ripple.triple_count() RETURNS BIGINT`
- **Error taxonomy** (`src/error.rs`): `thiserror`-based PT error codes (PT001–PT099 dictionary, PT100–PT199 storage).
- GUC: `pg_ripple.default_graph` (named-graph IRI; empty = built-in default graph `g=0`).
- Human-readable statistics view: `pg_ripple.predicate_stats`.
- pg_regress test suites: `sql/dictionary.sql`, `sql/basic_crud.sql`.
