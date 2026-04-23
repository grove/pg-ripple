# pg-ripple × pg-trickle Relay — Hub-and-Spoke Integration

> **Status**: Exploration (2026-04-23)
> **Related**: [pg-trickle relay plan](https://github.com/grove/pg-trickle/blob/main/plans/relay/PLAN_RELAY_CLI.md) · [pg-ripple ROADMAP](../ROADMAP.md)

## Vision

Use pg-ripple as a **semantic hub** sitting between operational data sources and
downstream consumers. pg-trickle's relay CLI provides the bidirectional transport
layer — collecting data from spokes via **reverse mode** and distributing enriched
data to spokes via **forward mode** — while pg-ripple provides the knowledge
graph layer: vocabulary alignment, entity resolution, Datalog inference, SHACL
quality enforcement, and SPARQL query capabilities.

```
                          ┌────────────────────────────────┐
                          │         pg-ripple hub           │
                          │   (PostgreSQL + pg-ripple ext)  │
    INBOUND               │                                │               OUTBOUND
    ───────               │  ┌──────────┐  ┌───────────┐  │               ────────
                          │  │ Datalog  │  │  SHACL    │  │
  ┌──────────┐  relay     │  │ inference│  │ validation│  │     relay    ┌──────────┐
  │  Kafka   │──reverse──▶│  └────┬─────┘  └─────┬─────┘  │──forward──▶│  NATS    │
  │ (orders) │            │       │              │        │             │ (events) │
  └──────────┘            │  ┌────▼──────────────▼─────┐  │             └──────────┘
                          │  │                         │  │
  ┌──────────┐  relay     │  │   RDF Triple Store      │  │     relay    ┌──────────┐
  │  NATS    │──reverse──▶│  │   (VP tables, HTAP)     │──│──forward──▶│  Webhook  │
  │(sensors) │            │  │                         │  │             │ (API)     │
  └──────────┘            │  └────▲──────────────▲─────┘  │             └──────────┘
                          │       │              │        │
  ┌──────────┐  relay     │  ┌────┴─────┐  ┌────┴─────┐  │     relay    ┌──────────┐
  │ Webhook  │──reverse──▶│  │ owl:sameAs│ │ SPARQL   │  │──forward──▶│  Kafka    │
  │ (CRM)    │            │  │ linking   │ │ federation│  │             │(enriched)│
  └──────────┘            │  └──────────┘  └──────────┘  │             └──────────┘
                          │                                │
                          │  pg-trickle stream tables      │
                          │  (inbox → transform → outbox)  │
                          └────────────────────────────────┘
```

## How the Pieces Fit Together

### Database Layout

Both extensions coexist in the same PostgreSQL 18 database. pg-trickle manages
stream tables, inboxes, and outboxes in the `pgtrickle` schema. pg-ripple
manages VP tables, the dictionary, and subscriptions in `_pg_ripple` /
`pg_ripple` schemas. They share the same transaction context, which enables
zero-copy data flow between them.

```sql
CREATE EXTENSION pg_trickle;
CREATE EXTENSION pg_ripple;
```

### Inbound: External Sources → Triplestore

**Step 1 — Relay reverse mode delivers JSON to pg-trickle inbox tables:**

```sql
-- Configure a reverse pipeline: Kafka topic → pg-trickle inbox
SELECT pgtrickle.set_relay_inbox(
    'sensor-readings',
    inbox  => 'sensor_inbox',
    source => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}","topic":"iot.sensors"}'
);
```

The relay process polls Kafka and writes JSON events into `sensor_inbox`:

```json
{"event_id": "kafka:iot.sensors:0:42", "event_type": "sensor_reading",
 "payload": {"device": "sensor-7", "temp": 22.5, "unit": "°C", "ts": "2026-04-23T10:00:00Z"}}
```

**Step 2 — pg-trickle stream table transforms JSON → RDF triples:**

A pg-trickle stream table watches the inbox and transforms each JSON event into
`pg_ripple.load_ntriples()` calls using a trigger or pg-trickle's built-in
trigger functions:

```sql
-- pg-trickle stream table that transforms inbox JSON into RDF
CREATE OR REPLACE FUNCTION transform_sensor_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    device_iri TEXT;
    obs_iri TEXT;
    ntriples TEXT;
BEGIN
    device_iri := '<https://example.org/device/' || NEW.payload->>'device' || '>';
    obs_iri := '<https://example.org/observation/' || NEW.event_id || '>';

    ntriples := obs_iri || ' <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://saref.etsi.org/core/Measurement> .' || E'\n'
             || obs_iri || ' <https://saref.etsi.org/core/measurementMadeBy> ' || device_iri || ' .' || E'\n'
             || obs_iri || ' <https://saref.etsi.org/core/hasValue> "' || (NEW.payload->>'temp') || '"^^<http://www.w3.org/2001/XMLSchema#decimal> .' || E'\n'
             || obs_iri || ' <https://saref.etsi.org/core/hasTimestamp> "' || (NEW.payload->>'ts') || '"^^<http://www.w3.org/2001/XMLSchema#dateTime> .';

    PERFORM pg_ripple.load_ntriples(ntriples, false);
    RETURN NEW;
END;
$$;
```

**Step 3 — Datalog rules run inference on newly ingested triples:**

```prolog
% Derive alerts from observations exceeding thresholds
alert(Obs, Device, "high_temperature") :-
    saref:measurementMadeBy(Obs, Device),
    saref:hasValue(Obs, Val),
    Val > 40.0.

% Entity resolution: link devices across data sources via owl:sameAs
owl:sameAs(D1, D2) :-
    schema:serialNumber(D1, SN),
    schema:serialNumber(D2, SN),
    D1 \= D2.
```

**Step 4 — SHACL validates data quality:**

```turtle
ex:ObservationShape a sh:NodeShape ;
    sh:targetClass saref:Measurement ;
    sh:property [
        sh:path saref:measurementMadeBy ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:class saref:Device ;
    ] ;
    sh:property [
        sh:path saref:hasTimestamp ;
        sh:minCount 1 ;
        sh:datatype xsd:dateTime ;
    ] .
```

### Outbound: Triplestore → External Consumers

**Step 5 — CDC subscriptions detect enriched/inferred triples:**

pg-ripple's CDC system fires `NOTIFY` when new triples are inserted (including
inferred triples from Datalog). A bridge function listens for these notifications
and writes them to a pg-trickle outbox-compatible stream table:

