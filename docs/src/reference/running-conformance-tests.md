# Running Conformance Tests

pg_ripple ships four complementary conformance suites that can be run locally
or in CI.  This page covers how to set up data, run each suite, and interpret results.

## Prerequisites

- A working pg_ripple development environment
- `cargo pgrx` installed and initialised for PostgreSQL 18
- `curl` or `wget` for downloading test data
- Docker (optional) for generating the WatDiv dataset

## One-command setup

Download all test data for all three suites at once:

```sh
bash scripts/fetch_conformance_tests.sh
```

Or fetch individual suites:

```sh
bash scripts/fetch_conformance_tests.sh --w3c      # W3C SPARQL 1.1 only
bash scripts/fetch_conformance_tests.sh --jena     # Apache Jena only
bash scripts/fetch_conformance_tests.sh --watdiv   # WatDiv only
bash scripts/fetch_conformance_tests.sh --force    # re-download everything
```

## W3C SPARQL 1.1 suite

### Data location

`tests/w3c/data/` (default) or the directory in `W3C_TEST_DIR`.

### Running

```sh
# Start pg_ripple
cargo pgrx start pg18

# Smoke subset (180 tests, ~30s — fastest feedback):
cargo test --test w3c_smoke -- --nocapture

# Full suite (3 000+ tests, ~2min with 8 threads):
W3C_THREADS=8 cargo test --test w3c_suite -- --nocapture
```

### Known failures

Edit `tests/conformance/known_failures.txt` with lines prefixed `w3c:`:

```
# Example — property-path regression, fix in progress
w3c:http://www.w3.org/2009/sparql/docs/tests/data-sparql11/property-path/manifest#pp35  pp inside GRAPH
```

Remove entries when the underlying bug is fixed.

---

## Apache Jena suite

### Data location

`tests/jena/data/` (default) or the directory in `JENA_TEST_DIR`.

### Running

```sh
# Download Jena test data (one-time):
bash scripts/fetch_conformance_tests.sh --jena

# Run the full suite (~1 000 tests, target < 3 minutes):
JENA_THREADS=8 cargo test --test jena_suite -- --nocapture
```

### Coverage

Jena tests focus on implementation edge cases:

- **Type coercion** — XSD numeric promotions, mixed-type arithmetic
- **Date/time** — timezone-aware comparisons, `YEAR()`, `MONTH()`, `DAY()`, `HOURS()`, `MINUTES()`, `SECONDS()`, `TZ()`
- **Blank-node scoping** — CONSTRUCT templates, GRAPH boundaries, OPTIONAL
- **String functions** — `STRLEN()`, `SUBSTR()`, `UCASE()`, `LCASE()`, `STRSTARTS()`, `STRENDS()`, `CONTAINS()`, `ENCODE_FOR_URI()`, `CONCAT()`
- **Numeric precision** — `xsd:decimal` arithmetic, `ROUND()`, `CEIL()`, `FLOOR()`, `ABS()`

### Known failures

Prefix entries with `jena:` in `tests/conformance/known_failures.txt`:

```
# Example — timezone-aware dateTime comparison
jena:http://jena.example.org/tests/sparql-query/manifest#dateTime-tz-offset  TZ offset handling
```

The CI job is **non-blocking** until pass rate ≥ 95%.

---

## WatDiv benchmark suite

### Data location

- Templates: `tests/watdiv/templates/` (or `WATDIV_TEMPLATE_DIR`)
- RDF data: `tests/watdiv/data/` (or `WATDIV_DATA_DIR`)
- Baselines: `tests/watdiv/baselines.json` (or `WATDIV_BASELINE_FILE`)

### Data generation

The WatDiv 10M-triple dataset is generated once and cached as a CI artifact.

```sh
# Using Docker:
docker run --rm dcslab/watdiv -s 1 -t 10000000 > tests/watdiv/data/watdiv-10M.nt

# Using a local binary:
WATDIV_BINARY=/usr/local/bin/watdiv bash scripts/fetch_conformance_tests.sh --watdiv
```

### Loading the dataset

Before running the WatDiv suite, load the dataset into pg_ripple:

