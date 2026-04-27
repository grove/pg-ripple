# Cookbook: Probabilistic Rules for Soft Constraints

**Goal.** You want rules that *propagate confidence*, not just facts. *"If A is similar to B with confidence 0.9, and B is similar to C with confidence 0.85, then A is similar to C with confidence ≥ 0.85 × 0.9."* Classical Datalog cannot do this; it deals in true/false. **Lattice Datalog** can.

**Why pg_ripple.** Ships built-in lattices (`min`, `max`, `set`, `interval`) and lets you register custom ones. Inference fixpoints over a lattice instead of a boolean.

**Time to first result.** ~10 minutes.

---

## The intuition

Standard Datalog: facts are *in the relation* or *not*. There is one truth value: derived.

Lattice Datalog: facts have an *associated value* — a confidence, a cost, a probability, a time interval. The lattice tells the engine how to *combine* multiple derivations of the same fact. For confidence we use `min`: a derivation chain is only as confident as its weakest link.

---

## Step 1 — Pick a lattice

For confidence propagation, the built-in `min` lattice is exactly what we want. (Top of the lattice = 1.0 = certain. Bottom = 0.0 = unknown. Multiple derivations of the same fact take the *strongest* — i.e. *minimum* of the weak-link confidences.)

If you prefer max-of-min semantics (the strongest single chain wins), build a `max` lattice over chains of `min`. The bundled `min` lattice is the most common starting point.

## Step 2 — Encode confidence with RDF-star

Confidence is a *property of a triple*, so RDF-star is the natural encoding:

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:  <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:alice ex:similarTo ex:bob   .
ex:bob   ex:similarTo ex:carol .
ex:carol ex:similarTo ex:dan   .

<< ex:alice ex:similarTo ex:bob   >> ex:confidence "0.90"^^xsd:decimal .
<< ex:bob   ex:similarTo ex:carol >> ex:confidence "0.85"^^xsd:decimal .
<< ex:carol ex:similarTo ex:dan   >> ex:confidence "0.95"^^xsd:decimal .
$TTL$);
```

## Step 3 — Write a lattice rule

```sql
SELECT pg_ripple.load_rules($RULES$
# Transitive similarity: confidence is the min of the chain.
?x ex:transSimilarTo ?y :- ?x ex:similarTo ?y .

?x ex:transSimilarTo ?z :-
    ?x ex:similarTo      ?y ,
    ?y ex:transSimilarTo ?z .

# Lattice-typed binding: each derived ex:transSimilarTo carries a confidence.
@lattice ex:transSimilarTo confidence min .
$RULES$, 'similarity');

SELECT pg_ripple.infer_lattice('similarity', 'min');
```

The `@lattice` directive tells the engine: *whenever a `ex:transSimilarTo` triple is derived, its confidence is the `min` of the confidences of the body atoms*. The engine then iterates to a fixpoint with the lattice as the join operator.

## Step 4 — Query

```sql
SELECT * FROM pg_ripple.sparql($$
    PREFIX ex: <https://example.org/>
    SELECT ?z ?conf WHERE {
        <https://example.org/alice> ex:transSimilarTo ?z .
        << <https://example.org/alice> ex:transSimilarTo ?z >> ex:confidence ?conf .
    }
    ORDER BY DESC(?conf)
$$);
```

```
?z              ?conf
ex:bob          0.90
ex:carol        0.85   (min of 0.90, 0.85)
ex:dan          0.85   (min of 0.90, 0.85, 0.95)
```

Note that `ex:dan` keeps the bottleneck of `0.85`, not `0.95 × 0.85 × 0.90`. That is exactly what `min` semantics gives you — *the weakest link*. If you want *multiplicative* propagation, register a custom lattice (next section).

## Step 5 — Custom lattice for multiplicative confidence

```sql
-- The PostgreSQL aggregate that combines two confidences multiplicatively.
CREATE OR REPLACE FUNCTION conf_mul(state DOUBLE PRECISION, val DOUBLE PRECISION)
RETURNS DOUBLE PRECISION
LANGUAGE plpgsql IMMUTABLE AS $$ BEGIN RETURN COALESCE(state, 1.0) * val; END; $$;

CREATE AGGREGATE prob_join(DOUBLE PRECISION) (
    SFUNC = conf_mul, STYPE = DOUBLE PRECISION, INITCOND = '1.0'
);

SELECT pg_ripple.create_lattice(
    name    := 'probability',
    join_fn := 'prob_join',
    bottom  := '0.0'
);

SELECT pg_ripple.infer_lattice('similarity', 'probability');
```

Now `?dan`'s confidence is `0.90 × 0.85 × 0.95 = 0.726` — chain decay, not weakest-link.

---

## When lattice Datalog is the right tool

- **Confidence propagation** (this recipe).
- **Shortest path** (`min` lattice over edge weights).
- **Maximum bandwidth** (`max` lattice over edge capacity, then `min` along the chain).
- **Time-interval reasoning** (`interval` lattice — *"the period during which all of these are true"*).
- **Provenance semirings** (custom lattice over witness sets).

When the rule's body has only boolean conjunction and the head needs only *true/false*, classical Datalog is simpler. Lattice Datalog earns its complexity only when the value associated with a fact matters.

---

## See also

- [Lattice-Based Datalog reference](../reference/lattice-datalog.md)
- [Reasoning & Inference](../features/reasoning-and-inference.md)
