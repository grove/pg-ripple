# R2RML — Map Relational Tables to RDF

Most enterprises already have their data in relational tables. Re-modelling that data as triples by hand is tedious and error-prone. **R2RML** ([W3C Recommendation](https://www.w3.org/TR/r2rml/)) is the standard mapping language that says, declaratively: *"this column becomes this predicate, this row becomes this subject IRI"*. pg_ripple ships an R2RML executor that runs the mapping inside the same database — no ETL pipeline required.

> Available since v0.55.0 via `pg_ripple.r2rml_load(mapping_ttl)`.

---

## Why R2RML?

| Without R2RML | With R2RML |
|---|---|
| Bespoke loader scripts per table | One mapping document, version-controlled |
| Mapping logic spread across application code | Mapping logic *is* the schema, in standard syntax |
| Re-running requires a full export | Re-runs incrementally; only changed rows produce new triples |
| Ontology drift is invisible | Ontology drift surfaces as SHACL violations |
| Hard to reason about what gets exported | The R2RML document *is* the contract |

If you already have an RDF triple store and are migrating from PostgreSQL, R2RML lets you avoid touching the source schema. If you are growing an RDF graph *alongside* a relational schema, it lets the two stay in sync with one declarative artefact.

---

## A worked example

Suppose you have a tiny relational schema:

```sql
CREATE TABLE customer (
    id          SERIAL PRIMARY KEY,
    full_name   TEXT NOT NULL,
    email       TEXT,
    country     CHAR(2)
);

CREATE TABLE purchase (
    id          SERIAL PRIMARY KEY,
    customer_id INT REFERENCES customer(id),
    sku         TEXT,
    amount_cents INT
);

INSERT INTO customer (full_name, email, country) VALUES
    ('Alice Chen', 'alice@example.org', 'US'),
    ('Bob Smith',  'bob@example.org',   'GB');

INSERT INTO purchase (customer_id, sku, amount_cents) VALUES
    (1, 'WIDGET-1', 1999),
    (2, 'WIDGET-2', 2999);
```

The R2RML mapping below turns each row into a triple cluster.

```sql
SELECT pg_ripple.r2rml_load($TTL$
@prefix rr:    <http://www.w3.org/ns/r2rml#> .
@prefix rml:   <http://semweb.mmlab.be/ns/rml#> .
@prefix ex:    <https://example.org/> .
@prefix foaf:  <http://xmlns.com/foaf/0.1/> .
@prefix schema:<https://schema.org/> .

# Map customer rows to ex:customer/{id}
<#CustomerMap>
    rr:logicalTable      [ rr:tableName "customer" ] ;
    rr:subjectMap        [ rr:template "https://example.org/customer/{id}" ;
                           rr:class    foaf:Person ] ;
    rr:predicateObjectMap [ rr:predicate foaf:name  ; rr:objectMap [ rr:column "full_name" ] ] ;
    rr:predicateObjectMap [ rr:predicate foaf:mbox  ; rr:objectMap [ rr:template "mailto:{email}" ;
                                                                     rr:termType rr:IRI ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:addressCountry ; rr:objectMap [ rr:column "country" ] ] .

# Map purchase rows to ex:purchase/{id}, with a foreign-key reference to the customer
<#PurchaseMap>
    rr:logicalTable      [ rr:tableName "purchase" ] ;
    rr:subjectMap        [ rr:template "https://example.org/purchase/{id}" ;
                           rr:class    schema:Order ] ;
    rr:predicateObjectMap [ rr:predicate schema:customer ;
                            rr:objectMap [ rr:template "https://example.org/customer/{customer_id}" ;
                                           rr:termType rr:IRI ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:sku    ; rr:objectMap [ rr:column "sku" ] ] ;
    rr:predicateObjectMap [ rr:predicate schema:price  ; rr:objectMap [ rr:column "amount_cents" ;
                                                                        rr:datatype <http://www.w3.org/2001/XMLSchema#integer> ] ] .
$TTL$);
-- Returns the count of triples produced.
```

After the call, your knowledge graph contains:

```turtle
<https://example.org/customer/1>
    a foaf:Person ;
    foaf:name "Alice Chen" ;
    foaf:mbox <mailto:alice@example.org> ;
    schema:addressCountry "US" .

<https://example.org/purchase/1>
    a schema:Order ;
    schema:customer <https://example.org/customer/1> ;
    schema:sku "WIDGET-1" ;
    schema:price 1999 .
```

…and SPARQL queries work immediately:

```sql
SELECT * FROM pg_ripple.sparql($$
    PREFIX schema: <https://schema.org/>
    PREFIX foaf:   <http://xmlns.com/foaf/0.1/>
    SELECT ?name ?sku WHERE {
        ?order schema:customer ?c ; schema:sku ?sku .
        ?c     foaf:name       ?name .
    }
$$);
```

---

## What pg_ripple's R2RML supports

| R2RML feature | Status |
|---|---|
| Subject maps with `rr:template`, `rr:column`, `rr:constant` | ✅ |
| Predicate-object maps with `rr:column`, `rr:template`, `rr:datatype`, `rr:language` | ✅ |
| `rr:class` shortcut on subject maps | ✅ |
| `rr:logicalTable` with `rr:tableName` or `rr:sqlQuery` (R2RML view) | ✅ |
| `rr:joinCondition` between two triples maps | ✅ |
| `rr:graphMap` (assign triples to a named graph) | ✅ |
| `rr:termType` (`rr:IRI`, `rr:Literal`, `rr:BlankNode`) | ✅ |
| RML extensions for non-SQL sources | ❌ — use a separate ETL step |

---

## Patterns and recipes

### Per-row provenance via a graph map

Route every triple from a table into its own named graph for downstream tenant isolation or audit:

```turtle
<#CustomerMap>
    rr:graphMap [ rr:template "https://example.org/source/customer-table" ] ;
    ...
```

### Soft-delete handling

Restrict what gets exported with `rr:sqlQuery`:

```turtle
<#ActiveCustomersMap>
    rr:logicalTable [ rr:sqlQuery "SELECT * FROM customer WHERE deleted_at IS NULL" ] ;
    ...
```

### Re-running incrementally

R2RML is idempotent: running it twice produces the same triples (dictionary IDs are deterministic from XXH3-128 hashes, so no duplicates accumulate). Schedule it as a cron job that runs after your relational ETL.

### Validate the result with SHACL

Pair every R2RML mapping with a SHACL shape that encodes the *intended* shape of the output. The shape catches mapping bugs and source-data drift in a single check:

```sql
SELECT pg_ripple.shacl_validate();
```

### Combining with FDW

`rr:tableName` accepts any table — including a foreign table provided by `postgres_fdw`. This lets you map a *remote* relational database into the local triple store without copying data.

---

## When **not** to use R2RML

- The source data is already RDF (use `load_turtle()` instead).
- The mapping is one-shot and you will never re-run it (a hand-crafted INSERT is faster to write).
- You need bidirectional sync (R2RML is one-way: relational → RDF).

---

## See also

- [Loading Data](loading-data.md) — for direct RDF formats.
- [Validating Data Quality](validating-data-quality.md) — pair every R2RML run with SHACL.
- [Cookbook: Knowledge graph from a relational catalogue](../cookbook/relational-to-rdf.md)
