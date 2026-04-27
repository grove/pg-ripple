# SPARQL Entailment Regime Test Suite (v0.61.0)

This directory contains the test driver for the W3C SPARQL 1.1 Entailment Regime
test suite against pg_ripple's combined SPARQL + Datalog stack.

## Coverage

- **RDFS Entailment**: Basic RDFS inference via Datalog rules
- **OWL 2 RL Entailment**: OWL 2 RL inference rules

## Running the suite

```bash
# Run against a local pg_ripple instance
cargo test --test entailment_suite -- --nocapture

# Or via justfile
just test-entailment
```

The CI job `entailment-suite` runs this suite with `continue-on-error: true`
(informational until v1.0.0 when it becomes blocking).

## Structure

- `manifest.json`     — list of test cases with expected outcomes
- `runner.sh`         — shell driver that executes each test via psql
- `rdfs/`             — RDFS entailment test fixtures
- `owl2rl/`           — OWL 2 RL entailment test fixtures