```sql
-- Bridge table: captures enriched triples for outbound relay
CREATE TABLE enriched_events (
    id         BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload    JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- CDC subscription triggers writes to the bridge table
-- Option A: Direct trigger on VP delta tables
-- Option B: Named subscription + LISTEN/NOTIFY + background worker
-- Option C: pg-trickle stream table watching _pg_ripple CDC channel
```

**Step 6 — pg-trickle outbox + relay forward mode distributes changes:**

```sql
-- Enable outbox on the enriched_events stream table
SELECT pgtrickle.enable_outbox('enriched_events');

-- Configure forward pipelines to multiple sinks
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-nats',
    outbox => 'enriched_events',
    group  => 'enriched-publisher',
    sink   => '{"type":"nats","url":"nats://localhost:4222",
                "subject_template":"ripple.enriched.{event_type}"}'
);

SELECT pgtrickle.set_relay_outbox(
    'alerts-to-kafka',
    outbox => 'enriched_events',
    group  => 'alert-publisher',
    sink   => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}",
                "topic":"ripple.alerts"}'
);

SELECT pgtrickle.set_relay_outbox(
    'enriched-to-webhook',
    outbox => 'enriched_events',
    group  => 'webhook-publisher',
    sink   => '{"type":"http","url":"https://api.downstream.com/triples",
                "method":"POST"}'
);
```

## Key Integration Patterns

### Pattern 1: Multi-Source Entity Resolution

Multiple spokes contribute data about the same real-world entities using
different identifiers. pg-ripple resolves them:

