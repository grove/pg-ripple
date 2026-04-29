# SHACL-SPARQL Rules and Constraints

Stock SHACL Core covers ~95 % of the constraints anyone needs in practice — cardinalities, datatypes, value ranges, property paths. The other 5 % is where SHACL Core runs out of expressiveness: cross-shape conditions, complex logical compositions, "this attribute must equal the sum of those attributes", and so on. **SHACL-SPARQL** ([Advanced Features](https://www.w3.org/TR/shacl-af/)) closes the gap by letting you embed a SPARQL query inside a shape.

pg_ripple supports `sh:SPARQLConstraint` (validation) and `sh:TripleRule` (inference).
`sh:SPARQLRule` (SPARQL-based inference) is **not yet supported** — see the note below.

---

## `sh:SPARQLConstraint` — custom validation

A `sh:SPARQLConstraint` runs an `ASK` or `SELECT` query for every focus node. If the query returns true (for `ASK`) or any rows (for `SELECT`), pg_ripple records a violation.

The classic example is *"a person's birth date must be earlier than their death date"* — not expressible in pure SHACL Core because it requires comparing two properties of the same node:

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:LifeSpanShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    sh:sparql [
        sh:message  "Birth date must be earlier than death date" ;
        sh:select   """
            SELECT $this WHERE {
                $this <https://example.org/birthDate> ?b ;
                      <https://example.org/deathDate> ?d .
                FILTER(?d <= ?b)
            }
        """ ;
    ] .
$TTL$);

-- Validate the whole store.
SELECT focus_node, message FROM pg_ripple.shacl_validate();
```

Inside the query, the special variable `$this` is bound to the focus node. The query is evaluated by the same SPARQL engine you would use for any other query, so anything you can write in SPARQL — property paths, FILTER, BIND, sub-SELECT — is fair game inside a constraint.

---

## `sh:TripleRule` — inference from shapes

A `sh:TripleRule` adds triples to the store for every focus node that matches the shape. It is the recommended SHACL-AF inference primitive in pg_ripple because it compiles directly to a Datalog rule.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .

ex:AdultRule a sh:NodeShape ;
    sh:targetClass <https://schema.org/Person> ;
    sh:rule [
        a sh:TripleRule ;
        sh:subject   sh:this ;
        sh:predicate ex:isAdult ;
        sh:object    "true"^^<http://www.w3.org/2001/XMLSchema#boolean> ;
    ] .
$TTL$);

-- Apply the rule.
SELECT pg_ripple.shacl_apply_rules();
```

Triples produced by `sh:TripleRule` are written with `source = 1` (inferred) — they coexist with explicit triples but stay distinguishable.

---

## `sh:SPARQLRule` — not yet supported

`sh:SPARQLRule` (SPARQL CONSTRUCT-based inference) is **not implemented** in v0.70.0.

When pg_ripple encounters a `sh:SPARQLRule` in a loaded SHACL document, it emits a PostgreSQL `WARNING` (error code PT481) and skips the rule without error. No inference is performed.

```
WARNING:  PT481: sh:SPARQLRule is not supported in this version; rule skipped
```

Full `sh:SPARQLRule` support is planned for v1.0.0. In the meantime, the equivalent inference can be expressed using the Datalog engine or a SPARQL CONSTRUCT writeback rule:

```sql
-- Alternative: Datalog rule (same semantics as an sh:SPARQLRule)
SELECT pg_ripple.add_rule(
    'adult(?person)',
    'type(?person, schema:Person), age(?person, ?a), ?a >= 18'
);
```

---

## When to use SHACL-SPARQL vs Datalog

Both can express custom validation and inference. Pick by audience:

| Concern | SHACL-SPARQL | Datalog |
|---|---|---|
| **Audience** | Data architects who already write SHACL | Engineers comfortable with logic programming |
| **Tooling** | Standard SHACL editors and validators | pg_ripple-specific `.pl`-style files |
| **Expressiveness** | Full SPARQL inside the shape | Recursion, magic sets, lattices, well-founded semantics |
| **Performance** | Each query runs once per focus node | Compiled to a single SQL `INSERT … SELECT` per stratum |
| **Recursion** | Limited — you can recurse manually with property paths | First-class — semi-naive evaluation, fixpoint |
| **Negation** | SPARQL `FILTER NOT EXISTS` | Stratified negation; well-founded semantics |

Rule of thumb: if the rule fits in one SHACL `sh:TripleRule`, write it in SHACL. If it is naturally recursive or needs negation, use Datalog.

---

## Performance notes

- `sh:SPARQLConstraint` is evaluated per focus node. For shapes whose target matches millions of nodes, pre-filter the target with a tighter `sh:targetClass` or `sh:targetSubjectsOf`.
- `sh:TripleRule` is reapplied on every `shacl_apply_rules()` call. It is *not* incremental. For incremental inference, use the Datalog engine.
- Both run in a single transaction; either everything succeeds or nothing changes.

---

## See also

- [Validating Data Quality](validating-data-quality.md) — SHACL Core constraints.
- [Reasoning & Inference](reasoning-and-inference.md) — the Datalog alternative.

---

## `sh:SPARQLConstraint` — custom validation

A `sh:SPARQLConstraint` runs an `ASK` or `SELECT` query for every focus node. If the query returns true (for `ASK`) or any rows (for `SELECT`), pg_ripple records a violation.

The classic example is *"a person's birth date must be earlier than their death date"* — not expressible in pure SHACL Core because it requires comparing two properties of the same node:

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:LifeSpanShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    sh:sparql [
        sh:message  "Birth date must be earlier than death date" ;
        sh:select   """
            SELECT $this WHERE {
                $this <https://example.org/birthDate> ?b ;
                      <https://example.org/deathDate> ?d .
                FILTER(?d <= ?b)
            }
        """ ;
    ] .
$TTL$);

