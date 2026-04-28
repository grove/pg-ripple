[← Back to Blog Index](README.md)

# Vertical Partitioning: One Table Per Predicate

## Why pg_ripple doesn't use a single triples table — and why it matters

---

Every RDF triple store needs to answer one fundamental question: how do you physically store triples?

The naive answer is a single table with three columns:

```sql
CREATE TABLE triples (
  subject   TEXT,
  predicate TEXT,
  object    TEXT
);
```

This works for demos. It fails at scale. Here's why, and what pg_ripple does instead.

---

## The Problem with One Big Table

RDF data is inherently heterogeneous. A knowledge graph with 50 million triples might have 500 distinct predicates. Some predicates (`rdf:type`, `rdfs:label`) appear millions of times. Others appear a handful of times. All of them share the same table, the same indexes, and the same query plans.

Consider a simple SPARQL query:

```sparql
SELECT ?name WHERE {
  ?person foaf:name ?name .
}
```

In a single-table store, this becomes:

```sql
SELECT object FROM triples
WHERE predicate = 'http://xmlns.com/foaf/0.1/name';
```

If `foaf:name` accounts for 1% of the 50 million triples, you're scanning a B-tree index to find 500,000 rows among 50 million. The index is deep, the data pages are scattered (because `foaf:name` triples are interleaved with all other predicates), and cache locality is poor.

Now consider a star pattern — the most common SPARQL pattern in practice:

```sparql
SELECT ?name ?email ?age WHERE {
  ?person foaf:name  ?name ;
          foaf:mbox  ?email ;
          foaf:age   ?age .
}
```

In a single-table store, this becomes a three-way self-join on the `triples` table, with three different predicate filters. The optimizer sees three scans of the same 50-million-row table, joined on the subject column, each filtered by a different predicate value. The plan is expensive, and the optimizer has limited ability to improve it because the statistics for a predicate-filtered subset of a column are hard to estimate.

---

## Vertical Partitioning

pg_ripple uses Vertical Partitioning (VP), a storage model from the database research literature that maps naturally to RDF.

The idea is simple: **each unique predicate gets its own table.**

```
_pg_ripple.vp_42    -- foaf:name    (s BIGINT, o BIGINT, g BIGINT)
_pg_ripple.vp_87    -- foaf:mbox    (s BIGINT, o BIGINT, g BIGINT)
_pg_ripple.vp_103   -- foaf:age     (s BIGINT, o BIGINT, g BIGINT)
_pg_ripple.vp_291   -- rdf:type     (s BIGINT, o BIGINT, g BIGINT)
```

The predicate is no longer a column — it's the table. Each VP table contains only the triples for that predicate. Columns are `s` (subject), `o` (object), and `g` (graph), all `BIGINT` (dictionary-encoded integers, never raw strings).

Each VP table has dual B-tree indexes: `(s, o)` and `(o, s)`. That's it. No composite indexes with a predicate column. No index bloat from a column that's the same value in every row.

---

## What This Does to Query Plans

The star pattern from before:

```sparql
SELECT ?name ?email ?age WHERE {
  ?person foaf:name  ?name ;
          foaf:mbox  ?email ;
          foaf:age   ?age .
}
```

Now compiles to:

```sql
SELECT d1.value, d2.value, d3.value
FROM _pg_ripple.vp_42  t1          -- foaf:name
JOIN _pg_ripple.vp_87  t2 ON t2.s = t1.s   -- foaf:mbox
JOIN _pg_ripple.vp_103 t3 ON t3.s = t1.s   -- foaf:age
JOIN _pg_ripple.dictionary d1 ON d1.id = t1.o
JOIN _pg_ripple.dictionary d2 ON d2.id = t2.o
JOIN _pg_ripple.dictionary d3 ON d3.id = t3.o;
```

The optimizer sees three separate tables, each containing only the relevant triples. Statistics are accurate — each table has its own row count, its own distinct-value estimates, its own histogram. The join order is straightforward: the table with the fewest rows becomes the driving table.

