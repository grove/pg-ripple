[← Back to Blog Index](README.md)

# R2RML: Your Relational Tables Are Already a Knowledge Graph

## Map existing PostgreSQL tables to RDF without copying a single row

---

You have a `customers` table, a `products` table, and an `orders` table. Together they contain a knowledge graph — entities with types, properties, and relationships. But they're in relational form: rows and columns, not triples.

R2RML is the W3C standard for mapping relational data to RDF. pg_ripple implements it, which means your existing PostgreSQL tables can participate in SPARQL queries without ETL, without data duplication, and without changing your relational schema.

---

## The Mapping

An R2RML mapping declares how to convert rows into triples:

```turtle
@prefix rr: <http://www.w3.org/ns/r2rml#> .

ex:CustomerMapping a rr:TriplesMap ;
  rr:logicalTable [ rr:tableName "customers" ] ;
  rr:subjectMap [
    rr:template "http://example.org/customer/{id}" ;
    rr:class schema:Customer ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate foaf:name ;
    rr:objectMap [ rr:column "name" ] ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate schema:email ;
    rr:objectMap [ rr:column "email" ] ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate ex:region ;
    rr:objectMap [ rr:column "region" ] ;
  ] .
```

This mapping says:
- Each row in `customers` becomes an entity with IRI `http://example.org/customer/{id}`.
- Each entity has type `schema:Customer`.
- The `name` column maps to `foaf:name`.
- The `email` column maps to `schema:email`.
- The `region` column maps to `ex:region`.

---

## Loading the Mapping

```sql
-- First, load the R2RML mapping document as RDF
SELECT pg_ripple.load_turtle('
  @prefix rr: <http://www.w3.org/ns/r2rml#> .
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .
  @prefix schema: <https://schema.org/> .
  @prefix ex: <http://example.org/> .

  ex:CustomerMapping a rr:TriplesMap ;
    rr:logicalTable [ rr:tableName "customers" ] ;
    rr:subjectMap [
      rr:template "http://example.org/customer/{id}" ;
      rr:class schema:Customer ;
    ] ;
    rr:predicateObjectMap [
      rr:predicate foaf:name ;
      rr:objectMap [ rr:column "name" ] ;
    ] ;
    rr:predicateObjectMap [
      rr:predicate schema:email ;
      rr:objectMap [ rr:column "email" ] ;
    ] .
');

-- Execute the mapping: read the relational table, generate triples
SELECT pg_ripple.r2rml_load('http://example.org/CustomerMapping');
```

`r2rml_load()` reads the mapping from the graph, executes the SQL against the relational table, generates RDF triples, and bulk-inserts them into VP tables.

---

## What Gets Generated

For a `customers` table with:

| id | name | email |
|----|------|-------|
| 1 | Alice | alice@example.com |
| 2 | Bob | bob@example.com |

The mapping produces:

```turtle
<http://example.org/customer/1> a schema:Customer ;
  foaf:name "Alice" ;
  schema:email "alice@example.com" .

<http://example.org/customer/2> a schema:Customer ;
  foaf:name "Bob" ;
  schema:email "bob@example.com" .
```

These triples are stored in VP tables, indexed, and queryable with SPARQL — alongside any other triples in the graph.

---

## Join Mappings

R2RML can map foreign key relationships to RDF properties:

```turtle
ex:OrderMapping a rr:TriplesMap ;
  rr:logicalTable [ rr:tableName "orders" ] ;
  rr:subjectMap [
    rr:template "http://example.org/order/{id}" ;
    rr:class schema:Order ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate schema:customer ;
    rr:objectMap [
      rr:parentTriplesMap ex:CustomerMapping ;
      rr:joinCondition [
        rr:child "customer_id" ;
        rr:parent "id" ;
      ] ;
    ] ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate schema:totalPrice ;
    rr:objectMap [ rr:column "total" ; rr:datatype xsd:decimal ] ;
  ] .
```

The `rr:parentTriplesMap` + `rr:joinCondition` generates a foreign key triple:

```turtle
<http://example.org/order/42> schema:customer <http://example.org/customer/1> .
```

No explicit join in the RDF — the relationship is a property that SPARQL can traverse.

---

## SQL Views as Sources

R2RML supports SQL views as sources, not just base tables:

```turtle
ex:ActiveCustomerMapping a rr:TriplesMap ;
  rr:logicalTable [
    rr:sqlQuery """
      SELECT c.id, c.name, c.email, COUNT(o.id) AS order_count
      FROM customers c
      JOIN orders o ON o.customer_id = c.id
      WHERE c.active = true
      GROUP BY c.id, c.name, c.email
    """
  ] ;
  rr:subjectMap [
    rr:template "http://example.org/customer/{id}" ;
    rr:class ex:ActiveCustomer ;
  ] ;
  rr:predicateObjectMap [
    rr:predicate ex:orderCount ;
    rr:objectMap [ rr:column "order_count" ; rr:datatype xsd:integer ] ;
  ] .
```

This lets you project aggregated, filtered, or joined views of your relational data into the knowledge graph. The SQL query runs once during `r2rml_load()` and the results become triples.

---

## Why Not Just Use a Wrapper?

Some systems (e.g., Ontop, D2RQ) provide virtual R2RML mappings — SPARQL queries are rewritten on-the-fly to SQL against the relational tables, without materializing triples.

pg_ripple materializes instead. The reasons:

1. **Join performance.** Materialized triples in VP tables participate in pg_ripple's star-join optimization, leapfrog triejoin, and HTAP merge. Virtual mappings rewrite every SPARQL query to a SQL query against the original tables, which may not have the right indexes.

2. **Inference.** Datalog rules operate on materialized triples. You can't infer new facts from virtual triples without materializing the derivation.

3. **Consistency.** Materialized triples are part of the same snapshot as other graph data. Virtual mappings read from the relational tables at query time, which may have changed since the last SPARQL query.

The trade-off is freshness: materialized mappings are stale as soon as the relational table changes. To keep them fresh, re-run `r2rml_load()` periodically, or use CDC triggers to incrementally update the mapped triples.

---

## The Integration Pattern

For teams with existing relational schemas that want to add a knowledge graph layer:

1. **Define R2RML mappings** for the relational tables you want in the graph.
2. **Load the mappings** with `r2rml_load()`.
3. **Add SHACL shapes** to validate the mapped data.
4. **Add Datalog rules** to derive new facts from the mapped data.
5. **Query with SPARQL** — the mapped data is now first-class graph data.
6. **Schedule re-mapping** with `pg_cron` or CDC triggers to keep the graph fresh.

The relational tables don't change. The applications that use them don't change. The knowledge graph is an additive layer — it consumes relational data and enriches it with graph capabilities.

This is the migration path for teams that want knowledge graph features without rewriting their data layer. Start with R2RML mappings. Add SPARQL queries where they provide value. Keep the relational schema as the source of truth.
