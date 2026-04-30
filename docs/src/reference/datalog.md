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
| `pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom') → BIGINT` | Parse and store a Datalog rule set; returns rule count |
| `pg_ripple.load_rules_builtin(name TEXT) → BIGINT` | Load a built-in rule set (`'rdfs'` or `'owl-rl'`) |
| `pg_ripple.add_rule(rule_set TEXT, rule_text TEXT) → BIGINT` | Add a single rule to an existing rule set; returns new rule ID |
| `pg_ripple.remove_rule(rule_id BIGINT) → BIGINT` | Remove a rule by catalog ID; returns triples retracted |
| `pg_ripple.drop_rules(rule_set TEXT) → BIGINT` | Drop all rules in a named rule set; returns rule count |
| `pg_ripple.enable_rule_set(name TEXT) → void` | Enable a rule set without re-loading |
| `pg_ripple.disable_rule_set(name TEXT) → void` | Disable a rule set without dropping it |
| `pg_ripple.list_rules() → JSONB` | List all stored rules with id, rule_set, rule_text, stratum, active |
| `pg_ripple.list_rule_sets() → TABLE(rule_set, active, rule_count, created_at)` | List all named rule sets |
| `pg_ripple.infer(rule_set TEXT DEFAULT 'custom') → BIGINT` | Run forward-chaining inference; returns triple count |
| `pg_ripple.infer_with_stats(rule_set TEXT DEFAULT 'custom') → JSONB` | Run semi-naive inference with detailed statistics |
| `pg_ripple.infer_goal(rule_set TEXT, goal TEXT) → JSONB` | Goal-directed inference using magic sets |
| `pg_ripple.infer_agg(rule_set TEXT DEFAULT 'custom') → JSONB` | Run Datalog^agg inference for aggregate rules |
| `pg_ripple.infer_wfs(rule_set TEXT DEFAULT 'custom') → JSONB` | Well-founded semantics inference for cyclic programs |
| `pg_ripple.infer_lattice(rule_set TEXT, lattice_name TEXT) → JSONB` | Lattice-based monotone fixpoint inference |
| `pg_ripple.retract_inferred(rule_set TEXT) → BIGINT` | Delete all materialised triples for a rule set; returns count |
| `pg_ripple.check_constraints(rule_set TEXT DEFAULT NULL) → JSONB` | Run integrity constraint rules; returns violations |
| `pg_ripple.explain_inference(s TEXT, p TEXT, o TEXT, g TEXT DEFAULT NULL) → TABLE` | Return derivation tree for an inferred triple |
| `pg_ripple.explain_datalog(rule_set_name TEXT) → JSONB` | Full explain document: strata, rules, SQL, last run stats |
| `pg_ripple.dred_on_delete(pred_id BIGINT, s BIGINT, o BIGINT, g BIGINT) → BIGINT` | Manual DRed retraction for a deleted base triple |

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
