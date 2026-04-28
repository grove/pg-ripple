[← Back to Blog Index](README.md)

# SHACL: Schema Validation for the Schema-Less

## Declarative data quality rules for RDF, enforced inside PostgreSQL

---

RDF is schema-less by design. You can say anything about anything. `ex:alice foaf:age "banana"` is a perfectly legal triple. So is `ex:alice foaf:age 25` and `ex:alice foaf:age 25, 26, 27`. RDF doesn't care.

Your application does. Your API expects age to be an integer, singular, and between 0 and 150. Your SPARQL queries assume every person has exactly one name. Your compliance reports assume every entity has a creation date.

SHACL (Shapes Constraint Language) is the W3C standard for declaring these expectations as machine-readable shapes, and pg_ripple enforces them inside PostgreSQL.

---

## What SHACL Looks Like

A SHACL shape declares constraints on a set of nodes:

```turtle
ex:PersonShape a sh:NodeShape ;
  sh:targetClass foaf:Person ;
  sh:property [
    sh:path foaf:name ;
    sh:minCount 1 ;        # At least one name
    sh:maxCount 1 ;        # At most one name
    sh:datatype xsd:string ;  # Must be a string
  ] ;
  sh:property [
    sh:path foaf:age ;
    sh:maxCount 1 ;
    sh:datatype xsd:integer ;
    sh:minInclusive 0 ;
    sh:maxInclusive 150 ;
  ] ;
  sh:property [
    sh:path foaf:mbox ;
    sh:minCount 1 ;
    sh:pattern "^mailto:" ;  # Must start with mailto:
  ] .
```

This shape says: every `foaf:Person` must have exactly one string name, at most one integer age between 0 and 150, and at least one email starting with `mailto:`.

In pg_ripple, you load this shape and it becomes an enforceable constraint:

```sql
SELECT pg_ripple.shacl_load_shapes('
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .
  @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
  @prefix ex: <http://example.org/> .

  ex:PersonShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    sh:property [
      sh:path foaf:name ;
      sh:minCount 1 ;
      sh:maxCount 1 ;
      sh:datatype xsd:string ;
    ] .
');
```

---

## How pg_ripple Enforces SHACL

pg_ripple compiles SHACL shapes into two enforcement mechanisms:

### 1. DDL Constraints (Synchronous)

Some SHACL constraints map directly to SQL constraints:

| SHACL Constraint | SQL Equivalent |
|-----------------|----------------|
| `sh:maxCount 1` | `UNIQUE (s)` on the VP table |
| `sh:datatype xsd:integer` | CHECK constraint on decoded value |
| `sh:minInclusive / sh:maxInclusive` | CHECK constraint with range |
| `sh:pattern` | CHECK constraint with regex |

These constraints are enforced synchronously — a triple that violates them is rejected at INSERT time with a standard PostgreSQL error.

```sql
-- This fails with a constraint violation:
SELECT pg_ripple.sparql_update('
  INSERT DATA {
    ex:alice foaf:name "Alice" .
    ex:alice foaf:name "Alicia" .  -- Violates sh:maxCount 1
  }
');
-- ERROR: SHACL violation: foaf:name on ex:alice exceeds sh:maxCount 1
```

### 2. Async Validation Pipeline (Deferred)

Some SHACL constraints can't be expressed as SQL DDL — they involve graph patterns, property paths, or cross-entity checks:

| SHACL Constraint | Why It Can't Be DDL |
|-----------------|---------------------|
| `sh:minCount 1` (on target class) | Requires checking ALL members of a class |
| `sh:qualifiedValueShape` | Involves nested graph patterns |
| `sh:equals`, `sh:disjoint` | Cross-property comparisons |
| `sh:sparql` (custom SPARQL) | Arbitrary graph queries |

These constraints are validated asynchronously by a background worker that runs after data changes:

```sql
-- Validate all loaded shapes against current data
SELECT pg_ripple.shacl_validate();
```

The validation report follows the W3C SHACL Validation Report format:

```sql
SELECT * FROM pg_ripple.shacl_validate();
-- Returns: focus_node | path | constraint | severity | message
-- ex:bob   | foaf:mbox | sh:minCount  | Violation | expected at least 1 value
```

---

## SHACL Hints for the Query Planner

This is where SHACL gets interesting for performance, not just data quality.

When the SPARQL-to-SQL translator generates a query plan, it checks the loaded SHACL shapes for hints:

### sh:maxCount 1 → No DISTINCT Needed

