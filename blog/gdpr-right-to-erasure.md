[← Back to Blog Index](README.md)

# GDPR Right-to-Erasure in a Knowledge Graph

## Deleting a person across every VP table, every inference, every embedding — in one transaction

---

A user requests deletion under GDPR Article 17. In a relational database, you find their rows and delete them. In a knowledge graph, "their rows" are scattered across dozens of VP tables, the dictionary, inferred triples, embeddings, provenance records, and the audit log.

Missing any one of these is a compliance violation.

pg_ripple provides `erase_subject()` — a single function that handles the complete erasure.

---

## The Problem

A knowledge graph stores information about a person across many predicates:

```turtle
ex:alice foaf:name "Alice Smith" .
ex:alice foaf:mbox "mailto:alice@example.com" .
ex:alice ex:department ex:engineering .
ex:alice ex:hireDate "2020-01-15"^^xsd:date .
ex:alice ex:manages ex:bob .
ex:alice ex:manages ex:carol .
ex:alice rdf:type ex:Employee .
ex:alice rdf:type ex:Manager .         # inferred via Datalog
ex:alice ex:worksAt ex:acme .
```

Each predicate is stored in a separate VP table. Inferred triples (`rdf:type ex:Manager`) are in the same tables but with `source = 1`. RDF-star annotations (provenance, timestamps) reference the person's triples. KGE embeddings encode the person's graph neighborhood. The dictionary contains the IRI.

Deleting Alice means:
1. Find and delete all triples where `ex:alice` is the **subject**.
2. Find and delete all triples where `ex:alice` is the **object** (e.g., `ex:bob ex:reportsTo ex:alice`).
3. Find and delete all RDF-star annotations about triples involving Alice.
4. Find and delete all inferred triples that were derived from Alice's explicit triples (DRed retraction).
5. Find and delete KGE embeddings for Alice.
6. Find and delete PROV-O provenance records for Alice's data.
7. Remove Alice's IRI from the dictionary (if no other references remain).
8. Log the erasure for the compliance record.

Doing this manually across 50+ VP tables is error-prone. One missed table and you've failed the GDPR requirement.

---

## The Function

```sql
SELECT pg_ripple.erase_subject('http://example.org/alice');
```

Returns:

```json
{
  "subject": "http://example.org/alice",
  "triples_deleted": {
    "explicit": 7,
    "inferred": 3,
    "annotations": 12,
    "as_object": 5
  },
  "embeddings_deleted": 1,
  "provenance_deleted": 4,
  "dictionary_cleaned": true,
  "timestamp": "2026-04-28T14:32:00Z"
}
```

One function call. One transaction. Every trace of the subject is removed.

---

## How It Works

### Step 1: Enumerate VP Tables

`erase_subject()` reads the predicate catalog to find every VP table where the subject's dictionary ID appears:

```sql
-- Find all VP tables containing triples about this subject
SELECT table_oid FROM _pg_ripple.predicates
WHERE EXISTS (
  SELECT 1 FROM _pg_ripple.vp_{id} WHERE s = subject_id
  UNION ALL
  SELECT 1 FROM _pg_ripple.vp_{id} WHERE o = subject_id
);
```

This covers both subject and object positions. If Alice is someone's manager (`ex:bob ex:reportsTo ex:alice`), that triple is found and deleted too.

### Step 2: Delete Explicit Triples

For each VP table, delete rows where `s = subject_id` or `o = subject_id`:

```sql
DELETE FROM _pg_ripple.vp_{id}_delta WHERE s = $1 OR o = $1;
-- Also mark in tombstones for main table rows
INSERT INTO _pg_ripple.vp_{id}_tombstones
SELECT s, o, g FROM _pg_ripple.vp_{id}_main WHERE s = $1 OR o = $1;
```

### Step 3: Retract Inferred Triples

DRed retraction runs for every deleted explicit triple. If Alice's `rdf:type ex:Employee` was the basis for the inferred `rdf:type ex:Manager`, the inferred triple is also retracted.

### Step 4: Delete Annotations

RDF-star quoted triples that reference the subject are found via the dictionary's `qt_s` and `qt_o` columns:

```sql
-- Find quoted triples involving the subject
SELECT id FROM _pg_ripple.dictionary
WHERE kind = 3 AND (qt_s = $1 OR qt_o = $1);
```

Each quoted triple ID is then treated as a subject and its annotations are deleted from VP tables.

### Step 5: Clean Embeddings and Provenance

```sql
DELETE FROM _pg_ripple.embeddings WHERE entity_id = $1;
DELETE FROM _pg_ripple.kge_embeddings WHERE entity_id = $1;
-- Delete PROV-O triples about this subject
-- (handled by the same VP table sweep)
```

### Step 6: Dictionary Cleanup

If the subject's IRI is no longer referenced by any VP table or quoted triple, it's removed from the dictionary:

```sql
DELETE FROM _pg_ripple.dictionary WHERE id = $1
AND NOT EXISTS (
  SELECT 1 FROM _pg_ripple.vp_rare WHERE s = $1 OR o = $1 OR p = $1
);
```

### Step 7: Audit Log

The erasure is logged (with the count of deleted items, but not the deleted data itself — that would defeat the purpose):

```sql
INSERT INTO _pg_ripple.erasure_log (subject_iri, deleted_at, summary)
VALUES ($1, now(), $2);
```

---

## Transactional Guarantee

The entire erasure runs in a single PostgreSQL transaction. If any step fails — a foreign key constraint, a permission error, a disk-full condition — the entire transaction rolls back. Either everything is deleted or nothing is. There's no partial erasure state.

This is a significant advantage over systems where erasure requires coordination across multiple services. A REST API that deletes from a triplestore, then from an embedding service, then from a provenance store, can fail between any two calls — leaving the subject partially erased.

---

## Batch Erasure

For bulk erasure (e.g., deleting all users who requested deletion this month):

```sql
SELECT pg_ripple.erase_subject(subject_iri)
FROM deletion_requests
WHERE status = 'pending';
```

Each erasure is its own transaction. For very large batches, this can be parallelized across multiple sessions.

---

## What Erasure Doesn't Do

- **Remove from backups.** `erase_subject()` removes data from the live database. Backups (pg_dump, PITR) still contain the data. Backup retention and rotation policies are a separate concern.
- **Remove from downstream consumers.** If CDC events were already published to Kafka, the downstream copies exist. Erasure events can be published via CDC to notify downstream systems.
- **Remove from logs.** PostgreSQL's WAL and any application logs may contain the data. Log rotation handles this over time; `erase_subject()` handles the database.

---

## When to Use It

GDPR Article 17 is the obvious case. But `erase_subject()` is useful whenever you need complete removal of an entity:

- **Test data cleanup.** Remove synthetic entities after load testing.
- **Decommissioning.** Remove a retired product, service, or organizational unit from the graph.
- **Data quality.** Remove entities that SHACL validation flagged as irredeemably broken.

The common thread: "remove this entity and every trace of it, correctly, completely, in one call." That's what `erase_subject()` does.
