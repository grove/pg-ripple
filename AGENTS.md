# pg_ripple — Agent Guidelines

**pg_ripple** is a PostgreSQL 18 extension written in Rust (pgrx 0.17) that implements a high-performance RDF triple store with native SPARQL query execution. See [plans/implementation_plan.md](plans/implementation_plan.md) for the full architecture and [ROADMAP.md](ROADMAP.md) for the phased delivery plan.

> **Implementation status** (as of 2026-04-14): v0.1.0 code exists and builds. The dictionary encoder (`src/dictionary/`), VP storage engine (`src/storage/`), and basic triple CRUD SQL functions are implemented. The SPARQL engine, SHACL, Datalog, and all higher-level features remain unimplemented. When external reviewers say "no code has been written yet" they are working from an earlier state; that is no longer accurate.

## Tech Stack

| Concern | Technology |
|---|---|
| Language | Rust, Edition 2024 |
| PG binding | pgrx 0.17 (`pg18` feature flag) |
| PostgreSQL target | 18.x only |
| SPARQL parser | `spargebra` |
| SPARQL optimizer | `sparopt` (first-pass algebra optimizer) |
| RDF parsers | `rio_turtle`, `rio_xml`; `oxttl` / `oxrdf` added at v0.4.0 for RDF-star |
| Hashing | `xxhash-rust` (XXH3-128) |
| Serialization | `serde` + `serde_json` |
| Tests | `#[pg_test]`, `cargo pgrx regress`, `pgbench` via `pgrx-bench` |

## Architecture

```
src/lib.rs          — pgrx entry points, _PG_init, GUC parameters
src/dictionary/     — IRI/blank-node/literal → i64 encoder (XXH3-128 + LRU cache)
src/storage/        — VP tables, HTAP delta/main partitions, merge background worker
src/sparql/         — SPARQL text → spargebra algebra → SQL → SPI execution → decode
src/datalog/        — Datalog rule parser, stratifier, SQL compiler, built-in RDFS/OWL RL
src/shacl/          — SHACL shapes → DDL constraints + async validation pipeline
src/export/         — Turtle / N-Triples / JSON-LD serialization
src/stats/          — Monitoring, pg_stat_statements integration
src/admin/          — vacuum, reindex, prefix registry
```

All user-visible objects live in the `pg_ripple` schema; internal tables and VP tables live in `_pg_ripple`.

## Storage Conventions

- **Dictionary encoding**: every IRI, blank node, and literal is mapped to `BIGINT` (i64) via XXH3-128 hash before being stored in the unified `_pg_ripple.dictionary` table. VP tables **never** contain raw strings.
- **VP table naming**: `_pg_ripple.vp_{predicate_id}` — one table per unique predicate. Columns: `s BIGINT, o BIGINT, g BIGINT, i BIGINT NOT NULL DEFAULT nextval('statement_id_seq'), source SMALLINT DEFAULT 0`. Dual B-tree indices on `(s, o)` and `(o, s)`. The `i` column is a globally-unique statement identifier (SID) from a shared sequence introduced in v0.1.0; the `source` column (v0.10.0) distinguishes explicit (`0`) from inferred (`1`) triples.
- **Rare-predicate consolidation**: predicates with fewer than `pg_ripple.vp_promotion_threshold` triples (default: 1,000) are stored in `_pg_ripple.vp_rare (p, s, o, g, i, source)` instead of a dedicated VP table. Auto-promoted when the threshold is crossed.
- **HTAP split** (v0.6.0+): writes go to `vp_{id}_delta` (heap + B-tree); deletes of main-resident triples go to `vp_{id}_tombstones`; the background merge worker combines main + delta (minus tombstones) into a fresh `vp_{id}_main` (BRIN-indexed). Query path is `(main EXCEPT tombstones) UNION ALL delta`. In v0.1.0–v0.5.1, each VP table is a single flat table with no delta/main split.
- **Default graph ID**: `0`; named graphs > 0.
- **Predicate catalog**: `_pg_ripple.predicates (id, table_oid, triple_count)` — look up the VP table OID here before any dynamic SQL.

## Code Conventions

- **Safe Rust only**; `unsafe` is permitted solely at required FFI boundaries — always add a `// SAFETY:` comment.
- Expose SQL functions via `#[pg_extern]`; never write raw `PG_FUNCTION_INFO_V1` C macros.
- Use `pgrx::SpiClient` for all SQL executed inside extension code.
- Shared memory state uses `pgrx::PgSharedMem` — size driven by GUC `pg_ripple.dictionary_cache_size`.
- Background workers use `pgrx::BackgroundWorker` with `BGWORKER_SHMEM_ACCESS`.
- All batch dictionary operations use `ON CONFLICT DO NOTHING … RETURNING` rather than SELECT-then-INSERT.
- Error messages follow PostgreSQL style: lowercase first word, no trailing period.

## Build & Test

```bash
# Install and test against PG18
cargo pgrx init --pg18 $(which pg18)
cargo pgrx test pg18

# Run pgregress suite
cargo pgrx regress pg18

# Install into a local PG18 instance
cargo pgrx install --pg-config $(which pg_config)
```

