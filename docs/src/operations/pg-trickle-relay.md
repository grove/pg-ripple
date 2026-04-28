# pg-trickle Relay: Hub-and-Spoke Integration

> **Available since**: v0.52.0 (JSON→RDF helpers, CDC bridge worker, pg-trickle runtime detection)
>
> **Requires**: pg-trickle 0.25.0+ (relay CLI); pg_ripple 0.52.0+

Use pg_ripple as a **semantic hub** sitting between operational data sources and
downstream consumers. pg-trickle's relay CLI provides the bidirectional transport
layer — collecting data from spokes via **reverse mode** and distributing enriched
data to spokes via **forward mode** — while pg_ripple provides the knowledge-graph
layer: vocabulary alignment, entity resolution, Datalog inference, SHACL quality
enforcement, and SPARQL query capabilities.

```
                          ┌────────────────────────────────┐
                          │         pg-ripple hub           │
                          │   (PostgreSQL + pg_ripple ext)  │
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
  │ Webhook  │──reverse──▶│  │owl:sameAs│  │ SPARQL   │  │──forward──▶│  Kafka    │
  │ (CRM)    │            │  │ linking  │  │federation│  │             │(enriched)│
  └──────────┘            │  └──────────┘  └──────────┘  │             └──────────┘
                          │                                │
                          │  pg-trickle stream tables      │
                          │  (inbox → transform → outbox)  │
                          └────────────────────────────────┘
```

Both extensions coexist in the same PostgreSQL 18 database. pg-trickle manages
stream tables, inboxes, and outboxes in the `pgtrickle` schema. pg_ripple manages
VP tables, the dictionary, and subscriptions in `_pg_ripple` / `pg_ripple` schemas.
They share the same transaction context, enabling zero-copy data flow between them.

---

## Prerequisites

1. **pg_ripple 0.52.0+** installed.
2. **pg-trickle 0.25.0+** installed (relay binary and PostgreSQL extension).
3. Both extensions installed in the same database:

```sql
CREATE EXTENSION pg_trickle;
CREATE EXTENSION pg_ripple;
```

pg_ripple detects pg-trickle at runtime and lazily enables bridge features.
If pg-trickle is absent, `create_subscription()` and bridge functions return
a `WARNING: pg_trickle is not installed; CDC subscriptions are unavailable`
and degrade gracefully.

---

## Step 1 — Inbound: External Sources → Triplestore

Configure a relay reverse pipeline so the relay process delivers JSON events
into a pg-trickle inbox table:

```sql
-- Reverse pipeline: Kafka topic → pg-trickle inbox
SELECT pgtrickle.set_relay_inbox(
    'sensor-readings',
    inbox  => 'sensor_inbox',
    source => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}","topic":"iot.sensors"}'
);
```

Write a trigger on the inbox table that converts the JSON payload to RDF triples
using `pg_ripple.jsonld_to_ntriples()` (for JSON-LD inputs) or by constructing
N-Triples directly:

```sql
CREATE OR REPLACE FUNCTION transform_sensor_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    device_iri TEXT;
    obs_iri    TEXT;
    ntriples   TEXT;
BEGIN
    device_iri := '<https://example.org/device/' || (NEW.payload->>'device') || '>';
    obs_iri    := '<https://example.org/observation/' || NEW.event_id || '>';

    ntriples :=
        obs_iri || ' <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://saref.etsi.org/core/Measurement> .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/measurementMadeBy> ' || device_iri || ' .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/hasValue> "'
                || (NEW.payload->>'temp')
                || '"^^<http://www.w3.org/2001/XMLSchema#decimal> .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/hasTimestamp> "'
                || (NEW.payload->>'ts')
                || '"^^<http://www.w3.org/2001/XMLSchema#dateTime> .';

    PERFORM pg_ripple.load_ntriples(ntriples, false);
    RETURN NEW;
END;
$$;

CREATE TRIGGER sensor_to_rdf
AFTER INSERT ON sensor_inbox
FOR EACH ROW EXECUTE FUNCTION transform_sensor_to_rdf();
```

