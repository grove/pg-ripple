# Record Linkage and Entity Resolution

> Other names for this problem: *entity resolution*, *deduplication*, *master data management (MDM)*, *identity resolution*, *fuzzy matching*. They all mean: **find records that refer to the same real-world thing, and merge them safely.**

Record linkage is one of the most consequential and difficult problems in data management. A hospital merging two patient databases, a bank consolidating customer records, a retailer unifying online and in-store profiles — all of them must answer the same question: *do these two rows describe the same person?*

pg_ripple ships a complete record-linkage stack inside PostgreSQL. It combines four techniques that traditionally each lived in a separate tool:

1. **Knowledge-graph embeddings** for fast, fuzzy candidate generation.
2. **Vector similarity** over text embeddings for semantic matching.
3. **SHACL hard rules** to veto unsafe merges (e.g. *"never merge two patients with different blood types"*).
4. **`owl:sameAs` canonicalization** so the rest of your queries see one entity, not two.

Each step is a single SQL function call. Every decision is auditable.

---

## The pipeline

```
   Source A triples           Source B triples
        │                          │
        └────────┬─────────────────┘
                 │
                 ▼
   ┌─────────────────────────────┐
   │ 1. Generate candidate pairs │
   │    suggest_sameas()         │ ← KGE or text embeddings + HNSW
   │    or find_alignments()     │
   └──────────────┬──────────────┘
                  │ (s1, s2, similarity) rows
                  ▼
   ┌─────────────────────────────┐
   │ 2. Apply hard rules         │
   │    SHACL shapes block pairs │ ← e.g. sh:disjoint on bloodType
   │    that violate constraints │
   └──────────────┬──────────────┘
                  │ filtered candidate rows
                  ▼
   ┌─────────────────────────────┐
   │ 3. Human review (optional)  │
   │    surface candidates       │ ← UI / approval queue
   │    with similarity, source, │
   │    and rule trail           │
   └──────────────┬──────────────┘
                  │ accepted pairs
                  ▼
   ┌─────────────────────────────┐
   │ 4. Apply owl:sameAs         │
   │    apply_sameas_candidates  │ ← inserts both directions
   └──────────────┬──────────────┘
                  │
                  ▼
   ┌─────────────────────────────┐
   │ 5. Canonicalize on read     │
   │    sameas_reasoning = on    │ ← SPARQL & Datalog see one entity
   │    audit_log captures all   │
   │    UPDATEs                  │
   └─────────────────────────────┘
```

---

## A worked example: linking customer records across two systems

Suppose you operate two retail brands, each with its own customer database. You want a unified view without losing brand-specific history.

### Step 1: Load both sources into named graphs

Using named graphs preserves provenance — you can always tell which record came from where.

```sql
-- Source A
SELECT pg_ripple.load_turtle_into_graph(
    'https://example.org/source-a',
    $TTL$
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:a/c1  foaf:name  "Robert Smith"  ; foaf:mbox <mailto:rob.smith@example.com> .
ex:a/c2  foaf:name  "Jane Doe"      ; foaf:mbox <mailto:jdoe@example.com> .
$TTL$);

-- Source B
SELECT pg_ripple.load_turtle_into_graph(
    'https://example.org/source-b',
    $TTL$
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:b/c1  foaf:name  "Bob Smith"     ; foaf:mbox <mailto:rob.smith@example.com> .
ex:b/c2  foaf:name  "Jane Q. Doe"   ; foaf:mbox <mailto:jane.doe@example.com> .
$TTL$);
```

### Step 2: Generate candidate pairs

You have two options for candidate generation. Pick whichever fits your data.

**Option A — text embeddings.** Best when you have rich textual descriptions (names, addresses, product titles).

```sql
-- Embed every customer using their label.
SELECT pg_ripple.embed_entities();

-- Suggest sameAs pairs at a permissive threshold first.
SELECT s1, s2, similarity
FROM pg_ripple.suggest_sameas(threshold := 0.85)
ORDER BY similarity DESC
LIMIT 50;
```

