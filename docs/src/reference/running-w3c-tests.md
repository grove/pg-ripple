# Running W3C SPARQL 1.1 Tests

This guide explains how to run the W3C SPARQL 1.1 conformance test suite locally
and how to manage expected failures.

## Prerequisites

- A working pg_ripple development environment (see the [Getting Started](../getting-started/installation.md) guide)
- `cargo pgrx` installed and initialised for PostgreSQL 18
- `curl` or `wget` for downloading the test data

## Download the test data

The W3C test data is not included in the repository (it is cached in CI).
Download it once with the provided script:

```sh
bash scripts/fetch_w3c_tests.sh
```

This downloads the official W3C SPARQL 1.1 test archive from
`https://www.w3.org/2009/sparql/docs/tests/` and extracts it to
`tests/w3c/data/`.  The script is idempotent — re-running it does nothing
if the data is already present.

To force a re-download:

```sh
bash scripts/fetch_w3c_tests.sh --force
```

To use a custom directory:

```sh
W3C_TEST_DIR=/path/to/sparql11 bash scripts/fetch_w3c_tests.sh
```

## Start pg_ripple

The test harness connects to a running pg_ripple instance via the
`DATABASE_URL` environment variable (or the pgrx default socket/port).

```sh
# Start the pgrx PostgreSQL 18 instance
cargo pgrx start pg18
```

## Run the smoke subset

The **smoke subset** covers 180 tests from the `optional`, `aggregates`, and
`grouping` categories:

```sh
cargo test --test w3c_smoke -- --nocapture
```

Expected output:

```
W3C smoke subset results:
  180 passed, 0 failed, 0 skipped, 0 timeout, 0 xfail, 0 xpass / 180 total
  optional: 80/80
  aggregates: 60/60
  grouping: 40/40
```

The smoke test is the **required CI check** — failures block merge.

## Run the full suite

The full suite covers all 13 sub-suites (~3 000 tests):

```sh
# Run with 8 parallel threads (matches CI runner):
cargo test --test w3c_suite -- --test-threads 1 --nocapture

# Or use more threads via env var:
W3C_THREADS=8 cargo test --test w3c_suite -- --nocapture
```

The full suite uploads a `report.json` artifact to `tests/w3c/report.json`
with per-category pass/fail/skip/timeout counts.

## Override paths

| Environment variable | Default | Description |
|---|---|---|
| `W3C_TEST_DIR` | `tests/w3c/data/` | Path to the W3C test data directory |
| `DATABASE_URL` | pgrx socket at `~/.pgrx:28818` | PostgreSQL connection string |
| `W3C_THREADS` | `8` | Thread count for the full suite |

## Managing expected failures

Failures in `tests/w3c/known_failures.txt` are reported as `XFAIL`
(expected failure) rather than `FAIL`.  The file format is:

```
# Comment lines are ignored
<test-IRI>  one-line reason for the failure
```

### Adding an expected failure

When you confirm a real bug (not a test harness issue):

1. Run the full suite and note the failing test IRI from the output.
2. Add an entry to `tests/w3c/known_failures.txt`:
   ```
   http://www.w3.org/2009/sparql/docs/tests/data-sparql11/optional/manifest#opt-filter-equals   filter-over-optional edge case — fix pending in vX.Y.Z
   ```
3. Open a GitHub issue for the underlying bug and link it from the entry.

### Removing an expected failure

When a bug is fixed:

1. Remove the entry from `known_failures.txt`.
2. Verify the test passes in the full suite:
   ```sh
   cargo test --test w3c_suite -- --nocapture 2>&1 | grep "XPASS\|<test-name>"
   ```
3. Commit the removal together with the fix.

### XPASS warnings

If a test in `known_failures.txt` unexpectedly passes, CI reports it as
`XPASS`.  This is a signal to remove the entry — the bug has been fixed.

## Interpreting results

| Status | Meaning |
|---|---|
| `Pass` | Test executed and result matched expected output |
| `Fail` | Test executed but result did not match |
| `Skip` | Test not run (unsupported type, missing file, etc.) |
| `Timeout` | Test exceeded the per-test time limit (5 s for full suite, 30 s for smoke) |
| `XFail` | Test failed as expected (listed in `known_failures.txt`) |
| `XPass` | Test was expected to fail but passed — remove from `known_failures.txt` |

`Fail`, `Timeout`, and `XPass` are unexpected failures and block the smoke CI check.
