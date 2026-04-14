---
name: fix-ci
description: 'Fix CI workflow failures for pg_ripple. Use when: a GitHub Actions run fails, a test panics, cargo pgrx errors appear, PostgreSQL build deps are missing, argument-passing errors occur, or deadlocks appear in the test suite. Covers pgrx 0.17 + PostgreSQL 18 specific patterns and all known failure modes encountered in this project.'
argument-hint: 'Optionally provide the failing run URL or ID, e.g. https://github.com/grove/pg-ripple/actions/runs/12345'
---

# Fix CI Workflow Failures

## Authoritative Sources

- [.github/workflows/ci.yml](../../../.github/workflows/ci.yml) — the workflow under repair
- [AGENTS.md](../../../AGENTS.md) — build commands, code conventions, git workflow

---

## Step 1 — Fetch the failure log

If the user provides a run URL or ID, fetch the failure output immediately:

```bash
gh run view <RUN_ID> --log-failed 2>&1 | tail -80
```

If the log is truncated, grep for the specific failure:

```bash
gh run view <RUN_ID> --log-failed 2>&1 | grep -A 50 "FAILED\|error\[E\|Client Error"
```

Parse the job name from the output header (`Test (pg18)` vs `pg_regress (pg18)`) to know which job failed.

---

## Step 2 — Match the failure pattern

Work through the catalogue below. Most failures map to one entry.

---

## Known Failure Catalogue

### A. Missing system dependencies

**Symptom:**
```
configure: error: readline library not found
configure: error: ICU library not found
/usr/bin/ld: cannot find -lreadline
```

**Cause:** `cargo pgrx init --pg18 download` compiles PostgreSQL 18 from source. The CI runner (`ubuntu-latest`) lacks headers for readline, ICU, bison, and flex.

**Fix:** Both `test` and `regress` jobs need the full dependency list:

```yaml
- name: Install system dependencies
  run: |
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      libssl-dev \
      libclang-dev \
      clang \
      libreadline-dev \
      libicu-dev \
      bison \
      flex
```

> Both jobs compile PostgreSQL independently; the dependency list must appear in each job's `steps`.

---

### B. `--test-threads` argument rejected by cargo

**Symptom:**
```
error: unexpected argument '--test-threads' found
  tip: to pass '--test-threads' as a value, use '-- --test-threads'
```

**Cause:** `cargo pgrx test` consumes the first `--` as its own argument separator before invoking `cargo test`. A single `--` delivers `--test-threads` as a cargo flag, not a test-binary flag.

**Fix:** Use a double `--` to pass through to the test binary:

```yaml
- name: Run pg_test suite
  run: cargo pgrx test pg18 -- -- --test-threads=1
```

The chain is: `cargo pgrx test` → first `--` → `cargo test` → second `--` → test binary.

---

### C. Test deadlock (`deadlock detected`)

**Symptom:**
```
ERROR:  deadlock detected at character 27
DETAIL:  Process 24031 waits for RowExclusiveLock on relation 16391 ...
Client Error:
deadlock detected
postgres location: deadlock.c:1133
```

**Cause:** `cargo test` runs test functions in parallel threads by default. Each thread gets its own PostgreSQL backend connection. When two tests call `encode()` at the same time, they both execute the `WITH ins AS (INSERT ... ON CONFLICT DO NOTHING RETURNING id) SELECT COALESCE(...)` upsert concurrently, causing a deadlock on the dictionary table.

**Fix:** Serialize all `#[pg_test]` functions — they share a single PostgreSQL instance:

```yaml
- name: Run pg_test suite
  run: cargo pgrx test pg18 -- -- --test-threads=1
```

> Do not increase dictionary concurrency to "fix" this — the tests are integration tests that legitimately need serial execution.

---

### D. `pg_` prefix schema restriction

**Symptom:**
```
ERROR:  unacceptable schema name "pg_ripple"
DETAIL:  The prefix "pg_" is reserved for system schemas.
```

**Cause:** PostgreSQL 18 rejects `CREATE SCHEMA pg_ripple` without `allow_system_table_mods = on`. The extension bootstrap DDL runs before GUC is set.

**Fix — two parts:**

1. Set `superuser = true` in `pg_ripple.control` so the extension script runs as superuser.

2. Wrap the bootstrap `CREATE SCHEMA` in a `DO $$` block that sets the GUC locally:

```sql
DO $bootstrap$
BEGIN
  EXECUTE 'SET LOCAL allow_system_table_mods = on';
  EXECUTE 'CREATE SCHEMA IF NOT EXISTS pg_ripple';
END
$bootstrap$;
```

3. For the `#[pg_test]` harness, return the GUC from `postgresql_conf_options`:

```rust
#[pg_guard]
pub fn postgresql_conf_options() -> Vec<&'static str> {
    vec!["allow_system_table_mods = on"]
}
```

