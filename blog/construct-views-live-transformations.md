[← Back to Blog Index](README.md)

# SPARQL CONSTRUCT Views: Live Materialized Graph Transformations

## Derived graphs that update themselves when the source data changes

---

You have raw data in one schema. Your API serves data in a different schema. Between them is a transformation: rename predicates, reshape structures, filter irrelevant triples, derive new relationships.

In relational databases, you'd use a materialized view. In pg_ripple, you use a CONSTRUCT view — a SPARQL CONSTRUCT query registered as a persistent, live-updating derived graph.

---

## The Problem

Your source data uses the schema.org vocabulary:

```turtle
ex:alice schema:jobTitle "Senior Engineer" ;
         schema:worksFor ex:acme ;
         schema:name "Alice Smith" .
```

Your API contract uses the FOAF vocabulary:

```turtle
ex:alice foaf:title "Senior Engineer" ;
         org:memberOf ex:acme ;
         foaf:name "Alice Smith" .
```

A SPARQL CONSTRUCT query expresses the transformation:

```sparql
CONSTRUCT {
  ?person foaf:title ?title .
  ?person org:memberOf ?org .
  ?person foaf:name ?name .
}
WHERE {
  ?person schema:jobTitle ?title ;
          schema:worksFor ?org ;
          schema:name ?name .
}
```

But running this query every time the API is called is wasteful — especially when the source data changes infrequently. You want to materialize the result and keep it updated.

---

## Creating a CONSTRUCT View

```sql
SELECT pg_ripple.create_construct_view(
  name  => 'api_persons',
  query => '
    CONSTRUCT {
      ?person foaf:title ?title .
      ?person org:memberOf ?org .
      ?person foaf:name ?name .
    }
    WHERE {
      ?person schema:jobTitle ?title ;
              schema:worksFor ?org ;
              schema:name ?name .
    }
  ',
  target_graph => 'http://example.org/api_view'
);
```

This creates a named graph `http://example.org/api_view` containing the CONSTRUCT results. The triples are stored in VP tables just like any other triples — queryable with SPARQL, indexable, and participating in joins with other graph data.

---

## Incremental Updates

When the source data changes — Alice gets a promotion, Bob joins the company — the CONSTRUCT view detects the change and updates incrementally:

1. **Insert triggers:** When new triples matching the WHERE pattern are inserted, the CONSTRUCT template generates new triples in the target graph.
2. **Delete handling:** When source triples are deleted, the corresponding CONSTRUCT triples are retracted.
3. **Update handling:** Updates are decomposed into delete + insert.

The incremental mechanism uses the same Delete-Rederive (DRed) algorithm as Datalog:

- When a source triple is deleted, tentatively delete all CONSTRUCT triples derived from it.
- Re-derive from remaining sources to check which tentatively deleted triples are still valid.
- Finalize the deletions.

This ensures correctness: if Alice's `schema:jobTitle` appears in two CONSTRUCT views, deleting the source triple only removes the CONSTRUCT triple if no other derivation path exists.

---

## DESCRIBE Views

DESCRIBE views are a special case: they materialize everything known about a set of entities.

```sql
SELECT pg_ripple.create_describe_view(
  name    => 'product_profiles',
  query   => '
    DESCRIBE ?product WHERE {
      ?product rdf:type ex:Product ;
               ex:active true .
    }
  ',
  target_graph => 'http://example.org/product_profiles'
);
```

This materializes the full description (all predicates, all values) for every active product. When a product becomes inactive, its description is removed from the view. When a product's properties change, the materialized description updates.

---

## ASK Views

ASK views materialize boolean conditions as flags:

```sql
SELECT pg_ripple.create_ask_view(
  name  => 'has_critical_alerts',
  query => '
    ASK {
      ?alert rdf:type ex:CriticalAlert ;
             ex:status "active" .
    }
  '
);
```

The view is a single boolean value that updates whenever the condition changes. Useful for dashboard indicators, circuit breakers, and conditional logic.

```sql
SELECT pg_ripple.ask_view_value('has_critical_alerts');
-- Returns: true
```

---

## Chaining Views

CONSTRUCT views can reference other CONSTRUCT views, creating a transformation pipeline:

```sql
-- Layer 1: Vocabulary alignment (schema.org → FOAF)
SELECT pg_ripple.create_construct_view(
  name => 'foaf_persons',
  query => '...',
  target_graph => 'http://example.org/foaf'
);

-- Layer 2: Enrichment (add inferred relationships)
SELECT pg_ripple.create_construct_view(
  name => 'enriched_persons',
  query => '
    CONSTRUCT {
      ?person ex:seniorStaff true .
    }
    WHERE {
      GRAPH <http://example.org/foaf> {
        ?person foaf:title ?title .
      }
      FILTER(CONTAINS(?title, "Senior"))
    }
  ',
  target_graph => 'http://example.org/enriched'
);
```

Changes propagate through the chain: a new person in the source data triggers a new FOAF triple (layer 1), which triggers a new `ex:seniorStaff` triple (layer 2) if the title contains "Senior."

pg_ripple evaluates the chain in dependency order, using the same topological sort that Datalog uses for rule strata. Cycles are detected and rejected at view creation time.

---

## Combined with CDC

CONSTRUCT views integrate with CDC subscriptions. Subscribe to changes in a materialized view to drive downstream systems:

```sql
SELECT pg_ripple.cdc_subscribe(
  name      => 'api_person_changes',
  predicate => 'foaf:name',
  graph     => 'http://example.org/api_view'
);
```

Now every time the API view updates (because source data changed), a CDC event fires. Connect it to pg_trickle's outbox and relay, and your API consumers get notified in real time.

---

## When to Use CONSTRUCT Views vs. Datalog

| Feature | CONSTRUCT View | Datalog Rule |
|---------|---------------|-------------|
| Input | SPARQL WHERE pattern | Datalog body atoms |
| Output | Named graph with transformed triples | New triples in the base graph |
| Recursion | No (but chainable) | Yes (fixpoint) |
| Negation | FILTER NOT EXISTS | Stratified / WFS |
| Best for | Vocabulary mapping, API shaping | Inference, transitive closure |

Use CONSTRUCT views for non-recursive transformations: vocabulary alignment, property renaming, structural reshaping. Use Datalog for recursive inference: transitive closure, type propagation, rule chains.

Both produce materialized triples. Both support incremental updates. The choice is about expressiveness and intent.
