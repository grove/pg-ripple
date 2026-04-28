[← Back to Blog Index](README.md)

# The Semantic Hub: pg_ripple × pg_trickle Relay

## Using a knowledge graph as the integration layer between event sources and consumers

---

You have Kafka topics with order events. NATS subjects with sensor readings. Webhooks from CRM systems. Each speaks a different schema, uses different identifiers for the same entities, and publishes at different cadences.

Somewhere downstream, consumers need a unified view: the same customer across all sources, enriched with inferred relationships, validated against quality rules, and delivered to Kafka, NATS, or webhooks in a schema they understand.

This is the integration hub pattern. Most teams build it with Kafka Connect, schema registries, stream processors, and a lot of YAML. pg_ripple and pg_trickle build it inside PostgreSQL.

---

## The Architecture

```
  INBOUND                    SEMANTIC HUB                    OUTBOUND
  ───────                    ────────────                    ────────

  Kafka ──┐                                               ┌── NATS
           │  pg-trickle    ┌─────────────┐  pg-trickle  │
  NATS  ──┼── relay ──────▶│  pg_ripple   │──── relay ──┼── Kafka
           │  (reverse)     │             │  (forward)   │
  HTTP  ──┘                 │  Inference  │               └── Webhooks
                            │  Validation │
                            │  Resolution │
                            │  CDC events │
                            └─────────────┘
```

Both extensions live in the same PostgreSQL 18 instance. pg_trickle handles the transport — its relay CLI speaks Kafka, NATS, SQS, Redis Streams, and HTTP. pg_ripple handles the semantics — vocabulary alignment, entity resolution, Datalog inference, SHACL validation, and SPARQL query.

They share the same transaction context. A Kafka message that arrives via pg_trickle's reverse relay, gets transformed to RDF, triggers Datalog inference, passes SHACL validation, and lands in pg_trickle's outbox for forward relay — all of that can happen within a single PostgreSQL transaction.

---

## Inbound: Sources to Graph

### Step 1: Relay Delivers Events to an Inbox

pg_trickle's relay process runs outside PostgreSQL as a lightweight binary. In reverse mode, it consumes from external sources and writes to inbox tables:

```sql
-- Kafka topic → pg_trickle inbox
SELECT pgtrickle.set_relay_inbox(
  'order-events',
  inbox  => 'order_inbox',
  source => '{"type":"kafka","brokers":"kafka:9092","topic":"orders.events"}'
);
```

Events arrive as JSON rows in `order_inbox`:

```json
{
  "event_id": "kafka:orders.events:0:1234",
  "event_type": "order_created",
  "payload": {
    "order_id": "ORD-5678",
    "customer_id": "CUST-42",
    "total": 299.99,
    "items": [{"sku": "WIDGET-A", "qty": 3}]
  }
}
```

### Step 2: Transform JSON to RDF

A trigger on the inbox transforms each JSON event into RDF triples:

```sql
CREATE OR REPLACE FUNCTION transform_order_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
  order_iri TEXT;
  ntriples TEXT;
BEGIN
  order_iri := '<https://example.org/order/' || NEW.payload->>'order_id' || '>';

  ntriples := order_iri || ' a <https://schema.org/Order> .' || E'\n'
    || order_iri || ' <https://schema.org/customer> <https://example.org/customer/'
    || NEW.payload->>'customer_id' || '> .' || E'\n'
    || order_iri || ' <https://schema.org/totalPrice> "'
    || (NEW.payload->>'total')
    || '"^^<http://www.w3.org/2001/XMLSchema#decimal> .';

  PERFORM pg_ripple.load_ntriples(ntriples, false);
  RETURN NEW;
END;
$$;

CREATE TRIGGER order_to_rdf
  AFTER INSERT ON order_inbox
  FOR EACH ROW EXECUTE FUNCTION transform_order_to_rdf();
```

Each Kafka message becomes RDF triples in the knowledge graph — inside the same transaction that consumed the message.

### Step 3: Enrich with Inference

Datalog rules derive new facts from the ingested data:

```sql
-- Infer VIP customers from order history
SELECT pg_ripple.datalog_add_rule(
  'vip_customer(C) :- schema_customer(O, C), schema_totalPrice(O, V), V > 10000.'
);

-- Entity resolution: match customers across sources by email
SELECT pg_ripple.datalog_add_rule(
  'owl_sameAs(C1, C2) :- schema_email(C1, E), schema_email(C2, E), C1 != C2.'
);
```

### Step 4: Validate with SHACL

Quality rules catch problems before they propagate downstream:

