# WatDiv Benchmark Results

[WatDiv](https://dsg.uwaterloo.ca/watdiv/) (Waterloo SPARQL Diversity Test Suite) tests
pg_ripple's correctness and query performance under realistic data distributions.

## What WatDiv tests

WatDiv generates a synthetic e-commerce dataset at configurable scale and defines 100 query
templates across four structural classes, each exercising different join patterns:

| Class | Templates | What it stresses |
|---|---|---|
| **Star** (S1–S7) | 7 | Same subject, multiple predicates — VP table scan and star-join optimisation |
| **Chain** (C1–C3) | 3 | Linear predicate path — join ordering |
| **Snowflake** (F1–F5) | 5 | Star + chain hybrid — mixed join strategies |
| **Complex** (B1–B12, L1–L5) | 17 | Multi-hop with OPTIONAL and UNION — full algebra |

## Correctness criterion

Each template is run against a 10M-triple dataset and the result row count is
compared to a pre-computed baseline.  A template **passes** when its row count is
within **±0.1%** of the baseline.  Row-count failures indicate SQL planner regressions
or VP table correctness bugs.

## Performance criterion

Median query latency per template is recorded and compared to the previous release baseline.
A regression > 20% triggers a **CI warning** (not a failure).  The WatDiv suite is always
non-blocking because performance naturally varies with hardware.

## Running locally

```sh
# 1. Fetch WatDiv templates and generate the 10M-triple dataset:
bash scripts/fetch_conformance_tests.sh --watdiv

# 2. Load the dataset into pg_ripple (requires a running instance):
cargo pgrx start pg18
psql -c "SELECT pg_ripple.load_ntriples(pg_read_file('tests/watdiv/data/watdiv-10M.nt'), false);"

# 3. Run the suite:
cargo test --test watdiv_suite
```

## CI job

The `watdiv-suite` CI job runs on every push to `main` and:
1. Checks correctness (row count ±0.1% per template)
2. Records per-template median latency
3. Writes results to `tests/conformance/report.json` as a CI artifact

The job is **non-blocking** (performance regressions are warnings, not failures).

## Results table (v0.44.0, 10M triples, 8-core CI runner)

Results are updated automatically on each release.  The table below reflects
the v0.44.0 baseline; updated figures appear in the `conformance_report` CI artifact.

| Template | Class | Expected rows | Status |
|---|---|---|---|
| S1 | Star | — | — |
| S2 | Star | — | — |
| S3 | Star | — | — |
| S4 | Star | — | — |
| S5 | Star | — | — |
| S6 | Star | — | — |
| S7 | Star | — | — |
| C1 | Chain | — | — |
| C2 | Chain | — | — |
| C3 | Chain | — | — |
| F1 | Snowflake | — | — |
| F2 | Snowflake | — | — |
| F3 | Snowflake | — | — |
| F4 | Snowflake | — | — |
| F5 | Snowflake | — | — |
| B1–B12 | Complex | — | — |
| L1–L5 | Complex | — | — |

> **Note:** Row counts and latency baselines are populated on first run against
> a freshly generated WatDiv 10M dataset.  The `—` entries above are filled in
> by the CI artifact `tests/watdiv/baselines.json` after the first run.

## Known limitations

- Templates that use `%var%` substitution markers require concrete IRI bindings
  sampled from the dataset.  Templates without substitution markers run as-is.
- The WatDiv data generator (`watdiv` binary or Docker image) must be available
  to generate the 10M-triple dataset.  CI uses the pre-cached artifact from the
  first successful run.

## See also

- [Running Conformance Tests](running-conformance-tests.md) — how to fetch data and run all suites
- [W3C Conformance](w3c-conformance.md) — W3C SPARQL 1.1 and Jena suite results