-- Validate the whole store.
SELECT focus_node, message FROM pg_ripple.shacl_validate();
```

Inside the query, the special variable `$this` is bound to the focus node. The query is evaluated by the same SPARQL engine you would use for any other query, so anything you can write in SPARQL — property paths, FILTER, BIND, sub-SELECT — is fair game inside a constraint.

---

## `sh:SPARQLRule` — inference from validation shapes

A `sh:SPARQLRule` runs a `CONSTRUCT` query whose graph pattern is evaluated for every focus node, and the constructed triples are added to the store. This is essentially "Datalog spelled in SPARQL" — useful when your validation already lives in SHACL and you want to derive new facts on the same data without writing a separate Datalog rule set.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .

ex:AdultRule a sh:NodeShape ;
    sh:targetClass <https://schema.org/Person> ;
    sh:rule [
        a sh:SPARQLRule ;
        sh:construct """
            CONSTRUCT { $this a ex:Adult }
            WHERE     { $this <https://example.org/age> ?age .
                        FILTER(?age >= 18) }
        """ ;
    ] .
$TTL$);

-- Apply the rule.
SELECT pg_ripple.shacl_apply_rules();
```

Triples constructed by `sh:SPARQLRule` are written with `source = 1` (inferred) — they coexist with explicit triples but stay distinguishable.

---

## When to use SHACL-SPARQL vs Datalog

Both can express custom validation and inference. Pick by audience:

| Concern | SHACL-SPARQL | Datalog |
|---|---|---|
| **Audience** | Data architects who already write SHACL | Engineers comfortable with logic programming |
| **Tooling** | Standard SHACL editors and validators | pg_ripple-specific `.pl`-style files |
| **Expressiveness** | Full SPARQL inside the shape | Recursion, magic sets, lattices, well-founded semantics |
| **Performance** | Each query runs once per focus node | Compiled to a single SQL `INSERT … SELECT` per stratum |
| **Recursion** | Limited — you can recurse manually with property paths | First-class — semi-naive evaluation, fixpoint |
| **Negation** | SPARQL `FILTER NOT EXISTS` | Stratified negation; well-founded semantics |

Rule of thumb: if the rule fits in one SHACL shape, write it in SHACL-SPARQL. If it is naturally recursive or needs negation, use Datalog.

---

## Performance notes

- `sh:SPARQLConstraint` is evaluated per focus node. For shapes whose target matches millions of nodes, pre-filter the target with a tighter `sh:targetClass` or `sh:targetSubjectsOf`.
- `sh:SPARQLRule` is reapplied on every `shacl_apply_rules()` call. It is *not* incremental. For incremental inference, use the Datalog engine.
- Both run in a single transaction; either everything succeeds or nothing changes.

---

## See also

- [Validating Data Quality](validating-data-quality.md) — SHACL Core constraints.
- [Reasoning & Inference](reasoning-and-inference.md) — the Datalog alternative.
- [SHACL Advanced Features (W3C)](https://www.w3.org/TR/shacl-af/)
