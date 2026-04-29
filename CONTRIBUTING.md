# Contributing to pg_ripple

Thank you for your interest in contributing to pg_ripple — a high-performance
RDF triple store and SPARQL engine built as a PostgreSQL 18 extension in Rust.

---

## Quick links

- [Architecture overview](AGENTS.md)
- [Roadmap](ROADMAP.md)
- [Implementation plan](plans/implementation_plan.md)
- [Release checklist](AGENTS.md#release-checklist)

---

## Branch naming conventions

| Prefix | Use for |
|---|---|
| `feat/` | New features (e.g., `feat/v0.74.0`) |
| `fix/` | Bug fixes (e.g., `fix/sparql-filter-silent-drop`) |
| `docs/` | Documentation-only changes |
| `chore/` | Non-functional changes (CI, tooling, deps) |

**Rule**: Never create a new branch from `main` unless the current branch is
`main`.  Feature branches track the version they belong to.

---

## Commit message format

pg_ripple uses [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]

[optional footer(s)]
```

| Type | When to use |
|---|---|
| `feat` | New feature or SQL function |
| `fix` | Bug fix |
| `docs` | Documentation changes |
| `test` | Test additions/fixes (no production code change) |
| `chore` | Tooling, CI, dependency updates |
| `refactor` | Code restructuring with no behavior change |
| `perf` | Performance improvement |

**Examples**:

```
feat(subscriptions): add subscribe_sparql() and unsubscribe_sparql() SQL functions
fix(sparql): fix FILTER silent-drop when expression type-errors
docs(r2rml): clarify materialization-only scope in features/r2rml.md
```

---

## Pre-commit checklist

Run these before every `git commit`:

```bash
# 1. Format (auto-fix)
cargo fmt --all

# 2. Lint (auto-fix then verify — must be zero warnings)
cargo clippy --fix --allow-dirty --features pg18
cargo clippy --features pg18 -- -D warnings

# 3. Unit + integration tests
cargo pgrx test pg18

# 4. pg_regress suite
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

All four steps must pass before pushing.

---

## Migration script discipline

Every release version must include a migration SQL script at:
`sql/pg_ripple--<prev>--<next>.sql`

See [AGENTS.md — Release Checklist](AGENTS.md#release-checklist) for the full
process.  A missing migration script blocks users from running
`ALTER EXTENSION pg_ripple UPDATE`.

---

## Adding a new `#[pg_extern]` function

1. Write the Rust implementation in the appropriate `src/` module.
2. Expose it via `#[pg_extern]` inside the `pg_ripple` schema module.
3. Add an entry to `feature_status()` in `src/feature_status.rs` with an
   honest initial status (`experimental` or `stub`).
4. Add a docs page under `docs/src/` (new features) or update an existing page.
5. Add a pg_regress test in `tests/pg_regress/sql/<feature>.sql` with a
   matching expected output in `tests/pg_regress/expected/<feature>.out`.
6. Run the pre-commit checklist.

---

## PR checklist

Before opening a pull request:

- [ ] All pre-commit steps pass locally.
- [ ] Migration script added if schema changes are included.
- [ ] `pg_ripple.control` `default_version` updated.
- [ ] CHANGELOG.md updated under `[Unreleased]`.
- [ ] `feature_status()` entry added or updated.
- [ ] At least one pg_regress test covers the new functionality.
- [ ] Docs page added or updated.

Run `just assess-release` to check for common omissions before pushing.

---

## Running tests

```bash
# All pgrx tests (unit + integration)
cargo pgrx test pg18

# pg_regress test suite
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"

# Migration chain test (verifies all migration scripts in sequence)
bash tests/test_migration_chain.sh

# Clippy (must be zero warnings)
cargo clippy --features pg18 -- -D warnings
```

---

## Getting help

Open an issue on GitHub describing the problem, your PostgreSQL and Rust
versions, and the full error output.  For architectural questions, read
[plans/implementation_plan.md](plans/implementation_plan.md) first.
