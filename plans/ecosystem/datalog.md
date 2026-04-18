# Datalog Reasoning Engine for pg_ripple

## 1. Motivation

The current inference plan (implementation_plan.md §4.10.4, pg_trickle.md §2.6) hard-codes RDFS entailment as manually written `WITH RECURSIVE` stream tables for `rdfs:subClassOf` and `rdfs:subPropertyOf`. This covers two rules out of a potential universe of hundreds.

A general-purpose Datalog engine subsumes and generalizes all of these:

| Approach | What it covers | Flexibility |
|---|---|---|
| Hard-coded RDFS closures (current plan) | `rdfs:subClassOf`, `rdfs:subPropertyOf` only | None — hand-written SQL |
| OWL RL profile | ~80 entailment rules from the W3C spec | Fixed rule set |
| Datalog engine | All of the above + arbitrary user-defined rules | Fully extensible |

A quad `(s, p, o, g)` is how pg_ripple actually stores data: every VP table carries `(s BIGINT, o BIGINT, g BIGINT)` where `g` is the dictionary-encoded named graph IRI (0 = default graph). Datalog rules over this quad structure are exactly how RDFS, OWL RL, and custom domain rules are formally specified. The graph dimension is first-class — rules can read from and write into specific named graphs, propagate facts across graphs, or operate graph-agnostically.

### Use cases beyond standard entailment

- **Custom domain reasoning**: derive `ex:indirectManager` from transitive `ex:manager` chains
- **Access control**: derive `ex:canRead` from role hierarchies and graph membership
- **Data quality**: derive `ex:missingEmail` from negation over expected properties
- **Derived graphs**: materialize computed subgraphs for downstream consumers
- **Ontology alignment**: rules mapping between vocabularies in multi-source datasets

---

## 2. Architecture

```
User rules (Datalog syntax or built-in rule set name)
    │
    ▼
Rule parser → Rule IR (head ← body₁, body₂, …, ¬bodyₙ)
    │
    ▼
Dependency analysis → Stratification (detect negation layers)
    │
    ▼
Per-stratum SQL generator:
  - Each body atom → VP table join (integer-encoded)
  - Recursive rules → WITH RECURSIVE … CYCLE
  - Negation-as-failure → NOT EXISTS (only in higher strata)
    │
    ▼
Two execution modes:
  ├─ Materialized (pg_trickle stream table per derived predicate)
  └─ On-demand (inline CTEs injected into SPARQL→SQL at query time)
```

### Module location

```
src/datalog/
    mod.rs          — public API (#[pg_extern] functions)
    parser.rs       — rule text → Rule IR
    stratify.rs     — dependency graph, stratification, cycle detection
    compiler.rs     — Rule IR → SQL (per stratum)
    builtins.rs     — built-in rule sets (RDFS, OWL RL)
    catalog.rs      — _pg_ripple.rules table CRUD
```

---

## 3. Rule Syntax

A Turtle-flavoured Datalog notation that reuses the prefix registry already in pg_ripple. Each rule is a line of the form `head :- body .` where head and body are triple patterns with variables (`?x`) and constants (prefixed IRIs or literals).

### 3.1 Basic rules

```prolog
# RDFS subclass transitivity
?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .

# OWL RL: symmetric property
?y ?p ?x :- ?x ?p ?y, ?p rdf:type owl:SymmetricProperty .

# Custom: transitive manager
?x ex:indirectManager ?z :- ?x ex:manager ?z .
?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .
```

### 3.2 Negation (stratified)

Negation-as-failure uses `NOT` before a body atom. The negated predicate must be fully computable in a lower stratum (standard stratified Datalog semantics).

```prolog
# Flag people without email
?x ex:missingEmail "true"^^xsd:boolean :- ?x rdf:type foaf:Person, NOT ?x foaf:mbox ?_ .
```

### 3.3 Named graph scoping (quad patterns)

`GRAPH` can appear in both head and body atoms. The graph term can be a **constant IRI** or a **variable** (`?g`), making full quad patterns possible.

#### Constant graph — read from a specific named graph

```prolog
# Derive into default graph from a trusted named graph
?x ex:verified "true"^^xsd:boolean :- GRAPH ex:trusted { ?x rdf:type ex:Entity } .
```

#### Constant graph — write derived triples into a named graph

```prolog
# All RDFS inference output goes into ex:inferred
GRAPH ex:inferred { ?x rdf:type ?c } :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .
```

#### Variable graph — propagate within the same graph (graph-transparent rule)

```prolog
# Derive ex:indirectManager within the same graph the source facts live in
GRAPH ?g { ?x ex:indirectManager ?z } :- GRAPH ?g { ?x ex:manager ?z } .
GRAPH ?g { ?x ex:indirectManager ?z } :- GRAPH ?g { ?x ex:manager ?y }, GRAPH ?g { ?y ex:indirectManager ?z } .
```

#### Variable graph — derive provenance metadata

```prolog
# Record which named graphs an entity appears in
?x ex:appearsIn ?g :- GRAPH ?g { ?x rdf:type ex:Entity } .
```

#### Cross-graph rules

```prolog
# Merge type assertions from all named graphs into the default graph
?x rdf:type ?c :- GRAPH ?g { ?x rdf:type ?c } .
```

**Default behaviour when `GRAPH` is omitted**: controlled by the GUC `pg_ripple.rule_graph_scope`:
- `'default'` *(recommended)*: unscoped atoms match only the default graph (`g = 0`). Derived triples are written to the default graph.
- `'all'`: unscoped atoms match triples in any graph. Useful for ontology-level rules that should span the whole dataset.

See §7.3 for the GUC definition.

### 3.4 Grammar (informal EBNF)

```
RuleSet       ::= (Rule)*
Rule          ::= Head ':-' Body '.'
Head          ::= GraphPattern? TriplePattern
Body          ::= Literal (',' Literal)*
Literal       ::= 'NOT'? GraphPattern? TriplePattern
GraphPattern  ::= 'GRAPH' GraphTerm '{' '}'
GraphTerm     ::= Variable | PrefixedIRI | FullIRI   -- variables allowed: ?g
TriplePattern ::= Term Term Term
Term          ::= Variable | PrefixedIRI | FullIRI | RDFLiteral
Variable      ::= '?' [a-zA-Z_][a-zA-Z0-9_]*
```

Key point: `GraphTerm` admits variables, enabling full quad patterns. A `Variable` used as a graph term (`?g`) is unified across all body atoms and the head atom that share it, exactly as subject/predicate/object variables are.

---

## 4. Internal Representation

```rust
/// A single Datalog rule: head :- body.
struct Rule {
    head: Atom,
    body: Vec<BodyLiteral>,
}

/// A triple pattern with an optional graph.
struct Atom {
    s: Term,
    p: Term,
    o: Term,
    g: Term,  // Term::Default for unspecified graph
}

enum Term {
    Var(String),       // ?x — unified across the rule during compilation
    Const(i64),        // dictionary-encoded IRI/literal
    AnyGraph,          // unscoped atom — resolved to g = 0 or ANY g per rule_graph_scope GUC
}

enum BodyLiteral {
    Positive(Atom),
    Negated(Atom),
}

/// Output of stratification.
struct StratifiedProgram {
    strata: Vec<Stratum>,
}

struct Stratum {
    rules: Vec<Rule>,
    is_recursive: bool,       // contains mutually recursive rules
    derived_predicates: Vec<i64>,  // predicate IDs defined in this stratum
}
```

---

## 5. Stratification

