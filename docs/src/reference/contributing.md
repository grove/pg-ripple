# Contributing

Thank you for your interest in contributing to pg_ripple. This guide covers environment setup, testing, code conventions, and the pull request workflow.

```admonish note title="Contribute"
pg_ripple is open source and welcomes contributions of all kinds — bug reports, documentation fixes, test cases, and feature implementations. If you are unsure whether an idea fits, open a GitHub issue to discuss it before writing code.
```

---

## Development Environment

### Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| Rust | Edition 2024, stable toolchain | Language |
| PostgreSQL | 18.x | Target database |
| pgrx | 0.17 | PostgreSQL extension framework |
| cargo-pgrx | 0.17 | Build and test tooling |
| git | 2.x+ | Version control |

### Setup

```bash
# 1. Clone the repository
git clone https://github.com/your-org/pg_ripple.git
cd pg_ripple

# 2. Install cargo-pgrx if not already installed
cargo install cargo-pgrx --version 0.17 --locked

# 3. Initialize pgrx with PostgreSQL 18
cargo pgrx init --pg18 $(which pg_config)

# 4. Verify the build
cargo build
```

```admonish tip title="macOS"
On macOS, install PostgreSQL 18 via Homebrew: `brew install postgresql@18`. Ensure `pg_config` is on your `PATH`.
```

---

## Running Tests

pg_ripple uses three levels of testing:

### Unit and integration tests (pgrx)

Runs Rust tests inside a temporary PostgreSQL instance:

```bash
cargo pgrx test pg18
```

This starts a temporary PG18 cluster, installs the extension, runs all `#[pg_test]` functions, and tears down the cluster.

### Regression tests (pg_regress)

Runs SQL-based regression tests that compare expected output:

```bash
cargo pgrx regress pg18
```

The test SQL files live in `sql/` and expected output in `expected/`. If you add a new SQL function, add a regression test for it.

### Migration chain test

Verifies that all migration scripts (`sql/pg_ripple--X.Y.Z--X.Y.Z+1.sql`) can be applied in sequence:

```bash
# Requires pgrx PG18 running
cargo pgrx start pg18
bash tests/test_migration_chain.sh
```

### Running a subset of tests

```bash
# Run a single test by name
cargo pgrx test pg18 -- test_name_pattern

# Run tests with output visible
cargo pgrx test pg18 -- --nocapture
```

---

## Code Conventions

These conventions are enforced by CI and code review.

### Safe Rust only

All code must be safe Rust. `unsafe` is permitted **only** at required FFI boundaries (pgrx macros, shared memory access) and must include a `// SAFETY:` comment explaining why it is correct.

### SQL function exposure

Expose SQL functions via the `#[pg_extern]` attribute. Never write raw `PG_FUNCTION_INFO_V1` C macros.

```rust
#[pg_extern]
fn my_function(input: &str) -> String {
    // implementation
}
```

### SPI for all internal SQL

Use `pgrx::SpiClient` for all SQL executed inside extension code. Never use raw libpq or string-based query execution.

```rust
Spi::connect(|client| {
    client.select("SELECT count(*) FROM _pg_ripple.dictionary", None, None)?;
    Ok(())
})?;
```

### Integer joins everywhere

SPARQL-to-SQL translation must encode all bound terms to `i64` **before** generating SQL. VP table queries must never contain string comparisons — this is a bug.

### No dynamic SQL string concatenation for table names

Always look up the VP table OID in `_pg_ripple.predicates` and use `format!`-style quoting with proper escaping. Never interpolate user input into table names.

### Error messages

Follow PostgreSQL style: lowercase first word, no trailing period.

```rust
// Good
return Err(pg_ripple_error!("dictionary encode failed: hash collision detected"));

// Bad
return Err(pg_ripple_error!("Dictionary encode failed: hash collision detected."));
```

### Batch dictionary operations

Use `ON CONFLICT DO NOTHING … RETURNING` for all batch inserts into the dictionary. Never use a SELECT-then-INSERT pattern.

---

## Project Structure

```
src/
├── lib.rs              # Entry points, _PG_init, GUC parameters
├── dictionary/         # IRI/blank-node/literal → i64 encoder
├── storage/            # VP tables, HTAP delta/main, merge worker
├── sparql/             # SPARQL → algebra → SQL → SPI
├── datalog/            # Datalog parser, stratifier, SQL compiler
├── shacl/              # SHACL shapes → DDL constraints + validation
├── export/             # Turtle / N-Triples / JSON-LD serialization
├── stats/              # Monitoring, pg_stat_statements integration
└── admin/              # Vacuum, reindex, prefix registry

sql/                    # Migration scripts and regression test SQL
tests/                  # Shell-based integration tests
docs/                   # mdBook documentation site
```

---

## Pull Request Workflow

### Branch policy