```
Spoke A (CRM)    →  relay reverse  →  load_ntriples()  →  Customer entities
Spoke B (ERP)    →  relay reverse  →  load_ntriples()  →  Account entities
Spoke C (Support)→  relay reverse  →  load_ntriples()  →  Ticket entities

                     ↓ Datalog rules ↓

owl:sameAs(crm:C1, erp:A1)    (matched by email)
owl:sameAs(crm:C1, sup:T1)    (matched by phone number)

                     ↓ Canonicalization ↓

Unified customer graph  →  relay forward  →  Spoke D (Analytics)
                                          →  Spoke E (Dashboard)
```

### Pattern 2: Vocabulary Alignment & Standardization

Each spoke uses its own schema; pg-ripple maps everything to a shared ontology:

```prolog
% Align CRM vocabulary to Schema.org
schema:name(X, V)  :- crm:customerName(X, V).
schema:email(X, V) :- crm:emailAddress(X, V).

% Align ERP vocabulary to Schema.org
schema:name(X, V)  :- erp:accountTitle(X, V).
schema:email(X, V) :- erp:contact_email(X, V).
```

Downstream consumers see a uniform Schema.org vocabulary regardless of which
spoke produced the data.

### Pattern 3: Event-Driven Enrichment Pipeline

```
1. External event arrives (relay reverse → inbox)
2. Stream table trigger converts JSON → RDF triples
3. Datalog inference fires (materialization rules)
4. SHACL validation runs (quality gate)
5. CDC triggers on inferred triples
6. Enriched event written to outbox
7. Relay forward distributes to consumers
```

Latency: Steps 2–6 can complete within a single transaction if the Datalog
rules and SHACL shapes are pre-materialized. Expected end-to-end latency for
a single event: < 50 ms (intra-database) + relay poll interval.

### Pattern 4: SPARQL-Driven Views as Outbox Sources

Instead of forwarding raw triple changes, use SPARQL CONSTRUCT views (v0.18.0)
to shape outbound data:

```sql
-- Materialized CONSTRUCT view produces JSON-LD for downstream
SELECT pg_ripple.sparql('
    CONSTRUCT {
        ?customer schema:name ?name ;
                  schema:email ?email ;
                  ex:riskScore ?score .
    }
    WHERE {
        ?customer a schema:Customer ;
                  schema:name ?name ;
                  schema:email ?email .
        OPTIONAL { ?customer ex:riskScore ?score }
    }
');
```

The view's output can be serialized as JSON-LD and placed directly into a
pg-trickle outbox for relay distribution.

### Pattern 5: Federation + Relay for Hybrid Queries

Combine local enriched data with live remote data:

```sparql
SELECT ?customer ?name ?orderTotal ?sentiment
WHERE {
    ?customer a schema:Customer ;
              schema:name ?name .

    # Local: enriched data from Datalog inference
    ?customer ex:lifetimeValue ?orderTotal .

    # Remote: live sentiment from external SPARQL endpoint
    SERVICE <https://analytics.example.com/sparql> {
        ?customer ex:sentimentScore ?sentiment .
    }
}
```

Results can be materialized into a pg-trickle stream table for periodic
outbound relay distribution.

## Bridge Layer: CDC → pg-trickle Outbox

The main engineering challenge is bridging pg-ripple's NOTIFY-based CDC
(integer dictionary IDs) with pg-trickle's JSON-based outbox system. Three
approaches are viable:

### Approach A: Trigger-Based Bridge (Lowest Latency)

Add a second trigger to VP delta tables that, in addition to NOTIFY, directly
inserts decoded triples into a pg-trickle stream table:

```sql
CREATE OR REPLACE FUNCTION _pg_ripple.bridge_to_outbox()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO enriched_events (event_type, payload)
    VALUES (
        TG_OP,
        jsonb_build_object(
            'subject',   pg_ripple.decode_id(NEW.s),
            'predicate', pg_ripple.decode_id(TG_ARGV[0]::bigint),
            'object',    pg_ripple.decode_id(NEW.o),
            'graph',     pg_ripple.decode_id(NEW.g)
        )
    );
    RETURN NEW;
END;
$$;
```

**Pros**: Single-transaction guarantee, lowest latency.
**Cons**: `decode_id()` in a hot-path trigger adds overhead; selective filtering
requires additional logic.

### Approach B: Background Worker Bridge (Best Throughput)