If a property is declared `sh:maxCount 1`, the translator knows the join on that property will never produce duplicates. It can omit `DISTINCT` from the generated SQL, avoiding a sort or hash step.

For a query joining 5 properties, each with `sh:maxCount 1`, this eliminates 5 deduplication operations. On large result sets, the speedup is measurable — 20–30% for queries that would otherwise need `DISTINCT`.

### sh:minCount 1 → INNER JOIN Instead of LEFT JOIN

If a property is declared `sh:minCount 1` on the target class, every entity in the class has at least one value for that property. The translator can use `INNER JOIN` instead of `LEFT JOIN`, which gives the optimizer more freedom (inner joins can be reordered; left joins can't).

### sh:datatype → Type-Aware Comparison

If a property is declared `sh:datatype xsd:integer`, the translator can use numeric comparison operators instead of string comparison when applying FILTER expressions. This avoids the decode-compare-encode round trip for numeric filters.

---

## The Full SHACL Core Constraint Set

pg_ripple implements all 35 SHACL Core constraints as of v0.48.0:

**Value type constraints:** `sh:class`, `sh:datatype`, `sh:nodeKind`

**Cardinality constraints:** `sh:minCount`, `sh:maxCount`

**Value range constraints:** `sh:minInclusive`, `sh:maxInclusive`, `sh:minExclusive`, `sh:maxExclusive`

**String constraints:** `sh:minLength`, `sh:maxLength`, `sh:pattern`, `sh:languageIn`, `sh:uniqueLang`

**Property pair constraints:** `sh:equals`, `sh:disjoint`, `sh:lessThan`, `sh:lessThanOrEquals`

**Logical constraints:** `sh:not`, `sh:and`, `sh:or`, `sh:xone`

**Shape-based constraints:** `sh:node`, `sh:qualifiedValueShape`, `sh:qualifiedMinCount`, `sh:qualifiedMaxCount`

**Other constraints:** `sh:closed`, `sh:ignoredProperties`, `sh:hasValue`, `sh:in`

---

## SHACL + Datalog

SHACL shapes can reference Datalog-derived predicates. If your ontology infers `rdf:type` relationships through Datalog rules, SHACL constraints that target `rdf:type` classes are enforced against the inferred data, not just the explicit data.

This means:

1. Load your ontology and SHACL shapes.
2. Run Datalog inference to derive implicit type assertions.
3. Run SHACL validation to check that the inferred types satisfy the shapes.

Example: a SHACL shape requires every `ex:Employee` to have a `foaf:mbox`. Through RDFS inference, `ex:Manager rdfs:subClassOf ex:Employee`, so Alice (a Manager) is also an Employee. If Alice doesn't have a `foaf:mbox`, SHACL validation catches it — even though Alice was never explicitly typed as an Employee.

---

## When SHACL Is Not Enough

SHACL Core covers a wide range of constraints. But some data quality rules require custom logic:

- **Cross-graph constraints:** "Every entity in the production graph must also exist in the quality-assurance graph." SHACL doesn't natively support cross-graph patterns, though `sh:sparql` constraints (SHACL-SPARQL) can express them.
- **Temporal constraints:** "Every medication order must have a start date before its end date." This requires date comparison logic that `sh:lessThan` can express if the paths are set up correctly.
- **External validation:** "Every IRI in the `foaf:homepage` property must resolve to a live URL." SHACL can't call external services.

For these cases, SHACL-SPARQL constraints (supported since v0.53.0) let you write arbitrary SPARQL queries as validation rules:

```turtle
ex:LiveHomepageShape a sh:NodeShape ;
  sh:targetClass foaf:Person ;
  sh:sparql [
    sh:select """
      SELECT $this WHERE {
        $this foaf:homepage ?url .
        FILTER NOT EXISTS { $this foaf:homepageVerified true }
      }
    """ ;
    sh:message "Person has unverified homepage" ;
  ] .
```

This is the escape hatch: when SHACL Core constraints can't express what you need, SHACL-SPARQL lets you write a SPARQL query that pg_ripple evaluates as part of the validation pipeline.

---

## The Point

RDF's schema-lessness is a feature for data integration — you can merge datasets without schema alignment. But it's a liability for data quality — there's no built-in enforcement.

SHACL bridges the gap: declare your expectations as shapes, and pg_ripple enforces them. The shapes live alongside the data, version-controlled and machine-readable. When the expectations change, update the shapes. When the data violates the shapes, get a validation report that tells you exactly what's wrong and where.

No migration scripts. No ALTER TABLE. Just shapes and triples.