- **Never create a new branch from `main`** unless the current branch is `main`.
- Use descriptive branch names: `feat/sparql-lateral`, `fix/dictionary-collision`, `docs/glossary`.

### Before opening a PR

1. **Run all tests** and ensure they pass:

```bash
cargo pgrx test pg18
cargo pgrx regress pg18
```

2. **Run clippy** with no warnings:

```bash
cargo clippy --all-targets -- -D warnings
```

3. **Format code**:

```bash
cargo fmt --check
```

4. **Update documentation** if you changed any SQL function signatures or added new functions.

5. **Create or update migration scripts** if the release version changed (see below).

### Commit messages

- Use present tense: "add lateral join support" not "added lateral join support"
- Group discrete changes into separate commits
- Reference issue numbers when applicable: "fix dictionary collision (#42)"

### Migration scripts

Every release requires a migration script (`sql/pg_ripple--X.Y.Z--X.Y.Z+1.sql`), even if it only contains comments. See the [Release Process](release-process.md) for the full checklist.

---

## Documentation Contributions

The documentation site uses [mdBook](https://rust-lang.github.io/mdBook/) with the [mdbook-admonish](https://github.com/tommilligan/mdbook-admonish) plugin for callout boxes.

### Building the docs locally

```bash
# Install mdbook and plugins
cargo install mdbook mdbook-admonish

# Build and serve
cd docs
mdbook serve --open
```

### Callout syntax

Use fenced code blocks with `admonish` for callout boxes:

````markdown
```admonish tip title="Performance"
Use `load_ntriples_file()` for large datasets — it is 10× faster than string loading.
```

```admonish warning
This operation cannot be undone.
```

```admonish note
Available since v0.16.0.
```
````

### Adding a new page

1. Create the Markdown file in the appropriate `docs/src/` subdirectory.
2. Add the page to `docs/src/SUMMARY.md`.
3. Run `mdbook build` to verify it compiles.

---

## Property-Based Testing (v0.46.0)

pg_ripple uses `proptest` for randomised property-based tests that assert algebraic invariants. These tests run entirely in pure Rust — no database connection required.

### Running proptest suites

```bash
# Run all property-based tests
cargo test --test proptest_suite

# Run with more cases (default: 256)
PROPTEST_CASES=10000 cargo test --test proptest_suite

# Run a specific suite
cargo test --test proptest_suite sparql_roundtrip
cargo test --test proptest_suite dictionary
cargo test --test proptest_suite jsonld_framing
```

### Adding a new property test

1. Add your test to the appropriate file in `tests/proptest/`:
   - SPARQL translator invariants → `sparql_roundtrip.rs`
   - Dictionary encoder invariants → `dictionary.rs`
   - JSON-LD framing invariants → `jsonld_framing.rs`
   - New domain → create `tests/proptest/<domain>.rs` and add `mod <domain>;` to `tests/proptest_suite.rs`

2. Use `proptest!` macros for property tests; regular `#[test]` for deterministic fixtures.

3. Run the suite with `PROPTEST_CASES=10000` to verify 10,000 cases pass.

### Debugging a proptest failure

When a test fails, proptest prints the minimal failing input. Reproduce it:

```rust
// Add to the failing test to fix the seed:
ProptestConfig::with_cases(1).with_proptest_rng(seed)
```

---

## Fuzz Testing (v0.46.0)

pg_ripple uses `cargo-fuzz` to test the federation result decoder against arbitrary byte sequences.

### Running the fuzz target

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Run for 10 minutes
cargo fuzz run federation_result -- -max_total_time=600

# Run indefinitely
cargo fuzz run federation_result

# Minimise a crashing corpus entry
cargo fuzz tmin federation_result artifacts/federation_result/crash-<hash>
```

### Adding a new fuzz target

1. Create `fuzz/fuzz_targets/<target_name>.rs` with the fuzz target function.
2. Add a `[[bin]]` entry to `fuzz/Cargo.toml`.
3. Add the target to the `fuzz-<target_name>` CI job in `.github/workflows/ci.yml`.

### Fuzz target contract

Every fuzz target must:
- Use `#![no_main]` and `libfuzzer_sys::fuzz_target!`
- **Never panic** regardless of input (panics are treated as fuzz failures)
- Return `Err(...)` for invalid input, never crash

---

## Reporting Issues

When filing a bug report, please include:

- **pg_ripple version**: `SELECT pg_ripple.canary();` and the output of `\dx pg_ripple`
- **PostgreSQL version**: `SELECT version();`
- **Minimal reproducer**: the smallest SQL script that triggers the issue
- **Full error output**: use `\errverbose` in psql for detailed error context
- **Platform**: OS and architecture

```admonish warning title="Security issues"
If you discover a security vulnerability, please report it privately via GitHub Security Advisories rather than opening a public issue.
```