---

## Step 2 — Enrichment: Datalog Inference & SHACL Validation

Load Datalog rules to enrich inbound triples:

```sql
SELECT pg_ripple.load_rules('sensor_enrichment', $$
    % Alert when temperature exceeds threshold
    ex:tempAlert(Obs, Device) :-
        saref:measurementMadeBy(Obs, Device),
        saref:hasValue(Obs, Val),
        Val > 40.0.

    % Entity resolution: link devices across sources via serial number
    owl:sameAs(D1, D2) :-
        schema:serialNumber(D1, SN),
        schema:serialNumber(D2, SN),
        D1 \= D2.
$$);
```

Add SHACL shapes to enforce data quality before forwarding:

```sql
SELECT pg_ripple.load_shacl($$
    ex:ObservationShape a sh:NodeShape ;
        sh:targetClass saref:Measurement ;
        sh:property [
            sh:path saref:measurementMadeBy ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
        ] ;
        sh:property [
            sh:path saref:hasTimestamp ;
            sh:minCount 1 ;
            sh:datatype xsd:dateTime ;
        ] .
$$);
```

---

## Step 3 — Outbound: Triplestore → External Consumers

Create a bridge table that captures enriched triples as JSON-LD events and
configure pg-trickle outbox + relay forward pipelines:

```sql
-- Bridge table for outbound events
CREATE TABLE enriched_events (
    id         BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload    JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Subscribe to inferred alert triples
SELECT pg_ripple.create_named_subscription(
    'high-temp-alerts',
    'FILTER(?p = <https://example.org/tempAlert>)',
    NULL
);

-- Enable outbox on the bridge table
SELECT pgtrickle.enable_outbox('enriched_events');

-- Forward to NATS for real-time consumers
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-nats',
    outbox => 'enriched_events',
    group  => 'nats-publisher',
    sink   => '{"type":"nats","url":"nats://nats:4222",
                "subject_template":"ripple.enriched.{event_type}"}'
);

-- Forward to Kafka for analytics pipeline
SELECT pgtrickle.set_relay_outbox(
    'alerts-to-kafka',
    outbox => 'enriched_events',
    group  => 'kafka-publisher',
    sink   => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}",
                "topic":"ripple.alerts"}'
);

-- Forward to a partner webhook
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-partner',
    outbox => 'enriched_events',
    group  => 'partner-publisher',
    sink   => '{"type":"http","url":"https://partner.example.com/events",
                "method":"POST"}'
);
```

---

## CDC → Outbox Bridge Approaches

Three approaches are available for bridging pg_ripple's CDC NOTIFY events with
pg-trickle's JSON outbox. Choose based on your latency and throughput requirements:

### Approach A — Trigger bridge (lowest latency)

Add a trigger on VP delta tables that directly inserts decoded triples into the
bridge table in the same transaction:

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

**Best for**: High-priority alerts, strict transactional guarantees.  
**Trade-off**: `decode_id()` overhead on every row; selective filtering requires additional logic.

### Approach B — Background worker bridge (best throughput)

The pg_ripple background worker (enabled since v0.52.0) listens for CDC NOTIFY
events, batches them, bulk-decodes dictionary IDs in a single SPI call, and
batch-inserts into the bridge table.

**Best for**: Bulk enriched data, high-volume streams.  
**Trade-off**: Adds configurable milliseconds of latency.

### Approach C — Named subscription + SPARQL CONSTRUCT view (most flexible)

Use named subscriptions (v0.42.0+) with a SPARQL `FILTER` to capture only
high-value changes, then materialize them via a `CONSTRUCT` view. Combine
with `pg_cron` or pg-trickle's own scheduling for periodic drain:

```sql
SELECT pg_ripple.create_named_subscription(
    'alerts',
    'FILTER(?p = <https://example.org/alert>)',
    NULL
);
```

