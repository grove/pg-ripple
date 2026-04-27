# Advanced Inference: WCOJ, DRed, and Tabling

Three terms appear in the pg_ripple release notes — **worst-case optimal joins**, **DRed** (Decremental Re-evaluation), and **tabling** — that have no obvious meaning to a SQL or SPARQL practitioner. This page explains what each one does, when the engine uses it, and why it matters for your queries.

You do not need to configure any of them. They are automatic. But knowing they exist helps you understand why certain queries are fast, why retraction is safe, and why recursive queries do not loop infinitely.

---

## Worst-Case Optimal Joins (WCOJ)

### The problem

Classical binary join plans — the kind every SQL query planner uses — are suboptimal for certain triangle and clique patterns. Given three tables R, S, T:

```sql
-- Triangle query: find all A-B-C triples where all three pair-wise edges exist.
SELECT r.a, s.b, t.c
FROM   R r JOIN S s ON r.b = s.b
           JOIN T t ON s.c = t.c
           WHERE r.a = t.a;   -- closing the triangle
```

A binary join plan processes R⋈S first (potentially generating many intermediate rows), then joins with T. For dense triangle patterns, the intermediate result can be quadratically larger than the final output. The join is correct, but wasteful.

**Worst-case optimal joins** (specifically, the **Leapfrog Triejoin** algorithm) process all three relations simultaneously, interleaving the enumeration so the intermediate result is never larger than the final output. The result is *output-sensitive* performance: fast on sparse results, fast on dense results, never pathological.

### When pg_ripple uses it

WCOJ is used automatically for multi-predicate star patterns and triangle patterns in SPARQL. You trigger it without knowing:

```sparql
# Triangle pattern: Alice-knows-someone-who-created-something-that-Alice-rated.
SELECT ?mid ?thing WHERE {
    ex:alice foaf:knows ?mid .
    ?mid schema:creator ?thing .
    ex:alice ex:rated ?thing .
}
```

This is a triangle (Alice → knows → mid → creator → thing ← rated ← Alice). pg_ripple's SPARQL-to-SQL translator detects the pattern and emits a WCOJ plan rather than two binary joins.

### When to care

You almost never need to think about this. The relevant scenario: if you see a SPARQL query involving three or more VP tables in a cycle pattern performing significantly worse than expected, run `EXPLAIN SPARQL` and check that the plan shows a leapfrog node. If it shows a nested-loop join instead, the query optimizer may have missed the opportunity — file an issue with the query.

---

## DRed — Decremental Re-evaluation

### The problem

Materialised Datalog inference creates derived triples. When you **delete** a source triple, some derived triples may no longer be valid. Naïve re-derivation would mean rerunning the entire inference from scratch — expensive for large graphs.

**DRed** (Decremental Re-evaluation with Deletion) is a more efficient algorithm:

1. Mark the derived triples that *might* be invalid (the "over-delete" set).
2. Re-derive everything in the over-delete set from scratch, using only surviving source triples.
3. Re-insert anything that can still be derived.

The cost is proportional to the *affected sub-graph*, not the entire graph.

### In pg_ripple

DRed kicks in automatically whenever you call `sparql_update` with a `DELETE` or `DELETE WHERE` that removes a triple that is used as a body atom in an active rule set.

```sparql
# Delete a source triple.
DELETE DATA {
    <https://example.org/alice> foaf:knows <https://example.org/bob>
}
```

If any Datalog rule derives triples from `foaf:knows`, pg_ripple marks those derived triples for DRed re-evaluation and launches the algorithm. The derived triples that are still provable (via other paths) are kept; the ones that have no other derivation are removed.

### What this means for you

- **Deletes are safe.** You never have to manually reconcile derived triples with base triples after a deletion. The engine handles it.
- **Deletes are proportionally expensive.** The cost of a deletion is proportional to the number of derived triples that depended on the deleted fact, not on the total triple count. For well-factored rule sets, this is small.
- **Avoid manual derived-triple cleanup.** Do not write `DELETE WHERE` queries that target `_pg_ripple.vp_*` tables directly. The DRed bookkeeping uses the OID of the source triple; direct table manipulation bypasses it and can leave the derived set inconsistent.

---

## Tabling

### The problem

Recursive Datalog rules can loop. Given:

```
ancestor(X, Y) :- parent(X, Y) .
ancestor(X, Z) :- ancestor(X, Y), ancestor(Y, Z) .
```

A naïve evaluator would keep expanding `ancestor` forever if there is a cycle in the parent graph (a real risk with, for example, `owl:sameAs` or `skos:broader*`).

**Tabling** (also called *memoisation* or *SLG resolution*) avoids infinite loops by caching — *tabling* — intermediate results and suspending evaluation when a recursive call would revisit a goal already on the active stack.

### In pg_ripple

pg_ripple's Datalog evaluator uses a bottom-up semi-naïve iteration (SN) as its primary strategy. Tabling is used as a fallback for rule sets that cannot be safely stratified.

Under SN evaluation:

1. Start from the base facts.
2. Derive every new triple that the rules allow (the "delta" derivation).
3. Repeat with the new facts as seed until no new triples are produced (fixpoint).

The fixpoint condition ensures termination. Tabling is used during the fixpoint iteration to detect and break circular derivation chains that would otherwise prevent convergence.

### What this means for you

- **You can write mutually recursive rules safely.** pg_ripple does not require you to manually identify the fixed point.
- **Cycles in the data do not cause infinite loops.** `owl:sameAs` chains, `skos:broader` cycles, or `rdfs:subClassOf` diamond hierarchies are all handled.
- **Well-founded semantics for negation.** When a rule uses negation-as-failure (`NOT` in the body), pg_ripple uses well-founded semantics (WFS) to assign a third truth value ("unknown") to atoms that participate in cycles through negation. This is the most principled treatment of negation under recursion available in any Datalog system.

---

## How they compose

All three mechanisms operate together on the same query/rule execution:

| Mechanism | Role |
|---|---|
| WCOJ (Leapfrog Triejoin) | Speed up *join evaluation* for multi-way patterns |
| Tabling | Ensure *fixpoint termination* for recursive rules |
| DRed | Maintain *consistency* of derived triples after base-triple deletions |

None of them requires configuration. They are part of the engine's normal operation and are activated automatically based on query shape, rule structure, and update type.

---

## See also

- [Reasoning and Inference](reasoning-and-inference.md) — the full Datalog reference.
- [Lattice Datalog](lattice-datalog.md) — when your reasoning needs to propagate *values*, not just facts.
- [OWL 2 Profiles](owl-profiles.md) — the specific built-in rule sets.
- [SPARQL Query Debugger](../user-guide/explain-sparql.md) — inspect which join strategy the planner chose.