Stratification partitions rules into layers such that every negated predicate is fully computed in a lower stratum before its negation is evaluated. This is standard Datalog semantics and guarantees a unique minimal model.

### Algorithm

1. Build the **predicate dependency graph**: for each rule, the head predicate depends on every body predicate (positive edge) or negated body predicate (negative edge).
2. Compute **strongly connected components** (SCCs) of the dependency graph.
3. Check: if any SCC contains a negative edge, the program is **unstratifiable** → reject with a clear error message naming the offending predicates.
4. Topologically sort the SCCs → each SCC (or group of SCCs with no negative edges between them) becomes a stratum.
5. Mark strata containing cycles as `is_recursive = true`.

### Example

```
Stratum 0 (recursive, no negation):
  - rdfs:subClassOf closure
  - rdf:type expansion via subclass

Stratum 1 (non-recursive, negation of base/stratum-0 predicates):
  - ex:missingEmail (negates foaf:mbox, which is base data)
```

### Error reporting

```
ERROR: unstratifiable rule set — negation cycle detected
DETAIL: ex:foo negates ex:bar, which depends on ex:foo
HINT: remove the negation cycle or split into separate rule sets
```

---

## 6. SQL Compilation

All SQL generation follows pg_ripple's core design constraint: **integer joins everywhere**. All constants in rule bodies are dictionary-encoded before SQL generation. Derived VP tables use the same `(s BIGINT, o BIGINT, g BIGINT)` schema as base VP tables.

### 6.1 Non-recursive rules

Each rule compiles to a single `INSERT … SELECT`:

```sql
-- Rule: ?y ?p ?x :- ?x ?p ?y, ?p rdf:type owl:SymmetricProperty .
-- (where rdf:type = 7, owl:SymmetricProperty = 201)
INSERT INTO _pg_ripple.vp_{p_id}_delta (s, o, g)
SELECT t1.o, t1.s, t1.g
FROM _pg_ripple.vp_{p_id} t1
JOIN _pg_ripple.vp_7 t2 ON t2.s = t1.p AND t2.o = 201
ON CONFLICT DO NOTHING
```

Note: symmetric-property rules produce one derived VP table per symmetric predicate. The compiler iterates over predicates that match `?p rdf:type owl:SymmetricProperty` at rule-load time and generates one SQL statement per predicate.

### 6.2 Recursive rules

Recursive rules in the same stratum compile to `WITH RECURSIVE … CYCLE`:

```sql
-- Rules:
--   ?x ex:indirectManager ?z :- ?x ex:manager ?z .
--   ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .
-- (where ex:manager = 42, ex:indirectManager = derived_43)

WITH RECURSIVE indirect_manager(s, o, g) AS (
    -- Base case: direct manager
    SELECT s, o, g
    FROM _pg_ripple.vp_42           -- ex:manager
  UNION
    -- Recursive step
    SELECT m.s, im.o, m.g
    FROM _pg_ripple.vp_42 m         -- ex:manager
    JOIN indirect_manager im ON im.s = m.o
)
CYCLE s, o SET is_cycle USING path
SELECT s, o, g FROM indirect_manager WHERE NOT is_cycle
```

The `CYCLE … SET is_cycle USING path` clause is PostgreSQL 14+ syntax for cycle detection — mandatory to guard against circular graphs.

When the graph term is a **variable** (`?g`), the `CYCLE` clause must include `g` to detect cycles within each graph independently:

```sql
-- Variable-graph transitive rule:
-- GRAPH ?g { ?x ex:indirectManager ?z } :- GRAPH ?g { ?x ex:manager ?z } .
-- GRAPH ?g { ?x ex:indirectManager ?z } :- GRAPH ?g { ?x ex:manager ?y }, GRAPH ?g { ?y ex:indirectManager ?z } .

WITH RECURSIVE indirect_manager(s, o, g) AS (
    SELECT s, o, g FROM _pg_ripple.vp_42   -- ex:manager (all graphs)
  UNION
    SELECT m.s, im.o, m.g
    FROM _pg_ripple.vp_42 m
    JOIN indirect_manager im ON im.s = m.o AND im.g = m.g  -- same graph
)
CYCLE s, o, g SET is_cycle USING path   -- g included: cycles are per-graph
SELECT s, o, g FROM indirect_manager WHERE NOT is_cycle
```

If `g` is a constant (specific named graph or default graph), it is pushed into the `WHERE` clause instead and `CYCLE` only needs `(s, o)`.

### 6.3 Negation (higher strata)

Negated body atoms compile to `NOT EXISTS`:

```sql
-- Rule: ?x ex:missingEmail "true"^^xsd:boolean :- ?x rdf:type foaf:Person, NOT ?x foaf:mbox ?_ .
-- (where rdf:type = 7, foaf:Person = 99, foaf:mbox = 15, ex:missingEmail = derived_50, "true"^^xsd:boolean = 301)

INSERT INTO _pg_ripple.vp_derived_50_delta (s, o, g)
SELECT t.s, 301, t.g
FROM _pg_ripple.vp_7 t
WHERE t.o = 99                    -- foaf:Person
  AND NOT EXISTS (
      SELECT 1 FROM _pg_ripple.vp_15 m WHERE m.s = t.s
  )
ON CONFLICT DO NOTHING
```

### 6.4 Star patterns in rule bodies

When multiple body atoms share the same subject variable, the compiler generates a single join chain (consistent with the SPARQL→SQL star-pattern optimization):

```sql
-- Rule: ?x ex:eligible "true" :- ?x rdf:type ex:Employee, ?x ex:age ?a, ?a > 18 .
SELECT t1.s, 501, t1.g
FROM _pg_ripple.vp_7 t1             -- rdf:type
JOIN _pg_ripple.vp_55 t2 ON t2.s = t1.s   -- ex:age
WHERE t1.o = 200                    -- ex:Employee
  AND t2.o > 18                     -- FILTER (integer-encoded comparison)
```

---

## 7. Execution Modes

### 7.1 Materialized (with pg_trickle)

Each derived predicate becomes a pg_trickle stream table that is incrementally maintained as base VP tables change:

```sql
-- Derived predicate ex:indirectManager → stream table
SELECT pgtrickle.create_stream_table(
    name     => '_pg_ripple.vp_derived_43',
    query    => $$ WITH RECURSIVE indirect_manager(s, o, g) AS ( … ) SELECT … $$,
    schedule => '10s'
);
```

pg_trickle's DAG scheduler handles inter-stratum dependencies: stratum 0 stream tables refresh before stratum 1 stream tables that negate them.

**Benefits**:
- Derived triples are always fresh (within the schedule interval)
- SPARQL queries see derived VP tables identically to base VP tables — no special handling in the query engine
- Incremental maintenance: only changed base triples trigger recomputation

**Requirements**: pg_trickle must be installed. This is the recommended mode for production workloads.

### 7.2 On-demand (no pg_trickle needed)

Rules are compiled to inline CTEs that the SPARQL→SQL generator inserts when a query references a derived predicate:

```sql
-- SPARQL: SELECT ?x WHERE { ?x ex:indirectManager ex:Alice }
-- On-demand mode: the CTE is inlined into the query

WITH RECURSIVE indirect_manager(s, o, g) AS (
    SELECT s, o, g FROM _pg_ripple.vp_42
  UNION
    SELECT m.s, im.o, m.g
    FROM _pg_ripple.vp_42 m
    JOIN indirect_manager im ON im.s = m.o
)
CYCLE s, o SET is_cycle USING path
SELECT im.s
FROM indirect_manager im
WHERE im.o = 43  -- ex:Alice (encoded)
  AND NOT im.is_cycle
```