**Best for**: Complex SPARQL-shaped payloads, scheduled reports, ad-hoc shapes.  
**Trade-off**: Polling-based unless combined with a LISTEN/NOTIFY wake-up.

### Recommended combination

| Data path | Mechanism | Typical latency |
|---|---|---|
| High-priority alerts | Approach A (trigger bridge) | < 10 ms |
| Bulk enriched data | Approach B (background worker) | 50–500 ms |
| Scheduled reports | Approach C (SPARQL CONSTRUCT view) | Cron-driven |

---

## Patterns

### Multi-source entity resolution

Multiple spokes contribute data about the same real-world entities using different
identifiers. Datalog `owl:sameAs` rules merge them into a unified graph that
is then forwarded to analytics spokes via relay.

```prolog
% Align CRM and ERP customer records by shared email
owl:sameAs(CrmCust, ErpAcct) :-
    crm:emailAddress(CrmCust, E),
    erp:contact_email(ErpAcct, E).
```

### Vocabulary alignment

Each spoke uses its own schema; pg_ripple maps everything to a shared ontology
(e.g., Schema.org) so downstream consumers see a uniform vocabulary:

```prolog
schema:name(X, V)  :- crm:customerName(X, V).
schema:email(X, V) :- crm:emailAddress(X, V).

schema:name(X, V)  :- erp:accountTitle(X, V).
schema:email(X, V) :- erp:contact_email(X, V).
```

### SPARQL-driven outbox views

Instead of forwarding raw triple changes, use a SPARQL `CONSTRUCT` view
(v0.18.0+) to shape outbound data:

```sql
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

---

## Outbound Payload Format

Outbound events are serialized as JSON-LD for interoperability. pg_ripple
provides `export_jsonld()` and JSON-LD framing (v0.17.0+) for this purpose.
Use `"ripple:{statement_id}"` (the `i` column from VP tables) as the relay
dedup key to guarantee idempotent delivery across relay restarts:

```json
{
    "@context": "https://schema.org/",
    "@id": "https://example.org/customer/C1",
    "@type": "Customer",
    "name": "Jane Doe",
    "email": "jane@example.com",
    "_relay_dedup_key": "ripple:4200042"
}
```

---

## Deployment Topology

```yaml
# docker-compose.yml sketch
services:
  postgres:
    image: postgres:18
    # pg_ripple and pg-trickle extensions installed

  relay-inbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
      KAFKA_BROKERS: kafka:9092
    # Handles all reverse pipelines (Kafka/NATS/webhooks → inbox)

  relay-outbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
    # Handles all forward pipelines (outbox → Kafka/NATS/webhooks)

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

Relay pods are stateless; advisory locks prevent duplicate processing. For
high-throughput deployments, run separate relay pods per inbound source and per
outbound sink. pg_ripple's parallel merge workers (v0.42.0+) handle the storage
layer independently.

---

## Backpressure & Schema Evolution

**Backpressure**: If Datalog inference produces high fan-out (many inferred
triples per input event), the outbox may grow faster than the relay drains it.
Mitigate with:
- pg-trickle's built-in retention drain to bound outbox size.
- The relay's `/health/drained` endpoint for Kubernetes backpressure signaling.
- pg_ripple's `source` column (`0` = explicit, `1` = inferred) to selectively
  bridge only certain triple types.

**Schema evolution**: When Datalog rules or SHACL shapes change:
- Version the outbox subject template: `ripple.v2.enriched.{type}`.
- Include a `@context` version field in JSON-LD payloads.
- Use the relay's full-refresh mode to re-snapshot downstream state after
  rule changes.

---

## Related Pages

- [CDC Operations](cdc.md)
- [Citus + pg-trickle Integration](citus-integration.md)
- [Cookbook: CDC → Kafka via JSON-LD Outbox](../cookbook/cdc-to-kafka.md)
- [Reasoning and Inference (Datalog)](../features/reasoning-and-inference.md)
- [Validating Data Quality (SHACL)](../features/validating-data-quality.md)
