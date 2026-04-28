[← Back to Blog Index](README.md)

# Property Paths Are Just Recursive CTEs

## SPARQL graph traversal compiled to WITH RECURSIVE … CYCLE

---

SPARQL has a feature that SQL users envy: property paths. Write `foaf:knows+` and you get transitive closure. Write `skos:broader*` and you get a full ancestry chain. Write `(foaf:knows|foaf:worksWith)+` and you get reachability across alternating relationship types.

Property paths are the SPARQL equivalent of recursive CTEs, except the syntax is one line instead of fifteen. Here's how pg_ripple compiles them to SQL, and why PostgreSQL 18's cycle detection makes it safe.

---

## What Property Paths Express

A property path in SPARQL is a regular expression over predicates. The standard operators:

| Syntax | Meaning |
|--------|---------|
| `foaf:knows` | Exactly one hop |
| `foaf:knows/foaf:name` | Sequence: two hops |
| `foaf:knows+` | One or more hops (transitive closure) |
| `foaf:knows*` | Zero or more hops (reflexive transitive closure) |
| `foaf:knows?` | Zero or one hop |
| `foaf:knows\|foaf:worksWith` | Alternative: either predicate |
| `^foaf:knows` | Inverse: traverse backwards |
| `!(rdf:type\|rdfs:label)` | Negated property set: any predicate except these |

The recursive operators (`+` and `*`) are the interesting ones. They express graph traversal without a fixed depth — "follow this edge type until you can't go further."

---

## The Compilation

A simple transitive path:

```sparql
SELECT ?ancestor WHERE {
  ex:Alice skos:broader+ ?ancestor .
}
```

Compiles to:

```sql
WITH RECURSIVE path(node) AS (
  -- Base case: direct parents of Alice
  SELECT o AS node
  FROM _pg_ripple.vp_57  -- skos:broader
  WHERE s = 847291       -- ex:Alice (encoded)

  UNION

  -- Recursive step: parents of nodes already found
  SELECT t.o AS node
  FROM _pg_ripple.vp_57 t
  JOIN path p ON t.s = p.node
)
CYCLE node SET is_cycle USING cycle_path
SELECT d.value AS ancestor
FROM path
JOIN _pg_ripple.dictionary d ON d.id = path.node
WHERE NOT is_cycle;
```

The structure is standard recursive CTE:
1. **Base case:** Find the direct `skos:broader` objects of `ex:Alice`.
2. **Recursive step:** For each node found so far, find its `skos:broader` objects.
3. **Cycle detection:** The `CYCLE` clause prevents infinite loops.
4. **Decode:** Dictionary lookup on the final result.

---

## Why CYCLE Matters

Graph data has cycles. A taxonomy might not, but a social network definitely does. Even hierarchical datasets can have accidental cycles due to data quality issues.

Without cycle detection, a recursive CTE on cyclic data runs forever (or until `statement_timeout` kills it). PostgreSQL 18's `CYCLE` clause adds hash-based cycle detection that terminates the recursion when a node is revisited.

The alternative — using `UNION` (which deduplicates) instead of `UNION ALL` — also prevents infinite loops but has different performance characteristics. `UNION` deduplicates at every iteration, which can be expensive for wide result sets. `CYCLE` with `UNION ALL` tracks only the cycle-detection columns, which is cheaper when the result has many columns.

pg_ripple always generates the `CYCLE` form because:
1. It's explicit about what's being checked for cycles.
2. The cycle detection column (`is_cycle`) can be used to distinguish cyclic from non-cyclic paths if needed.
3. It's the idiomatic PostgreSQL 18 approach.

---

## Bounded Depth

Even with cycle detection, unbounded property paths can be expensive. A `foaf:knows+` query on a dense social graph with 10 million edges might explore millions of nodes before terminating.

pg_ripple bounds recursive depth via the `pg_ripple.max_path_depth` GUC (default: 100). The recursive CTE includes a depth counter that stops expansion beyond this limit. For most hierarchical data, 100 levels is far more than sufficient. For social network traversals, a lower bound (5–10) often makes more sense and should be set explicitly in the query context.

