# Cookbook: Knowledge Graph from a Relational Catalogue

**Goal.** You have a working PostgreSQL schema for, say, a product catalogue. You want a queryable RDF graph that **stays in sync** with that schema, validates the output, and lets analysts answer relationship questions in SPARQL.

**Why pg_ripple.** Both schemas live in the same database. There is no ETL job, no eventual consistency, and a single SHACL contract guards the output.

**Time to first result.** ~15 minutes.

---

## What you build

```
   relational tables                 RDF triples                SPARQL clients
   ─────────────────                 ───────────                ──────────────
   product (id, name, …)   ──┐      <…/product/42> a Product
   category (id, parent_id) ──┼──►  <…/product/42> in <…/category/7>      ──►  pg_ripple.sparql(...)
   inventory (sku, qty)    ──┘      <…/product/42> sku "WIDGET-1"              "all out-of-stock products
                                    <…/category/7> parent <…/category/3>        in the toy hierarchy"
                                    …
                                              ▲
                                              │
                                       SHACL validation
                                       fires on every load
```

---

## Step 1 — The relational source

```sql
CREATE TABLE category (
    id        SERIAL PRIMARY KEY,
    name      TEXT NOT NULL,
    parent_id INT REFERENCES category(id)
);

CREATE TABLE product (
    id          SERIAL PRIMARY KEY,
    sku         TEXT UNIQUE NOT NULL,
    name        TEXT NOT NULL,
    category_id INT REFERENCES category(id),
    price_cents INT
);

CREATE TABLE inventory (
    product_id INT REFERENCES product(id),
    qty        INT NOT NULL DEFAULT 0
);
```

Populate it with a few rows; the recipe doesn't care about the data.

## Step 2 — Define the R2RML mapping

```sql
SELECT pg_ripple.r2rml_load($TTL$
@prefix rr:    <http://www.w3.org/ns/r2rml#> .
@prefix ex:    <https://example.org/cat/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix schema:<https://schema.org/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .

<#CategoryMap>
    rr:logicalTable [ rr:tableName "category" ] ;
    rr:subjectMap   [ rr:template "https://example.org/cat/{id}" ;
                      rr:class    skos:Concept ] ;
    rr:predicateObjectMap [ rr:predicate skos:prefLabel ; rr:objectMap [ rr:column "name" ] ] ;
    rr:predicateObjectMap [ rr:predicate skos:broader   ;
                            rr:objectMap [ rr:template "https://example.org/cat/{parent_id}" ;
                                           rr:termType rr:IRI ] ] .

<#ProductMap>
    rr:logicalTable [ rr:tableName "product" ] ;
    rr:subjectMap   [ rr:template "https://example.org/product/{id}" ;
                      rr:class    schema:Product ] ;
    rr:predicateObjectMap [ rr:predicate schema:sku   ; rr:objectMap [ rr:column "sku" ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:name  ; rr:objectMap [ rr:column "name" ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:category ;
                            rr:objectMap [ rr:template "https://example.org/cat/{category_id}" ;
                                           rr:termType rr:IRI ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:price ;
                            rr:objectMap [ rr:column "price_cents" ;
                                           rr:datatype xsd:integer ] ] .

<#InventoryMap>
    rr:logicalTable [ rr:sqlQuery "SELECT product_id, qty FROM inventory" ] ;
    rr:subjectMap   [ rr:template "https://example.org/product/{product_id}" ] ;
    rr:predicateObjectMap [ rr:predicate <https://example.org/onHand> ;
                            rr:objectMap [ rr:column "qty" ; rr:datatype xsd:integer ] ] .
$TTL$);
```

## Step 3 — Define a SHACL contract

This is the single most underappreciated trick in this recipe. The shape encodes the *intended shape* of the output. If the source schema drifts (a column is renamed, a foreign key is dropped), the SHACL run flags it immediately.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix schema:<https://schema.org/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/> .

<https://shapes.example.org/ProductShape> a sh:NodeShape ;
    sh:targetClass schema:Product ;
    sh:property [ sh:path schema:sku   ; sh:minCount 1 ; sh:maxCount 1 ] ;
    sh:property [ sh:path schema:name  ; sh:minCount 1 ; sh:datatype xsd:string ] ;
    sh:property [ sh:path schema:category ; sh:minCount 1 ; sh:nodeKind sh:IRI ] ;
    sh:property [ sh:path schema:price ; sh:datatype xsd:integer ; sh:minInclusive 0 ] ;
    sh:property [ sh:path ex:onHand    ; sh:maxCount 1 ; sh:datatype xsd:integer ] .
$TTL$);

ALTER SYSTEM SET pg_ripple.shacl_mode = 'sync';
SELECT pg_reload_conf();
```

## Step 4 — Re-run incrementally

Schedule the R2RML load to run after every relational ETL pass. Because the dictionary IDs are deterministic, repeated loads are idempotent — only changed rows generate new triples.

```sql
-- Cron job, once per minute:
SELECT pg_ripple.r2rml_reload();   -- shorthand: re-runs the most recent r2rml_load()
SELECT * FROM pg_ripple.shacl_validate() LIMIT 10;
```

If `shacl_validate()` returns rows, the relational source has drifted and an alert fires.

## Step 5 — Query in SPARQL

```sql
SELECT * FROM pg_ripple.sparql($$
    PREFIX schema: <https://schema.org/>
    PREFIX skos:   <http://www.w3.org/2004/02/skos/core#>
    PREFIX ex:     <https://example.org/>

    # Out-of-stock products in the "Toys" hierarchy.
    SELECT ?sku ?name WHERE {
        ?p a schema:Product ;
           schema:sku ?sku ;
           schema:name ?name ;
           schema:category/skos:broader* <https://example.org/cat/toys> ;
           ex:onHand 0 .
    }
$$);
```

The `skos:broader*` property path walks the category hierarchy with no recursion-depth cap. Try expressing that in pure SQL and you will rediscover why graph queries exist.

---

## Variations

- **Multi-source.** Add a `rr:graphMap` to each map so triples land in per-source named graphs (`https://example.org/source/postgres-prod`). Then [Multi-Tenant Graphs](../features/multi-tenant-graphs.md) gives you per-source RLS.
- **Soft delete.** Replace `rr:tableName` with an `rr:sqlQuery` that filters out `deleted_at IS NOT NULL`. Re-runs will *remove* the corresponding triples.
- **Foreign-data wrapper.** `rr:tableName` accepts foreign tables, so you can ingest a remote PostgreSQL or MySQL schema without copying data.

---

## See also

- [R2RML](../features/r2rml.md)
- [Validating Data Quality](../features/validating-data-quality.md)
