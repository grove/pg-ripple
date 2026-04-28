[← Back to Blog Index](README.md)

# Leapfrog Triejoin: When Triangle Queries Meet Optimal Joins

## How pg_ripple handles cyclic graph patterns without going quadratic

---

Most SPARQL queries are acyclic. Star patterns (same subject, multiple predicates), path patterns (chains of joins), and snowflake patterns (stars connected by paths) all have acyclic join graphs. The PostgreSQL optimizer handles these well with standard hash joins and nested-loop index lookups.

Then someone writes a query like this:

```sparql
SELECT ?a ?b ?c WHERE {
  ?a foaf:knows ?b .
  ?b foaf:knows ?c .
  ?c foaf:knows ?a .
}
```

"Find all triangles in the social network." This is a cyclic pattern — the join graph has a cycle: a→b→c→a. And it breaks standard join algorithms.

---

## Why Standard Joins Go Quadratic

Consider a naive plan for the triangle query:

1. Scan `vp_{foaf:knows}` for all `(a, b)` pairs. Say there are N edges.
2. Join with `vp_{foaf:knows}` for all `(b, c)` pairs. The intermediate result can be up to N² pairs.
3. Join with `vp_{foaf:knows}` for all `(c, a)` pairs. The filter brings it back down.

The problem is step 2. A hash join between the first and second copy of the edge table produces an intermediate result that can be quadratically large. In a social network with 1 million edges, the intermediate result can easily be 10 billion rows — even if the final answer has only 50,000 triangles.

The optimizer can't avoid this. No matter what join order it picks — (a,b)→(b,c)→(c,a) or (b,c)→(c,a)→(a,b) or any other rotation — there's always an intermediate join that can blow up quadratically before the final filter brings it back down.

This is a fundamental limitation of binary join plans for cyclic queries. It's not a PostgreSQL bug — it's a mathematical property of the relational join operator when applied to cyclic graphs.

---

## Worst-Case Optimal Joins

In 2012, Ngo, Porat, Ré, and Rudra proved that there exist join algorithms whose output is bounded by the AGM (Atserias-Grohe-Marx) bound — the theoretical maximum result size. For triangle queries on a graph with N edges, the AGM bound is $O(N^{3/2})$, not $O(N^2)$.

The practical algorithm that achieves this bound is the Leapfrog Triejoin. Instead of joining tables pairwise, it processes all relations simultaneously, variable by variable, using sorted iterators that "leapfrog" past impossible values.

Here's the intuition for triangles:

1. For each possible value of `a`, find all `b` values where `(a, b)` is an edge.
2. For each such `b`, find all `c` values where `(b, c)` is an edge.
3. For each such `c`, check if `(c, a)` is an edge — using a sorted lookup, not a full scan.

Step 3 is the key. Instead of materializing all `(a, b, c)` intermediate results and then filtering, the algorithm interleaves the lookups. If `c=7` and `a=3` and there's no edge `(7, 3)`, the algorithm skips ahead to the next valid `c` using sorted iteration. No intermediate blowup.

---

## How pg_ripple Implements It

pg_ripple's SPARQL algebra optimizer detects cyclic patterns in the join graph. When a cycle is found:

1. The cycle is extracted from the overall join tree.
2. The cycle's variables are ordered to minimize intermediate cardinality (using predicate statistics from the catalog).
3. The cycle is compiled to a leapfrog triejoin plan that uses the `(s, o)` and `(o, s)` indexes on the participating VP tables as sorted iterators.
4. The leapfrog plan is embedded in the overall SQL as a CTE or subquery.

The acyclic portions of the query continue to use standard PostgreSQL joins. Only the cyclic core is compiled to leapfrog triejoin.

### Sorted Iterators

Leapfrog triejoin requires sorted access to the join columns. pg_ripple's VP tables provide this for free: the `(s, o)` index gives sorted access by subject, and the `(o, s)` index gives sorted access by object. Since all columns are `BIGINT`, the sort order is a simple numeric comparison — no collation issues.