**Benefits**:
- No pg_trickle dependency
- No write amplification (no derived tables stored)
- Always perfectly fresh (computed at query time)

**Trade-offs**:
- Recursive rules re-execute the full closure on every query
- Not suitable for large, frequently-queried rule sets
- Query latency increases with rule complexity

### 7.3 Mode selection

```sql
-- Inference execution mode
SET pg_ripple.inference_mode = 'materialized';  -- default when pg_trickle is present
SET pg_ripple.inference_mode = 'on_demand';      -- default when pg_trickle is absent
SET pg_ripple.inference_mode = 'off';            -- disable inference entirely

-- Graph scope for unscoped body atoms (atoms without an explicit GRAPH clause)
SET pg_ripple.rule_graph_scope = 'default';  -- match only g = 0 (recommended)
SET pg_ripple.rule_graph_scope = 'all';      -- match triples in any graph
```

`rule_graph_scope = 'default'` is the safe default: rules that don't mention `GRAPH` operate only on the default graph, preventing unintended cross-graph reasoning. Set `'all'` for ontology-level rules (RDFS, OWL RL) that should span the whole dataset regardless of which named graph the facts live in.

---

## 8. Built-in Rule Sets

Standard entailment profiles ship as pre-packaged Datalog rule sets stored in the extension. Users can inspect, override, or extend them.

### 8.1 RDFS Entailment (13 rules)

The W3C RDF Semantics specification defines the RDFS entailment rules. Key rules:

```prolog
# rdfs2: domain inference
?x rdf:type ?c :- ?x ?p ?_ , ?p rdfs:domain ?c .

# rdfs3: range inference
?y rdf:type ?c :- ?_ ?p ?y , ?p rdfs:range ?c .

# rdfs5: subPropertyOf transitivity
?p rdfs:subPropertyOf ?r :- ?p rdfs:subPropertyOf ?q, ?q rdfs:subPropertyOf ?r .

# rdfs7: subPropertyOf propagation
?x ?q ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?q .

# rdfs9: subClassOf type propagation
?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .

# rdfs11: subClassOf transitivity
?b rdfs:subClassOf ?c :- ?b rdfs:subClassOf ?a, ?a rdfs:subClassOf ?c .
```

### 8.2 OWL RL Profile (~80 rules)

The W3C OWL 2 RL profile is the subset of OWL that can be implemented as Datalog rules (no disjunction, no existentials). Key categories:

- **Symmetric/transitive/inverse properties**: 3 rules each
- **Class axioms**: equivalentClass, disjointWith, intersectionOf, unionOf
- **Property axioms**: equivalentProperty, inverseOf, propertyChainAxiom
- **Restriction axioms**: someValuesFrom, allValuesFrom, hasValue
- **Equality**: sameAs transitivity and replacement rules

### 8.3 Loading built-in rule sets

```sql
-- Load RDFS rules
SELECT pg_ripple.load_rules_builtin('rdfs');

-- Load OWL RL rules (includes RDFS as stratum 0)
SELECT pg_ripple.load_rules_builtin('owl-rl');

-- View loaded rules
SELECT * FROM pg_ripple.list_rules();
```

---

## 9. Catalog Tables

### 9.1 Rule storage

```sql
CREATE TABLE _pg_ripple.rules (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    rule_set      TEXT NOT NULL,             -- e.g. 'rdfs', 'owl-rl', 'custom'
    rule_text     TEXT NOT NULL,             -- original Datalog text
    head_pred     BIGINT NOT NULL,           -- dictionary-encoded derived predicate IRI
    stratum       INT NOT NULL,              -- computed stratum number
    is_recursive  BOOLEAN NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 9.2 Derived predicate registry

Derived predicates are registered alongside base predicates in `_pg_ripple.predicates` with a `derived` flag:

```sql
ALTER TABLE _pg_ripple.predicates ADD COLUMN derived BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE _pg_ripple.predicates ADD COLUMN rule_set TEXT;
```

This allows the SPARQL engine to look up derived VP tables the same way it looks up base VP tables — no special handling in the query planner.

---

## 10. API Surface

```sql
-- Load rules from Datalog text
SELECT pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom');

-- Load a built-in rule set
SELECT pg_ripple.load_rules_builtin(name TEXT);  -- 'rdfs' | 'owl-rl'

-- Materialize all derived predicates as pg_trickle stream tables
SELECT pg_ripple.materialize_rules(schedule TEXT DEFAULT '10s');

-- List active rules
SELECT * FROM pg_ripple.list_rules();
-- Returns: id, rule_set, rule_text, head_pred_iri, stratum, is_recursive, created_at

-- Drop rules by rule set name
SELECT pg_ripple.drop_rules(rule_set TEXT);

-- Drop all rules and derived tables
SELECT pg_ripple.drop_all_rules();

-- Set inference mode
SET pg_ripple.inference_mode = 'materialized' | 'on_demand' | 'off';
```

---

## 11. Interaction with Existing Components

### 11.1 SPARQL engine

The query translation engine needs one addition: when `pg_ripple.inference_mode != 'off'` and a query references a derived predicate:

- **Materialized mode**: the predicate's VP table is a stream table — no change needed, the existing join generation works as-is.
- **On-demand mode**: the compiler prepends the derived predicate's CTE to the generated SQL. Multiple derived predicates in one query → multiple CTEs. The CTE names are the derived VP table names.

### 11.2 SHACL

SHACL validation should run against inferred triples too. In materialized mode, this happens naturally (derived VP tables are real tables; SHACL triggers fire on them). In on-demand mode, validation would need to inline the rules — a known limitation documented for users.

### 11.3 pg_trickle

The DAG of stream tables for a stratified program:

```
Base VP tables (CDC-tracked)
    │
    Stratum 0 stream tables (recursive, no negation)
    │   ├── rdfs:subClassOf closure
    │   ├── rdfs:subPropertyOf closure
    │   └── rdf:type expansion
    │
    Stratum 1 stream tables (may negate stratum 0)
    │   └── ex:missingEmail
    │
    Stratum 2 stream tables (may negate stratum 1)
        └── ...
