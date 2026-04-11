# Datalog Reasoning Engine for pg_triple

## 1. Motivation

The current inference plan (implementation_plan.md §4.10.4, pg_trickle.md §2.6) hard-codes RDFS entailment as manually written `WITH RECURSIVE` stream tables for `rdfs:subClassOf` and `rdfs:subPropertyOf`. This covers two rules out of a potential universe of hundreds.

A general-purpose Datalog engine subsumes and generalizes all of these:

| Approach | What it covers | Flexibility |
|---|---|---|
| Hard-coded RDFS closures (current plan) | `rdfs:subClassOf`, `rdfs:subPropertyOf` only | None — hand-written SQL |
| OWL RL profile | ~80 entailment rules from the W3C spec | Fixed rule set |
| Datalog engine | All of the above + arbitrary user-defined rules | Fully extensible |

A quad `(s, p, o, g)` is how pg_triple actually stores data: every VP table carries `(s BIGINT, o BIGINT, g BIGINT)` where `g` is the dictionary-encoded named graph IRI (0 = default graph). Datalog rules over this quad structure are exactly how RDFS, OWL RL, and custom domain rules are formally specified. The graph dimension is first-class — rules can read from and write into specific named graphs, propagate facts across graphs, or operate graph-agnostically.

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
    catalog.rs      — _pg_triple.rules table CRUD
```

---

## 3. Rule Syntax

A Turtle-flavoured Datalog notation that reuses the prefix registry already in pg_triple. Each rule is a line of the form `head :- body .` where head and body are triple patterns with variables (`?x`) and constants (prefixed IRIs or literals).

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

**Default behaviour when `GRAPH` is omitted**: controlled by the GUC `pg_triple.rule_graph_scope`:
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

All SQL generation follows pg_triple's core design constraint: **integer joins everywhere**. All constants in rule bodies are dictionary-encoded before SQL generation. Derived VP tables use the same `(s BIGINT, o BIGINT, g BIGINT)` schema as base VP tables.

### 6.1 Non-recursive rules

Each rule compiles to a single `INSERT … SELECT`:

```sql
-- Rule: ?y ?p ?x :- ?x ?p ?y, ?p rdf:type owl:SymmetricProperty .
-- (where rdf:type = 7, owl:SymmetricProperty = 201)
INSERT INTO _pg_triple.vp_{p_id}_delta (s, o, g)
SELECT t1.o, t1.s, t1.g
FROM _pg_triple.vp_{p_id} t1
JOIN _pg_triple.vp_7 t2 ON t2.s = t1.p AND t2.o = 201
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
    FROM _pg_triple.vp_42           -- ex:manager
  UNION
    -- Recursive step
    SELECT m.s, im.o, m.g
    FROM _pg_triple.vp_42 m         -- ex:manager
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
    SELECT s, o, g FROM _pg_triple.vp_42   -- ex:manager (all graphs)
  UNION
    SELECT m.s, im.o, m.g
    FROM _pg_triple.vp_42 m
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

INSERT INTO _pg_triple.vp_derived_50_delta (s, o, g)
SELECT t.s, 301, t.g
FROM _pg_triple.vp_7 t
WHERE t.o = 99                    -- foaf:Person
  AND NOT EXISTS (
      SELECT 1 FROM _pg_triple.vp_15 m WHERE m.s = t.s
  )
ON CONFLICT DO NOTHING
```

### 6.4 Star patterns in rule bodies

When multiple body atoms share the same subject variable, the compiler generates a single join chain (consistent with the SPARQL→SQL star-pattern optimization):

```sql
-- Rule: ?x ex:eligible "true" :- ?x rdf:type ex:Employee, ?x ex:age ?a, ?a > 18 .
SELECT t1.s, 501, t1.g
FROM _pg_triple.vp_7 t1             -- rdf:type
JOIN _pg_triple.vp_55 t2 ON t2.s = t1.s   -- ex:age
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
    name     => '_pg_triple.vp_derived_43',
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
    SELECT s, o, g FROM _pg_triple.vp_42
  UNION
    SELECT m.s, im.o, m.g
    FROM _pg_triple.vp_42 m
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
SET pg_triple.inference_mode = 'materialized';  -- default when pg_trickle is present
SET pg_triple.inference_mode = 'on_demand';      -- default when pg_trickle is absent
SET pg_triple.inference_mode = 'off';            -- disable inference entirely