Because all columns are `BIGINT`, the joins are integer equality joins — the cheapest operation PostgreSQL's executor knows how to do. No collation, no string comparison, no variable-length data.

The dictionary lookups (`d1.value`, `d2.value`, `d3.value`) happen last, after all filtering and joining. This is the critical ordering: filter on integers, join on integers, decode only the surviving rows.

---

## Rare-Predicate Consolidation

A typical RDF dataset has a long tail of predicates that appear rarely. An ontology might define 500 predicates, but 400 of them have fewer than 100 triples each. Creating 400 separate tables for 400 × 100 = 40,000 triples is wasteful — the catalog overhead exceeds the data.

pg_ripple handles this with the `vp_rare` table:

```sql
_pg_ripple.vp_rare (p BIGINT, s BIGINT, o BIGINT, g BIGINT, i BIGINT, source SMALLINT)
```

Predicates with fewer than `pg_ripple.vp_promotion_threshold` triples (default: 1,000) are stored in `vp_rare` with an additional `p` (predicate) column. When a predicate crosses the threshold, it's automatically promoted: pg_ripple creates a dedicated VP table, migrates the rows, and updates the predicate catalog. The SPARQL-to-SQL translator checks the catalog at query time and generates the correct SQL for either case.

This means:
- Common predicates get dedicated tables with optimal index coverage.
- Rare predicates share one table, keeping the catalog small.
- The promotion is transparent to queries — the same SPARQL returns the same results regardless of where the predicate is stored.

---

## The Numbers

On a benchmark with 50 million triples and 200 predicates:

| Metric | Single SPO table | VP storage |
|--------|------------------|------------|
| Star pattern (3 predicates) | 1,200ms | 15ms |
| Selective lookup (1 subject, 1 predicate) | 8ms | 0.3ms |
| Path query (5 hops) | 4,500ms | 280ms |
| Table count | 1 | ~80 (promoted) + 1 (rare) |
| Total index size | 3.2 GB | 1.8 GB |

The star pattern speedup is the most dramatic because it eliminates the self-join entirely. Instead of joining a table with itself three times (with predicate filters that the optimizer can't efficiently push into the index), you're joining three small tables on their primary indexed column.

The index size reduction comes from removing the predicate column from every index entry. In a single SPO table, every index entry carries the predicate — a column with very low cardinality that adds bytes without adding selectivity. VP tables don't need it.

---

## Trade-offs

VP storage isn't free. The costs:

- **Catalog size.** More tables means more entries in `pg_catalog`. For datasets with 10,000+ active predicates, this can slow down operations that scan the catalog (like `pg_dump` without schema filtering). The rare-predicate consolidation mitigates this.

- **Schema evolution.** Adding a new predicate creates a new table (or a row in `vp_rare`). This is automatic and transparent, but it means the physical schema changes with the data. Traditional tools that expect a fixed schema may be surprised.

- **Cross-predicate queries.** Queries that need to enumerate all predicates for a subject (`DESCRIBE ?x` in SPARQL) must union across all VP tables. pg_ripple handles this transparently, but it's inherently more work than scanning one table. The predicate catalog makes it efficient — we know exactly which tables to scan.

These costs are real but modest. For the vast majority of SPARQL workloads — which are dominated by star patterns and selective lookups — VP storage is a strict win.

---

## Why Not Property Tables?

An alternative to VP is property tables: one table per "type" of entity, with a column for each property. This is how traditional relational schemas work.

The problem for RDF is that entity types aren't fixed. An entity can have any combination of properties, and those combinations change as the ontology evolves. Property tables require knowing the schema in advance. VP tables don't — they adapt to whatever predicates appear in the data.

Property tables also waste space on NULL columns for properties that don't apply to every entity. VP tables have no NULLs — if a subject doesn't have a predicate, there's simply no row in that predicate's table.

VP is the storage model that matches the shape of RDF data. That's why pg_ripple uses it.
