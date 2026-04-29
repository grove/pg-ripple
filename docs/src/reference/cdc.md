# CDC (Change Data Capture) Reference

This page is the reference for pg_ripple's CDC (Change Data Capture) integration.

## Overview

pg_ripple can subscribe to PostgreSQL logical replication streams and convert
row-level change events into RDF triples. The CDC bridge supports:

- PostgreSQL logical decoding via `pg_logical_slot_get_changes()`
- Configurable table-to-RDF mapping (subject template, predicate template, object template)
- Outbox bridge worker for reliable at-least-once delivery
- CDC lifecycle events (insert, update, delete) mapped to named graphs
- JSON-LD event serialization for downstream consumers

All CDC-ingested triples are recorded in the mutation journal, ensuring
CONSTRUCT writeback rules fire for CDC-derived data.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE 'cdc%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.register_cdc_subscription(slot_name TEXT, mapping_name TEXT, target_graph TEXT) → void` | Register a CDC subscription |
| `pg_ripple.drop_cdc_subscription(slot_name TEXT) → void` | Remove a CDC subscription |
| `pg_ripple.list_cdc_subscriptions() → SETOF record` | List active CDC subscriptions |
| `pg_ripple.process_cdc_batch(slot_name TEXT, batch_size INT) → BIGINT` | Process next batch from a slot |

## Mapping Configuration

Each CDC subscription references a `mapping_name` that defines how table rows
are converted to triples. The mapping specifies:

- **Subject template**: URI pattern using column values (e.g., `http://example.org/employee/{id}`)
- **Predicate mapping**: column name → predicate IRI
- **Object type**: literal (with datatype/lang), IRI, or blank node

Mappings are stored in `_pg_ripple.cdc_subscriptions`.

## PG18 Logical Decoding

On PostgreSQL 18, the CDC bridge uses the built-in logical replication
infrastructure. A replication slot is created for each subscription, and
the bridge worker polls for changes using `pg_logical_slot_get_changes()`.

## Reliability and Delivery

The outbox bridge pattern ensures at-least-once delivery:
1. Changes are first written to `_pg_ripple.cdc_outbox`.
2. The bridge worker processes the outbox transactionally.
3. Successfully processed entries are deleted; failures are retried.

## Related Pages

- [Architecture Internals](architecture.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
