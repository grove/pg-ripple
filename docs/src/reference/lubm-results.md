# LUBM Conformance Results

This page summarises pg_ripple's results on the Lehigh University Benchmark (LUBM), a canonical benchmark for OWL RL knowledge-base systems.

## Overview

LUBM (Guo et al., 2005) defines 14 canonical SPARQL queries over a synthetic university-domain ontology (`univ-bench.owl`). The queries exercise:

- `rdf:type` lookups with OWL RL subclass/subproperty entailment
- Multi-hop property chains (`memberOf` + `subOrganizationOf` + `undergraduateDegreeFrom`)
- Domain and range reasoning
- Conjunctive patterns over asserted and inferred triples

As of v0.44.0, all 14 LUBM queries pass against the bundled `univ1` synthetic fixture with **0 known failures**.

## Test Fixture

pg_ripple uses a self-contained synthetic fixture (`tests/lubm/fixtures/univ1.ttl`) rather than the original Java UBA generator. The fixture models 1 university, 1 department, 1 research group, 4 faculty, 7 graduate students, 5 undergraduate students, 6 graduate courses, 1 undergraduate course, and 4 publications — all with explicit supertype assertions so that no OWL RL inference pass is required to match the reference counts.

A complementary **Datalog validation sub-suite** (`tests/lubm/datalog/`) separately validates that running `pg_ripple.load_rules_builtin('owl-rl')` and `pg_ripple.infer('owl-rl')` on an implicit-type-only version of the same data produces identical query results.

## Query Results (univ1 fixture)

| Query | Description | Inference rules exercised | Expected | Result | Status |
|---|---|---|---|---|---|
| Q1  | Graduate students taking GraduateCourse0 | `rdf:type` + subclass entailment | 3 | 3 | ✅ PASS |
| Q2  | Graduate students whose department is part of their undergrad university | Multi-hop join | 2 | 2 | ✅ PASS |
| Q3  | Publications by AssistantProfessor0 | Direct lookup | 2 | 2 | ✅ PASS |
| Q4  | Professors in Dept0 with name/email/phone | Property star pattern | 4 | 4 | ✅ PASS |
| Q5  | Persons in Dept0 taking GraduateCourse0 | `ub:Person` superclass | 3 | 3 | ✅ PASS |
| Q6  | All students | `ub:Student` superclass | 12 | 12 | ✅ PASS |
| Q7  | Students taking GC0 advised by a FullProfessor | Advisor + course conjunction | 3 | 3 | ✅ PASS |
| Q8  | Students in Dept0/University0 with email | Full join pattern | 12 | 12 | ✅ PASS |
| Q9  | Students taking courses from AssistantProfessors | 3-way join | 7 | 7 | ✅ PASS |
| Q10 | Graduate students in ResearchGroup0 | `ub:memberOf` lookup | 3 | 3 | ✅ PASS |
| Q11 | Sub-organizations of Department0 | `ub:subOrganizationOf` | 1 | 1 | ✅ PASS |
| Q12 | Professors heading Department0 | `ub:headOf` | 1 | 1 | ✅ PASS |
| Q13 | Professors acting as teaching assistants | `ub:teachingAssistantOf` | 1 | 1 | ✅ PASS |
| Q14 | All undergraduate students | `ub:UndergraduateStudent` subclass | 5 | 5 | ✅ PASS |

## Datalog Validation Sub-suite

The Datalog sub-suite validates the OWL RL inference engine independently of the SPARQL translator.

| Test | What it validates | Status |
|---|---|---|
| Rule compilation | `load_rules_builtin('owl-rl')` compiles ≥ 20 rules | ✅ PASS |
| Inference iteration | `infer_with_stats()` reaches fixpoint in 1–10 iterations | ✅ PASS |
| Inferred triple counts | Key supertype entailments produce correct row counts | ✅ PASS |
| Goal queries | `infer_goal()` and SPARQL counts agree for Q1/Q6/Q14 | ✅ PASS |
| Materialization perf | `infer('owl-rl')` completes in < 5 s on univ1 | ✅ PASS |
| Custom rules | User-defined transitive-closure rule works correctly | ✅ PASS |

## Running LUBM Locally

```sh
# Start pg_ripple (uses pgrx default port 28818)
cargo pgrx start pg18

# Run the LUBM suite (self-contained — no data download required)
cargo test --test lubm_suite -- --nocapture
```

To run the Datalog sub-suite SQL files manually:

```sh
# Assumes pg_ripple is installed and running
psql -c "SELECT pg_ripple.load_turtle(pg_read_file('tests/lubm/fixtures/univ1.ttl'), false)"
psql -f tests/lubm/datalog/rule_compilation.sql
psql -f tests/lubm/datalog/inference_iterations.sql
psql -f tests/lubm/datalog/inferred_triples.sql
```

## Adding Known Failures

If a LUBM query fails, add a `lubm:Q{N}` entry to `tests/conformance/known_failures.txt`:

```
# Example — Q2 fails due to multi-hop join bug, fix in progress
lubm:Q2  multi-hop memberOf/subOrganizationOf join returns wrong count
```

Remove the entry once the underlying bug is fixed.

## See Also

- [Running Conformance Tests](running-conformance-tests.md) — local setup and all suites
- [W3C Conformance](w3c-conformance.md) — W3C SPARQL 1.1, Jena, WatDiv results
- [WatDiv Results](watdiv-results.md) — performance benchmark details
