# Datalog Reference

This page is the reference for pg_ripple's Datalog inference engine.

## Overview

pg_ripple includes a full Datalog engine that compiles Datalog rules to
recursive SQL (`WITH RECURSIVE`), executes inference in PostgreSQL, and
materializes derived triples back into the triple store. The engine supports
stratified negation, semi-naive evaluation, aggregation (`Datalog^agg`),
magic sets for goal-directed inference, `owl:sameAs` entity canonicalization,
well-founded semantics for cyclic ontologies, tabling, Delete-Rederive (DRed)
for retraction, and parallel stratum evaluation.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE 'datalog%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.create_rule_set(name TEXT) → void` | Create a named Datalog rule set |
| `pg_ripple.drop_rule_set(name TEXT) → void` | Drop a rule set and all its rules |
| `pg_ripple.add_rule(rule_set TEXT, rule_text TEXT) → void` | Add a Datalog rule to a rule set |
| `pg_ripple.remove_rule(rule_set TEXT, rule_id BIGINT) → void` | Remove a specific rule from a set |
| `pg_ripple.materialize(rule_set TEXT) → BIGINT` | Run forward-chaining inference, return triple count |
| `pg_ripple.retract(rule_set TEXT) → BIGINT` | Retract all inferred triples for a rule set (DRed) |
| `pg_ripple.query_goal(rule_set TEXT, goal TEXT) → SETOF record` | Goal-directed query with magic sets |
| `pg_ripple.explain_inference(rule_set TEXT, triple TEXT) → TEXT` | Return derivation tree for a triple |
| `pg_ripple.list_rules(rule_set TEXT) → SETOF record` | List all rules in a rule set |
| `pg_ripple.list_rule_sets() → SETOF record` | List all rule sets |
| `pg_ripple.validate_datalog_constraints(rule_set TEXT) → SETOF record` | Run integrity constraints |

## Rule Syntax

Rules use Turtle-style IRI notation or prefix-qualified names:

```
:ancestor(?x, ?z) :- :parent(?x, ?y), :ancestor(?y, ?z).
:ancestor(?x, ?y) :- :parent(?x, ?y).
```

Built-in RDFS/OWL RL rules are included and activated automatically when
`pg_ripple.enable_owl_rl = true` (default: false).

## Inference Architecture

1. Rules are parsed and stratified (negation-as-failure via WFS for cyclic rules).
2. Each stratum is compiled to a `WITH RECURSIVE` SQL query.
3. Semi-naive evaluation tracks the delta between iterations.
4. Magic sets transform rules for demand-driven (goal-directed) evaluation.
5. Derived triples are inserted into the triple store with `source = 1`.

## OWL RL Support

The built-in OWL 2 RL rule set covers the complete set of ~100 OWL RL rules
including:
- Class and property hierarchy (`rdfs:subClassOf`, `rdfs:subPropertyOf`)
- Inverse, symmetric, transitive, and functional properties
- `owl:allValuesFrom`, `owl:someValuesFrom`, `owl:hasValue`
- `owl:sameAs` canonicalization

## Related Pages

- [Datalog SQL Reference](../user-guide/sql-reference/datalog.md)
- [Lattice-Based Datalog](lattice-datalog.md)
- [LUBM OWL RL Results](lubm-results.md)
- [OWL 2 RL Results](owl2rl-results.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
