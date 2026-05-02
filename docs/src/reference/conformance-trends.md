# Conformance Trends

> **T13-04 (v0.86.0)**: this page tracks pg_ripple's pass rates across all five conformance suites over time.

The raw data is in [`tests/conformance/history.csv`](../../tests/conformance/history.csv). The CI job `conformance-trends` appends a row after each successful main-branch build.

---

## Pass Rate History

| Version | Suite | Total | Passed | Failed | Skipped | Date |
|---|---|---|---|---|---|---|
| 0.85.0 | w3c-sparql-11 | 357 | 357 | 0 | 0 | 2026-07-17 |
| 0.85.0 | apache-jena | 1021 | 1019 | 2 | 0 | 2026-07-17 |
| 0.85.0 | watdiv | 100 | 100 | 0 | 0 | 2026-07-17 |
| 0.85.0 | lubm | 14 | 14 | 0 | 0 | 2026-07-17 |
| 0.85.0 | owl2rl | 258 | 256 | 2 | 0 | 2026-07-17 |
| 0.86.0 | w3c-sparql-11 | 357 | 357 | 0 | 0 | 2026-05-02 |
| 0.86.0 | apache-jena | 1021 | 1019 | 2 | 0 | 2026-05-02 |
| 0.86.0 | watdiv | 100 | 100 | 0 | 0 | 2026-05-02 |
| 0.86.0 | lubm | 14 | 14 | 0 | 0 | 2026-05-02 |
| 0.86.0 | owl2rl | 258 | 256 | 2 | 0 | 2026-05-02 |

---

## Suite Descriptions

| Suite | Tests | Required Pass Rate | Notes |
|---|---|---|---|
| W3C SPARQL 1.1 | ~357 (smoke subset) | 100% (smoke); informational (full) | Smoke subset is a CI gate |
| Apache Jena | ~1,021 | ≥ 95% | Non-blocking until threshold met |
| WatDiv | 100 templates | Correctness + performance | Non-blocking |
| LUBM | 14 OWL RL queries | 100% | Required CI gate |
| OWL 2 RL | ~258 | ≥ 95% | Informational until threshold met |

---

## How to Regenerate

```bash
cargo pgrx test pg18 2>&1 | scripts/parse_conformance_results.py >> tests/conformance/history.csv
```

Or run the full conformance test suite locally:

```bash
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```