```

pg_trickle's DAG scheduler automatically respects stratum ordering because stream tables in stratum N reference stream tables from stratum N-1 as source tables.

### 11.4 SPARQL views (§2.2 of pg_trickle.md)

A SPARQL view created via `pg_ripple.create_sparql_view()` can reference derived predicates. If the derived predicates are materialized, the SPARQL view's stream table depends on them in pg_trickle's DAG — refresh order is automatic. If on-demand, the SPARQL view's SQL includes the inlined CTEs.

---

## 12. Evaluation Strategy: Semi-naive

The standard Datalog evaluation strategy is **semi-naive evaluation**, which processes only newly derived facts in each iteration rather than re-scanning all facts (naive evaluation).

For materialized mode with pg_trickle, semi-naive evaluation maps naturally to pg_trickle's differential refresh: each IVM cycle processes only the delta (new/changed rows) from the previous cycle.

For on-demand mode, full recomputation happens at query time — semi-naive is not applicable (the `WITH RECURSIVE` SQL engine handles fixed-point iteration internally).

---

## 13. Performance Considerations

| Scenario | Without Datalog engine | With Datalog engine (materialized) | With Datalog engine (on-demand) |
|---|---|---|---|
| RDFS `rdf:type` with subclass hierarchy | Recursive CTE at query time | Stream table scan | Inline CTE at query time |
| Transitive `ex:manager` chains | Not available (manual SQL) | Stream table scan | Inline CTE at query time |
| OWL RL symmetric property | Not available | Stream table scan | Inline CTE at query time |
| Negation-based quality checks | Manual SHACL or triggers | Stream table + IMMEDIATE mode | Inline NOT EXISTS |

### Write amplification (materialized mode)

Each derived triple is stored in a derived VP table. For a rule set that derives N triples from M base triples, storage grows by `N * 24 bytes` (three BIGINTs). The RDFS rule set on a typical ontology with 100 classes and 50 properties derives 500–2000 additional triples — negligible storage.

OWL RL on a rich ontology can derive significantly more (10K–100K triples). The `schedule` parameter on materialization controls how often this cost is paid.

### Query-time cost (on-demand mode)

On-demand CTEs add query planning and execution overhead proportional to the number of rules and the size of the transitive closure. For small rule sets (RDFS) this is <10ms. For large rule sets (OWL RL) on complex ontologies, this can be 100ms–1s — hence the recommendation to use materialized mode for production.

---

## 14. Included in v0.10.0, Limitations, and Future Work

### Arithmetic built-ins (v0.10.0)

A set of built-in predicates for arithmetic comparison and computation, compiled directly to SQL operators:

| Built-in | SQL mapping | Example |
|---|---|---|
| `?x > ?y`, `>=`, `<`, `<=`, `=`, `!=` | `>`, `>=`, `<`, `<=`, `=`, `<>` | `?age > 18` |
| `?z IS ?x + ?y` | `?x + ?y AS z` | `?total IS ?price + ?tax` |
| `?z IS ?x * ?y` | `?x * ?y AS z` | `?area IS ?w * ?h` |
| `STRLEN(?s) > ?n` | `LENGTH(d.value) > ?n` | `STRLEN(?name) > 0` |
| `REGEX(?s, ?pattern)` | `d.value ~ ?pattern` | `REGEX(?email, '@')` |

Arithmetic built-ins appear only in rule bodies and compile to `WHERE` clause expressions. They do not derive new quads — they filter. Arithmetic over dictionary-encoded integers works directly (encoded literals preserve numeric sort order for `xsd:integer`, `xsd:decimal`, `xsd:double`). String functions require a dictionary decode join.

Example:

```prolog
# Eligible for senior discount: type Employee, age > 60
?x ex:seniorDiscount "true"^^xsd:boolean :- ?x rdf:type ex:Employee, ?x ex:age ?a, ?a >= 60 .
```

Compiled SQL:

```sql
INSERT INTO _pg_ripple.vp_derived_88_delta (s, o, g)
SELECT t1.s, 301, t1.g
FROM _pg_ripple.vp_7 t1      -- rdf:type
JOIN _pg_ripple.vp_55 t2 ON t2.s = t1.s   -- ex:age
WHERE t1.o = 200              -- ex:Employee
  AND t2.o >= 60              -- arithmetic filter
ON CONFLICT DO NOTHING
```

### Constraint rules — integrity constraints (v0.10.0)

Rules with an **empty head** express integrity constraints: fact patterns that must never hold. When the body is satisfiable, the constraint is violated.

```prolog
# A person cannot be their own manager
:- ?x ex:manager ?x .

# A resource cannot be both a class and an individual
:- ?x rdf:type owl:Class, ?x rdf:type owl:NamedIndividual .