4. For pg_regress, pass the GUC on the command line:

```yaml
- name: Run pg_regress suite
  run: cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

---

### E. `Err(InvalidPosition)` from pgrx SPI on empty RETURNING

**Symptom:**
```
called `Result::unwrap()` on an `Err` value: InvalidPosition
```

**Cause:** In pgrx 0.17, `get_one_with_args` / `Spi::get_one` returns `Err(InvalidPosition)` when an `INSERT ... ON CONFLICT DO NOTHING RETURNING id` fires the `DO NOTHING` branch and returns zero rows.

**Fix:** Use a CTE upsert that always returns exactly one row:

```sql
WITH ins AS (
    INSERT INTO _pg_ripple.dictionary (hash, value, kind)
    VALUES ($1, $2, $3)
    ON CONFLICT (hash) DO NOTHING
    RETURNING id
)
SELECT COALESCE(
    (SELECT id FROM ins),
    (SELECT id FROM _pg_ripple.dictionary WHERE hash = $1)
)
```

This never returns zero rows: if the INSERT fires, the id comes from `ins`; if it conflicts, the fallback SELECT finds the existing row.

---

### F. pg_regress expected output mismatch

**Symptom:**
```
FAILED: differences in tests/pg_regress/expected/dictionary.out
```

**Cause:** A query result changed (type, format, NULL vs empty string, trailing whitespace) but the expected file was not updated.

**Fix:**

```bash
# Run regress locally and accept the diff
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"

# Inspect the diff
diff tests/pg_regress/expected/dictionary.out \
     tests/pg_regress/results/dictionary.out

# If the new output is correct, update
cp tests/pg_regress/results/dictionary.out \
   tests/pg_regress/expected/dictionary.out
```

> Never blindly accept — verify that the new output is actually correct first.

---

### G. `cargo-pgrx` version mismatch

**Symptom:**
```
error: package `pgrx v0.17.x` cannot be built because it requires rustc ...
# or
error[E0...]: no method named `...` found for struct `PgHeapTuple`
```

**Cause:** The installed `cargo-pgrx` version does not match the `pgrx` version pinned in `Cargo.toml`.

**Fix:** Pin the exact version in the CI install step:

```yaml
- name: Install cargo-pgrx
  run: cargo install cargo-pgrx --version "=0.17.0" --locked
```

Verify the pin in `Cargo.toml`:

```toml
[dependencies]
pgrx = "=0.17.0"

[dev-dependencies]
pgrx-tests = "=0.17.0"
```

All three (`cargo-pgrx` binary, `pgrx`, `pgrx-tests`) must share the same exact version.

---

### H. Clippy warnings treated as errors

**Symptom:**
```
error: unused variable: `x` [-D warnings]
```

**Cause:** CI runs clippy with `-D warnings`. Any new warning breaks the build.

**Fix:** Either suppress with `#[allow(...)]` at the call site, or fix the root warning. Common suppressions in this codebase:

```rust
#[allow(dead_code)]          // error taxonomy constants not yet used
#[allow(unused_variables)]   // scaffolding for future phases
```

Do not use `#![allow(...)]` at crate level — that silences future real warnings.

---

### I. pg_regress test hangs / no output

**Symptom:** The `pg_regress` job runs for >10 minutes with no output, then times out.

**Cause:** The PostgreSQL process failed to start (e.g. port conflict, data directory permission) and `pg_regress` is waiting for it.

**Fix:**

```bash
# Run locally with verbose output
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on" -- -v

# Check for port conflicts
lsof -i :5432
```

In CI, check whether a previous job left a PostgreSQL process running on the same runner (rare with `ubuntu-latest` ephemeral runners, but possible with self-hosted runners).

---

## Reference: Correct CI workflow state

The known-good workflow for this project:

```yaml
- name: Install system dependencies
  run: |
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends \
      build-essential pkg-config libssl-dev libclang-dev clang \
      libreadline-dev libicu-dev bison flex

- name: Install cargo-pgrx
  run: cargo install cargo-pgrx --version "=0.17.0" --locked

- name: Initialise pgrx (PostgreSQL 18)
  run: cargo pgrx init --pg18 download

- name: Run pg_test suite
  run: cargo pgrx test pg18 -- -- --test-threads=1

# (regress job)
- name: Run pg_regress suite
  run: cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

---

## Step 3 — Verify the fix locally before pushing

```bash
# Quick compile check
cargo check --features pg18

# Full test run (mirrors CI)
cargo pgrx test pg18 -- -- --test-threads=1

# pg_regress
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

If the fix is only to the workflow YAML (not to Rust code), you can push directly and observe the next CI run.

---

## Step 4 — Commit

Follow AGENTS.md commit style: lowercase first word, imperative mood, no trailing period.

```bash
git add .github/workflows/ci.yml   # or src/... if code was fixed
git commit -m "ci: <short description of what was fixed>"
```
