# pg-trickle Relay: Hub-and-Spoke Integration

> **Available since**: v0.52.0
>
> **Requires**: pg-trickle 0.25.0+ (relay CLI); pg_ripple 0.52.0+

## What this integration does

Imagine you have IoT sensor readings arriving via Kafka, customer records coming
in from a CRM webhook, and order data flowing through NATS — all using different
field names, different identifiers for the same real-world entities, and different
data shapes. Downstream teams want a clean, unified, enriched view of this data
pushed to their own systems in real time, without any of them having to understand
the messy source schemas.

This is the hub-and-spoke pattern. **pg-trickle** acts as the transport network:
its relay CLI pulls data in from any source (Kafka, NATS, webhooks) and pushes
enriched data out to any sink. **pg_ripple** acts as the intelligent hub in the
middle: it turns the incoming JSON into a knowledge graph, runs inference rules to
derive new facts, validates data quality with SHACL, and serializes the enriched
results as JSON-LD for downstream consumers.

The whole pipeline runs inside a single PostgreSQL database. Both extensions share
the same transaction context, so data moves from inbox to triplestore to outbox
without ever leaving the database process.

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

The data flow through the hub has five stages:

1. **Ingest** — pg-trickle relay reverse mode delivers raw JSON events into inbox tables.
2. **Transform** — a trigger converts the JSON into RDF triples and loads them into the triplestore.
3. **Enrich** — Datalog inference rules derive new facts (alerts, entity links, risk scores).
4. **Validate** — SHACL shapes enforce data quality before anything leaves the hub.
5. **Distribute** — pg-trickle relay forward mode pushes enriched, validated JSON-LD events to any number of sinks.

---

## Prerequisites

You need both extensions installed in the same database. The order does not matter
— pg_ripple detects pg-trickle lazily at runtime, so there is no boot-order
dependency:

```sql
CREATE EXTENSION pg_trickle;
CREATE EXTENSION pg_ripple;
```

If pg-trickle is not installed, pg_ripple simply degrades gracefully: CDC
subscription functions return a warning and continue rather than raising an error,
so you can develop and test without a full pg-trickle setup.

```
WARNING: pg_trickle is not installed; CDC subscriptions are unavailable
```

---

## A worked example: IoT sensor hub

The following walkthrough builds a complete hub that ingests temperature readings
from IoT sensors via Kafka, detects anomalies using inference rules, and pushes
alerts to NATS and Kafka consumers. Each step shows both the SQL to run and what
the data looks like at that point.

### Step 1 — Pull sensor events from Kafka

The relay process runs outside PostgreSQL and continuously polls configured
sources. You tell it what to poll and where to write the results using
`pgtrickle.set_relay_inbox()`. Here we subscribe to the `iot.sensors` Kafka
topic and direct its events into a table called `sensor_inbox`:

```sql
SELECT pgtrickle.set_relay_inbox(
    'sensor-readings',
    inbox  => 'sensor_inbox',
    source => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}","topic":"iot.sensors"}'
);
```

Each time the relay receives a message from Kafka, it inserts a row into
`sensor_inbox`. The original Kafka message payload arrives as a `JSONB` column.
A typical row looks like this:

```json
{
  "event_id": "kafka:iot.sensors:0:42",
  "event_type": "sensor_reading",
  "payload": {
    "device": "sensor-7",
    "temp": 22.5,
    "unit": "°C",
    "ts": "2026-04-28T10:00:00Z"
  }
}
```

### Step 2 — Convert JSON events to RDF triples