```turtle
ex:OrderShape a sh:NodeShape ;
  sh:targetClass schema:Order ;
  sh:property [
    sh:path schema:customer ;
    sh:minCount 1 ;
    sh:maxCount 1 ;
    sh:class schema:Customer ;
  ] ;
  sh:property [
    sh:path schema:totalPrice ;
    sh:minCount 1 ;
    sh:datatype xsd:decimal ;
    sh:minExclusive 0 ;
  ] .
```

Orders that fail validation are flagged, not silently ingested.

---

## Outbound: Graph to Consumers

### Step 5: CDC Captures Changes

pg_ripple's CDC subscriptions detect new and changed triples — including inferred ones:

```sql
SELECT pg_ripple.cdc_subscribe(
  name      => 'enriched_orders',
  predicate => 'schema:Order',
  include_inferred => true
);
```

The CDC bridge writes events to a table compatible with pg_trickle's outbox:

```sql
SELECT pg_ripple.cdc_enable_bridge(
  subscription => 'enriched_orders',
  target_table => 'enriched_events'
);
```

### Step 6: Relay Delivers to Consumers

pg_trickle's outbox + relay distributes the enriched events:

```sql
SELECT pgtrickle.enable_outbox('enriched_events');

-- Enriched data → Kafka
SELECT pgtrickle.set_relay_outbox(
  'enriched-to-kafka',
  outbox => 'enriched_events',
  group  => 'enriched-publisher',
  sink   => '{"type":"kafka","brokers":"kafka:9092","topic":"enriched.orders"}'
);

-- Alerts → NATS
SELECT pgtrickle.set_relay_outbox(
  'alerts-to-nats',
  outbox => 'enriched_events',
  group  => 'alert-publisher',
  sink   => '{"type":"nats","url":"nats://localhost:4222",
              "subject_template":"alerts.{event_type}"}'
);
```

---

## Why This Works

### Zero-Copy Data Flow

Both extensions operate on the same PostgreSQL tables. There's no serialization/deserialization between pg_trickle and pg_ripple — the inbox trigger writes triples directly to VP tables. The CDC bridge writes events directly to the outbox table. No intermediate queue, no network hop, no format conversion.

### Transactional Consistency

The entire pipeline — ingest, transform, infer, validate, publish — can run in a single PostgreSQL transaction. If SHACL validation fails, the transaction rolls back. The Kafka offset isn't committed. The message is retried. No partial state, no inconsistency.

### Schema Evolution Without Downtime

When the upstream Kafka schema changes (new field, renamed field), you update the trigger function. When the downstream schema changes, you update the CDC subscription or the outbox format. The RDF graph in the middle absorbs schema differences — that's what ontologies are for.

### Entity Resolution Across Sources

The killer feature: when Kafka, NATS, and webhook sources all refer to the same customer with different IDs, Datalog rules and `owl:sameAs` canonicalization unify them. Downstream consumers see one customer, regardless of how many sources contributed data about them.

---

## The Alternative: Kafka Connect + ksqlDB

The standard integration stack:

| Component | Purpose | Equivalent in pg_ripple + pg_trickle |
|-----------|---------|-------------------------------------|
| Kafka Connect | Source/sink connectors | pg_trickle relay (reverse/forward) |
| Schema Registry | Schema validation | SHACL shapes |
| ksqlDB | Stream processing | Datalog rules + SPARQL |
| Debezium | CDC from databases | pg_ripple CDC subscriptions |
| Kafka Streams | Entity resolution | owl:sameAs + Datalog |

The pg_ripple stack replaces 5 separate systems with 2 PostgreSQL extensions and a relay binary. The operational complexity difference is significant — one database to back up, one transaction model to reason about, one set of monitoring metrics.

---

## When This Pattern Doesn't Fit

- **Throughput above 100K events/second sustained.** At that volume, a dedicated stream processor (Flink, Kafka Streams) is more appropriate. pg_ripple handles 10–50K events/second comfortably; beyond that, the single-writer PostgreSQL model becomes the bottleneck.

- **No semantic enrichment needed.** If you're just routing events from source to sink with simple transformations, pg_trickle alone (without pg_ripple) is simpler. The knowledge graph adds value when you need inference, entity resolution, or vocabulary alignment.

- **Truly global distribution.** PostgreSQL is a single-region system. If sources and consumers span continents and need sub-50ms latency, a globally distributed message bus is the right choice.

For most enterprise integration use cases — connecting 5–20 sources, unifying customer/product/entity identifiers, enforcing data quality, enriching with business rules — the semantic hub pattern inside PostgreSQL is simpler, cheaper, and more correct than the multi-system alternative.