# Disjoint classes: Cat and Dog cannot share instances
:- ?x rdf:type ex:Cat, ?x rdf:type ex:Dog .
```

Constraint rules compile to existence checks:

```sql
-- :- ?x ex:manager ?x .
SELECT EXISTS (
    SELECT 1 FROM _pg_ripple.vp_42 WHERE s = o  -- self-loop
) AS violated;
```

**Execution modes**:

- **Materialized (pg_trickle)**: each constraint rule becomes a stream table; any row in the table = a violation. With `IMMEDIATE` refresh, violations are caught within the same transaction as the DML. This directly complements and extends SHACL validation.
- **On-demand**: `pg_ripple.check_constraints()` runs all constraint queries and returns violations as JSONB.
- **Enforcement**: `pg_ripple.enforce_constraints = 'error' | 'warn' | 'off'` GUC controls behaviour on insert — reject the transaction (`error`), log a warning (`warn`), or do nothing (`off`).

Catalog: constraint rules are stored in `_pg_ripple.rules` with `head_pred = NULL` to distinguish them from derivation rules.

API:

```sql
-- Check all constraints, return violations
SELECT * FROM pg_ripple.check_constraints();
-- Returns: rule_id, rule_text, violating_subjects (BIGINT[]), violation_count
```

### Initial release limitations (v0.10.0)

- **No aggregation in rule bodies** (Datalog^agg): rules cannot use `COUNT`, `SUM`, `MIN`, `MAX` in body atoms. Aggregation is handled by SPARQL queries over derived quads. Deferred to post-1.0 (requires aggregation-stratification spec).
- **No function symbols**: standard Datalog restriction — no Skolem functions or computed terms. Existential rules (Datalog+) are deferred.
- **No disjunction in rule heads**: one head atom per rule (standard Datalog). OWL axioms requiring disjunction (outside OWL RL) are not supported.
- **No magic sets optimization**: full materialization or full on-demand CTE. Magic sets (goal-directed evaluation) is deferred to post-1.0.
- **Cross-graph negation**: `NOT GRAPH ?g { … }` (negating a variable-graph pattern) is not supported in the initial release. Negation is restricted to constant-graph or default-graph body atoms.

### Future extensions

| Feature | Description | Tier | Target |
|---|---|---|---|
| Aggregation in rule bodies (Datalog^agg) | `COUNT`, `SUM`, `MIN`, `MAX` in body atoms with aggregation-stratification; `GROUP BY` semantics. Enables analytics-derived rules and graph metrics. | 1 | Post-1.0 |
| `owl:sameAs` merging | Entity canonicalization: all facts about `?x` also apply to `?y` when `?x owl:sameAs ?y`. Pre-pass canonicalization in the SQL compiler. Completes OWL RL coverage. | 1 | Post-1.0 |
| Rule provenance (why-provenance) | Track which base quads caused each derived quad in a parallel `_pg_ripple.rule_provenance` table. `pg_ripple.explain_derivation(s, p, o)` returns a derivation tree. Critical for trust and debugging. | 1 | Post-1.0 |
| Magic sets optimization | Goal-directed evaluation: only derive facts relevant to a specific query, reducing materialization cost for large rule sets. Well-studied SQL encoding. | 1 | Post-1.0 |
| Incremental rule updates | Add/remove individual rules without recomputing the entire program. Requires dependency-aware invalidation of affected strata only. | 1 | Post-1.0 |
| Graph analytics rules | Shortest paths, connected components, PageRank expressed as recursive Datalog rules with aggregation. Requires Datalog^agg. Maps to `WITH RECURSIVE` + aggregate window functions. | 2 | Post-1.0 |
| Existential rules (Datalog+/−) | Existentially quantified variables in rule heads → Skolem blank node generation. Extends coverage from OWL RL to OWL DL subset. Well-understood but non-trivial implementation. | 2 | Post-1.0 |
| Temporal Datalog | Rules over time-stamped quads with temporal operators (`BEFORE`, `AFTER`, `DURING`). Aligns with ROADMAP v1.3 (TimescaleDB integration). | 2 | Post-1.0 |
| Well-founded semantics | Three-valued model (true/false/unknown) for non-stratifiable programs. More permissive than stratification for cyclic ontologies with defaults. Known SQL encoding via iterative fixpoint. | 2 | Post-1.0 |
| Multi-head rules | Syntactic sugar: single rule body → multiple head atoms. Desugars to multiple single-head rules at compile time. Low implementation cost. | — | v0.10.0 |
| Rule priorities / defeasible logic | Priority ordering for contradictory derived facts. Standard in Description Logic reasoners. Complex semantics but important for ontology merging. | 3 | Post-1.0 |
| Active rules (ECA) | Event-condition-action rules that trigger side-effects (`NOTIFY`, function calls) rather than deriving quads. Breaks pure declarative model; maps to PG `NOTIFY` + triggers. | 3 | Post-1.0 |
| Probabilistic rules | Weighted rules for uncertain reasoning (e.g., link prediction). Requires probability propagation semantics (ProbLog-style). | 3 | Post-1.0 |
| SWRL integration | Semantic Web Rule Language as an alternative rule syntax. Turtle-based; maps to the same IR. | 3 | Post-1.0 |
| SHACL-AF `sh:rule` bridge | Detect `sh:rule` entries in SHACL shapes, compile to Datalog IR. Bidirectional: SHACL shapes inform Datalog constraints; derived triples visible to SHACL validation. | 1 | v0.10.0 |
| Datalog views | Incremental stream tables for Datalog rule sets with a goal pattern. Bundles rules + query as one self-contained artifact. | 1 | v0.11.0 |
| Magic sets optimization | Goal-directed evaluation: only derive facts relevant to a specific query, reducing materialization cost for large rule sets. Well-studied SQL encoding (Bancilhon et al., 1986; Ullman, 1989). | 1 | v0.29.0 |
| Cost-based body atom reordering | Use `pg_class` statistics to reorder body atoms (joins) by selectivity within the SQL compiler. Evaluate the most selective joins first to reduce intermediate result size. | 1 | v0.29.0 |
| Subsumption checking | Detect and eliminate redundant rules at compile time. If rule R1's head and body are a strict generalization of R2, R2 can be pruned. Reduces fixpoint iterations and SQL query count. | 1 | v0.29.0 |
| Difference-join negation | Replace `NOT EXISTS` subqueries with anti-join (`LEFT JOIN … WHERE … IS NULL`) patterns for negated body atoms. PostgreSQL's planner often generates better plans for anti-joins than correlated `NOT EXISTS`. | 1 | v0.29.0 |
| Predicate-filter pushdown | Push arithmetic and comparison guards (`?a > 18`, `REGEX(…)`) as early as possible in the join tree rather than applying them at the outermost WHERE clause. Reduces intermediate cardinality. | 1 | v0.29.0 |
| Delta table indexing | Create targeted B-tree indexes on semi-naive delta tables (`_dl_delta_{pred_id}`) for high-arity rules. Currently deltas are unindexed heap tables; indexing them reduces join cost in subsequent iterations. | 1 | v0.29.0 |
| Incremental maintenance (DRed) | Delete-Rederive algorithm: when base triples are deleted, identify and retract affected derived triples, then re-derive any that remain supported. Avoids full re-materialization on data changes. | 1 | Post-1.0 |
| Compiled rule plans | Cache the generated SQL for each rule across inference runs. Currently SQL is regenerated on every `infer()` call. Caching avoids repeated dictionary lookups and SQL string construction. | 2 | Post-1.0 |
| Parallel stratum evaluation | Evaluate independent rules within the same stratum in parallel using PostgreSQL's background workers. Each rule's INSERT … SELECT runs as a separate SPI transaction; results are merged at the end of the iteration. | 2 | Post-1.0 |
| Worst-case optimal joins | Replace pairwise hash-joins with worst-case optimal join algorithms (Leapfrog Triejoin / Generic Join) for rules with ≥3 body atoms sharing variables. Avoids intermediate result blowup on cyclic join patterns. | 3 | Post-1.0 |
| Lattice-based Datalog (Datalog^L) | Extend the rule IR to support user-defined lattice types (e.g., intervals, sets, trust levels) with monotone aggregation. Generalizes Datalog^agg; inspired by Flix and Datafun. | 3 | Post-1.0 |
| Demand transformation | A generalization of magic sets: rewrite the rule program so that only demanded facts are derived, propagating binding patterns from the query goal through the rule bodies. More flexible than magic sets for complex rule topologies. | 2 | Post-1.0 |
| Tabling / memoization | Cache intermediate derived facts across queries (subsumptive tabling). When the same subgoal appears in multiple queries, reuse the previously computed result instead of recomputing from scratch. Inspired by XSB Prolog. | 2 | Post-1.0 |
| Bounded-depth early termination | For rules that are known to produce derivation chains of bounded depth (e.g., `rdfs:subClassOf` in ontologies with known max depth), terminate fixpoint iteration early when the depth bound is reached rather than running the final empty-delta check iteration. | 2 | Post-1.0 |

---

## 14.2 Optimization Techniques — Detailed Design

This section provides detailed design notes for Datalog optimizations planned beyond the current semi-naive evaluation baseline. Optimizations are grouped by category: **evaluation strategy**, **SQL compilation**, and **maintenance**.

### 14.2.1 Magic Sets Transformation (v0.29.0)

The most impactful single optimization for goal-directed Datalog evaluation. Magic sets transforms a Datalog program + query into a *more efficient* program that computes only the facts needed to answer the query, while still using bottom-up semi-naive evaluation.

**Problem**: Currently, `pg_ripple.infer('rdfs')` materializes the *entire* RDFS closure — every possible `rdf:type` and `rdfs:subClassOf` derivation. If a user only needs "all types of entity X", 99% of the computed closure is wasted.

**Algorithm** (Bancilhon, Maier, Sagiv, Ullman — 1986):

1. **Adornment**: given a query like `?x rdf:type foaf:Person`, annotate the goal predicate with a *binding pattern* — here `rdf:type^bf` (bound-free: the object is bound, subject is free).
2. **Propagation**: push binding patterns through rule bodies. For `?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c`, the binding on `?c` propagates to `rdfs:subClassOf^fb`.
3. **Magic rule generation**: for each adorned predicate, generate a "magic" predicate that captures the set of demanded bindings:
   ```prolog
   magic_rdf_type_bf(?c) :- .  -- seed: the query constant foaf:Person
   magic_rdf_type_bf(?c) :- magic_rdf_type_bf(?b), ?b rdfs:subClassOf ?c .
   ```
4. **Modified rules**: add a filter to each original rule body that restricts it to demanded tuples:
   ```prolog
   ?x rdf:type ?c :- magic_rdf_type_bf(?c), ?x rdf:type ?b, ?b rdfs:subClassOf ?c .
   ```
5. **Evaluate the modified program** using standard semi-naive evaluation. The magic predicates are small (only containing demanded constants), so joins are much cheaper.

**SQL encoding** (pg_ripple-specific):

- Magic predicates compile to temporary tables: `CREATE TEMP TABLE _magic_rdf_type_bf (o BIGINT PRIMARY KEY)`
- Modified rules compile to `INSERT … SELECT … JOIN _magic_rdf_type_bf m ON m.o = t.o`
- Semi-naive delta variants include magic table joins
- After inference completes, magic temp tables are dropped

**Integration point**: Datalog views (§15) already provide a goal pattern — this is exactly the input magic sets needs. `create_datalog_view()` will automatically apply magic sets when the goal has bound constants.

**Expected impact**: 10×–1000× reduction in materialization time for selective goals on large datasets. On a 10M-triple dataset with RDFS rules, deriving types for a single entity takes ~5ms with magic sets vs. ~5s for full closure.

**References**:
- Bancilhon, F., Maier, D., Sagiv, Y., Ullman, J. D. (1986). "Magic sets and other strange ways to implement logic programs." PODS '86.
- Ullman, J. D. (1989). "Bottom-up beats top-down for Datalog." PODS '89.
- Beeri, C., Ramakrishnan, R. (1991). "On the power of magic." Journal of Logic Programming.

### 14.2.2 Cost-Based Body Atom Reordering (v0.29.0)

Currently, body atoms in a rule are joined in the order they appear in the rule text. This can be highly suboptimal: if the first atom matches 1M rows and the second matches 100, the join produces a massive intermediate result.

**Algorithm**:

1. At rule compilation time, query `pg_class.reltuples` for each VP table referenced by a body atom.
2. For atoms with bound constants, estimate selectivity as `1 / n_distinct` (from `pg_statistic`).
3. Sort body atoms by estimated selectivity (most selective first).
4. For atoms sharing variables with earlier atoms, prefer those that join on indexed columns (`(s,o)` or `(o,s)`).

**SQL impact**: changes the `FROM` / `JOIN` order in generated SQL. PostgreSQL's query planner performs its own join reordering, but providing a good initial order helps the planner (especially when `join_collapse_limit` is exceeded for rules with many body atoms).

**Expected impact**: 2×–10× speedup on rules with >3 body atoms and skewed predicate cardinalities.

### 14.2.3 Subsumption Checking (v0.29.0)

When multiple rules derive the same predicate, some rules may be *subsumed* — they can never produce a fact that isn't already produced by a more general rule.

**Example**:
```prolog
# Rule A (general): ?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .
# Rule B (specific): ?x rdf:type owl:Thing :- ?x rdf:type ?b, ?b rdfs:subClassOf owl:Thing .
```
Rule B is subsumed by Rule A — every fact B derives is also derived by A.

**Algorithm**:
1. For each pair of rules deriving the same predicate, check if one head is a substitution instance of the other.
2. If the body of the more general rule is a subset of the specific rule's body (modulo variable renaming), the specific rule is redundant.
3. Remove subsumed rules before SQL generation.

**Expected impact**: reduces the number of `INSERT … SELECT` statements per iteration. Particularly effective for OWL RL, where several rules overlap.

### 14.2.4 Difference-Join Negation (v0.29.0)

Replace correlated `NOT EXISTS` subqueries with anti-join patterns:

```sql
-- Current (NOT EXISTS):
WHERE NOT EXISTS (SELECT 1 FROM _pg_ripple.vp_15 m WHERE m.s = t.s)

