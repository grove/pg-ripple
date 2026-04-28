[← Back to Blog Index](README.md)

# Datalog Inside PostgreSQL

## Automatic fact derivation from rules — RDFS, OWL RL, and custom reasoning

---

You have a knowledge graph with explicit facts: "Alice is a Manager. Managers are Employees. Employees are People." A user queries "find all People." If your triple store only knows what you told it literally, the query returns nothing — because nobody explicitly said "Alice is a Person."

This is the inference problem. The information is there — it's implied by the chain of subclass relationships — but it's not materialized. You need a system that can derive "Alice is a Person" from the rules and facts you've already declared.

pg_ripple solves this with Datalog, a logic programming language that runs inside PostgreSQL, materializing inferred facts as real triples that participate in SPARQL queries.

---

## Why Not Just Use SPARQL?

You can express inference-like queries in SPARQL using property paths:

```sparql
SELECT ?person WHERE {
  ?person rdf:type/rdfs:subClassOf* ex:Person .
}
```

This works. But it has two problems:

1. **Performance.** The property path `rdfs:subClassOf*` is evaluated at query time, every time. For complex ontologies with hundreds of classes and deeply nested hierarchies, this adds hundreds of milliseconds to every query.

2. **Completeness.** RDFS and OWL RL have dozens of entailment rules, not just subclass transitivity. There's domain/range inference, property chain axioms, class equivalence, inverse properties, and more. Encoding all of these as SPARQL property paths is theoretically possible but practically unmaintainable.

Datalog gives you a rule language designed for exactly this kind of recursive, rule-based inference — and it runs it once, storing the results so every subsequent query is a simple lookup.

---

## Datalog in 60 Seconds

Datalog is a subset of Prolog without function symbols. Rules look like this:

```
head(X, Y) :- body1(X, Z), body2(Z, Y).
```

This reads: "X is related to Y by `head` if X is related to Z by `body1` and Z is related to Y by `body2`."

Rules are declarative. You state what's true, not how to compute it. The Datalog engine figures out the how.

A complete example for RDFS subclass inference:

```
% If X is of type C, and C is a subclass of D, then X is of type D.
rdf_type(X, D) :- rdf_type(X, C), rdfs_subClassOf(C, D).

% Subclass is transitive.
rdfs_subClassOf(X, Z) :- rdfs_subClassOf(X, Y), rdfs_subClassOf(Y, Z).
```

With these two rules and the facts "Alice rdf:type Manager" and "Manager rdfs:subClassOf Employee" and "Employee rdfs:subClassOf Person", the engine derives:

- `Manager rdfs:subClassOf Person` (transitivity)
- `Alice rdf:type Employee` (type inference through Manager → Employee)
- `Alice rdf:type Person` (type inference through Manager → Employee → Person)

These derived facts are stored as regular triples (with `source = 1` to mark them as inferred). SPARQL queries see them alongside explicit triples.

---

## How pg_ripple Runs Datalog

pg_ripple's Datalog engine compiles rules to SQL and executes them using PostgreSQL's query executor. The compilation is straightforward:

```
rdf_type(X, D) :- rdf_type(X, C), rdfs_subClassOf(C, D).
```

Becomes:

```sql
INSERT INTO _pg_ripple.vp_{rdf_type} (s, o, g, source)
SELECT t.s, sc.o, t.g, 1   -- source=1 marks inferred
FROM _pg_ripple.vp_{rdf_type}      t
JOIN _pg_ripple.vp_{rdfs_subClassOf} sc ON sc.s = t.o
ON CONFLICT DO NOTHING;
```

This is a SQL INSERT-SELECT that joins two VP tables and inserts the results. PostgreSQL's optimizer handles the join planning, index selection, and parallel execution.

### Semi-Naive Evaluation

The naive approach — run all rules repeatedly until no new facts appear — recomputes everything at every iteration. If iteration 3 produces 500 new facts, iteration 4 re-joins the entire table to find them, including all the facts from iterations 1 and 2 that were already processed.

Semi-naive evaluation avoids this. It tracks the *delta* — the new facts from each iteration — and joins only the delta against the full table. This reduces the work from $O(n^2)$ per iteration to $O(\Delta \times n)$ where $\Delta$ is the new facts per iteration.

pg_ripple implements semi-naive evaluation since v0.24.0. Each rule maintains a delta table, and the SQL for each iteration joins the delta against the base:

