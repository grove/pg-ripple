# Contributing to pg_ripple

Thank you for your interest in contributing to pg_ripple. This guide covers how to set up a development environment, run tests, and submit a pull request.

---

## Development setup

### Prerequisites

- Rust (Edition 2024, `rust-version` in `Cargo.toml`)
- PostgreSQL 18 development headers
- `cargo-pgrx` 0.17

### Install pgrx and initialize PostgreSQL 18

```bash
cargo install cargo-pgrx --version "=0.17.0"
cargo pgrx init --pg18 $(which pg18)
```

### Build the extension

```bash
cargo build --features pg18
```

### Install into a local PostgreSQL 18 instance

```bash
cargo pgrx install --pg-config $(which pg_config)
```

---

## Running tests

### Unit and integration tests

```bash
cargo pgrx test pg18
```

This spins up an ephemeral PostgreSQL 18 instance and runs all `#[pg_test]` functions.

### pg_regress suite

```bash
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

All test suites live in `tests/pg_regress/sql/`. Expected output files are in `tests/pg_regress/expected/`.

### Migration chain test

```bash
bash tests/test_migration_chain.sh
# or
just test-migration
```

Verifies that all migration SQL scripts run in sequence without error.

---

## Code conventions

See [AGENTS.md](https://github.com/grove/pg-ripple/blob/main/AGENTS.md) for the full list. Key points:

- **Safe Rust only** — `unsafe` only at required FFI boundaries with a `// SAFETY:` comment
- **No string comparisons in VP tables** — always encode to `i64` first
- **`#[pg_extern]` for all SQL-callable functions** — never raw `PG_FUNCTION_INFO_V1`
- **`pgrx::SpiClient` for all SQL inside extension code**
- **Integer joins everywhere** — the SPARQL→SQL pipeline must encode all bound terms before generating SQL

### Pre-commit checklist

```bash
cargo fmt --all
cargo clippy --features pg18 -- -D warnings
cargo pgrx test pg18
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

All four must pass with zero warnings and zero failures before committing.

---

## Submitting a pull request

1. Fork the repository and create a feature branch from `main`.
2. Implement your changes following the code conventions above.
3. Add or update `#[pg_test]` integration tests.
4. Add or update pg_regress test files in `tests/pg_regress/sql/` with matching expected output in `tests/pg_regress/expected/`.
5. If your change adds a new SQL function, add a migration script in `sql/` and document it in `docs/src/user-guide/sql-reference/`.
6. Open a pull request against `main`.

### PR description

Include:
- A one-sentence summary of what the PR does
- The ROADMAP.md deliverable it addresses (if applicable)
- How to test the change manually

---

## Project governance

pg_ripple is maintained by [Grove](https://github.com/grove). The repository follows the [AGENTS.md](https://github.com/grove/pg-ripple/blob/main/AGENTS.md) conventions for automated agents and contributors alike.

Bug reports and feature requests: open a GitHub Issue.
Security vulnerabilities: see [security.md](../reference/security.md).