-- Better (anti-join):
LEFT JOIN _pg_ripple.vp_15 m ON m.s = t.s
WHERE m.s IS NULL
```

PostgreSQL's planner often converts `NOT EXISTS` to an anti-join internally, but pre-empting this transformation ensures consistent behavior across planner versions and cost model edge cases.

**Expected impact**: 10–50% speedup on rules with negated body atoms on large base predicates.

### 14.2.5 Predicate-Filter Pushdown (v0.29.0)

Arithmetic guards (`?a > 18`, `STRLEN(?s) > 0`, `REGEX(…)`) currently appear in the outermost `WHERE` clause. If the filtered variable is bound by an early body atom, the filter can be pushed into that atom's `JOIN … ON` clause or immediately after it.

**Algorithm**:
1. For each comparison/string guard in the rule body, identify which body atom first binds the guard's variable.
2. Move the guard to immediately after that atom in the join order.
3. For range filters (`?a > 18`), combine with the VP table's B-tree index to get an index scan.

**SQL impact**: changes WHERE clause placement; may enable index-only scans on VP tables.

### 14.2.6 Delta Table Indexing (v0.29.0)

Semi-naive evaluation creates temporary delta tables (`_dl_delta_{pred_id}`) that are currently unindexed heap tables. For rules with many body atoms, joins against these tables degrade to sequential scans.

**Solution**: after each iteration populates the delta table, create a B-tree index on the columns used in subsequent joins (typically `s` or `o`). Use `CREATE INDEX CONCURRENTLY` or a simple `CREATE INDEX` (within the same transaction, concurrency is not needed).

**Trade-off**: index creation adds ~1ms per delta table per iteration. For rules that converge in ≤5 iterations, the overhead is negligible; for long-chain derivations (10+ iterations), the cumulative indexing cost is recouped by faster joins.

### 14.2.7 Incremental Maintenance — DRed Algorithm (Post-1.0)

The Delete-Rederive (DRed) algorithm handles incremental maintenance when base triples are deleted:

1. **Over-delete**: when a base triple is deleted, delete all derived triples that *might* depend on it (pessimistic).
2. **Re-derive**: re-evaluate rules to check if any over-deleted triples can be re-derived via alternative derivation paths.
3. **Commit**: the triples that survive re-derivation are kept; the rest are permanently deleted.

This avoids full re-materialization and is the standard algorithm used by RDFox and other production Datalog systems.

**SQL encoding**: step 1 uses the existing rule SQL with the deleted triple as a negative filter; step 2 re-runs the same rule SQL but restricted to the over-deleted set.

### 14.2.8 Worst-Case Optimal Joins (Post-1.0)

For rules with cyclic join patterns (e.g., triangle queries: `?x ?p ?y, ?y ?q ?z, ?z ?r ?x`), pairwise hash-joins can produce intermediate results of size $O(n^2)$ even when the final result is $O(n)$. Worst-case optimal (WCO) join algorithms like Leapfrog Triejoin achieve $O(n^{3/2})$ for triangle queries.

**PostgreSQL limitation**: WCO joins cannot be expressed as standard SQL `JOIN` operators. Implementation would require a custom scan node (using PostgreSQL's CustomScan API) or a Rust-side in-memory join that reads from VP table cursors.

**Applicability**: most RDF/OWL rules are acyclic (chain patterns). WCO joins matter primarily for graph analytics rules (triangle counting, community detection). Deferred to post-1.0.

### 14.2.9 Demand Transformation (Post-1.0)

A generalization of magic sets that handles more complex binding-pattern propagation, including cases where bindings flow in both directions through a rule body. The demand transformation:

1. Annotates each predicate occurrence with a *demand pattern* (which arguments will be bound when this predicate is evaluated).
2. Generates auxiliary "demand" predicates that capture the set of demanded bindings.
3. Rewrites rules to filter against demand predicates.

Unlike magic sets, demand transformation can handle rules where bindings are generated by intermediate body atoms (not just the goal). This makes it more effective for complex rule topologies like OWL RL with intersecting class hierarchies.

### 14.2.10 Tabling / Memoization (Post-1.0)

Inspired by XSB Prolog's tabling mechanism. When the same derived predicate is queried with the same binding pattern across multiple SPARQL queries or inference runs:

1. Check a **memo table** keyed by `(predicate_id, binding_hash)`.
2. If a cached result exists and the underlying VP tables haven't changed (checked via table modification counters from `pg_stat_user_tables`), return the cached result.
3. Otherwise, compute and cache.

**Subsumptive tabling**: if a previous query computed `ancestor(Alice, ?)` (all ancestors of Alice) and a new query asks `ancestor(Alice, Bob)`, the answer can be looked up from the cached result without re-evaluation.

### 14.2.11 Parallel Stratum Evaluation (Post-1.0)

Within a single stratum, multiple rules that derive *different* predicates are independent and can be evaluated in parallel. Use PostgreSQL background workers (one per rule or per group of rules) to execute INSERT … SELECT statements concurrently.

**Correctness constraint**: rules within a stratum that derive the *same* predicate must be serialized (or use `ON CONFLICT DO NOTHING` to handle concurrent inserts to the same delta table).

**Expected impact**: linear speedup proportional to the number of independent derived predicates per stratum. RDFS has ~5 independent rule groups in stratum 0; OWL RL has ~10.

### 14.2.12 Bounded-Depth Early Termination (Post-1.0)

Many real-world ontologies have bounded-depth class/property hierarchies. If the stratifier can determine (via SHACL shape constraints or user-provided annotations) that the maximum derivation chain depth is `d`, the fixpoint loop can terminate after `d` iterations without running the final empty-delta check.

**Integration with SHACL**: SHACL constraints like `sh:maxCount` on `rdfs:subClassOf` paths provide formal bounds on hierarchy depth. The Datalog compiler can read these constraints and set `max_iterations = d + 1`.

### 14.2.13 Compiled Rule Plans (Post-1.0)

Cache the generated SQL strings and their dictionary-encoded constants across `infer()` calls. Currently, every call to `infer()` re-parses rules, re-encodes constants, and re-generates SQL. With a plan cache:

1. On first `infer()`, generate and cache SQL plans keyed by `(rule_set, rule_hash)`.
2. On subsequent calls, reuse cached plans (invalidate if rules or prefixes change).
3. Optionally, use PostgreSQL's `PREPARE` to create named prepared statements for the hottest rules.

**Expected impact**: 50–80% reduction in `infer()` overhead for repeated calls on the same rule set.

### 14.2.14 Lattice-Based Datalog — Datalog^L (Post-1.0)

Extend the rule IR to support user-defined lattice types. Instead of deriving plain facts, rules can derive facts with *lattice values* that are combined using a monotone join operation:

```prolog
# Trust propagation with meet-semilattice:
?x ex:trustLevel ?t :- ?x ex:directTrust ?t .
?x ex:trustLevel (MIN ?t1 ?t2) :- ?x ex:knows ?y, ?y ex:trustLevel ?t1, ?x ex:trustLevel ?t2 .
```

Lattice operations (`MIN`, `MAX`, `UNION`, `INTERSECTION`) are monotone, so fixpoint computation is well-defined. This generalizes aggregation (Datalog^agg) and enables richer analytical rules.

**Inspired by**: Flix (monotone lattice Datalog), Datafun (functional Datalog on semilattices), LogicBlox (lattice predicates).

---

## 14.1 RDF-star Integration in Datalog (v0.10.0)

Builds on RDF-star / statement identifiers delivered in v0.4.0. Quoted triples and SIDs can appear in Datalog rule heads and bodies, enabling provenance rules, annotation propagation, and meta-reasoning.

### Quoted triples in rule bodies

```prolog
# Find all assertions made by Carol
?s ?p ?o :- << ?s ?p ?o >> ex:assertedBy ex:Carol .
```

The quoted triple `<< ?s ?p ?o >>` is resolved via the dictionary: the encoder looks up (or creates) a composite dictionary entry for the triple tuple, and the SQL compiler joins against the `_pg_ripple.quoted_triples` dictionary table to bind `?s`, `?p`, `?o`.

### Quoted triples in rule heads

```prolog
# Annotate every derived triple with its provenance rule
<< ?s ?p ?o >> ex:derivedBy ex:transitiveManagerRule :- ?s ex:indirectManager ?o .
```

The head's quoted triple `<< ?s ?p ?o >>` is dictionary-encoded at materialization time. The derived annotation triple uses the quoted triple's dictionary ID as its subject.

### Statement identifiers in rule bodies

```prolog
# Copy confidence annotations from base statements to derived statements
<< ?s ex:indirectManager ?z >> ex:confidence ?c :-
    ?s ex:manager ?y,
    << ?y ex:manager ?z >> ex:confidence ?c .