The early fixpoint termination optimization (v0.34.0) goes further: if an iteration produces no new nodes, the recursion stops immediately rather than continuing to the depth limit. For a hierarchy with 8 levels, this means 8 iterations, not 100.

---

## Complex Paths

Property paths can be more complex than simple transitive closure.

### Sequence Paths

```sparql
SELECT ?grandparent WHERE {
  ex:Alice foaf:knows/foaf:parent ?grandparent .
}
```

Compiles to a two-step join — no recursion needed:

```sql
SELECT d.value AS grandparent
FROM _pg_ripple.vp_42 t1    -- foaf:knows
JOIN _pg_ripple.vp_88 t2    -- foaf:parent
  ON t2.s = t1.o
JOIN _pg_ripple.dictionary d ON d.id = t2.o
WHERE t1.s = 847291;        -- ex:Alice
```

### Alternation Paths

```sparql
SELECT ?connection WHERE {
  ex:Alice (foaf:knows|foaf:worksWith)+ ?connection .
}
```

Compiles to a recursive CTE that unions two VP tables at each step:

```sql
WITH RECURSIVE path(node) AS (
  SELECT o FROM _pg_ripple.vp_42 WHERE s = 847291   -- foaf:knows
  UNION ALL
  SELECT o FROM _pg_ripple.vp_93 WHERE s = 847291   -- foaf:worksWith

  UNION

  SELECT t.o FROM (
    SELECT s, o FROM _pg_ripple.vp_42
    UNION ALL
    SELECT s, o FROM _pg_ripple.vp_93
  ) t
  JOIN path p ON t.s = p.node
)
CYCLE node SET is_cycle USING cycle_path
SELECT d.value FROM path
JOIN _pg_ripple.dictionary d ON d.id = path.node
WHERE NOT is_cycle;
```

### Inverse Paths

```sparql
SELECT ?descendant WHERE {
  ex:TopCategory ^skos:broader+ ?descendant .
}
```

The `^` operator reverses the path direction. Instead of following `skos:broader` forward (child → parent), it follows backward (parent → child):

```sql
WITH RECURSIVE path(node) AS (
  SELECT s FROM _pg_ripple.vp_57 WHERE o = 193847   -- TopCategory encoded
  UNION
  SELECT t.s FROM _pg_ripple.vp_57 t
  JOIN path p ON t.o = p.node
)
CYCLE node SET is_cycle USING cycle_path
SELECT d.value FROM path
JOIN _pg_ripple.dictionary d ON d.id = path.node;
```

Note the swap: the base case selects `s` where `o` matches (instead of `o` where `s` matches), and the join is on `t.o = p.node` instead of `t.s = p.node`. The `(o, s)` index on the VP table makes the reversed traversal equally efficient.

---

## Performance: Paths vs. Materialized Closure

For read-heavy workloads where the same transitive closure is queried repeatedly, Datalog materialization is faster. pg_ripple's Datalog engine can materialize the transitive closure of `skos:broader` as a set of inferred triples:

```sql
SELECT pg_ripple.datalog_add_rule(
  'ancestor(X, Y) :- skos_broader(X, Y).'
);
SELECT pg_ripple.datalog_add_rule(
  'ancestor(X, Y) :- skos_broader(X, Z), ancestor(Z, Y).'
);
SELECT pg_ripple.datalog_infer();
```

After materialization, querying the closure is a single VP table scan — no recursion. The trade-off is that materialization takes time and must be re-run when the data changes (or maintained incrementally with DRed).

Property paths compute the closure on the fly, every time. For ad-hoc queries or small graphs, this is fine. For dashboards or API endpoints that query the same closure thousands of times per second, materialization wins.

pg_ripple gives you both. Use property paths for flexibility; use Datalog for performance. The choice is per-query, not per-deployment.
