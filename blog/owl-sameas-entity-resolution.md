[← Back to Blog Index](README.md)

# owl:sameAs Without the Explosion

## Entity canonicalization that doesn't turn your graph into molasses

---

`owl:sameAs` is the simplest concept in OWL: "these two URIs refer to the same thing." Link `ex:alice` to `dbpedia:Alice_Smith` and every triple about one should be queryable through the other.

In theory, this is beautiful — it enables data integration across datasets without rewriting any data. In practice, it's the single most dangerous predicate in the Semantic Web. Here's why, and how pg_ripple handles it without the performance explosion.

---

## The sameAs Explosion

`owl:sameAs` is transitive and symmetric. If A sameAs B and B sameAs C, then A sameAs C (and C sameAs A, and B sameAs A, etc.). This creates equivalence classes.

The problem appears when you query. Consider:

```sparql
SELECT ?email WHERE {
  ex:alice foaf:mbox ?email .
}
```

If `ex:alice owl:sameAs dbpedia:Alice_Smith`, then this query should also return emails for `dbpedia:Alice_Smith`. And if `dbpedia:Alice_Smith owl:sameAs wikidata:Q12345`, it should return emails for `wikidata:Q12345` too.

The naive implementation: for every triple pattern, expand the subject (or object) to its full equivalence class, and query for all members:

```sql
-- Naive expansion
SELECT o FROM vp_foaf_mbox
WHERE s IN (
  SELECT equivalent_id
  FROM sameas_closure
  WHERE canonical_id = encode('ex:alice')
);
```

For a single entity with 5 aliases, this is fine. But datasets like DBpedia and Wikidata have `owl:sameAs` chains that connect millions of entities. The LOD Cloud contains over 500 million `owl:sameAs` links. Computing the full transitive closure produces equivalence classes with thousands of members, and every triple pattern lookup becomes a thousand-element `IN (...)` query.

This is the sameAs explosion: a simple graph query becomes exponentially more expensive because every lookup fans out across the equivalence class.

---

## The Canonicalization Approach

pg_ripple avoids the explosion by canonicalizing at write time rather than expanding at query time.

The idea: for each equivalence class, choose one canonical representative. When triples are stored, rewrite all members to the canonical ID. When queried, the canonical ID is the only one that appears in the VP tables, so no expansion is needed.

```
ex:alice           → canonical ID 847291
dbpedia:Alice_Smith → canonical ID 847291
wikidata:Q12345    → canonical ID 847291
```

All three URIs map to the same dictionary ID (847291). The VP tables store `s = 847291` for all triples about this entity. A query for `ex:alice foaf:mbox ?email` encodes `ex:alice` to 847291 and does a single-value lookup — no expansion.

---

## Union-Find in the Dictionary

pg_ripple implements canonicalization using a union-find (disjoint-set) data structure integrated with the dictionary.

When `owl:sameAs` triples are loaded:

1. `owl:sameAs(A, B)` is intercepted before normal triple storage.
2. The dictionary IDs for A and B are looked up (or created).
3. The union-find merges their equivalence classes.
4. One ID is chosen as canonical (the smaller ID, for determinism).
5. All existing triples for the non-canonical ID are rewritten to the canonical ID.
6. The dictionary mapping for the non-canonical URI is updated to point to the canonical ID.

Step 5 is the expensive part — rewriting existing triples. But it happens once, at write time, not at every query. For a new `owl:sameAs` link between two entities with 50 and 30 triples respectively, 30 triples are rewritten (the smaller set merges into the larger). This is O(smaller set) per merge — the classic union-by-rank optimization.

---

## Query-Time Behavior

After canonicalization, queries are simple:

```sql
-- Query for ex:alice — encodes to canonical ID 847291
SELECT o FROM vp_foaf_mbox WHERE s = 847291;
```

One lookup. One index scan. No expansion. The query doesn't know or care that 847291 is an equivalence class of three URIs.

The decode step handles the reverse mapping: when returning results to the user, pg_ripple can optionally show the original URI instead of the canonical one, using the dictionary's alias chain.

---

## What About Late-Arriving sameAs Links?

This is the hard case. You have 10 million triples. Then someone loads a new `owl:sameAs` link that connects two previously separate entities.

pg_ripple handles this with a merge operation:

1. Compute the union of the two equivalence classes.
2. Choose the canonical ID for the merged class.
3. Rewrite all triples for the non-canonical IDs to the canonical ID.
4. Update the dictionary mappings.

For two entities with 500 and 300 triples respectively, this requires rewriting ~300 triples across multiple VP tables. On modern hardware, this takes a few milliseconds.

For bulk imports of `owl:sameAs` datasets (like linking DBpedia to Wikidata), pg_ripple batches the merges: all `owl:sameAs` triples are processed first to compute the full equivalence classes, then the rewrites are batched by VP table. This avoids rewriting the same triple multiple times as the equivalence class grows incrementally.

---

## The Alternative: Query-Time Expansion

Some triplestores (like Apache Jena) handle `owl:sameAs` at query time: they maintain the equivalence classes but expand them during query evaluation. Each triple pattern lookup returns results for all members of the equivalence class.

This has the advantage of never rewriting stored triples. It has the disadvantage of making every query slower, proportional to the size of the equivalence classes.

For datasets with small equivalence classes (average size 2–3), query-time expansion is fine. For datasets with large equivalence classes (common in Linked Open Data, where sameAs chains can connect thousands of entities), the per-query cost is prohibitive.

pg_ripple's write-time canonicalization trades write cost for read performance. Since most knowledge graph workloads are read-heavy (queries outnumber writes by 100:1 or more), this is the right trade-off.

---

## Handling owl:sameAs Correctly

`owl:sameAs` has subtle semantics that many implementations get wrong:

### Symmetry and Transitivity
`owl:sameAs(A, B)` implies `owl:sameAs(B, A)` and, combined with `owl:sameAs(B, C)`, implies `owl:sameAs(A, C)`. The union-find data structure handles both automatically.

### Reflexivity
Every entity is `owl:sameAs` itself. pg_ripple doesn't store these explicitly — the identity mapping is implicit in the dictionary.

### Interaction with SHACL
SHACL validation must account for canonicalization. If a SHACL shape constrains `foaf:mbox` to have `sh:maxCount 1`, and two previously separate entities each had one email, merging them via `owl:sameAs` creates a violation (the canonical entity now has two emails). pg_ripple's SHACL validator runs post-merge to catch these.

### Interaction with Named Graphs
`owl:sameAs` links can span named graphs. Entity A in graph G1 and entity B in graph G2 are the same entity. Canonicalization merges them across graphs, which is semantically correct but can be surprising. pg_ripple logs graph-spanning merges for auditability.

---

## Numbers

On a dataset with 10 million triples and 100,000 `owl:sameAs` links forming 40,000 equivalence classes:

| Approach | Query latency (avg) | Write overhead | Storage |
|----------|---------------------|----------------|---------|
| No sameAs handling | 2ms | 0 | Baseline |
| Query-time expansion | 35ms | 0 | + 2MB (class index) |
| Write-time canonicalization | 2ms | +15% on sameAs loads | - 5% (deduplication) |

The query latency for canonicalization is identical to no-sameAs because the VP table layout is the same — just with fewer distinct subject/object IDs. The storage actually decreases because merged entities consolidate duplicate triples.

The write overhead is a one-time cost during sameAs ingestion. After canonicalization, subsequent queries run at full speed with no per-query penalty.

For knowledge graph applications that integrate data from multiple sources — which is the primary use case for `owl:sameAs` — this is the correct trade-off: pay once at write time, benefit at every query.
