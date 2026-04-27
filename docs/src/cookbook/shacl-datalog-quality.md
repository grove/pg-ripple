# Cookbook: SHACL + Datalog Data Quality Pipeline

**Goal.** Validate a bibliographic (or any domain) graph against SHACL rules, identify violations, repair or enrich the data with Datalog inference, and confirm quality in a single reproducible pipeline.

**Why pg_ripple.** SHACL validation and Datalog inference coexist in the same transaction. The repair-then-revalidate loop runs entirely in SQL — no round trips to an external validator.

**Time to first result.** ~10 minutes.

---

## The pattern

```
   raw data load
        │
        ▼
   define SHACL shapes
        │
        ▼
   run shacl_validate()  → violations found?
        │                        │
        │         yes            ▼
        │          ──── Datalog inference enriches missing triples
        │                        │
        │                        ▼
        │               run shacl_validate() again
        │                        │
        ▼         no violation   ▼
   accepted                  rejected / escalate
```

---

## Step 1 — Load an imperfect graph

The example uses a bibliographic graph where some books are missing mandatory metadata:

```sql
CREATE EXTENSION IF NOT EXISTS pg_ripple;

SELECT pg_ripple.load_turtle($TTL$
@prefix bib:    <https://example.org/bib/> .
@prefix schema: <https://schema.org/> .
@prefix dc:     <http://purl.org/dc/terms/> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .

bib:book1  a schema:Book ;
    dc:title   "Foundations of Databases" ;
    dc:creator  bib:author1 ;
    dc:date     "1995"^^xsd:gYear .

-- Intentionally missing: dc:creator and dc:date.
bib:book2  a schema:Book ;
    dc:title   "SPARQL 1.1 Query Language" .

bib:author1  a schema:Person ;
    schema:name "Abiteboul, Hull, Vianu" .
$TTL$);
```

## Step 2 — Define SHACL shapes

A book must have a title, at least one creator, and a publication date.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix schema: <https://schema.org/> .
@prefix dc:     <http://purl.org/dc/terms/> .

<https://shapes.example.org/BookShape>  a sh:NodeShape ;
    sh:targetClass  schema:Book ;
    sh:property [
        sh:path      dc:title ;
        sh:minCount  1 ;
        sh:datatype  <http://www.w3.org/2001/XMLSchema#string>
    ] ;
    sh:property [
        sh:path     dc:creator ;
        sh:minCount 1 ;
        sh:message  "A book must have at least one creator."
    ] ;
    sh:property [
        sh:path     dc:date ;
        sh:minCount 1 ;
        sh:message  "A book must have a publication date."
    ] .
$TTL$);
```

## Step 3 — Run the first validation pass

```sql
SELECT *
FROM   pg_ripple.shacl_validate()
WHERE  severity = 'Violation'
ORDER  BY focus_node, result_path;
```

Expected output:

```
focus_node         | result_path | message
───────────────────┼─────────────┼────────────────────────────────────
bib:book2          | dc:creator  | A book must have at least one creator.
bib:book2          | dc:date     | A book must have a publication date.
```

Two violations. Before escalating to a human reviewer, try to repair them with inference.

## Step 4 — Apply Datalog inference

Suppose you have a rule: *"if a book's W3C spec URI matches the known SPARQL spec, infer the working group as its creator"*. This is a domain-specific repair rule.

```sql
SELECT pg_ripple.load_rules($RULES$
# If a book has a known W3C spec URI, infer the W3C SPARQL WG as creator.
?book dc:creator ex:W3C_SPARQL_WG :-
    ?book a schema:Book ,
    ?book dc:title "SPARQL 1.1 Query Language" .

# Infer publication year from the spec publication record.
?book dc:date "2013"^^xsd:gYear :-
    ?book a schema:Book ,
    ?book dc:title "SPARQL 1.1 Query Language" .
$RULES$, 'bib_repair');

SELECT pg_ripple.infer('bib_repair');
```

For more general inference, materialise OWL RL axioms if your vocabulary uses `owl:sameAs` or `rdfs:subClassOf`:

```sql
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');
```

## Step 5 — Re-validate after inference

```sql
SELECT count(*) AS remaining_violations
FROM   pg_ripple.shacl_validate()
WHERE  severity = 'Violation';
```

If the count drops to zero, the data quality pipeline passes. If violations remain, escalate them to a data steward:

```sql
-- Export remaining violations as JSON for a ticket system.
SELECT jsonb_agg(to_jsonb(v))
FROM   pg_ripple.shacl_validate() v
WHERE  severity = 'Violation';
```

## Step 6 — Make the pipeline idempotent

Wrap Steps 3–5 in a function so it can be called after every load:

```sql
CREATE OR REPLACE FUNCTION run_quality_gate(
    rule_set TEXT DEFAULT 'bib_repair'
) RETURNS TABLE (violations BIGINT, violations_json JSONB) AS $$
BEGIN
    -- Re-run inference to pick up any new triples from the last load.
    PERFORM pg_ripple.infer(rule_set);

    RETURN QUERY
        SELECT count(*)::BIGINT,
               jsonb_agg(to_jsonb(v))
        FROM   pg_ripple.shacl_validate() v
        WHERE  v.severity = 'Violation';
END;
$$ LANGUAGE plpgsql;

SELECT * FROM run_quality_gate();
```

---

## Production patterns

### Hard-fail on load

Set `pg_ripple.shacl_mode = 'sync'` — any SPARQL UPDATE that creates a violation is rolled back immediately. Use this for schemas where invalid data must never enter the store.

### Async queue for review

Set `pg_ripple.shacl_mode = 'async'` — violations are written to `_pg_ripple.shacl_violations` without blocking the write. A periodic job checks the queue and routes flagged triples to a review workflow.

### Confidence-weighted violations

Combine SHACL with Datalog lattice confidence: only escalate violations where the offending triple has confidence below 0.7 (the data probably arrived by automated inference, not by human entry).

```sql
SELECT v.focus_node, v.result_message, conf.confidence
FROM   pg_ripple.shacl_validate() v
JOIN   LATERAL (
    SELECT CAST(o AS FLOAT) AS confidence
    FROM   pg_ripple.sparql(format(
        'SELECT ?conf WHERE { << <%s> %s ?o >> ex:confidence ?conf }',
        v.focus_node, v.result_path
    ))
) conf ON true
WHERE  v.severity = 'Violation'
  AND  conf.confidence < 0.7;
```

---

## See also

- [Validating Data Quality (SHACL)](../features/validating-data-quality.md) — all shape types and modes.
- [Reasoning and Inference (Datalog)](../features/reasoning-and-inference.md)
- [Cookbook: Audit Trail](audit-trail.md) — combine SHACL with PROV-O for a full evidence chain.
