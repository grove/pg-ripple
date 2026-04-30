# Development Reference

This page provides a reference for contributors and extension developers
working on pg_ripple.

## Overview

pg_ripple is written in Rust using the `pgrx` 0.18 framework for PostgreSQL 18.
The extension uses safe Rust exclusively (except at required FFI boundaries),
exposes SQL functions via `#[pg_extern]`, and uses `pgrx::SpiClient` for all
SQL executed inside extension code.

## Build and Test

```bash
# Install pgrx against PG18
cargo pgrx init --pg18 $(which pg18)

# Run all unit and integration tests
cargo pgrx test pg18

# Run pg_regress suite
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"

# Run the migration chain test
bash tests/test_migration_chain.sh

# Format code
cargo fmt --all

# Lint (zero warnings required)
cargo clippy --features pg18 -- -D warnings

# Install locally
cargo pgrx install --pg-config $(which pg_config)
```

## Project Layout

| Path | Description |
|---|---|
| `src/lib.rs` | Extension entry points, `_PG_init`, GUC parameters |
| `src/dictionary/` | IRI/blank-node/literal → i64 encoder (XXH3-128 + LRU) |
| `src/storage/` | VP tables, HTAP delta/main partitions, merge background worker |
| `src/sparql/` | SPARQL text → spargebra algebra → SQL → SPI execution → decode |
| `src/construct_rules/` | CONSTRUCT writeback rules |
| `src/datalog/` | Datalog rule parser, stratifier, SQL compiler |
| `src/shacl/` | SHACL shapes → DDL constraints + async validation |
| `src/export/` | Turtle / N-Triples / JSON-LD serialization |
| `src/stats/` | Monitoring, pg_stat_statements integration |
| `src/admin/` | vacuum, reindex, prefix registry |
| `pg_ripple_http/` | Axum HTTP companion service |
| `sql/` | Extension SQL and migration scripts |
| `tests/pg_regress/` | pg_regress test suite |

## Code Conventions

- **Safe Rust only**: `unsafe` is permitted solely at required FFI boundaries
  with a `// SAFETY:` comment.
- **SQL functions**: expose via `#[pg_extern]`, never raw `PG_FUNCTION_INFO_V1`.
- **Database access**: use `pgrx::SpiClient` for all SQL inside extension code.
- **Error messages**: follow PostgreSQL style (lowercase first word, no trailing period).
- **Integer joins**: all SPARQL-to-SQL translation must encode bound terms to
  `i64` before SQL generation; string comparisons in VP queries are a bug.

## Migration Scripts

Every release requires a `sql/pg_ripple--X.Y.Z--X.Y.(Z+1).sql` migration
script. If there are no schema changes, add a comment header explaining what
new functions/GUCs are provided.

```bash
# Example: create migration script for v0.74.0
cat > sql/pg_ripple--0.73.0--0.74.0.sql << 'EOF'
-- Migration 0.73.0 → 0.74.0
-- Schema changes: <describe here>
EOF
```

See [Release Process](release-process.md) for the full release checklist.

## Testing Conventions

- Unit tests: `#[pg_test]` in the same file as the implementation
- Integration tests: `tests/pg_regress/sql/*.sql` files
- Each version milestone should have a `v0XX_features.sql` regression file
- Expected output: `tests/pg_regress/expected/*.out`

## Related Pages

- [CONTRIBUTING.md](../../../CONTRIBUTING.md)
- [Release Process](release-process.md)
- [Architecture Internals](architecture.md)
- [API Stability Guarantees](api-stability.md)