## Key Design Constraints

- **Integer joins everywhere**: SPARQL→SQL translation must encode all bound terms to `i64` *before* generating SQL. String comparisons in VP table queries are a bug.
- **Filter pushdown**: encode FILTER constants at translation time; never decode and re-encode at runtime.
- **Self-join elimination**: star patterns (same subject, multiple predicates) must be detected in the algebra optimizer and collapsed into a single scan with multiple joins — do not emit redundant subqueries.
- **Property paths**: compile to `WITH RECURSIVE … CYCLE` — always use PG18's `CYCLE` clause for hash-based cycle detection.
- **SHACL hints**: if `sh:maxCount 1` is set for a predicate, the SQL generator may omit `DISTINCT`; if `sh:minCount 1`, downgrade `LEFT JOIN` to `INNER JOIN`.
- **No dynamic SQL string concatenation for table names** — always look up the OID in `_pg_ripple.predicates` and use `format_ident!`-style quoting.

## Git & GitHub Workflow

After editing files, output the git commands to stage and commit the changes. Summarize the changes in the commit message. Group discrete changes into separate commits when appropriate. **Do not run `git commit` unless the user explicitly says it is fine.**

Never create a new branch unless the current branch is `main`.

### Creating Pull Requests

Always write the PR description to a temporary file using the **`create_file` tool** (never a shell heredoc or `echo`), then pass it to `gh` via `--body-file`. Shell heredocs and terminal commands silently corrupt Unicode characters and can pick up stale content from a previous session's file at the same path.

**Guaranteed-safe workflow:**

1. Delete any stale file at the target path first:
   ```bash
   rm -f /tmp/pr_TICKETNAME.md
   ```

2. Use the `create_file` tool to write the description. The file is written in UTF-8 and read directly by `gh --body-file`, so Unicode characters (math symbols, em-dashes, etc.) are safe to use.

3. Verify the file is clean before using it:
   ```bash
   python3 -c "
   with open('/tmp/pr_TICKETNAME.md') as f:
       body = f.read()
   print('lines:', body.count(chr(10)))
   print('ok:', '####' not in body)
   print(body[:120])
   "
   ```

4. Create or update the PR:
   ```bash
   gh pr create --title "..." --body-file /tmp/pr_TICKETNAME.md
   # or, to fix a garbled description:
   gh pr edit <number> --body-file /tmp/pr_TICKETNAME.md
   ```

5. Verify the live PR body is not garbled:
   ```bash
   gh pr view <number> --json body --jq '.body' | head -20
   ```

## Extension Versioning & Migration Scripts

**CRITICAL**: Every release must include a corresponding `sql/pg_ripple--X.Y.Z--X.Y.(Z+1).sql` migration script before the version is tagged, even if the script is empty. PostgreSQL's `ALTER EXTENSION pg_ripple UPDATE` requires explicit migration paths; without them, users on earlier versions cannot upgrade.

### Release Checklist

When preparing a new release (v0.X.Y):

1. **Create the migration script** from the previous version:
   - File: `sql/pg_ripple--X.(Y-1).Z--X.Y.Z.sql`
   - If there are schema changes (ALTER TABLE, CREATE INDEX, etc.), include them in the script
   - If there are no schema changes (only Rust function changes), add a comment header explaining what new functions/GUCs are provided and note that no SQL changes are required
   - Examples:
     - `pg_ripple--0.1.0--0.2.0.sql` — no schema changes (bulk load functions are compiled from Rust)
     - `pg_ripple--0.3.0--0.4.0.sql` — adds `qt_s, qt_p, qt_o` columns to dictionary for RDF-star support

2. **Update `pg_ripple.control`** to set `default_version = 'X.Y.Z'` to match the new release.

3. **Update `CHANGELOG.md`** with the new version entry.

4. **Tag the release** with `git tag vX.Y.Z` after all above are committed.

### Why This Matters

- **Forward upgrade path**: users on v0.1.0 can upgrade to v0.2.0, then v0.3.0, etc., via a simple `ALTER EXTENSION pg_ripple UPDATE`
- **Without migration scripts**: upgrading fails with `ERROR: extension "pg_ripple" has no update path from version "X" to version "Y"` — users are forced to dump/restore or rebuild from scratch
- **One-time cost**: writing a few lines of documentation (and SQL if needed) saves every user an expensive migration

### Example Workflow

```bash
# Before tagging v0.5.0:

# 1. Create the migration script
cat > sql/pg_ripple--0.4.0--0.5.0.sql << 'EOF'
-- Migration 0.4.0 → 0.5.0: Property paths, UNION, aggregates, subqueries
-- Schema changes: None (pure query engine enhancements)
EOF

# 2. Update pg_ripple.control
# (edit the file to set default_version = '0.5.0')

# 3. Update CHANGELOG.md with release notes

# 4. Commit and tag
git add sql/pg_ripple--0.4.0--0.5.0.sql pg_ripple.control CHANGELOG.md
git commit -m "v0.5.0: Prepare migration scripts and update control file"
git tag v0.5.0
```