A pg-ripple background worker (`pgrx::BackgroundWorker`) listens for CDC
NOTIFY events, batches them, decodes dictionary IDs in bulk, and batch-inserts
into the pg-trickle stream table:

```rust
// Pseudocode for a background worker bridge
fn bridge_worker(notifications: Vec<CdcNotification>) {
    let ids: Vec<i64> = collect_all_ids(&notifications);
    let decoded = dictionary::batch_decode(&ids);  // single SPI call

    let rows: Vec<BridgeRow> = notifications.iter().map(|n| BridgeRow {
        event_type: n.op.clone(),
        payload: json!({
            "subject":   decoded[&n.s],
            "predicate": decoded[&n.p],
            "object":    decoded[&n.o],
            "graph":     decoded[&n.g],
        }),
    }).collect();

    batch_insert_to_stream_table(&rows);  // single COPY or multi-row INSERT
}
```

**Pros**: Amortized decode cost, configurable batch size + flush interval.
**Cons**: Adds milliseconds of latency, requires a new background worker.

### Approach C: Named Subscription → SPARQL View (Most Flexible)

Use pg-ripple v0.42.0 named subscriptions with a SPARQL FILTER to selectively
capture only high-value changes, then materialize them via a CONSTRUCT view:

```sql
-- Only capture inferred alerts
SELECT pg_ripple.create_named_subscription(
    'alerts',
    'FILTER(?p = <https://example.org/alert>)',
    NULL
);

-- Periodically materialize the alert view into the outbox
-- (could be driven by pg_cron or pg-trickle's own scheduling)
```

**Pros**: Most flexible, SPARQL-level filtering, supports complex shapes.
**Cons**: Polling-based unless combined with LISTEN/NOTIFY wake-up.

## Recommended Architecture

For a production hub-and-spoke deployment, **combine Approaches A and B**:

| Data Path | Mechanism | Latency |
|---|---|---|
| High-priority alerts | Trigger bridge (Approach A) | < 10 ms |
| Bulk enriched data | Background worker bridge (Approach B) | 50–500 ms |
| Scheduled reports | SPARQL CONSTRUCT views (Approach C) | Cron-driven |

## Implementation Considerations

### 1. Payload Format

The relay passes JSON payloads through as-is. pg-ripple should produce
**JSON-LD** for outbound events so consumers can parse them as linked data:

```json
{
    "@context": "https://schema.org/",
    "@id": "https://example.org/customer/C1",
    "@type": "Customer",
    "name": "Jane Doe",
    "email": "jane@example.com",
    "ex:riskScore": 0.87
}
```

pg-ripple already has `export_jsonld()` and JSON-LD framing (v0.17.0) for this.

### 2. Dedup Keys

- **Inbound**: The relay's `event_id` maps naturally to blank node scoping for
  the load generation (each event gets a unique blank node scope).
- **Outbound**: Use `"ripple:{statement_id}"` (the `i` column from VP tables)
  as the dedup key, ensuring idempotent delivery even across relay restarts.

### 3. Backpressure

If Datalog inference generates many inferred triples per input event (fan-out),
the outbox can grow faster than the relay drains it. Use:
- pg-trickle's built-in retention drain to bound outbox size
- The relay's `/health/drained` endpoint for Kubernetes backpressure signaling
- pg-ripple's `source` column (`0` = explicit, `1` = inferred) to selectively
  bridge only certain triple types

### 4. Schema Evolution

When ontology mappings change (new Datalog rules, updated SHACL shapes),
downstream consumers need to handle schema evolution. Options:
- Version the outbox subject template: `ripple.v2.enriched.{type}`
- Include a `@context` version in JSON-LD payloads
- Use the relay's full-refresh mode to re-snapshot after rule changes

### 5. Scaling

```
                    ┌─ relay pod 1 (reverse: Kafka → inbox)
                    ├─ relay pod 2 (reverse: NATS → inbox)
                    ├─ relay pod 3 (reverse: Webhooks → inbox)
  advisory locks    │
  ─────────────────▶├─ PostgreSQL (pg-ripple + pg-trickle)
                    │    ├─ merge background worker (HTAP)
                    │    ├─ Datalog inference worker
                    │    └─ SHACL validation worker
                    │
                    ├─ relay pod 4 (forward: outbox → Kafka)
                    ├─ relay pod 5 (forward: outbox → NATS)
                    └─ relay pod 6 (forward: outbox → webhooks)
```

