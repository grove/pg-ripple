[← Back to Blog Index](README.md)

# Change Data Capture for Knowledge Graphs

## Live triple subscriptions, JSON-LD events, and why CDC changes everything for RDF

---

You've loaded 10 million triples into your knowledge graph. Datalog inference has derived another 2 million. Downstream systems — dashboards, APIs, ML pipelines — need to know when things change.

The traditional approach: poll. Run a SPARQL query every 30 seconds. Diff the results. React to the differences.

The pg_ripple approach: subscribe. Register for changes to specific predicates. Get notified — in real time, inside a PostgreSQL transaction — the moment a triple is inserted, updated, or deleted.

---

## Per-Predicate Subscriptions

pg_ripple's CDC system operates at the predicate level. You subscribe to the predicates you care about:

```sql
-- Subscribe to changes in employee assignments
SELECT pg_ripple.cdc_subscribe(
  name      => 'assignment_changes',
  predicate => 'ex:assignedTo',
  callback  => 'notify'   -- use LISTEN/NOTIFY
);

-- Subscribe to new inferred alerts (from Datalog)
SELECT pg_ripple.cdc_subscribe(
  name      => 'alert_feed',
  predicate => 'ex:alert',
  include_inferred => true
);
```

When a triple with the subscribed predicate is inserted, deleted, or modified, pg_ripple fires a notification. The notification includes the full triple (decoded to IRIs/literals), the operation type (insert/delete), and a dedup key derived from the statement ID.

---

## The Notification Payload

Each CDC event is a JSON-LD document:

```json
{
  "@context": {
    "ex": "http://example.org/",
    "foaf": "http://xmlns.com/foaf/0.1/"
  },
  "operation": "insert",
  "subject": "ex:alice",
  "predicate": "ex:assignedTo",
  "object": "ex:project42",
  "graph": "ex:hr_graph",
  "statement_id": 847291,
  "source": "explicit",
  "dedup_key": "cdc:847291:insert",
  "timestamp": "2026-04-28T14:32:00Z"
}
```

The `dedup_key` is critical for exactly-once processing: downstream consumers that track processed keys can safely handle duplicates from retries or replay.

The `source` field distinguishes explicit triples (loaded by users) from inferred triples (derived by Datalog). This lets consumers filter — some pipelines only care about explicit facts; others want the full materialized closure.

---

## LISTEN/NOTIFY vs. Polling

pg_ripple supports two consumption patterns:

### Push: LISTEN/NOTIFY

```sql
LISTEN pg_ripple_cdc;

-- In another session, insert a triple:
SELECT pg_ripple.sparql_update('
  INSERT DATA { ex:alice ex:assignedTo ex:project42 }
');

-- The LISTEN session receives:
-- Async notification "pg_ripple_cdc" with payload:
-- {"subscription":"assignment_changes","op":"insert","s":"ex:alice","p":"ex:assignedTo","o":"ex:project42"}
```

Push is ideal for low-latency reactions: trigger a webhook, update a cache, send an alert. The notification arrives within the same transaction commit that created the triple.

### Pull: Polling the CDC Log

```sql
SELECT * FROM pg_ripple.cdc_poll(
  subscription => 'assignment_changes',
  since_sid    => 847000,
  limit        => 100
);
```

Polling is better for batch consumers that process changes in windows. The CDC log retains events for a configurable retention period (default: 7 days), allowing consumers to catch up after downtime.

---

## CDC for Inferred Triples

This is where it gets interesting. When Datalog inference runs and derives new triples, those triples fire CDC events just like explicit inserts. When DRed retraction removes an inferred triple, that fires a delete event.

This means downstream consumers can subscribe to *derived* predicates:

```sql
-- Subscribe to inferred "manages" relationships
SELECT pg_ripple.cdc_subscribe(
  name      => 'management_chain',
  predicate => 'ex:manages',
  include_inferred => true
);
```

When the org chart changes — Alice's direct report Bob is reassigned — Datalog rederives the transitive management chain. The inferred triples that changed fire CDC events. A dashboard subscribed to `ex:manages` updates automatically.

No polling. No diffing. No stale data.

---

## The Bridge to pg_trickle

CDC events are PostgreSQL-native, but most consumers aren't. They're Kafka topics, NATS subjects, REST webhooks, or message queues.

pg_ripple's CDC bridge worker writes events to a bridge table that's compatible with pg_trickle's outbox pattern:

```sql
-- Enable the CDC-to-outbox bridge
SELECT pg_ripple.cdc_enable_bridge(
  subscription => 'alert_feed',
  target_table => 'enriched_events'
);
```

From there, pg_trickle's relay takes over — delivering events to Kafka, NATS, SQS, or any configured sink. The triple starts in a VP table, fires a CDC event, lands in an outbox, and arrives in Kafka — all within a few hundred milliseconds, all transactionally consistent.

---

## Lifecycle Events

Beyond triple-level changes, pg_ripple emits lifecycle events for major operations:

- **Merge completed:** A background merge worker finished compacting delta into main.
- **Inference completed:** A `datalog_infer()` run finished with N new facts.
- **SHACL validation completed:** Validation found N violations.
- **Bulk load completed:** A file import finished with N triples loaded.

These events let operations teams monitor the graph's health without polling system tables:

```sql
SELECT pg_ripple.cdc_subscribe(
  name      => 'ops_events',
  predicate => '_lifecycle',
  lifecycle => true
);
```

---

## Event Serialization

CDC events can be serialized in multiple formats:

- **JSON-LD** (default): Full context, human-readable, compatible with any JSON consumer.
- **N-Triples**: Compact RDF serialization, compatible with any RDF toolkit.
- **Protobuf**: Binary format for high-throughput pipelines (planned).

The format is configurable per subscription:

```sql
SELECT pg_ripple.cdc_subscribe(
  name      => 'compact_feed',
  predicate => 'ex:measurement',
  format    => 'ntriples'
);
```

---

## When CDC Matters

If your knowledge graph is a static dataset loaded once and queried forever, CDC adds no value. But knowledge graphs are rarely static:

- **IoT data streams:** Sensors produce thousands of triples per second. Downstream analytics need to react in real time.
- **Compliance monitoring:** Regulatory changes trigger re-inference. Affected entities need to be flagged immediately.
- **Data integration hubs:** Multiple sources feed the graph. Downstream consumers need to see consolidated, enriched data as it arrives.
- **AI/ML pipelines:** Embedding models need to know when entities change so they can recompute vectors.

For all of these, the alternative to CDC is polling — running SPARQL queries on a timer and diffing results. Polling is simple, but it's wasteful (most polls find no changes), laggy (changes aren't detected until the next poll), and incorrect (you can miss changes between polls if the interval is too long).

CDC is the mechanism that makes a knowledge graph reactive instead of passive.
