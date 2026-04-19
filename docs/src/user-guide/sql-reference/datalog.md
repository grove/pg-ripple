# Datalog Reasoning

pg_ripple includes a built-in Datalog reasoning engine (v0.10.0+) that runs entirely inside PostgreSQL. Rules are parsed from a Turtle-flavoured syntax, stratified for evaluation order, and compiled to native SQL — no external reasoner needed.

Derived triples are written back into VP storage with `source = 1`, so explicit and inferred triples are always distinguishable.

---

## Quick start

```sql
-- Load two custom rules
SELECT pg_ripple.load_rules('
  ?x <http://example.org/grandparent> ?z :-
    ?x <http://example.org/parent> ?y ,
    ?y <http://example.org/parent> ?z .
', 'family');

-- Insert some data
SELECT pg_ripple.load_ntriples('
<http://example.org/alice> <http://example.org/parent> <http://example.org/bob> .
<http://example.org/bob>   <http://example.org/parent> <http://example.org/carol> .
');

-- Run inference — inserts alice grandparent carol
SELECT pg_ripple.infer('family');
-- Returns: 1 (one derived triple)

-- Query the result
SELECT * FROM pg_ripple.sparql('
  SELECT ?gp WHERE {
    <http://example.org/alice> <http://example.org/grandparent> ?gp
  }
');
```

---

## Rule syntax

Rules use a Turtle-flavoured Datalog syntax:

```
head :- body .
```

- **Variables** are written as `?x`, `?y`, etc.
- **IRIs** use angle brackets: `<http://example.org/knows>`
- **Prefixed IRIs** use `prefix:local` form (if prefixes are registered)
- **Literals** use quoted strings: `"hello"`, `"42"^^<xsd:integer>`
- **Body atoms** are separated by commas
- **Negation** uses `NOT`: `NOT ?x <http://example.org/blocked> ?y`

### Example rules

```
-- Transitive closure of knows
?x <http://example.org/knowsTransitive> ?z :-
  ?x <http://example.org/knows> ?y ,
  ?y <http://example.org/knowsTransitive> ?z .

?x <http://example.org/knowsTransitive> ?y :-
  ?x <http://example.org/knows> ?y .

-- Constraint (empty head): every Person must have a name
:- ?x <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Person> ,
   NOT ?x <http://example.org/name> ?n .
```

---

## load_rules

```sql
pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom') → BIGINT
```

Parses Turtle-flavoured Datalog rules, stratifies them (checks for negation cycles), compiles each stratum to SQL, and stores them in `_pg_ripple.rules`. Returns the number of rules loaded.

```sql
SELECT pg_ripple.load_rules('
  ?x <http://example.org/sibling> ?y :-
    ?x <http://example.org/parent> ?z ,
    ?y <http://example.org/parent> ?z .
', 'family');
-- Returns: 1
```

Rules are grouped by **rule set name**. You can load multiple rule sets independently and run inference on each one separately.

---

## load_rules_builtin

```sql
pg_ripple.load_rules_builtin(name TEXT) → BIGINT
```

Loads a pre-defined rule set by name. Returns the number of rules loaded.

| Name | Rules | Description |
|------|-------|-------------|
| `'rdfs'` | 13 | Full RDFS entailment (rdfs2–rdfs12, subclass, domain, range) |
| `'owl-rl'` | ~20 | Core OWL RL: class hierarchy, property chains, inverse, symmetric, transitive |

```sql
-- Load RDFS entailment rules
SELECT pg_ripple.load_rules_builtin('rdfs');
-- Returns: 13

-- Load OWL RL rules
SELECT pg_ripple.load_rules_builtin('owl-rl');
```

### What RDFS entailment does

If your data contains `rdfs:subClassOf`, `rdfs:domain`, `rdfs:range`, and similar RDFS vocabulary, running RDFS inference materializes all implied class memberships and property assignments. For example, if `ex:Student rdfs:subClassOf ex:Person` and `ex:alice rdf:type ex:Student`, then `ex:alice rdf:type ex:Person` is derived.

### What OWL RL reasoning does

OWL RL handles richer ontology constructs: `owl:inverseOf` (if `ex:knows` is the inverse of `ex:knownBy`, both directions are materialized), `owl:TransitiveProperty` (transitive closure), `owl:SymmetricProperty`, `owl:propertyChainAxiom`, and class hierarchy axioms.

---

## infer

```sql
pg_ripple.infer(rule_set TEXT DEFAULT 'custom') → BIGINT
```

Runs all strata in the named rule set and inserts derived triples with `source = 1`. Returns the number of new triples inserted. Safe to call repeatedly — duplicate triples are ignored.

```sql
SELECT pg_ripple.infer('rdfs');
-- Returns: 42 (the number of new derived triples)
```

Non-recursive strata use `INSERT … SELECT … ON CONFLICT DO NOTHING`. Recursive strata use `WITH RECURSIVE … CYCLE` (PostgreSQL 18 native cycle detection).

---

## infer_with_stats

