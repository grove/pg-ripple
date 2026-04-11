# pg_triple — Agent Guidelines

**pg_triple** is a PostgreSQL 18 extension written in Rust (pgrx 0.17) that implements a high-performance RDF triple store with native SPARQL query execution. See [plans/implementation_plan.md](plans/implementation_plan.md) for the full architecture and [ROADMAP.md](ROADMAP.md) for the phased delivery plan.

## Tech Stack

| Concern | Technology |
|---|---|
| Language | Rust, Edition 2024 |
| PG binding | pgrx 0.17 (`pg18` feature flag) |
| PostgreSQL target | 18.x only |
| SPARQL parser | `spargebra` |
| RDF parsers | `rio_turtle`, `rio_xml` |
| Hashing | `xxhash-rust` (XXH3-128) |
| Serialization | `serde` + `serde_json` |
| Tests | `#[pg_test]`, `cargo pgrx regress`, `pgbench` via `pgrx-bench` |

## Architecture

```
src/lib.rs          — pgrx entry points, _PG_init, GUC parameters
src/dictionary/     — IRI/blank-node/literal → i64 encoder (XXH3-128 + LRU cache)
src/storage/        — VP tables, HTAP delta/main partitions, merge background worker
src/sparql/         — SPARQL text → spargebra algebra → SQL → SPI execution → decode
src/shacl/          — SHACL shapes → DDL constraints + async validation pipeline
src/export/         — Turtle / N-Triples / JSON-LD serialization
src/stats/          — Monitoring, pg_stat_statements integration
src/admin/          — vacuum, reindex, prefix registry
```

All user-visible objects live in the `pg_triple` schema; internal tables and VP tables live in `_pg_triple`.

## Storage Conventions

- **Dictionary encoding**: every IRI, blank node, and literal is mapped to `BIGINT` (i64) via XXH3-128 hash before being stored. VP tables **never** contain raw strings.
- **VP table naming**: `_pg_triple.vp_{predicate_id}` — one table per unique predicate. Columns: `s BIGINT, o BIGINT, g BIGINT`. Dual B-tree indices on `(s, o)` and `(o, s)`.
- **HTAP split**: writes go to `vp_{id}_delta` (heap + B-tree); the background merge worker promotes rows to `vp_{id}_main` (BRIN-indexed). Query path is `UNION ALL` of both partitions.
- **Default graph ID**: `0`; named graphs > 0.
- **Predicate catalog**: `_pg_triple.predicates (id, table_oid, triple_count)` — look up the VP table OID here before any dynamic SQL.

## Code Conventions

- **Safe Rust only**; `unsafe` is permitted solely at required FFI boundaries — always add a `// SAFETY:` comment.
- Expose SQL functions via `#[pg_extern]`; never write raw `PG_FUNCTION_INFO_V1` C macros.
- Use `pgrx::SpiClient` for all SQL executed inside extension code.
- Shared memory state uses `pgrx::PgSharedMem` — size driven by GUC `pg_triple.dictionary_cache_size`.
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
- **Property paths**: compile to `WITH RECURSIVE … CYCLE` — always include cycle detection to guard against circular graphs.
- **SHACL hints**: if `sh:maxCount 1` is set for a predicate, the SQL generator may omit `DISTINCT`; if `sh:minCount 1`, downgrade `LEFT JOIN` to `INNER JOIN`.
- **No dynamic SQL string concatenation for table names** — always look up the OID in `_pg_triple.predicates` and use `format_ident!`-style quoting.

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