-- Graph scope for unscoped body atoms (atoms without an explicit GRAPH clause)
SET pg_triple.rule_graph_scope = 'default';  -- match only g = 0 (recommended)
SET pg_triple.rule_graph_scope = 'all';      -- match triples in any graph
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
SELECT pg_triple.load_rules_builtin('rdfs');

-- Load OWL RL rules (includes RDFS as stratum 0)
SELECT pg_triple.load_rules_builtin('owl-rl');

-- View loaded rules
SELECT * FROM pg_triple.list_rules();
```

---

## 9. Catalog Tables

### 9.1 Rule storage

```sql
CREATE TABLE _pg_triple.rules (
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

Derived predicates are registered alongside base predicates in `_pg_triple.predicates` with a `derived` flag:

```sql
ALTER TABLE _pg_triple.predicates ADD COLUMN derived BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE _pg_triple.predicates ADD COLUMN rule_set TEXT;
```

This allows the SPARQL engine to look up derived VP tables the same way it looks up base VP tables — no special handling in the query planner.

---

## 10. API Surface

```sql
-- Load rules from Datalog text
SELECT pg_triple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom');

-- Load a built-in rule set
SELECT pg_triple.load_rules_builtin(name TEXT);  -- 'rdfs' | 'owl-rl'

-- Materialize all derived predicates as pg_trickle stream tables
SELECT pg_triple.materialize_rules(schedule TEXT DEFAULT '10s');

-- List active rules
SELECT * FROM pg_triple.list_rules();
-- Returns: id, rule_set, rule_text, head_pred_iri, stratum, is_recursive, created_at

-- Drop rules by rule set name
SELECT pg_triple.drop_rules(rule_set TEXT);

-- Drop all rules and derived tables
SELECT pg_triple.drop_all_rules();

-- Set inference mode
SET pg_triple.inference_mode = 'materialized' | 'on_demand' | 'off';
```

---

## 11. Interaction with Existing Components

### 11.1 SPARQL engine

The query translation engine needs one addition: when `pg_triple.inference_mode != 'off'` and a query references a derived predicate:

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

A SPARQL view created via `pg_triple.create_sparql_view()` can reference derived predicates. If the derived predicates are materialized, the SPARQL view's stream table depends on them in pg_trickle's DAG — refresh order is automatic. If on-demand, the SPARQL view's SQL includes the inlined CTEs.

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

## 14. Included in v0.9.0, Limitations, and Future Work

### Arithmetic built-ins (v0.9.0)

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
INSERT INTO _pg_triple.vp_derived_88_delta (s, o, g)
SELECT t1.s, 301, t1.g
FROM _pg_triple.vp_7 t1      -- rdf:type
JOIN _pg_triple.vp_55 t2 ON t2.s = t1.s   -- ex:age
WHERE t1.o = 200              -- ex:Employee
  AND t2.o >= 60              -- arithmetic filter
ON CONFLICT DO NOTHING
```

### Constraint rules — integrity constraints (v0.9.0)

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
    SELECT 1 FROM _pg_triple.vp_42 WHERE s = o  -- self-loop
) AS violated;
```

**Execution modes**:

- **Materialized (pg_trickle)**: each constraint rule becomes a stream table; any row in the table = a violation. With `IMMEDIATE` refresh, violations are caught within the same transaction as the DML. This directly complements and extends SHACL validation.
- **On-demand**: `pg_triple.check_constraints()` runs all constraint queries and returns violations as JSONB.
- **Enforcement**: `pg_triple.enforce_constraints = 'error' | 'warn' | 'off'` GUC controls behaviour on insert — reject the transaction (`error`), log a warning (`warn`), or do nothing (`off`).

Catalog: constraint rules are stored in `_pg_triple.rules` with `head_pred = NULL` to distinguish them from derivation rules.

API:

```sql
-- Check all constraints, return violations
SELECT * FROM pg_triple.check_constraints();
-- Returns: rule_id, rule_text, violating_subjects (BIGINT[]), violation_count
```

### Initial release limitations (v0.9.0)

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
| Rule provenance (why-provenance) | Track which base quads caused each derived quad in a parallel `_pg_triple.rule_provenance` table. `pg_triple.explain_derivation(s, p, o)` returns a derivation tree. Critical for trust and debugging. | 1 | Post-1.0 |
| Magic sets optimization | Goal-directed evaluation: only derive facts relevant to a specific query, reducing materialization cost for large rule sets. Well-studied SQL encoding. | 1 | Post-1.0 |
| Incremental rule updates | Add/remove individual rules without recomputing the entire program. Requires dependency-aware invalidation of affected strata only. | 1 | Post-1.0 |
| Graph analytics rules | Shortest paths, connected components, PageRank expressed as recursive Datalog rules with aggregation. Requires Datalog^agg. Maps to `WITH RECURSIVE` + aggregate window functions. | 2 | Post-1.0 |
| Existential rules (Datalog+/−) | Existentially quantified variables in rule heads → Skolem blank node generation. Extends coverage from OWL RL to OWL DL subset. Well-understood but non-trivial implementation. | 2 | Post-1.0 |
| Temporal Datalog | Rules over time-stamped quads with temporal operators (`BEFORE`, `AFTER`, `DURING`). Aligns with ROADMAP v1.3 (TimescaleDB integration). | 2 | Post-1.0 |
| Well-founded semantics | Three-valued model (true/false/unknown) for non-stratifiable programs. More permissive than stratification for cyclic ontologies with defaults. Known SQL encoding via iterative fixpoint. | 2 | Post-1.0 |
| Multi-head rules | Syntactic sugar: single rule body → multiple head atoms. Desugars to multiple single-head rules at compile time. Low implementation cost. | 3 | Post-1.0 |
| Rule priorities / defeasible logic | Priority ordering for contradictory derived facts. Standard in Description Logic reasoners. Complex semantics but important for ontology merging. | 3 | Post-1.0 |
| Active rules (ECA) | Event-condition-action rules that trigger side-effects (`NOTIFY`, function calls) rather than deriving quads. Breaks pure declarative model; maps to PG `NOTIFY` + triggers. | 3 | Post-1.0 |
| Probabilistic rules | Weighted rules for uncertain reasoning (e.g., link prediction). Requires probability propagation semantics (ProbLog-style). | 3 | Post-1.0 |
| SWRL integration | Semantic Web Rule Language as an alternative rule syntax. Turtle-based; maps to the same IR. | 3 | Post-1.0 |

---

## 15. Relationship to pg_trickle.md §2.6

This document **supersedes** pg_trickle.md §2.6 ("Inference Materialization"). The hard-coded `WITH RECURSIVE` stream tables for RDFS closures described there are a special case of the general Datalog engine described here. Section 2.6 should be updated to reference this document and note that built-in RDFS/OWL RL rule sets are the recommended approach.

---

## 16. Roadmap Placement

The Datalog engine fits between serialization (v0.8.0) and SPARQL views (v0.10.0):

| Version | Deliverable |
|---|---|
| **v0.8.0** | Serialization, export, SPARQL CONSTRUCT/DESCRIBE |
| **v0.9.0** | **Datalog reasoning engine**: rule parser, stratifier, SQL compiler, built-in RDFS/OWL RL rule sets, arithmetic built-ins, constraint rules, on-demand mode, materialized mode (pg_trickle) |
| **v0.10.0** | Incremental SPARQL views, ExtVP stream tables |

---

## 17. Summary

A Datalog reasoning engine over pg_triple transforms the triple store from a passive data store into an active knowledge base. Users load rules (standard RDFS/OWL RL or custom), and the engine derives new triples either on-demand (inline CTEs, no dependencies) or materialized (pg_trickle stream tables, incrementally maintained).

The engine compiles rules to the same integer-join SQL that the SPARQL→SQL translator produces, so derived VP tables are indistinguishable from base VP tables to the query engine. Rules are fully quad-aware: the graph term (`g`) is first-class in the rule IR, SQL output, and cycle detection. Variable graph terms (`?g`) unify across body and head atoms, enabling same-graph propagation, cross-graph merging, and provenance-tracking rules. The `rule_graph_scope` GUC controls default matching behaviour for rules that omit an explicit `GRAPH` clause.

The implementation is pure Rust, uses the existing dictionary encoder and VP table infrastructure, and requires no changes to the core storage engine. pg_trickle is optional (on-demand mode works without it) but recommended for production (materialized mode with incremental maintenance).