```sh
cargo pgrx start pg18
psql -d postgres -c "SELECT pg_ripple.load_ntriples(pg_read_file('tests/watdiv/data/watdiv-10M.nt'), false);"
```

### Running

```sh
# Run all 100 templates (target < 5 min on 8-core runner):
WATDIV_THREADS=8 cargo test --test watdiv_suite -- --nocapture
```

### Interpreting results

- **Correctness pass**: row count within ±0.1% of baseline
- **Performance warning**: median latency > 20% above baseline (non-blocking)
- **Baselines**: stored in `tests/watdiv/baselines.json` — update after intentional performance changes

### Known failures

Prefix entries with `watdiv:` in `tests/conformance/known_failures.txt`:

```
# Example — complex template with OPTIONAL cardinality edge case
watdiv:B7  known cardinality mismatch with OPTIONAL
```

---

## LUBM benchmark suite (v0.44.0+)

The LUBM (Lehigh University Benchmark) suite validates OWL RL inference correctness
through 14 canonical SPARQL queries over a university-domain ontology.

### Data location

The LUBM suite is **self-contained** — no download or external data generation is needed.
The synthetic fixture is bundled at `tests/lubm/fixtures/univ1.ttl`.

### Running

```sh
# Start pg_ripple
cargo pgrx start pg18

# Run all 14 LUBM queries + Datalog validation sub-suite (< 30s):
cargo test --test lubm_suite -- --nocapture
```

### What is tested

- **14 canonical queries** (`tests/lubm/queries/q01.sparql` – `q14.sparql`) against the
  bundled univ1 fixture — exact row-count validation.
- **OWL RL rule loading** via `pg_ripple.load_rules_builtin('owl-rl')`.
- **Inference materialization** via `pg_ripple.infer('owl-rl')` — verifies fixpoint
  is reached in ≤ 10 iterations and completes in < 5 s.
- **Goal queries** via `pg_ripple.infer_goal()` — validates inference engine results
  match SPARQL query results.
- **Custom Datalog rules** — defines ad-hoc rules on LUBM data and validates correctness.

### Known failures

Prefix entries with `lubm:` in `tests/conformance/known_failures.txt`:

```
# Example — Q2 multi-hop join returns wrong count
lubm:Q2  multi-hop memberOf/subOrganizationOf join bug
```

### Regenerating baselines

If the fixture is changed, regenerate the baseline counts:

```sh
cargo pgrx start pg18
# Run the suite once, observe the actual counts in the output,
# then update tests/lubm/baselines/univ1.json accordingly.
```

### See also

- [LUBM Results](lubm-results.md) — full conformance table and Datalog sub-suite results

---

## Unified report

All suites write results to `tests/conformance/report.json`:

```json
{
  "w3c":    { "suite": "w3c",    "total": 3100, "passed": 3097, "failed": 0, ... },
  "jena":   { "suite": "jena",   "total": 1000, "passed": 983,  "failed": 0, ... },
  "watdiv": { "suite": "watdiv", "total": 100,  "passed": 100,  "failed": 0, ... }
}
```

This file is uploaded as the `conformance_report` CI artifact after each run.
(The LUBM suite writes pass/fail results to stdout; a JSON report artifact is planned for v0.45.0.)

## Updating baselines

After intentional performance improvements, regenerate the WatDiv baselines:

```sh
# Run the suite to populate baselines.json:
cargo test --test watdiv_suite -- --nocapture
# Then commit the updated baselines.json.
```

## Updating the known-failures manifest

The unified known-failures file lives at `tests/conformance/known_failures.txt`.
Format:

```
# Comment lines are ignored.
# Each entry: <suite>:<test-key>  <optional reason>
w3c:http://...    reason
jena:http://...   reason
watdiv:S3         reason
lubm:Q2           reason
```

Any test listed here that **unexpectedly passes** (XPASS) triggers a CI notice
to remove the entry.

## See also

- [W3C Conformance](../reference/w3c-conformance.md) — per-category pass rates
- [LUBM Results](../reference/lubm-results.md) — OWL RL conformance table
- [WatDiv Results](../reference/watdiv-results.md) — benchmark metrics and results table
