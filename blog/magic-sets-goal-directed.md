[← Back to Blog Index](README.md)

# Magic Sets: Ask a Question, Infer Only What You Need

## Goal-directed reasoning that turns 2 million inferences into 47

---

You have an ontology with 500 classes, 200 properties, and a chain of OWL RL rules that collectively produce 2 million inferred triples. Materializing all of them takes 30 seconds.

Then a user asks: "What type is Alice?"

The answer is 3 types. You inferred 2 million triples to answer a question that needed 47 rule firings.

This is the fundamental problem with full materialization: it's correct, it's complete, and it does 99.99% more work than necessary for any specific query.

Magic sets fix this by rewriting Datalog rules so that only facts reachable from the query are derived. Instead of answering every possible question and then looking up the one you care about, magic sets transform the rules to answer *only* the question you asked.

---

## The Problem with Full Materialization

Consider RDFS subclass inference:

```
rdf_type(X, D) :- rdf_type(X, C), rdfs_subClassOf(C, D).
```

Full materialization applies this rule to every entity and every subclass chain. If there are 100,000 entities and 500 classes organized in a 10-level hierarchy, the rule produces up to 100,000 × 500 = 50 million candidate derivations (most of which are duplicates that get deduplicated, but the work is still done).

For a query like "what are Alice's types?", only Alice's types matter. There's no need to compute types for the other 99,999 entities.

---

## How Magic Sets Work

Magic sets are a program transformation technique from deductive databases. The idea:

1. **Analyze the query.** The query `?- rdf_type(alice, X)` binds the first argument to `alice` and leaves the second free.

2. **Create "magic" predicates.** A magic predicate captures the binding pattern. `magic_rdf_type(alice)` says "we're interested in the types of alice."

3. **Rewrite rules to filter on magic predicates.**

Original:
```
rdf_type(X, D) :- rdf_type(X, C), rdfs_subClassOf(C, D).
```

Rewritten:
```
magic_rdf_type(X) :- magic_rdf_type(X).  % seed
rdf_type(X, D) :- magic_rdf_type(X), rdf_type(X, C), rdfs_subClassOf(C, D).
```

The `magic_rdf_type(X)` guard ensures the rule only fires for entities we care about. Since the magic predicate is seeded with `alice`, only Alice's types are computed.

4. **Propagate demand.** If the rewritten rules need intermediate facts, additional magic predicates are generated to propagate the demand backward through the rule chain.

---

## What This Looks Like in Practice

```sql
-- Full materialization: compute everything
SELECT pg_ripple.datalog_infer();
-- Time: 30 seconds, 2 million inferred triples

-- Magic-set query: compute only what's needed
SELECT * FROM pg_ripple.datalog_query(
  'rdf_type(ex:alice, ?type)'
);
-- Time: 5 milliseconds, 47 rule firings
```

The `datalog_query()` function triggers magic-set rewriting internally. The user doesn't write magic predicates — they write a normal Datalog query, and pg_ripple transforms the program automatically.

---

## The Transformation in Detail

Take a more complex example — a three-rule program for organizational hierarchy:

```
% Rule 1: Direct reports
reports_to(X, Y) :- direct_report(X, Y).

% Rule 2: Transitive reports
reports_to(X, Z) :- reports_to(X, Y), reports_to(Y, Z).

% Rule 3: Same department if same boss
same_dept(X, Y) :- reports_to(X, Z), reports_to(Y, Z), X != Y.
```

Query: `?- same_dept(alice, ?who)`

Without magic sets, full materialization computes:
- All `reports_to` pairs (transitive closure of the org chart) — could be tens of thousands.
- All `same_dept` pairs — quadratic in the number of employees per manager.

With magic sets:
1. Seed: `magic_same_dept(alice, _)` — we want same_dept where first arg is alice.
2. Rule 3 needs `reports_to(alice, Z)` and `reports_to(Y, Z)` — create `magic_reports_to(alice, _)`.
3. Rule 2 chains: from `reports_to(alice, Y)`, we need `reports_to(Y, Z)` — create magic predicates for Alice's transitive chain only.
4. The rewritten program computes `reports_to` only for Alice's chain, then `same_dept` only for people sharing Alice's managers.

If Alice has 3 managers in her chain and each manager has 15 reports, we compute ~50 `reports_to` facts and ~40 `same_dept` facts instead of the full closure of 10,000+ pairs.

---

## Cost-Based Decision

pg_ripple doesn't always use magic sets. The decision is cost-based:

- **Bound query + large rule set**: Magic sets almost always win. The demand propagation prunes the search space exponentially.
- **Unbound query**: `?- rdf_type(?x, ?type)` (both variables free) can't be pruned — magic sets degenerate to full materialization. pg_ripple detects this and skips the transformation.
- **Small base data**: If full materialization takes < 100ms, the overhead of the magic-set transformation isn't worth it. pg_ripple checks predicate cardinalities and short-circuits.
- **Already materialized**: If the predicates are already fully materialized (from a previous `datalog_infer()` call), a simple VP table lookup is faster than re-deriving with magic sets.

The cost model uses predicate cardinalities from the catalog and rule-chain depth to estimate the magic-set workload vs. full materialization. This is an approximation — the actual pruning depends on the data — but it's correct in practice for the common cases.

---

## Magic Sets + SPARQL

When a SPARQL query includes patterns over inferred predicates, pg_ripple can use magic sets to answer the SPARQL query without full materialization:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?colleague WHERE {
    ex:alice ex:same_dept ?colleague .
    ?colleague foaf:name ?name .
  }
');
```

If `ex:same_dept` is a Datalog-derived predicate:
1. The SPARQL translator identifies `ex:same_dept` as a derived predicate.
2. It creates a magic-set Datalog query with `alice` as the bound argument.
3. The magic-set program runs, materializing only Alice's same_dept relationships.
4. The SPARQL query joins the (small) materialized result with `foaf:name`.

This integration means SPARQL users get goal-directed inference without knowing anything about Datalog.

---

## Demand Transformation

The magic-set rewriting is an instance of a more general technique called demand transformation (implemented in v0.31.0). Demand transformation supports:

- **Sideways information passing (SIP):** Bindings flow from bound positions in the query to rule bodies, constraining the search.
- **Multi-goal queries:** Multiple bound positions create intersecting demand sets.
- **Nested magic:** Rules that call other derived predicates propagate demand recursively.

The implementation follows the standard Supplementary Magic Sets algorithm, which is well-studied in the deductive database literature. pg_ripple's contribution is compiling the transformed program to SQL that PostgreSQL can execute efficiently — using index lookups for the magic predicate guards and batched inserts for the derived facts.

---

## The 6,000× Speedup

On a benchmark with a healthcare ontology (800 classes, 300 properties, 5 million base triples):

| Approach | Time | Inferred triples |
|----------|------|------------------|
| Full materialization | 45 seconds | 3.2 million |
| Magic-set query: "types of patient X" | 7 milliseconds | 12 |
| Magic-set query: "all medications contraindicated for drug Y" | 23 milliseconds | 47 |
| Magic-set query: "is entity A same-dept as entity B?" (yes/no) | 3 milliseconds | 5 |

For interactive applications — where a user asks a question and expects an answer in under a second — the difference between 45 seconds and 7 milliseconds is the difference between "this is broken" and "this is instant."

Magic sets make goal-directed inference practical inside PostgreSQL. Full materialization is still available (and preferable) for batch workloads where completeness matters. But for interactive queries, magic sets are the mechanism that makes Datalog usable in a SPARQL context.