Each relay pod is stateless; advisory locks prevent duplicate processing.
pg-ripple's parallel merge workers (v0.42.0) handle the storage layer.
pg-ripple_http provides the SPARQL protocol endpoint for ad-hoc queries.

## What Needs to Be Built

| # | Component | Owner | Description |
|---|---|---|---|
| 1 | **JSON → RDF transform functions** | pg-ripple | SQL helper functions to convert common JSON patterns (nested objects, arrays, typed values) into N-Triples strings for `load_ntriples()`. Reduces boilerplate in stream table triggers. |
| 2 | **CDC → Outbox bridge worker** | pg-ripple | Background worker that batches CDC notifications, bulk-decodes dictionary IDs, and inserts decoded triples as JSON-LD into a pg-trickle stream table. |
| 3 | **Selective CDC bridge triggers** | pg-ripple | Configurable triggers on VP delta tables that can optionally write directly to a pg-trickle stream table (for low-latency paths). |
| 4 | **JSON-LD event serializer** | pg-ripple | Thin wrapper around `export_jsonld()` optimized for single-triple or small-batch event serialization (avoiding full-graph export overhead). |
| 5 | **Outbox dedup key from statement ID** | pg-ripple | Function that generates relay-compatible dedup keys from the VP `i` column. |
| 6 | **Schema mapping templates** | pg-ripple | Pre-built Datalog rule templates for common vocabulary alignment tasks (Schema.org ↔ FHIR, Schema.org ↔ SAREF, etc.). |
| 7 | **pg-trickle runtime detection** | pg-ripple | Runtime check for pg-trickle availability; degrade gracefully if absent (bridge features disabled, log WARNING). |
| 8 | **Integration test suite** | both | End-to-end tests: external source → relay reverse → inbox → RDF → Datalog → CDC → outbox → relay forward → external sink. |

## Example: Complete Hub-and-Spoke Setup

```sql
-- ═══════════════════════════════════════════════════════════════════
-- 1. Install extensions
-- ═══════════════════════════════════════════════════════════════════
CREATE EXTENSION pg_trickle;
CREATE EXTENSION pg_ripple;

-- ═══════════════════════════════════════════════════════════════════
-- 2. Inbound: Kafka orders → RDF triples
-- ═══════════════════════════════════════════════════════════════════

-- pg-trickle inbox for order events
SELECT pgtrickle.set_relay_inbox(
    'orders-inbound',
    inbox  => 'order_inbox',
    source => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}","topic":"orders"}'
);

-- Stream table trigger to convert JSON orders → RDF via JSON-LD toRdf
CREATE OR REPLACE FUNCTION transform_order_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE ntriples TEXT;
BEGIN
    ntriples := pg_ripple.jsonld_to_ntriples(
        NEW.payload
            || jsonb_build_object('@id', 'https://example.org/order/' || (NEW.payload->>'order_id'))
            || jsonb_build_object('@type', 'https://schema.org/Order'),
        context => 'schema-org'
    );
    PERFORM pg_ripple.load_ntriples(ntriples, false);
    RETURN NEW;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════
-- 3. Enrichment: Datalog inference rules
-- ═══════════════════════════════════════════════════════════════════

SELECT pg_ripple.load_rules('order_enrichment', $$
    % Classify high-value customers
    ex:highValueCustomer(C) :-
        schema:customer(O, C),
        schema:totalPaymentDue(O, V),
        V > 10000.

    % Cross-reference with CRM data
    owl:sameAs(OrderCust, CrmCust) :-
        schema:email(OrderCust, E),
        crm:emailAddress(CrmCust, E).
$$);

-- ═══════════════════════════════════════════════════════════════════
-- 4. Quality gate: SHACL validation
-- ═══════════════════════════════════════════════════════════════════

SELECT pg_ripple.load_shacl($$
    ex:OrderShape a sh:NodeShape ;
        sh:targetClass schema:Order ;
        sh:property [ sh:path schema:customer ; sh:minCount 1 ] ;
        sh:property [ sh:path schema:orderDate ; sh:minCount 1 ;
                      sh:datatype xsd:dateTime ] .
$$);

-- ═══════════════════════════════════════════════════════════════════
-- 5. Outbound bridge: CDC → pg-trickle outbox
-- ═══════════════════════════════════════════════════════════════════

-- Bridge table for enriched events
CREATE TABLE enriched_orders (
    id         BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload    JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Subscribe to inferred triples
SELECT pg_ripple.create_named_subscription(
    'high-value-alerts',
    'FILTER(?p = <https://example.org/highValueCustomer>)',
    NULL
);

-- Enable outbox on bridge table
SELECT pgtrickle.enable_outbox('enriched_orders');

-- ═══════════════════════════════════════════════════════════════════
-- 6. Outbound relays: distribute to multiple destinations
-- ═══════════════════════════════════════════════════════════════════

-- Push enriched orders to NATS for real-time consumers
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-nats',
    outbox => 'enriched_orders',
    group  => 'nats-publisher',
    sink   => '{"type":"nats","url":"nats://nats:4222",
                "subject_template":"ripple.orders.{event_type}"}'
);

-- Push alerts to Kafka for analytics pipeline
SELECT pgtrickle.set_relay_outbox(
    'alerts-to-kafka',
    outbox => 'enriched_orders',
    group  => 'kafka-publisher',
    sink   => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}",
                "topic":"enriched-orders"}'
);

-- Push to webhook for external partner API
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-partner',
    outbox => 'enriched_orders',
    group  => 'partner-publisher',
    sink   => '{"type":"http","url":"https://partner.example.com/orders",
                "method":"POST"}'
);
```