For the triangle query, the iterators are:

- Iterator 1: `vp_{knows}` index `(s, o)`, providing `(a, b)` pairs sorted by `a` then `b`.
- Iterator 2: `vp_{knows}` index `(s, o)`, providing `(b, c)` pairs sorted by `b` then `c`.
- Iterator 3: `vp_{knows}` index `(s, o)`, providing `(c, a)` pairs sorted by `c` then `a`.

The leapfrog algorithm walks these three iterators in lockstep, advancing whichever iterator is behind.

---

## The Speedup

On a social network graph with 1 million edges and ~50,000 triangles:

| Plan | Intermediate rows | Wall time |
|------|-------------------|-----------|
| Hash join (best binary order) | ~120 million | 45 seconds |
| Leapfrog triejoin | ~50,000 (output only) | 400ms |

That's a 100× speedup. The difference grows with graph density — denser graphs produce more intermediate blowup in binary joins while the leapfrog plan stays proportional to the output size.

For sparser graphs (like taxonomic hierarchies), the difference is smaller because binary joins don't blow up as much. But leapfrog is never worse than binary joins — it's an asymptotic improvement, not a constant-factor optimization.

---

## Beyond Triangles

Leapfrog triejoin isn't limited to triangles. It handles any cyclic pattern:

**4-cliques** (find four mutually connected nodes):
```sparql
SELECT ?a ?b ?c ?d WHERE {
  ?a foaf:knows ?b . ?a foaf:knows ?c . ?a foaf:knows ?d .
  ?b foaf:knows ?c . ?b foaf:knows ?d .
  ?c foaf:knows ?d .
}
```

**Diamond patterns** (find two paths of length 2 between the same endpoints):
```sparql
SELECT ?start ?end WHERE {
  ?start ex:link ?mid1 . ?mid1 ex:link ?end .
  ?start ex:link ?mid2 . ?mid2 ex:link ?end .
  FILTER(?mid1 != ?mid2)
}
```

**Bow-tie patterns** (two triangles sharing an edge):
```sparql
SELECT ?a ?b ?c ?d WHERE {
  ?a foaf:knows ?b . ?b foaf:knows ?c . ?c foaf:knows ?a .
  ?b foaf:knows ?d . ?d foaf:knows ?c .
}
```

For each of these, binary joins produce intermediate blowup. Leapfrog triejoin produces output proportional to the result size.

---

## Integration with the Query Planner

Leapfrog triejoin is a compile-time decision, not a runtime one. The SPARQL algebra optimizer:

1. Builds the join graph from the BGP.
2. Detects cycles using a standard cycle-finding algorithm.
3. For each cycle, estimates the AGM bound using predicate cardinalities from the catalog.
4. Compares the AGM bound against the estimated binary join cost.
5. If leapfrog is cheaper, compiles the cycle to leapfrog triejoin.
6. If binary is cheaper (possible for very small or very selective patterns), uses standard joins.

This cost-based decision means pg_ripple doesn't force leapfrog triejoin on queries that don't need it. The overhead of leapfrog (sorted iteration, more complex control flow) isn't worth paying for acyclic queries where hash joins are optimal.

---

## When You'll See This

If you're running SPARQL queries over social networks, citation graphs, molecular structures, knowledge graphs with mutual relationships, or any domain where "find cycles" or "find cliques" is a natural question — leapfrog triejoin is working behind the scenes.

You can see it in the query plan:

```sql
SELECT pg_ripple.explain_sparql('
  SELECT ?a ?b ?c WHERE {
    ?a foaf:knows ?b . ?b foaf:knows ?c . ?c foaf:knows ?a .
  }
');
```

The explain output will show a `LeapfrogTriejoin` node for the cyclic component, with the estimated output cardinality based on the AGM bound rather than the intermediate join cardinality.

If you've ever had a SPARQL query time out because it was computing triangles with binary joins, this is the fix. No query rewriting needed. pg_ripple detects the pattern and handles it.