Raw JSON has no standard semantics. Two sensor vendors might both call their
field `"temp"` but mean very different things. By converting to RDF we attach
well-defined, globally unique meanings to each field — in this case using the
[SAREF IoT ontology](https://saref.etsi.org/).

A trigger on `sensor_inbox` fires for every inserted row and calls
`pg_ripple.load_ntriples()` to store the event as a set of typed RDF triples:

```sql
CREATE OR REPLACE FUNCTION transform_sensor_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    device_iri TEXT;
    obs_iri    TEXT;
    ntriples   TEXT;
BEGIN
    -- Mint stable IRIs from the source identifiers
    device_iri := '<https://example.org/device/' || (NEW.payload->>'device') || '>';
    obs_iri    := '<https://example.org/observation/' || NEW.event_id || '>';

    -- Build N-Triples: one statement per fact
    ntriples :=
        obs_iri || ' <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>'
                || ' <https://saref.etsi.org/core/Measurement> .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/measurementMadeBy> '
                || device_iri || ' .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/hasValue> "'
                || (NEW.payload->>'temp')
                || '"^^<http://www.w3.org/2001/XMLSchema#decimal> .' || E'\n'
     || obs_iri || ' <https://saref.etsi.org/core/hasTimestamp> "'
                || (NEW.payload->>'ts')
                || '"^^<http://www.w3.org/2001/XMLSchema#dateTime> .';

    PERFORM pg_ripple.load_ntriples(data => ntriples, strict => false);
    RETURN NEW;
END;
$$;

CREATE TRIGGER sensor_to_rdf
AFTER INSERT ON sensor_inbox
FOR EACH ROW EXECUTE FUNCTION transform_sensor_to_rdf();
```

After the trigger fires, those four facts exist in the triplestore. If you
queried them back as JSON-LD immediately after load, you would see:

```json
{
  "@context": {
    "saref": "https://saref.etsi.org/core/",
    "xsd": "http://www.w3.org/2001/XMLSchema#"
  },
  "@id": "https://example.org/observation/kafka:iot.sensors:0:42",
  "@type": "saref:Measurement",
  "saref:measurementMadeBy": {
    "@id": "https://example.org/device/sensor-7"
  },
  "saref:hasValue": {
    "@value": "22.5",
    "@type": "xsd:decimal"
  },
  "saref:hasTimestamp": {
    "@value": "2026-04-28T10:00:00Z",
    "@type": "xsd:dateTime"
  }
}
```

This is the data at rest inside the triplestore, expressed as JSON-LD. The
`@type` and `@id` fields carry the full semantic meaning from the SAREF ontology,
so any consumer that understands SAREF can correctly interpret the reading.

### Step 3 — Add inference rules to detect anomalies

Datalog rules let you express facts that can be *derived* from the stored
triples. Rather than writing triggers for every business rule, you declare the
rules once and pg_ripple materialises the inferred triples automatically.

The rules below fire an alert whenever a measurement exceeds 40°C and link
devices across sources that share a serial number (entity resolution):

```sql
SELECT pg_ripple.load_rules(
    rules    => $$
        % Derive an alert for any observation above the threshold.
        % The inferred triple is: <obs> ex:tempAlert <device>
        ex:tempAlert(Obs, Device) :-
            saref:measurementMadeBy(Obs, Device),
            saref:hasValue(Obs, Val),
            Val > 40.0.

        % Link two devices if they share a serial number, even if they
        % appear under different identifiers in different source systems.
        owl:sameAs(D1, D2) :-
            schema:serialNumber(D1, SN),
            schema:serialNumber(D2, SN),
            D1 \= D2.
    $$,
    rule_set => 'sensor_enrichment'
);
```

When a 45°C reading arrives from `sensor-7`, these rules materialise a new
triple — an inferred fact that did not exist in the raw data:

```
<https://example.org/observation/kafka:iot.sensors:0:99>
    <https://example.org/tempAlert>
    <https://example.org/device/sensor-7> .
```

### Step 4 — Enforce data quality with SHACL

Before enriched data leaves the hub, SHACL shapes act as a quality gate. Any
observation that lacks a `measurementMadeBy` link or a timestamp will fail
validation and be flagged rather than silently forwarded to downstream consumers:

```sql
SELECT pg_ripple.load_shacl($$
    ex:ObservationShape a sh:NodeShape ;
        sh:targetClass saref:Measurement ;
        sh:property [
            sh:path saref:measurementMadeBy ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
            sh:message "Every measurement must reference exactly one device." ;
        ] ;
        sh:property [
            sh:path saref:hasTimestamp ;
            sh:minCount 1 ;
            sh:datatype xsd:dateTime ;
            sh:message "Every measurement must have an xsd:dateTime timestamp." ;
        ] .
$$);
```

### Step 5 — Route enriched events to downstream consumers

Now that observations are stored, enriched, and validated, we set up the outbound
pipeline. We create a bridge table to hold outbound events, subscribe to the
inferred alert triples, and configure relay forward pipelines to deliver them
wherever they are needed.

```sql
-- The bridge table holds outbound events in JSON-LD format.
-- pg-trickle watches this table and relays its rows to external sinks.
CREATE TABLE enriched_events (
    id         BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload    JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Subscribe to exactly the inferred alert triples we care about.
-- Only triples where the predicate is ex:tempAlert will be bridged.
SELECT pg_ripple.create_subscription(
    name          => 'high-temp-alerts',
    filter_sparql => 'FILTER(?p = <https://example.org/tempAlert>)'
);

-- Tell pg-trickle to treat this table as an outbox.
SELECT pgtrickle.enable_outbox('enriched_events');
```

Now configure where the relay should forward those events:

```sql
-- Push to NATS for real-time dashboard consumers
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-nats',
    outbox => 'enriched_events',
    group  => 'nats-publisher',
    sink   => '{"type":"nats","url":"nats://nats:4222",
                "subject_template":"ripple.enriched.{event_type}"}'
);

-- Push to Kafka for the analytics and ML pipeline
SELECT pgtrickle.set_relay_outbox(
    'alerts-to-kafka',
    outbox => 'enriched_events',
    group  => 'kafka-publisher',
    sink   => '{"type":"kafka","brokers":"${env:KAFKA_BROKERS}",
                "topic":"ripple.alerts"}'
);

-- Push to a partner API via webhook
SELECT pgtrickle.set_relay_outbox(
    'enriched-to-partner',
    outbox => 'enriched_events',
    group  => 'partner-publisher',
    sink   => '{"type":"http","url":"https://partner.example.com/events",
                "method":"POST"}'
);
```

The rows that land in downstream consumers look like this — a self-contained
JSON-LD document that any system can parse without knowing anything about
pg_ripple's internal storage:

```json
{
  "@context": {
    "saref": "https://saref.etsi.org/core/",
    "xsd":   "http://www.w3.org/2001/XMLSchema#",
    "ex":    "https://example.org/"
  },
  "@id":   "https://example.org/observation/kafka:iot.sensors:0:99",
  "@type": "saref:Measurement",
  "saref:measurementMadeBy": { "@id": "https://example.org/device/sensor-7" },
  "saref:hasValue": { "@value": "45.2", "@type": "xsd:decimal" },
  "saref:hasTimestamp": {
    "@value": "2026-04-28T11:32:00Z",
    "@type":  "xsd:dateTime"
  },
  "ex:tempAlert": { "@id": "https://example.org/device/sensor-7" },
  "_relay_dedup_key": "ripple:8301047"
}
```

The `_relay_dedup_key` field is derived from pg_ripple's internal statement ID
(the `i` column in VP tables). This guarantees that even if the relay restarts
and replays the outbox, downstream consumers can detect and discard duplicates.

---

## Choosing a CDC bridge approach

The five steps above describe the logical flow, but there is a question of
*when* exactly enriched triples get written to the `enriched_events` bridge
table. pg_ripple's CDC system fires a PostgreSQL `NOTIFY` whenever new triples
are inserted (including inferred triples from Datalog). Three approaches bridge
that notification to a pg-trickle outbox row, each with different latency and
throughput trade-offs.

### Approach A — Trigger bridge (lowest latency, < 10 ms)

The simplest approach: a trigger on VP delta tables fires in the same
transaction that inserted the triple. The triple is decoded from its internal
integer representation and written directly to the bridge table before the
transaction commits. Nothing can slip through — the data is in both places or
neither:

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
**Trade-off**: `decode_id()` is called once per row, which adds overhead on high-volume
write paths. For filtering — forwarding only alerts and not every measurement — you
need an extra `WHERE` condition in the trigger.

### Approach B — Background worker bridge (best throughput, 50–500 ms)

The pg_ripple background worker (enabled since v0.52.0) wakes up when it
receives CDC `NOTIFY` events, collects a batch of them, decodes all the integer
dictionary IDs in a single bulk SPI call, and inserts the decoded rows into the
bridge table in one go. The amortised cost per triple is much lower than the
trigger approach, and the batch size and flush interval are configurable.

This approach adds a small latency window (the batch collection time), but for
high-volume enriched-data streams — thousands of triples per second — the
throughput improvement is significant. Use this as the default for bulk data paths.

**Best for**: Bulk enriched data, high-volume streams.  
**Trade-off**: Adds configurable milliseconds of latency.

### Approach C — Named subscription with SPARQL CONSTRUCT view (most flexible)

Named subscriptions (v0.42.0+) let you attach a SPARQL `FILTER` expression
that controls exactly which triples are bridged. Only the triples that match
the filter expression will ever touch the outbox, which is useful when the
inference rules produce many intermediate triples that you do not want to
forward:

```sql
-- Only bridge the final alert triples, not intermediate inference steps
SELECT pg_ripple.create_subscription(
    name          => 'alerts',
    filter_sparql => 'FILTER(?p = <https://example.org/alert>)'
);
```

Combine this with a SPARQL `CONSTRUCT` view to shape the outbound payload into
any structure the downstream consumer expects, rather than forwarding raw
subject/predicate/object tuples:

```sql
SELECT pg_ripple.sparql('
    CONSTRUCT {
        ?device ex:alertLevel "HIGH" ;
                schema:name   ?name ;
                ex:latestTemp ?temp .
    }
    WHERE {
        ?obs ex:tempAlert ?device ;
             saref:hasValue ?temp .
        OPTIONAL { ?device schema:name ?name }
    }
');
```

The JSON-LD output from this view is rich and ready to consume:

```json
{
  "@context": {
    "ex":     "https://example.org/",
    "schema": "https://schema.org/",
    "xsd":    "http://www.w3.org/2001/XMLSchema#"
  },
  "@id": "https://example.org/device/sensor-7",
  "ex:alertLevel": "HIGH",
  "schema:name":   "Boiler Room Sensor 7",
  "ex:latestTemp": { "@value": "45.2", "@type": "xsd:decimal" }
}
```

**Best for**: Complex SPARQL-shaped payloads, scheduled reports, ad-hoc shapes.  
**Trade-off**: Polling-based unless combined with a LISTEN/NOTIFY wake-up.

### Which approach to use

In practice you will use all three for different data paths in the same hub:

| Data path | Mechanism | Typical latency |
|---|---|---|
| High-priority alerts | Approach A — trigger bridge | < 10 ms |
| Bulk enriched data | Approach B — background worker | 50–500 ms |
| Shaped reports / views | Approach C — SPARQL CONSTRUCT | Cron-driven |

---

## Common patterns

### Unifying records from multiple sources

A real hub almost always receives data about the same real-world entities from
multiple systems — a customer appears in the CRM as `crm:C1` and in the ERP as
`erp:A1`, but they are the same person. Datalog `owl:sameAs` rules detect these
overlaps from shared attributes (email address, serial number, phone number) and
create linking triples that allow downstream SPARQL queries to treat both records
as one:

```prolog
% Link CRM and ERP records that share an email address
owl:sameAs(CrmCust, ErpAcct) :-
    crm:emailAddress(CrmCust, E),
    erp:contact_email(ErpAcct, E).
```

Once those `owl:sameAs` triples are materialised, pg_ripple's OWL RL
canonicalisation ensures that any query for `crm:C1` will transparently find
data originally stored under `erp:A1` as well.

### Speaking a common language to downstream consumers

Each source system uses its own property names. Your CRM calls it
`crm:customerName`; the ERP calls it `erp:accountTitle`. Rather than
requiring every downstream consumer to understand every source vocabulary,
Datalog rules project everything onto a single shared ontology (here, Schema.org):

```prolog
% Both CRM and ERP names map to schema:name
schema:name(X, V)  :- crm:customerName(X, V).
schema:name(X, V)  :- erp:accountTitle(X, V).

% Both email fields map to schema:email
schema:email(X, V) :- crm:emailAddress(X, V).
schema:email(X, V) :- erp:contact_email(X, V).
```

Downstream consumers now only need to understand Schema.org. Source schema
changes are isolated to a single Datalog rule update in the hub — no downstream
changes required.

### Rich JSON-LD for event-driven downstream consumers

Here is what a fully enriched and shaped customer record looks like by the time
it reaches a downstream consumer via the relay. Notice how all the messy
source-system vocabulary has been replaced with clean Schema.org terms, and the
document is self-describing thanks to the `@context`:

```json
{
  "@context": {
    "schema": "https://schema.org/",
    "ex":     "https://example.org/",
    "xsd":    "http://www.w3.org/2001/XMLSchema#"
  },
  "@id":   "https://example.org/customer/C1",
  "@type": "schema:Customer",
  "schema:name":  "Jane Doe",
  "schema:email": "jane@example.com",
  "ex:riskScore": { "@value": "0.87", "@type": "xsd:decimal" },
  "ex:highValueCustomer": true,
  "owl:sameAs": [
    { "@id": "https://erp.example.com/accounts/A1" },
    { "@id": "https://support.example.com/tickets/T9" }
  ],
  "_relay_dedup_key": "ripple:4200042"
}
```

The `owl:sameAs` array shows the entity resolution result — this one record
links the CRM, ERP, and support-ticket identities together. The `ex:riskScore`
was derived by a Datalog rule from order history data. The `_relay_dedup_key`
ensures the relay can handle restarts safely.

---

## Deployment

Both extensions run in the same PostgreSQL instance. The relay binary is a
separate, stateless process that you deploy in as many copies as you need:
separate pods for each inbound source, separate pods for each outbound sink.
Advisory locks inside PostgreSQL prevent duplicate processing if you scale
relay pods horizontally.

```yaml
# docker-compose.yml sketch
services:
  postgres:
    image: postgres:18
    # Both pg_ripple and pg-trickle extensions installed

  relay-inbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
      KAFKA_BROKERS: kafka:9092
    # Handles all reverse (inbound) pipelines

  relay-outbound:
    image: grove/pgtrickle-relay:0.25.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
    # Handles all forward (outbound) pipelines

  pg-ripple-http:
    image: pg-ripple-http:latest
    environment:
      DATABASE_URL: postgres://ripple:pw@postgres/hub
    ports:
      - "8080:8080"
    # SPARQL endpoint for ad-hoc queries from dashboards and tools

  kafka:
    image: redpandadata/redpanda:latest

  nats:
    image: nats:latest
    command: ["-js"]   # JetStream enabled for durable subscriptions
```

For Kubernetes deployments, the relay's `/health/drained` endpoint integrates
with readiness probes. See [Kubernetes & Helm](kubernetes.md) for a full Helm
chart example.

---

## Things to watch out for

### Outbox growing faster than it drains (backpressure)

When a single inbound event triggers many inferred triples — for example, an
`owl:sameAs` merge that touches hundreds of related facts — the outbox can
accumulate rows faster than the relay drains them. Three controls help:

- Use **pg-trickle's retention drain** to cap outbox size and drop the oldest
  rows once a maximum depth is reached.
- Use the relay's `/health/drained` endpoint as a Kubernetes readiness signal
  so the cluster can apply back-pressure to inbound relay pods.
- Use pg_ripple's `source` column to bridge only explicit triples (`source = 0`)
  and suppress inferred triples (`source = 1`) from a particular outbox, reducing
  volume without changing the inference rules.

### Keeping consumers in sync after rule changes

When you update a Datalog rule or SHACL shape — adding a new derived property,
changing a threshold — downstream consumers are receiving a different data
shape than before. Manage this with:

- **Version the subject template** in the relay outbox configuration:
  `ripple.v2.enriched.{type}` rather than `ripple.enriched.{type}`.
- **Include a `@context` version** field in outbound JSON-LD payloads so
  consumers can detect schema changes programmatically.
- **Use the relay's full-refresh mode** to re-snapshot the entire outbox
  after a rule change, ensuring consumers that missed the transition catch up.

---

## Related pages

- [CDC Operations](cdc.md)
- [Citus + pg-trickle Integration](citus-integration.md)
- [Cookbook: CDC → Kafka via JSON-LD Outbox](../cookbook/cdc-to-kafka.md)
- [Reasoning and Inference (Datalog)](../features/reasoning-and-inference.md)
- [Validating Data Quality (SHACL)](../features/validating-data-quality.md)
- [Exporting and Sharing](../features/exporting-and-sharing.md)