**Option B — knowledge-graph embeddings (KGE).** Best when entities have rich relational structure (a customer's purchases, addresses, devices).

```sql
SET pg_ripple.kge_enabled = on;
SELECT pg_ripple.kge_train(model := 'TransE', epochs := 100);

SELECT * FROM pg_ripple.find_alignments(
    source_graph := 'https://example.org/source-a',
    target_graph := 'https://example.org/source-b',
    threshold    := 0.85
);
```

You can run both and union the results — text and KGE embeddings catch different mistakes.

### Step 3: Block unsafe merges with SHACL

Define hard rules that say *"these two records cannot be the same entity"*. The classic example is contradictory immutable attributes — different birth dates, different blood types, different national IDs.

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:CustomerSafetyShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    # If two people share owl:sameAs, their birth dates must agree.
    sh:property [
        sh:path ex:birthDate ;
        sh:disjoint ex:contradictoryBirthDate ;
    ] .
$TTL$);
```

When `pg_ripple.shacl_mode = 'sync'`, an `apply_sameas_candidates()` call that would violate this shape is rejected before commit.

### Step 4: Apply the surviving candidates

```sql
-- Apply at a strict threshold for auto-merge.
SELECT pg_ripple.apply_sameas_candidates(min_similarity := 0.95);

-- The remaining 0.85–0.95 band goes to a review queue.
CREATE TABLE review_queue AS
SELECT s1, s2, similarity
FROM pg_ripple.suggest_sameas(0.85)
WHERE similarity < 0.95;
```

A human reviewer can then approve or reject pairs from `review_queue`; approved pairs are applied with `pg_ripple.insert_triple(s1, '<http://www.w3.org/2002/07/owl#sameAs>', s2)` plus its inverse.

### Step 5: Query the unified graph

With `pg_ripple.sameas_reasoning = on` (default), every SPARQL and Datalog query sees the merged entity transparently. There is no separate "golden record" table to maintain.

```sql
-- Returns Robert/Bob Smith's purchases from BOTH source databases as one customer.
SELECT * FROM pg_ripple.sparql($$
    SELECT ?purchase WHERE {
        <https://example.org/a/c1>
            <https://example.org/purchased> ?purchase .
    }
$$);
```

---

## Tuning thresholds: precision vs. recall

| Threshold band | Precision | Recall | Recommended action |
|---|---|---|---|
| ≥ 0.98 | Very high | Low | Auto-apply with no review |
| 0.95 – 0.98 | High | Medium | Auto-apply, sample-audit weekly |
| 0.90 – 0.95 | Medium | High | Review queue for human approval |
| 0.85 – 0.90 | Low | Very high | Surface only for exploratory analysis |
| < 0.85 | Very low | Near-complete | Avoid — too noisy |

The right band depends on the cost of each error type. In healthcare a wrong merge can endanger a patient — push the auto-merge threshold to 0.99 and route the rest to clinicians. In ad-tech a missed merge means a less-personalised ad — a 0.92 auto-merge is fine.

---

## Auditability

Every record-linkage action leaves a trail. Three GUCs enable the audit chain:

| GUC | What it captures |
|---|---|
| `pg_ripple.audit_log_enabled = on` | All SPARQL UPDATEs (role, txid, query text) — see [Audit Log](../reference/audit-log.md) |
| `pg_ripple.prov_enabled = on` | A `prov:Activity` triple per bulk-load — see [Temporal & Provenance](temporal-and-provenance.md) |
| (RDF-star quoted triples) | Per-fact confidence, source, timestamp — see [Storing Knowledge](storing-knowledge.md) |

Together these three give you the regulator-defensible trail that pure-ML pipelines cannot.

---

## Why this is hard to get right elsewhere

Most record-linkage systems force a choice between *neural* (high-recall, opaque) and *symbolic* (auditable, low-recall). pg_ripple lets you compose both. The extended rationale is in `plans/neuro-symbolic-record-linkage.md` — that document is the strategic background for everything on this page.

---

## Functions reference

| Function | Purpose | Documented in |
|---|---|---|
| `embed_entities()` | Compute text embeddings for all labelled entities | [Vector & Hybrid Search](vector-and-hybrid-search.md) |
| `kge_train()` | Train a TransE/RotatE entity embedding | [Knowledge-Graph Embeddings](knowledge-graph-embeddings.md) |
| `suggest_sameas(threshold)` | Return candidate pairs from text embeddings | This page |
| `find_alignments(src, tgt, threshold)` | Return candidate pairs from KGE | [Knowledge-Graph Embeddings](knowledge-graph-embeddings.md) |
| `apply_sameas_candidates(min_similarity)` | Insert `owl:sameAs` for accepted pairs | This page |
| `load_shacl()` | Load hard-rule shapes that veto unsafe merges | [Validating Data Quality](validating-data-quality.md) |
| `point_in_time(ts)` | Replay record-linkage decisions as of a past timestamp | [Temporal & Provenance](temporal-and-provenance.md) |
