# Lattice Datalog — When and Why

The [Lattice Datalog reference](../reference/lattice-datalog.md) documents every function and GUC. This page answers a different question: **should you reach for a lattice at all?** Most users never need one. This page helps you decide.

---

## The short answer

Use a lattice when you need to propagate a *value* along a graph edge — not just *whether* a node is reachable, but *how much* something is worth at that node — and the propagation is **recursive**.

If your rules are not recursive, standard Datalog aggregates (`COUNT`, `SUM`, `AVG` over strata) are simpler.

---

## A concrete intuition

Standard Datalog asks: *can you get from A to B?*

Lattice Datalog asks: *what is the best way to get from A to B?*

The same graph, two different questions:

```
A ──0.9──► B ──0.85──► C ──0.95──► D
```

| Question | Answer | Tool |
|---|---|---|
| Is D reachable from A? | yes | Standard Datalog |
| What is the maximum single-hop weight on any path to D? | 0.95 | Standard Datalog + MAX aggregate (non-recursive) |
| What is the minimum weight along the best path from A to D? | 0.85 (the bottleneck) | **Lattice Datalog with `min`** |
| What is the product of weights along the best path from A to D? | 0.726 | **Custom lattice** |
| Which intermediate nodes does every path from A to D pass through? | {B, C} | **SetLattice** |

The moment "best path" is recursive — you don't know in advance which direction is best — you need a lattice.

---

## Choosing the right built-in lattice

### `min` — weakest-link reasoning

```
The strength of a chain is the strength of its weakest link.
```

Use `min` when:
- Propagating **trust / confidence** through a network (the result is only as trustworthy as the least-trusted step).
- Computing **shortest path** where the path cost is the maximum edge weight.
- Finding **bottleneck capacity** in a flow network (the flow is limited by the narrowest pipe).

```sql
-- Load rules (trust propagates, bottlenecked by the weakest hop).
SELECT pg_ripple.load_rules($RULES$
?x ex:trusts ?y :- ?x ex:directlyTrusts ?y .
?x ex:trusts ?z :- ?x ex:directlyTrusts ?y, ?y ex:trusts ?z .
@lattice ex:trusts confidence min .
$RULES$, 'trust');

SELECT pg_ripple.infer_lattice('trust', 'min');
```

### `max` — best-case reasoning

Use `max` when:
- Propagating **reputation / endorsement** scores where having one highly-rated connection is enough.
- Finding the **longest path weight** in a DAG.
- Any "optimistic" or "best evidence wins" scenario.

```sql
@lattice ex:endorses score max .
```

### `set` — provenance and multi-valued reasoning

Use `set` when:
- You need to track **which source triples** justify a derived fact (provenance semiring).
- You need the **union of all witnesses** along all derivation paths, not just one.
- Each node collects contributions from multiple parents and you need all of them.

```sql
-- Collect the set of all papers that support a hypothesis.
@lattice ex:supports evidence set .
```

Note: set-lattice results can be large. Consider a maximum set size or a bloom-filter approximation for large graphs.

### `interval` — when truth has a time range

Use `interval` when reasoning about temporal overlap: *"A and B are both true during the period when both of their valid intervals overlap"*.

```sql
-- Derived fact is valid only during the intersection of the body's intervals.
@lattice ex:validDuring interval interval .
```

---

## Picking between `min` and a custom multiplicative lattice

The difference matters when the graph has many hops:

| Hops | `min` result | Multiplicative result |
|---|---|---|
| A→B (0.9) | 0.9 | 0.9 |
| A→B→C (×0.85) | 0.85 | 0.765 |
| A→B→C→D (×0.95) | 0.85 | 0.726 |

- **`min`**: "how reliable is my weakest source?" — appropriate for trust chains where one bad link invalidates everything.
- **Multiplicative**: "what is the combined probability, assuming independence?" — appropriate for Bayesian-style confidence propagation.

For the multiplicative case, register a custom aggregate:

```sql
-- Step 1: create the multiplicative aggregate.
CREATE OR REPLACE FUNCTION prob_mul(state FLOAT8, val FLOAT8)
    RETURNS FLOAT8 LANGUAGE sql IMMUTABLE AS $$ SELECT COALESCE(state, 1.0) * val $$;

CREATE AGGREGATE prob_product(FLOAT8) (SFUNC = prob_mul, STYPE = FLOAT8, INITCOND = '1.0');

-- Step 2: register the lattice.
SELECT pg_ripple.create_lattice('probability', 'prob_product', '0.0');

-- Step 3: run inference.
SELECT pg_ripple.infer_lattice('my_rules', 'probability');
```

---

## Do I really need a lattice?

Before reaching for a lattice, check whether a simpler approach works:

| Scenario | Simpler alternative |
|---|---|
| Count edges reachable | Standard `WITH RECURSIVE` in SQL |
| Sum weights along a fixed-depth path | `SPARQL SELECT` with arithmetic |
| Aggregate over non-recursive rules | Standard Datalog with `@agg` directive |
| Confidence is additive, not multiplicative or min | `SUM` aggregate in a non-recursive rule |

Lattices add one piece of complexity: the fixpoint loop. If you can express the same computation as a single SQL `WITH RECURSIVE` query or a non-recursive Datalog rule, that is almost always simpler and faster.

---

## Convergence and termination

Every lattice computation is guaranteed to terminate if the lattice has the **ascending chain condition** — no infinite strictly ascending chains. All four built-in lattices satisfy this:
- `min` over bounded integers / floats: descending-only.
- `max` over bounded integers / floats: ascending but bounded.
- `set` of a finite universe: a finite powerset, every chain terminates.
- `interval` over bounded timestamps: bounded.

Custom lattices over unbounded domains can diverge. Set `pg_ripple.lattice_max_iterations` (default: 1000) as a safety cap. The engine emits a `PT540` warning if the cap is hit and returns the current (partial) fixpoint.

---

## See also

- [Lattice Datalog Reference](../reference/lattice-datalog.md) — all SQL functions, GUCs, and catalog tables.
- [Cookbook: Probabilistic Rules](../cookbook/probabilistic-rules.md) — end-to-end worked example.
- [Reasoning and Inference](reasoning-and-inference.md) — the full Datalog story.
