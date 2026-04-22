# OWL 2 RL Conformance Baseline (v0.48.0)

This page documents the OWL 2 RL conformance baseline for pg_ripple v0.48.0,
as measured against the OWL 2 RL rule suite. The CI gate was upgraded to
**required at ≥ 95%** in v0.48.0 (previously informational).

## Summary

| Category | Rules Tested | Passing | XFAIL | Notes |
|----------|-------------|---------|-------|-------|
| cls (class axioms) | 12 | 12 | 0 | Full pass |
| prp (property axioms) | 18 | 17 | 1 | prp-spo2 (complex chain) XFAIL |
| cax (class axiom entailments) | 8 | 8 | 0 | Full pass |
| scm (schema entailments) | 14 | 13 | 1 | scm-sco (cyclical subclass) XFAIL |
| eq (equality reasoning) | 10 | 9 | 1 | eq-diff1 with owl:differentFrom XFAIL |
| dt (datatype reasoning) | 4 | 3 | 1 | dt-type2 (xs:double precision) XFAIL |
| **Total** | **66** | **62** | **4** | **93.9% pass rate** |

## New in v0.48.0

The following OWL 2 RL rules were added or completed:

- **cax-sco**: Full `rdfs:subClassOf` transitive closure (previously single-step only)
- **prp-spo1**: `rdfs:subPropertyOf` full chain (previously binary case only)
- **prp-ifp**: Inverse-functional-property derived `owl:sameAs` propagation
- **cls-avf**: Chained `owl:allValuesFrom` interaction with subclass hierarchy
- **owl:minCardinality**, **owl:maxCardinality**, **owl:cardinality** entailment rules

## Known Failures (XFAIL)

These failures are documented in `tests/owl2rl/known_failures.txt` and tracked
release-to-release for regression detection.

### prp-spo2 — Complex sub-property chain

OWL 2 RL rule `prp-spo2` requires `owl:propertyChainAxiom` with two hops.
pg_ripple supports two-hop chains but the standard test case uses a three-hop
chain that requires recursive sub-property expansion not yet implemented.

**Impact**: Low — three-hop chains are rare in practice.
**Target**: v0.49.0

### scm-sco — Cyclical subclass entailment

The test graph contains a subclass cycle (`A rdfs:subClassOf B`, `B rdfs:subClassOf A`).
pg_ripple's WFS-based Datalog engine handles this correctly but the OWL 2 RL
test harness expects a specific owl:equivalentClass entailment that our
inferencer does not currently emit.

**Impact**: Low — owl:equivalentClass assertion from subclass cycles is a
non-essential derived fact for most workloads.
**Target**: v0.49.0

### eq-diff1 — owl:differentFrom combined with owl:sameAs

The test requires detecting inconsistency when a node is asserted both
`owl:sameAs` and `owl:differentFrom` another node and emitting the resulting
owl:Nothing entailment. pg_ripple detects the inconsistency (emits PT550
WARNING) but does not propagate the owl:Nothing conclusion to the triple store.

**Impact**: Very low — inconsistency detection is present; the inferred
owl:Nothing is rarely queried directly.
**Target**: v0.50.0

### dt-type2 — xs:double precision rounding

The OWL 2 RL test for `xs:double` datatype entailment requires
`"1.0E0"^^xsd:double` to be recognised as equal to `"1"^^xsd:integer` under
numeric promotion rules. pg_ripple's dictionary encodes each literal verbatim
and does not currently perform XSD numeric promotion on store.

**Impact**: Low — affects only mixed-type numeric comparison assertions.
**Target**: v0.51.0 (XSD numeric tower)

## Pass Rate History

| Version | Passing / Total | Pass Rate |
|---------|----------------|-----------|
| v0.46.0 | n/a (suite added) | — |
| v0.47.0 | 62 / 66 | 93.9% |

## Running the Suite

```bash
# Requires: cargo pgrx start pg18 first
cargo pgrx regress pg18 -- tests/pg_regress/sql/owl2rl_*.sql
```

Or with the justfile recipe:

```bash
just test-regress
```

The known-failure list is maintained in `tests/owl2rl/known_failures.txt`.
Any regression (a previously-passing test now failing) is a blocking CI
failure regardless of the overall pass rate.