```sql
-- Only join new rdf:type facts against rdfs:subClassOf
INSERT INTO delta_rdf_type (s, o, g, source)
SELECT d.s, sc.o, d.g, 1
FROM delta_rdf_type_prev d
JOIN _pg_ripple.vp_{rdfs_subClassOf} sc ON sc.s = d.o
ON CONFLICT DO NOTHING;
```

---

## Stratified Negation

Pure Datalog is monotone — adding facts never removes conclusions. But OWL RL includes non-monotone rules like disjointness constraints:

```
% If X is of type C and C is disjoint with D, then X is NOT of type D.
% (Or rather: flag a contradiction if X is of type both C and D.)
```

Handling negation in Datalog requires stratification: organize rules into layers (strata) where:
1. Rules without negation run in lower strata.
2. Rules with negation run in higher strata, after the facts they negate are fully computed.

pg_ripple's stratifier analyzes the rule dependency graph, detects negation cycles (which are rejected — they have no well-defined semantics under standard stratification), and orders the strata for correct evaluation.

Since v0.32.0, pg_ripple also supports well-founded semantics for cyclic programs with negation, using a three-valued logic (true/false/unknown) that gives a unique, well-defined answer even for circular negation.

---

## The Built-In Rule Sets

pg_ripple ships with built-in rule sets for standard ontology languages:

### RDFS

13 entailment rules covering:
- Subclass transitivity
- Subproperty transitivity
- Domain and range inference
- Type propagation through subclass/subproperty chains

### OWL 2 RL

~80 rules covering:
- Class equivalence, disjointness, intersection, union
- Property transitivity, symmetry, reflexivity, inverse
- Property chain axioms
- Has-value restrictions
- Same-as and different-from

You activate them with:

```sql
-- Load RDFS rules
SELECT pg_ripple.datalog_load_ruleset('rdfs');

-- Load OWL 2 RL rules
SELECT pg_ripple.datalog_load_ruleset('owl2rl');

-- Run inference
SELECT pg_ripple.datalog_infer();
```

After inference, SPARQL queries automatically see the derived triples.

---

## Custom Rules

Beyond standard entailment, you can define domain-specific rules:

```sql
-- Transitive manager chain
SELECT pg_ripple.datalog_add_rule(
  'manages(X, Z) :- manages(X, Y), manages(Y, Z).'
);

-- Conflict of interest: employee reports to spouse
SELECT pg_ripple.datalog_add_rule(
  'conflict_of_interest(X, Y) :- manages(X, Y), spouse_of(X, Y).'
);

-- Infer
SELECT pg_ripple.datalog_infer();
```

After inference, you can query derived predicates with SPARQL:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?manager ?subordinate WHERE {
    ?manager ex:conflict_of_interest ?subordinate .
  }
');
```

---

## Incremental Maintenance with DRed

When base facts change (triples are inserted or deleted), the materialized inferences need to be updated. Recomputing everything from scratch is correct but expensive.

pg_ripple uses Delete-and-Rederive (DRed) for incremental maintenance (since v0.34.0):

1. When a base triple is deleted, identify all inferred triples that *might* depend on it.
2. Tentatively delete those inferred triples.
3. Re-derive from the remaining base facts to check which tentatively deleted triples are still valid (reachable through alternative derivation paths).
4. Restore the still-valid triples; remove the ones that are truly gone.

This is more complex than full recomputation but much cheaper for small changes. Deleting one triple from a 10-million-triple graph with 500,000 inferred facts typically affects fewer than 100 inferred triples — DRed processes those 100, not the full 500,000.

---

## Parallel Stratum Evaluation

Since v0.35.0, independent Datalog strata (groups of rules with no dependencies between them) can be evaluated in parallel using background workers. For an ontology with 5 independent stratum groups, this gives up to a 5× speedup on inference.

The parallelization is safe because strata are, by definition, independent — no rule in one stratum depends on a rule in another stratum within the same group. Each worker runs its stratum's rules to fixpoint, and the results are merged after all workers complete.

---

## When Datalog Is Overkill

If your graph doesn't have an ontology — no `rdfs:subClassOf`, no `owl:TransitiveProperty`, no custom inference rules — you don't need Datalog. SPARQL property paths handle ad-hoc transitive queries efficiently enough, and the overhead of materializing inferences for a schema-less graph isn't justified.

Datalog shines when:
- You have a rich ontology (RDFS, OWL RL, or custom domain rules).
- The same inferences are queried repeatedly.
- Completeness matters — you need *all* entailed facts, not just the ones reachable from a specific query.
- You want to enforce integrity constraints (via negation rules that flag contradictions).

For most knowledge graph applications — healthcare ontologies, product taxonomies, compliance graphs, master data management — this is exactly the case.