## Deployment Topology

```yaml
# docker-compose.yml sketch
services:
  postgres:
    image: postgres:18
    # Both extensions installed
    volumes:
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql

  relay-inbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
      KAFKA_BROKERS: kafka:9092
    # Handles all reverse pipelines

  relay-outbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
    # Handles all forward pipelines

  pg-ripple-http:
    image: pg-ripple-http:latest
    environment:
      DATABASE_URL: postgres://ripple:pw@postgres/hub
    ports:
      - "8080:8080"
    # SPARQL protocol endpoint for ad-hoc queries

  kafka:
    image: redpandadata/redpanda:latest

  nats:
    image: nats:latest
    command: ["-js"]  # JetStream enabled
```

## Open Questions

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | Should the CDC→outbox bridge be a pg-ripple background worker or a separate pg-trickle stream table? | (a) BG worker in pg-ripple, (b) pg-trickle stream table trigger | (a) — keeps the bridge logic in Rust with batch decoding; avoids PL/pgSQL `decode_id()` overhead |
| 2 | What serialization format for outbound events? | (a) Flat JSON with s/p/o keys, (b) JSON-LD, (c) N-Triples text | (b) — JSON-LD is both human-readable and machine-parseable as linked data |
| 3 | Should pg-ripple detect pg-trickle at `_PG_init` or lazily? | (a) Startup check, (b) Lazy detection on first bridge call | (b) — `CREATE EXTENSION` order is unspecified; lazy detection avoids boot order issues |
| 4 | How to handle Datalog fan-out (1 input → N inferred triples) in the outbox? | (a) One outbox row per inferred triple, (b) Batched JSON array per inference run | (b) — reduces outbox row count; relay can unpack if needed |
| 5 | Should inbound JSON→RDF transformation be generic or schema-specific? | (a) Generic helper, (b) Per-source custom triggers | Both — ship `pg_ripple.jsonld_to_ntriples()` as a thin wrapper over the W3C JSON-LD 1.1 `toRdf` algorithm (symmetric with our existing JSON-LD framing export, conformance-tested against the W3C suite); allow PL/pgSQL custom triggers as the escape hatch for shapes no `@context` can express |
| 6 | Target version for initial integration? | Depends on pg-trickle relay availability | Could prototype with pg-trickle v0.25.0 relay; pg-ripple side could ship as v0.51.0 or later |