```

SIDs from the `i` column of VP tables can be referenced implicitly through quoted triple patterns. The SQL compiler resolves `<< ?y ex:manager ?z >>` to a dictionary lookup of the quoted triple, then joins against the annotation VP table.

### SQL compilation

Quoted triple patterns in rule bodies compile to a join against the quoted-triple dictionary:

```sql
-- << ?s ?p ?o >> ex:assertedBy ex:Carol
SELECT qt.s_id, qt.p_id, qt.o_id
FROM _pg_ripple.quoted_triple_dict qt
JOIN _pg_ripple.vp_{assertedBy_id} ann ON ann.s = qt.id
WHERE ann.o = {carol_id}
```

Quoted triple patterns in rule heads compile to a dictionary encode step before the VP table insert:

```sql
-- Encode the quoted triple, then insert the annotation
WITH new_qt AS (
    INSERT INTO _pg_ripple.quoted_triple_dict (s_id, p_id, o_id, hash)
    SELECT s, {indirectManager_id}, o, xxh3_128(s, {indirectManager_id}, o)
    FROM _pg_ripple.vp_{indirectManager_id}
    ON CONFLICT DO NOTHING
    RETURNING id, s_id, o_id
)
INSERT INTO _pg_ripple.vp_{derivedBy_id}_delta (s, o, g, source)
SELECT qt.id, {ruleIRI_id}, 0, 1
FROM new_qt qt
ON CONFLICT DO NOTHING
```

---

## 15. Datalog Views (Stream Tables for Datalog Queries)

The materialized execution mode (§7.1) creates one stream table per derived predicate, materializing the *full closure* of every rule. SPARQL views (pg_trickle.md §2.2) materialize the result of a SPARQL SELECT query. **Datalog views** fill the gap between these: they bundle a Datalog rule set with a goal pattern into a single, incrementally-maintained stream table that materializes only the facts relevant to the goal.

### 15.1 Motivation

| Approach | Scope | Write amplification | Read path |
|---|---|---|---|
| Materialized rules (§7.1) | Full closure — all derived triples | High for large rule sets | SPARQL over derived VP tables |
| SPARQL view over derived predicates | One SPARQL query | Low (only query result) | Table scan |
| **Datalog view** | Rules + goal — only relevant derivations | Low (goal-filtered) | Table scan |

Datalog views are the natural choice when:

- The user thinks in rules, not SPARQL — no context-switch needed
- Only a subset of the full closure is needed (goal-directed)
- Rules and query should be versioned and managed as one artifact
- Constraint monitoring needs a live violation stream from a specific rule set

### 15.2 Compilation Pipeline

```
Datalog rules + goal pattern
    │
    ▼  (existing rule parser → Rule IR)
Stratified program + goal atom
    │
    ▼  (existing SQL compiler — §6)
Goal-filtered SQL (WITH RECURSIVE + joins + WHERE for goal bindings)
    │
    ▼
pgtrickle.create_stream_table(name, query, schedule)
    │
    ▼
Stream table: incrementally maintained Datalog query result
```

The SQL compiler already produces the recursive CTE for a rule set (§6.2). The only addition is appending a `WHERE` clause that filters the outermost `SELECT` to the goal pattern's bound constants, and projecting only the goal's variables as named columns.

### 15.3 API Surface

```sql
-- Create a named, live-updating Datalog query result set
SELECT pg_ripple.create_datalog_view(
    name     => 'alice_managers',
    rules    => $$
        ?x ex:indirectManager ?z :- ?x ex:manager ?z .
        ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .
    $$,
    goal     => '?who ex:indirectManager ex:Alice .',
    schedule => '10s',
    decode   => FALSE  -- FALSE (recommended): keep integer IDs, thin decode view on top
);

