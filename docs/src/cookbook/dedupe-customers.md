# Cookbook: Deduplicate Customer Records Across Systems

**Goal.** Two source systems each hold a list of customers. Some customers are in both, with subtle differences (`Robert Smith` vs `Bob Smith`, `JaneDoe` vs `Jane Q. Doe`). You want a unified view in which each real-world customer appears as one entity, while the original records remain auditable.

**Why pg_ripple.** Combines knowledge-graph embeddings (high recall), SHACL hard rules (safe), and `owl:sameAs` canonicalization (transparent at query time) — the three pieces a record-linkage pipeline needs, with no external services.

**Time to first result.** ~20 minutes.

> This recipe is the practical flavour of [Record Linkage and Entity Resolution](../features/record-linkage.md). Read that page first for the strategic background.

---

## Step 1 — Load both sources into named graphs

Named graphs preserve the original provenance of every record.

```sql
SELECT pg_ripple.load_turtle_into_graph('https://example.org/source/crm', $TTL$
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:crm/c1 foaf:name "Robert Smith"  ; foaf:mbox <mailto:rob@x.com> ; ex:dob "1985-03-12"^^<http://www.w3.org/2001/XMLSchema#date> .
ex:crm/c2 foaf:name "Jane Doe"      ; foaf:mbox <mailto:jane@x.com>; ex:dob "1990-07-09"^^<http://www.w3.org/2001/XMLSchema#date> .
$TTL$);

SELECT pg_ripple.load_turtle_into_graph('https://example.org/source/erp', $TTL$
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:erp/c1 foaf:name "Bob Smith"     ; foaf:mbox <mailto:rob@x.com> ; ex:dob "1985-03-12"^^<http://www.w3.org/2001/XMLSchema#date> .
ex:erp/c2 foaf:name "Jane Q. Doe"   ; foaf:mbox <mailto:j.doe@x.com>; ex:dob "1990-07-09"^^<http://www.w3.org/2001/XMLSchema#date> .
ex:erp/c3 foaf:name "Carl Larsen"   ; foaf:mbox <mailto:carl@y.com>; ex:dob "1972-11-02"^^<http://www.w3.org/2001/XMLSchema#date> .
$TTL$);
```

## Step 2 — Add structural context

Customers gain matching power when they have *related* facts (purchases, addresses, interaction history). Load whatever you have. The example below uses purchases:

```sql
SELECT pg_ripple.load_turtle_into_graph('https://example.org/source/crm', $TTL$
@prefix ex: <https://example.org/> .
ex:crm/c1 ex:purchased ex:product/widget1, ex:product/widget2 .
ex:crm/c2 ex:purchased ex:product/widget3 .
$TTL$);

SELECT pg_ripple.load_turtle_into_graph('https://example.org/source/erp', $TTL$
@prefix ex: <https://example.org/> .
ex:erp/c1 ex:purchased ex:product/widget1, ex:product/widget4 .
ex:erp/c2 ex:purchased ex:product/widget3, ex:product/widget5 .
$TTL$);
```

## Step 3 — Generate candidate pairs

Run **both** text and KGE candidate generators, then union the results. They catch different mistakes.

```sql
-- Text-embedding candidates.
SELECT pg_ripple.embed_entities();

CREATE TEMP TABLE candidates AS
SELECT s1, s2, similarity, 'text' AS source
FROM pg_ripple.suggest_sameas(threshold := 0.85);

-- KGE candidates.
SET pg_ripple.kge_enabled = on;
SELECT pg_ripple.kge_train(model := 'TransE', epochs := 100);

INSERT INTO candidates
SELECT s1, s2, similarity, 'kge'
FROM pg_ripple.find_alignments(
    source_graph := 'https://example.org/source/crm',
    target_graph := 'https://example.org/source/erp',
    threshold    := 0.85
);

SELECT * FROM candidates ORDER BY similarity DESC;
```

## Step 4 — Block unsafe merges with SHACL

The `dob` (date of birth) is immutable and unique per person. Two records with different DOBs cannot be the same person, no matter how similar their other attributes look.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:CustomerSafetyShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    # If owl:sameAs links two persons, their dob must agree.
    sh:property [ sh:path ex:dob ; sh:maxCount 1 ] .
$TTL$);

ALTER SYSTEM SET pg_ripple.shacl_mode = 'sync';
SELECT pg_reload_conf();
```

When `apply_sameas_candidates()` would create a violation (two `dob` values for the merged person), the insert is rejected — the unsafe merge cannot happen.

## Step 5 — Apply the auto-merge tier

```sql
-- High-confidence pairs auto-merge.
SELECT pg_ripple.apply_sameas_candidates(min_similarity := 0.95);

-- Mid-confidence pairs go to a review table.
CREATE TABLE customer_review_queue AS
SELECT DISTINCT ON (least(s1, s2), greatest(s1, s2))
       s1, s2, max(similarity) AS similarity
FROM   candidates
WHERE  similarity BETWEEN 0.85 AND 0.95
GROUP  BY s1, s2;

SELECT * FROM customer_review_queue ORDER BY similarity DESC;
```

A reviewer marks pairs as `accepted` or `rejected` in `customer_review_queue`; an `accepted` row triggers:

```sql
SELECT pg_ripple.insert_triple(s1, '<http://www.w3.org/2002/07/owl#sameAs>', s2),
       pg_ripple.insert_triple(s2, '<http://www.w3.org/2002/07/owl#sameAs>', s1)
FROM   customer_review_queue WHERE status = 'accepted';
```

## Step 6 — Query the unified graph

`pg_ripple.sameas_reasoning = on` (the default) means SPARQL queries see merged customers as one entity:

```sql
-- Total spend by Robert/Bob — combined across CRM and ERP.
SELECT * FROM pg_ripple.sparql($$
    PREFIX ex: <https://example.org/>
    SELECT (COUNT(?p) AS ?n) WHERE {
        <https://example.org/crm/c1> ex:purchased ?p .
    }
$$);
```

The query targets the CRM identifier, but `sameas_reasoning` rewrites it to include the ERP identifier transparently.

---

## Auditing the merge

Every merge action is captured by the [audit log](../reference/audit-log.md). Combined with [point_in_time](../features/temporal-and-provenance.md), a regulator can replay exactly what the system thought at any past timestamp.

```sql
SELECT ts, role, query
FROM   _pg_ripple.audit_log
WHERE  query ILIKE '%owl:sameAs%'
ORDER  BY ts DESC
LIMIT 50;
```

---

## See also

- [Record Linkage](../features/record-linkage.md) — strategic background.
- [Knowledge-Graph Embeddings](../features/knowledge-graph-embeddings.md)
- [Validating Data Quality](../features/validating-data-quality.md)