```sql
pg_ripple.infer_with_stats(rule_set TEXT) → JSONB
```

Runs semi-naive fixpoint evaluation on the named rule set and returns a JSONB object with the number of derived triples and the number of fixpoint iterations taken (v0.24.0+).

```sql
SELECT pg_ripple.infer_with_stats('rdfs');
-- Returns: {"derived": 42, "iterations": 3, "eliminated_rules": []}
```

**Why use this instead of `infer()`?** For large ontologies, semi-naive evaluation is significantly faster because each fixpoint iteration only re-evaluates rules against *new* triples derived in the previous iteration (the ΔR delta), rather than rescanning the entire derived relation. The `iterations` counter tells you how many iterations the engine needed to reach the fixpoint — bounded by the longest derivation chain, not the size of the dataset.

The `eliminated_rules` array (v0.29.0+) lists any rules that were removed by **subsumption checking** before evaluation: a rule R2 is subsumed by R1 when R1's body is a multiset-subset of R2's body (R2 would only derive a subset of what R1 derives). Eliminating subsumed rules reduces the number of SQL statements executed per fixpoint iteration.

### Semi-naive evaluation mechanics

The engine maintains, for each derived relation `R`, a *delta table* `ΔR` containing only the rows derived in the most recent iteration. Each iteration:

1. For every rule in the current stratum, re-evaluate the rule body against `ΔR` (the delta of its input relations).
2. Insert any new rows into `ΔR_new` with `ON CONFLICT DO NOTHING`.
3. After all rules are processed: `ΔR ← ΔR_new`, then continue if `ΔR` is non-empty.

This means iteration cost scales with the *frontier* of new derivations, not the total size of the relation. On RDFS closure over a dataset where the longest subClassOf chain has depth 5, the engine converges in 5 iterations regardless of how many triples there are.

Stratified evaluation order is preserved: each stratum is fully converged before the next stratum begins. Semi-naive is applied *within* each stratum.

### OWL RL coverage

The built-in `owl-rl` rule set implements the following OWL 2 RL axioms:

| OWL RL Rule | Axiom | Status |
|---|---|---|
| rdfs2 | domain inference | ✅ |
| rdfs3 | range inference | ✅ |
| rdfs4a/4b | Resource membership | ✅ |
| rdfs5 | subPropertyOf transitivity | ✅ |
| rdfs7 / prp-spo1 | subPropertyOf propagation | ✅ |
| rdfs9 / cax-sco | subClassOf type propagation | ✅ |
| rdfs11 | subClassOf transitivity | ✅ |
| cls-avf | allValuesFrom chaining | ✅ |
| prp-ifp | InverseFunctionalProperty | ✅ |
| prp-sym | SymmetricProperty | ✅ |
| prp-trp | TransitiveProperty | ✅ |
| prp-inv1/2 | inverseOf | ✅ |
| prp-fp | FunctionalProperty | ✅ |
| cax-eqc1 | equivalentClass | ✅ |
| prp-eqp1 | equivalentProperty | ✅ |
| prp-chm | propertyChainAxiom (2-link) | ✅ |
| cls-hv1 | hasValue restriction | ✅ |
| cls-int1 | intersectionOf membership | ✅ |
| eq-sym | sameAs symmetry | ✅ |
| eq-trans | sameAs transitivity | ✅ |
| eq-rep-c | sameAs class propagation | ✅ |
| owl:onProperty + allValuesFrom | cls-avf full form | ✅ |

Rules that require decidable enumeration (e.g. `owl:oneOf`, `cls-oo`) or second-order patterns are outside the OWL RL profile and are not implemented.

---

## infer_goal

```sql
pg_ripple.infer_goal(rule_set TEXT, goal TEXT) → JSONB
```

Runs **goal-directed inference** using a simplified magic sets transformation (v0.29.0+). Instead of deriving every possible fact, only the facts relevant to the specified goal triple pattern are materialized. Returns a JSONB object with three fields:

| Field | Type | Description |
|-------|------|-------------|
| `derived` | bigint | Total triples inserted by the inference |
| `iterations` | integer | Number of fixpoint iterations |
| `matching` | bigint | Triples that match the goal pattern after inference |

```sql
-- How many rdfs:type triples can we derive with type foaf:Person?
SELECT pg_ripple.infer_goal('rdfs', '?x <http://xmlns.com/foaf/0.1/type> <http://xmlns.com/foaf/0.1/Person>');
-- Returns: {"derived": 14, "iterations": 2, "matching": 5}

-- Fully open goal (equivalent to infer_with_stats but goal-directed machinery still prunes internally)
SELECT pg_ripple.infer_goal('rdfs', '?x ?p ?y');
```

### Goal syntax

A goal is a triple pattern string. Variables are written as `?name`. IRIs use angle-bracket notation:

- `?x rdf:type ex:Person` — find all persons (prefix form — uses registered prefix map)
- `?x <http://example.org/knows> ?y` — all knows triples
- `<http://example.org/alice> ?p ?o` — all triples about Alice
- `?x ?p ?y` — fully open (all triples)

