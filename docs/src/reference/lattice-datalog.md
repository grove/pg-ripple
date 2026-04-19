# Lattice-Based Datalog Reference (v0.36.0)

> **Available since v0.36.0.** Lattice-Based Datalog (Datalog^L) extends pg_ripple's Datalog engine with monotone lattice aggregation, enabling recursive aggregation without stratification constraints.

---

## Background

Standard Datalog^agg stratifies aggregate functions: an aggregate can only appear at a strictly higher stratum than the predicate it aggregates over.  This makes recursive aggregation (e.g., propagating minimum trust scores through a social graph) impossible to express without manual loop unrolling.

**Lattice-Based Datalog** lifts this restriction by requiring only that the aggregation operation is *monotone* with respect to a user-supplied *lattice*.  A lattice is an algebraic structure (L, ⊔) where ⊔ is a commutative, associative, idempotent join operation with a bottom element ⊥.  Fixpoint computation over a lattice terminates by the ascending chain condition — the lattice has no infinite strictly ascending chains.

### Key references

- Abo Khamis et al., *PODS 2017* — lattice-structured aggregation in Datalog
- Alvaro et al., *CIDR 2011* — monotone logic programming (Bloom^L)
- Green et al., *PODS 2007* — provenance semirings as a generalization of lattices

---

## Built-in lattices

pg_ripple ships with four built-in lattice types that cover the most common use cases:

### MinLattice (`min`)

```
join:   LEAST(a, b)       (PostgreSQL: min aggregate)
bottom: +∞  (encoded as 9223372036854775807 = i64::MAX)
```

**Use cases:** trust propagation, shortest-path weights, minimum-cost routing.

**Example:** propagate the minimum trust score along a path — the trustworthiness of a chain is limited by its weakest link.

### MaxLattice (`max`)

```
join:   GREATEST(a, b)   (PostgreSQL: max aggregate)
bottom: −∞  (encoded as -9223372036854775808 = i64::MIN)
```

**Use cases:** reachability weights, longest-path annotation, maximum influence scores.

### SetLattice (`set`)

```
join:   UNION (array deduplication via array_agg)
bottom: {} (empty set)
```

**Use cases:** set-valued provenance annotation, multi-hop neighbourhood sets.

### IntervalLattice (`interval`)

```
join:   interval hull   (max of lower bound, max of upper bound)
bottom: empty interval (0)
```

**Use cases:** temporal reasoning, numeric range propagation.

---

## User-defined lattices

Register a custom lattice with any PostgreSQL aggregate function as the join:

```sql
-- Minimum-cost routing over decimal weights.
SELECT pg_ripple.create_lattice('route_cost', 'min', '1e308');

-- Custom bounded lattice (values 0–100, join = LEAST).
SELECT pg_ripple.create_lattice('reputation', 'min', '100');
```

The `join_fn` must be a registered PostgreSQL aggregate (verified via `pg_proc`).  A warning is emitted at registration time if the function is not yet visible, but the lattice is still stored — this allows pre-registering lattices before their custom aggregates are created.

---

## GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.lattice_max_iterations` | integer | 1000 | Maximum fixpoint iterations before error code PT540 warning and partial-result return. Set to 0 for unlimited (not recommended). |

```sql
-- Change the iteration limit.
SET pg_ripple.lattice_max_iterations = 5000;

-- Check current setting.
SHOW pg_ripple.lattice_max_iterations;
```

---

## SQL Functions

### `pg_ripple.create_lattice(name, join_fn, bottom)` → `boolean`

Register a new lattice type in the `_pg_ripple.lattice_types` catalog.

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | text | Unique lattice name (case-sensitive) |
| `join_fn` | text | PostgreSQL aggregate function name |
| `bottom` | text | Bottom element as a text string |

Returns `true` if newly registered, `false` if the name already exists (idempotent).

```sql
SELECT pg_ripple.create_lattice('trust', 'min', '100');   -- true
SELECT pg_ripple.create_lattice('trust', 'min', '100');   -- false (idempotent)
```

### `pg_ripple.list_lattices()` → `jsonb`

Return a JSON array of all registered lattice types (built-in and user-defined).