-- Always-fresh result — simple table scan
SELECT * FROM alice_managers;

-- Drop when no longer needed
SELECT pg_ripple.drop_datalog_view('alice_managers');

-- List all registered Datalog views
SELECT * FROM pg_ripple.list_datalog_views();
```

Internally `create_datalog_view` runs:
1. Parse rules → Rule IR (existing parser)
2. Stratify → StratifiedProgram (existing stratifier)
3. Parse goal pattern → goal Atom
4. Dictionary-encode all constants in rules and goal (integer joins everywhere)
5. Compile rules to SQL, append goal filter as `WHERE` clause
6. Register entry in `_pg_ripple.datalog_views`
7. Call `pgtrickle.create_stream_table(name => …, query => …, schedule => …)`

### 15.4 Catalog Table

```sql
CREATE TABLE _pg_ripple.datalog_views (
    name          TEXT PRIMARY KEY,
    rules_text    TEXT NOT NULL,          -- original Datalog rule text
    goal_text     TEXT NOT NULL,          -- original goal pattern text
    rule_set      TEXT,                   -- optional rule set reference
    generated_sql TEXT NOT NULL,          -- SQL sent to pg_trickle
    schedule      TEXT NOT NULL,          -- e.g. '10s' or 'IMMEDIATE'
    decode        BOOLEAN NOT NULL,       -- TRUE = store decoded strings, FALSE = integer IDs
    stream_table  TEXT NOT NULL,          -- fully qualified stream table name
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 15.5 Goal Pattern Semantics

The goal pattern is a single triple (or quad) pattern in the same Datalog syntax as rule bodies:

- `?who ex:indirectManager ex:Alice .` — bound object, project subject
- `ex:Bob ex:indirectManager ?whom .` — bound subject, project object
- `?x ex:indirectManager ?y .` — fully unbound, project both (equivalent to materializing the full derived predicate)
- `GRAPH ex:trusted { ?x rdf:type ?c } .` — goal scoped to a named graph

Bound constants are dictionary-encoded and pushed into the `WHERE` clause of the outermost `SELECT`. Unbound variables become named columns in the stream table.

### 15.6 Using Built-in Rule Sets

Instead of providing inline rules, a Datalog view can reference a loaded rule set by name:

```sql
-- First, load the built-in RDFS rules (if not already loaded)
SELECT pg_ripple.load_rules_builtin('rdfs');

-- Create a view over RDFS-inferred types for a specific class
SELECT pg_ripple.create_datalog_view(
    name     => 'all_persons',
    rule_set => 'rdfs',              -- reference loaded rule set
    goal     => '?x rdf:type foaf:Person .',
    schedule => '10s'
);
```

When `rule_set` is provided, the `rules` parameter is omitted; the engine reads rules from `_pg_ripple.rules` for the named set.

### 15.7 Constraint Monitoring Views

Constraint rules (empty-head rules, §6.3 / §14) combine naturally with Datalog views. The goal is implicit — any satisfying assignment is a violation:

```sql
-- Live violation monitor: people who are their own manager
SELECT pg_ripple.create_datalog_view(
    name     => 'self_manager_violations',
    rules    => $$
        :- ?x ex:manager ?x .
    $$,
    schedule => 'IMMEDIATE'  -- catch violations within the same transaction
);

-- Any row in this table = a violation
SELECT * FROM self_manager_violations;
```

For constraint rules the goal is synthesized automatically: the body variables become the projected columns, and any satisfying row represents a violation.

### 15.8 Interaction with pg_trickle DAG

Datalog views participate in pg_trickle's DAG alongside SPARQL views and materialized derived predicates:

```
Base VP tables (CDC-tracked)
    │
    ├── Materialized derived VP tables (§7.1, if active)
    │       │
    │       ├── SPARQL views over derived predicates
    │       └── Datalog views referencing derived predicates
    │
    └── Datalog views over base predicates only
```

The DAG scheduler ensures correct refresh ordering: a Datalog view that references a materialized derived predicate refreshes after that predicate's stream table.

### 15.9 Relationship to SPARQL Views

SPARQL views and Datalog views share the same underlying infrastructure (pg_trickle stream tables, dictionary encode/decode, catalog management). The key difference is the input language:

| | SPARQL view | Datalog view |
|---|---|---|
| Input | SPARQL SELECT query | Datalog rules + goal pattern |
| Recursion | Explicit property paths (`+`, `*`) | Implicit via recursive rules |
| Negation | `NOT EXISTS` / `MINUS` | Stratified `NOT` |
| Typical user | Query authors, dashboard builders | Ontology engineers, rule authors |
| Rule bundling | Separate from inference rules | Self-contained: rules + query in one artifact |

Both view types are listed together via `pg_ripple.list_sparql_views()` and `pg_ripple.list_datalog_views()` and can coexist in the same pg_trickle DAG.

### 15.10 Future: Magic Sets Integration

When magic sets optimization is added (post-1.0), Datalog views become the natural integration point. A goal pattern provides exactly the "query" that magic sets needs to generate a goal-directed rewriting of the rule program. This would reduce materialization cost from full-closure to only the facts reachable from the goal — a significant improvement for large rule sets with selective goals.

---

## 16. Relationship to pg_trickle.md §2.6

This document **supersedes** pg_trickle.md §2.6 ("Inference Materialization"). The hard-coded `WITH RECURSIVE` stream tables for RDFS closures described there are a special case of the general Datalog engine described here. Section 2.6 should be updated to reference this document and note that built-in RDFS/OWL RL rule sets are the recommended approach.

---

## 17. Roadmap Placement

The Datalog engine fits between serialization (v0.9.0) and views (v0.11.0):

| Version | Deliverable |
|---|---|
| **v0.9.0** | Serialization, export, SPARQL CONSTRUCT/DESCRIBE |
| **v0.10.0** | **Datalog reasoning engine**: rule parser, stratifier, SQL compiler, built-in RDFS/OWL RL rule sets, arithmetic built-ins, constraint rules, SHACL-AF `sh:rule` bridge, on-demand mode, materialized mode (pg_trickle) |
| **v0.11.0** | Incremental SPARQL views, **Datalog views**, ExtVP stream tables |

---

## 18. Summary

A Datalog reasoning engine over pg_ripple transforms the triple store from a passive data store into an active knowledge base. Users load rules (standard RDFS/OWL RL or custom), and the engine derives new triples either on-demand (inline CTEs, no dependencies) or materialized (pg_trickle stream tables, incrementally maintained).

The engine compiles rules to the same integer-join SQL that the SPARQL→SQL translator produces, so derived VP tables are indistinguishable from base VP tables to the query engine. Rules are fully quad-aware: the graph term (`g`) is first-class in the rule IR, SQL output, and cycle detection. Variable graph terms (`?g`) unify across body and head atoms, enabling same-graph propagation, cross-graph merging, and provenance-tracking rules. The `rule_graph_scope` GUC controls default matching behaviour for rules that omit an explicit `GRAPH` clause.

The implementation is pure Rust, uses the existing dictionary encoder and VP table infrastructure, and requires no changes to the core storage engine. pg_trickle is optional (on-demand mode works without it) but recommended for production (materialized mode with incremental maintenance).