### Magic sets strategy

For each bound term in the goal, the engine identifies which rules can derive triples matching that pattern and restricts the fixpoint evaluation to those rules. Magic temp tables (`_magic_{rule_set}_{pred}`) hold the demanded binding set and are automatically dropped at the end of inference.

Set `pg_ripple.magic_sets = false` to disable the transformation and fall back to full bottom-up evaluation (useful for debugging).

---

## check_constraints

```sql
pg_ripple.check_constraints(rule_set TEXT DEFAULT NULL) → JSONB
```

Evaluates all integrity constraints (rules with empty heads) and returns violations as a JSONB array. Pass `NULL` to check all rule sets, or a specific name to check one.

```sql
SELECT pg_ripple.check_constraints();
-- [
--   {"rule_set": "family", "rule_index": 3, "bindings": {"x": "<http://example.org/alice>"}},
--   ...
-- ]
```

An empty array means no violations.

---

## list_rules

```sql
pg_ripple.list_rules() → JSONB
```

Returns all active rules as a JSONB array. Each element includes the rule set name, stratum, head, body, and compiled SQL.

```sql
SELECT pg_ripple.list_rules();
```

---

## drop_rules

```sql
pg_ripple.drop_rules(rule_set TEXT) → BIGINT
```

Deletes all rules in a named rule set. Returns the number of rules deleted.

```sql
SELECT pg_ripple.drop_rules('family');
```

> **Note**: This does not delete triples that were already derived by those rules. To remove derived triples, delete rows where `source = 1` from the relevant VP tables, or use `vacuum_dictionary()` after clearing them.

---

## enable_rule_set / disable_rule_set

```sql
pg_ripple.enable_rule_set(name TEXT) → VOID
pg_ripple.disable_rule_set(name TEXT) → VOID
```

Toggle a rule set between active and inactive. Disabled rule sets are skipped by `infer()` and `check_constraints()` but remain in the catalog.

```sql
-- Temporarily disable OWL RL reasoning
SELECT pg_ripple.disable_rule_set('owl-rl');

-- Re-enable later
SELECT pg_ripple.enable_rule_set('owl-rl');
```

---

## prewarm_dictionary_hot

```sql
pg_ripple.prewarm_dictionary_hot() → BIGINT
```

Loads frequently-used IRIs (≤ 512 bytes) into an UNLOGGED hot table (`_pg_ripple.dictionary_hot`) for sub-microsecond lookups during inference. Returns the number of rows loaded.

The hot table survives connection pooling but not a database restart. It is automatically populated at `_PG_init` when `pg_ripple.inference_mode != 'off'`.

```sql
SELECT pg_ripple.prewarm_dictionary_hot();
-- Returns: 1024
```

---

## SHACL-AF bridge

When shapes loaded via `load_shacl()` contain `sh:rule` properties, pg_ripple detects them and registers placeholder entries in the Datalog rules catalog. This bridges SHACL Advanced Features (SHACL-AF) rule definitions with the Datalog engine.

---

## Configuration

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.inference_mode` | `'on_demand'` | `'off'` disables the engine entirely; `'on_demand'` evaluates via CTEs when `infer()` is called; `'materialized'` uses pg_trickle stream tables for automatic refresh |
| `pg_ripple.enforce_constraints` | `'warn'` | `'off'` silences constraint violations; `'warn'` logs them; `'error'` raises an exception |
| `pg_ripple.rule_graph_scope` | `'default'` | `'default'` applies rules to the default graph only; `'all'` applies rules across all named graphs |
| `pg_ripple.magic_sets` | `true` | Master switch for goal-directed magic sets inference (v0.29.0+) |
| `pg_ripple.datalog_cost_reorder` | `true` | Sort Datalog body atoms by VP-table cardinality at compile time (v0.29.0+) |
| `pg_ripple.datalog_antijoin_threshold` | `1000` | Minimum VP-table row count for using `LEFT JOIN … IS NULL` anti-join form for negation (v0.29.0+) |
| `pg_ripple.delta_index_threshold` | `500` | Minimum delta-table row count before creating a B-tree index on `(s, o)` (v0.29.0+) |

```sql
-- Enable strict constraint enforcement
SET pg_ripple.enforce_constraints = 'error';

-- Apply rules across all graphs
SET pg_ripple.rule_graph_scope = 'all';

-- Disable magic sets for debugging goal-directed inference
SET pg_ripple.magic_sets = false;

-- Force anti-join form for all negated atoms (even small tables)
SET pg_ripple.datalog_antijoin_threshold = 1;
```

---

## Internal tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.rules` | Stores each parsed rule with its set name, stratum, head, body, and compiled SQL |
| `_pg_ripple.rule_sets` | Tracks named rule sets with their active/inactive flag |
| `_pg_ripple.dictionary_hot` | UNLOGGED hot cache for frequently-used IRIs |

Derived triples are stored in the same VP tables as explicit triples, distinguished by the `source` column: `0` = explicit, `1` = derived.