```sql
SELECT jsonb_pretty(pg_ripple.list_lattices());
```

Each entry has: `name`, `join_fn`, `bottom`, `builtin`.

### `pg_ripple.infer_lattice(rule_set, lattice_name)` → `jsonb`

Run a monotone fixpoint over all active rules in `rule_set` using the specified lattice.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `rule_set` | `'custom'` | Rule set name as used in `load_rules()` |
| `lattice_name` | `'min'` | Lattice type to use for head-predicate joins |

Returns JSONB:

```json
{
  "derived":         42,
  "iterations":       5,
  "lattice":       "min",
  "rule_set":  "my_rules"
}
```

Errors:
- `infer_lattice: unknown lattice type '...'` — lattice not registered; call `create_lattice()` first.
- `PT540` WARNING — fixpoint did not converge within `lattice_max_iterations`.

---

## Catalog table

Lattice types are stored in `_pg_ripple.lattice_types`:

```sql
SELECT * FROM _pg_ripple.lattice_types;
```

| Column | Type | Description |
|--------|------|-------------|
| `name` | text | Primary key; lattice identifier |
| `join_fn` | text | PostgreSQL aggregate name |
| `bottom` | text | Bottom element as text |
| `builtin` | boolean | True for pre-registered lattices |
| `created_at` | timestamptz | Registration timestamp |

---

## Complete example: Trust propagation

This example propagates minimum trust scores through a social graph.  The trustworthiness of an indirect connection is bounded by the weakest link on the path.

```sql
-- 1. Create extension and configure lattice.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.create_lattice('trust', 'min', '100');

-- 2. Insert direct trust relationships (score: 0=no trust, 100=full trust).
SELECT pg_ripple.load_ntriples($$
  <https://trust.example/alice> <https://trust.example/directTrust> "90"^^<xsd:integer> .
  <https://trust.example/bob>   <https://trust.example/directTrust> "70"^^<xsd:integer> .
  <https://trust.example/carol> <https://trust.example/directTrust> "85"^^<xsd:integer> .
  <https://trust.example/alice> <https://trust.example/knows>       <https://trust.example/bob> .
  <https://trust.example/bob>   <https://trust.example/knows>       <https://trust.example/carol> .
$$);

-- 3. Write a trust-propagation rule (using Datalog syntax).
SELECT pg_ripple.load_rules($$
  ?y <https://trust.example/transitTrust> ?min_t :-
    ?x <https://trust.example/knows> ?y ,
    ?x <https://trust.example/directTrust> ?t1 ,
    ?y <https://trust.example/directTrust> ?t2 .
$$, 'trust_rules');

-- 4. Run lattice-based fixpoint.
SELECT pg_ripple.infer_lattice('trust_rules', 'trust');

-- 5. Query propagated trust values.
SELECT * FROM pg_ripple.sparql($$
  SELECT ?x ?t WHERE { ?x <https://trust.example/transitTrust> ?t }
$$);
```

---

## Error code PT540

**Meaning:** the lattice fixpoint did not converge within the configured iteration limit.

**Trigger:** emitted as a PostgreSQL WARNING (not ERROR) when `pg_ripple.lattice_max_iterations` is exceeded.

**Resolution options:**

1. Increase the limit:
   ```sql
   SET pg_ripple.lattice_max_iterations = 10000;
   ```

2. Verify your lattice is finite: every value domain used in rules must have a finite number of distinct elements reachable from the bottom.

3. Verify monotonicity: every operation in rule bodies must be monotone with respect to the lattice order.  A non-monotone operation (e.g., negation) in a recursive rule violates the convergence guarantee.

---

## Relationship to other pg_ripple inference modes

| Feature | Stratum requirement | Aggregation | Recursion |
|---------|---------------------|-------------|-----------|
| `infer()` — standard Datalog | Stratified | Not supported | Restricted |
| `infer_wfs()` — Well-Founded Semantics | None | Not supported | Full |
| `infer_lattice()` — Datalog^L | None | Monotone lattice joins | Full |

Use `infer_lattice()` when you need recursive aggregation with a convergence guarantee, for example: shortest paths, trust propagation, or set-reachability annotations.

---

*Introduced in v0.36.0.*
