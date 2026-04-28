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

The `${env:KAFKA_BROKERS}` syntax tells the relay to expand an environment variable
at runtime. Pipeline configs are stored in the database as JSONB, but sensitive
values like broker addresses can reference environment variables this way —
the actual value stays in the relay process's environment and never needs to be
stored in plaintext in the database.

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

The `pg_ripple.json_to_ntriples_and_load()` function (v0.52.0+) does this in one
call. Its `context` parameter works like a JSON-LD `@context`: it maps the incoming
JSON field names to the full predicate IRIs you want to store. The relay delivers
plain JSON; the IRI mapping is applied at load time inside PostgreSQL, so the
original message is never modified.

A trigger on `sensor_inbox` fires for every inserted row:

```sql
CREATE OR REPLACE FUNCTION transform_sensor_to_rdf()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_ripple.json_to_ntriples_and_load(
        payload     => NEW.payload,
        subject_iri => 'https://example.org/observation/' || NEW.event_id,
        type_iri    => 'https://saref.etsi.org/core/Measurement',
        context     => '{
            "@vocab":  "https://saref.etsi.org/core/",
            "device":  "https://saref.etsi.org/core/measurementMadeBy",
            "temp":    "https://saref.etsi.org/core/hasValue",
            "ts":      "https://saref.etsi.org/core/hasTimestamp",
            "unit":    "https://qudt.org/schema/qudt/unit"
        }'::jsonb
    );
    RETURN NEW;
END;
$$;

CREATE TRIGGER sensor_to_rdf
AFTER INSERT ON sensor_inbox
FOR EACH ROW EXECUTE FUNCTION transform_sensor_to_rdf();
```

The `context` object supports two resolution mechanisms:

- **`@vocab`** — a default IRI prefix applied to every unmapped key. Any field
  not explicitly listed gets expanded to `https://saref.etsi.org/core/{field}`.
- **Explicit entries** — override specific keys with the exact IRI you want,
  regardless of the `@vocab` default.

Nested JSON objects become blank nodes. Arrays produce one triple per element.
`null` values are silently skipped. This means you can point the relay at an
arbitrary third-party JSON event and describe the entire mapping in one JSONB
literal — no code changes needed when a vendor renames a field, only a context
update.

After the trigger fires, those five triples exist in the triplestore — one
`rdf:type` triple (from `type_iri`) and one for each of the four data fields in
the source JSON. If you queried them back as JSON-LD immediately after load, you
would see:

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
    @prefix sh:    <http://www.w3.org/ns/shacl#> .
    @prefix saref: <https://saref.etsi.org/core/> .
    @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
    @prefix ex:    <https://example.org/> .

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

-- Declare which triples the subscription should watch for.
-- Only triples where the predicate is ex:tempAlert will pass through.
SELECT pg_ripple.create_subscription(
    name          => 'high-temp-alerts',
    filter_sparql => 'FILTER(?p = <https://example.org/tempAlert>)'
);

-- Wire the subscription to the outbox table.
-- This installs a trigger on the VP delta table for ex:tempAlert; whenever a
-- matching triple lands, the trigger decodes it and writes a row to
-- enriched_events. That is the only writer to enriched_events in this pipeline.
SELECT pg_ripple.enable_cdc_bridge_trigger(
    name      => 'high-temp-alerts',
    predicate => 'https://example.org/tempAlert',
    outbox    => 'enriched_events'
);

-- Tell pg-trickle to treat this table as an outbox so the relay can poll it.
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

When paired with a framing trigger — as shown in the
[JSON-LD mapping section](#json-ld-mapping-inbound-context-and-outbound-framing)
below — the payload that lands in downstream consumers is a self-contained
JSON-LD document. The framing trigger queries back the full set of triples for
the alert's subject, so the output includes all the observation data, not just
the alert predicate that triggered it:

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

Step 5 used `enable_cdc_bridge_trigger` (Approach A below) — the simplest
option, with latency under 10 ms. Two other approaches offer different
trade-offs for different data paths in the same hub. All three write to the
same `enriched_events` outbox table; only the *mechanism* that detects new
triples and writes outbox rows differs.

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

The `pg_ripple.enable_cdc_bridge_trigger()` call in Step 5 installs exactly
this pattern automatically — you do not need to write the trigger function by
hand. To produce a richer shaped payload instead of a raw decoded triple,
replace the default function with a custom one that calls `export_jsonld_framed()`
(see [Outbound framing](#json-ld-mapping-inbound-context-and-outbound-framing)).

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
    filter_sparql => 'FILTER(?p = <https://example.org/tempAlert>)'
);
```

Combine this with `export_jsonld_framed()` to shape the outbound payload into
exactly the JSON structure the downstream consumer expects. A JSON-LD **frame**
is a template you write once that describes the desired nesting and field names.
pg_ripple translates it to a SPARQL CONSTRUCT query internally, executes it,
applies the W3C embedding algorithm, and compacts the result with your `@context`:

```sql
SELECT pg_ripple.export_jsonld_framed(
    frame => '{
        "@context": {
            "ex":     "https://example.org/",
            "schema": "https://schema.org/",
            "saref":  "https://saref.etsi.org/core/",
            "xsd":    "http://www.w3.org/2001/XMLSchema#",
            "alertLevel": "ex:alertLevel",
            "name":       "schema:name",
            "latestTemp": "ex:latestTemp"
        },
        "@type": "ex:Alert",
        "alertLevel": {},
        "name": {},
        "latestTemp": {}
    }'::jsonb
);
```

The framed output is a clean, nested JSON-LD document ready to drop straight
into the pg-trickle outbox:

```json
{
  "@context": {
    "ex":     "https://example.org/",
    "schema": "https://schema.org/",
    "xsd":    "http://www.w3.org/2001/XMLSchema#",
    "alertLevel": "ex:alertLevel",
    "name":       "schema:name",
    "latestTemp": "ex:latestTemp"
  },
  "@graph": [
    {
      "@id":       "https://example.org/device/sensor-7",
      "@type":     "ex:Alert",
      "alertLevel": "HIGH",
      "name":       "Boiler Room Sensor 7",
      "latestTemp": { "@value": "45.2", "@type": "xsd:decimal" }
    }
  ]
}
```

Use `jsonld_frame_to_sparql(frame => ...)` to inspect the generated CONSTRUCT
query before running the full export — this is useful for performance tuning.

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

## JSON-LD mapping: inbound context and outbound framing

JSON-LD mapping is the mechanism that lets pg_ripple act as a true semantic
bridge: arbitrary JSON goes in, canonicalised knowledge graph triples are stored,
and shaped JSON-LD comes out. The inbound and outbound sides each have a dedicated
function.

### Inbound — `json_to_ntriples_and_load()` with a context map

When the relay delivers a raw JSON event, `pg_ripple.json_to_ntriples_and_load()`
converts it to RDF triples in one step. The `context` parameter is a JSONB object
that works like a JSON-LD `@context`:

```
Incoming JSON                    context map                     stored triples
─────────────                    ───────────                     ──────────────
{ "temp": 45.2, "device": ... }  "temp"  → saref:hasValue   →   <obs> saref:hasValue "45.2"^^xsd:decimal
                                 "device"→ saref:madeby      →   <obs> saref:madeby <device-7>
                                 @vocab  → saref:             →   unmapped keys get saref: prefix
```

It supports:
- **`@vocab`** — default IRI prefix for all keys not explicitly listed.
- **Explicit key-to-IRI mappings** — override specific fields with exact predicate IRIs.
- **Nested objects** — become blank nodes with their own predicates resolved through the same context.
- **Arrays** — produce one triple per element.
- **`null` values** — silently skipped.

The context is stored only in the trigger definition, not in the triplestore. When
a source vendor renames a field, you update the context JSONB; no triples need to
change.

```sql
-- Full context example for a heterogeneous event shape
PERFORM pg_ripple.json_to_ntriples_and_load(
    payload     => NEW.payload,
    subject_iri => 'https://example.org/event/' || NEW.event_id,
    type_iri    => 'https://schema.org/Event',
    context     => '{
        "@vocab":      "https://schema.org/",
        "ts":          "https://schema.org/startDate",
        "location":    "https://schema.org/location",
        "description": "https://schema.org/description",
        "external_id": "https://example.org/externalId"
    }'::jsonb
);
```

### Outbound — `export_jsonld_framed()` with a frame template

On the way out, `pg_ripple.export_jsonld_framed()` (v0.17.0+) shapes the flat
RDF into whatever nested JSON structure the downstream consumer expects. A
**frame** is a JSON template that describes the desired structure; pg_ripple
handles everything else:

1. Translates the frame into a SPARQL CONSTRUCT query.
2. Executes the query against the triplestore.
3. Applies the W3C JSON-LD 1.1 embedding algorithm to produce nested nodes.
4. Compacts IRI strings using the frame's `@context`.

```
stored triples               frame template                  outbound JSON-LD
──────────────               ──────────────                  ───────────────
<device> schema:name "X"     "@type": "schema:Device"        { "@type": "Device",
<device> ex:temp 45.2        "name": {},                →      "name": "X",
<device> ex:alert "HIGH"     "temp": {},                       "temp": 45.2,
                             "alert": {}                        "alert": "HIGH" }
```

Use this inside the outbox bridge trigger to produce consumer-ready JSON-LD:

```sql
-- Write a framed JSON-LD event to the outbox every time an alert triple lands
CREATE OR REPLACE FUNCTION bridge_alert_to_outbox()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    framed JSONB;
BEGIN
    framed := pg_ripple.export_jsonld_framed(
        frame => '{
            "@context": {
                "schema": "https://schema.org/",
                "ex":     "https://example.org/",
                "xsd":    "http://www.w3.org/2001/XMLSchema#",
                "name":       "schema:name",
                "latestTemp": "ex:latestTemp",
                "alertLevel": "ex:alertLevel"
            },
            "@type": "ex:Alert",
            "name": {},
            "latestTemp": {},
            "alertLevel": {}
        }'::jsonb
    );

    INSERT INTO enriched_events (event_type, payload)
    VALUES ('alert', framed);

    RETURN NEW;
END;
$$;
```

The downstream consumer receives a document shaped exactly to the frame — with
short, readable property names from the `@context`, nested objects where the
frame requests them, and a self-describing `@context` block so it can be parsed
without any knowledge of pg_ripple's internals.

### Debugging the frame translation

Before running `export_jsonld_framed()` in production, use
`jsonld_frame_to_sparql()` to see the SPARQL CONSTRUCT query that will be
generated. This is useful for verifying that the frame matches your stored
triple shapes and for identifying any missing patterns before they cause
silent empty results:

```sql
SELECT pg_ripple.jsonld_frame_to_sparql(
    frame => '{
        "@context": { "schema": "https://schema.org/" },
        "@type":    "schema:Device",
        "schema:name": {}
    }'::jsonb
);
```

### Symmetric round-trip

The two functions form a symmetric pair: `json_to_ntriples_and_load()` maps
field names from the source JSON vocabulary to RDF predicates on the way in;
`export_jsonld_framed()` maps those same predicates back to the field names and
nested structure the consumer needs on the way out. You can use different
`@context` definitions for different consumers — the triplestore is the stable
canonical representation in the middle, and the vocabularies at each edge are
entirely configurable.

---

## Deployment

Both extensions run in the same PostgreSQL instance. The relay binary is a
separate, stateless process. A single relay instance handles **both directions**
— inbound pipelines (external source → inbox table) and outbound pipelines
(outbox table → external sink) run in the same process. You do not need separate
relay binaries for each direction.

For high availability, run two or three relay instances pointing at the same
PostgreSQL database. PostgreSQL advisory locks elect exactly one owner per
pipeline — if one instance dies, another acquires its pipelines on the next
discovery interval.

The relay only needs one environment variable to start: the database URL.
All pipeline configuration is registered in the database via SQL
(`pgtrickle.set_relay_outbox()` / `pgtrickle.set_relay_inbox()`), and the relay
reads it on startup and hot-reloads it when you make changes — no restart
required.

Sensitive values like broker addresses can use `${env:VAR}` placeholders inside
the JSONB config. The relay expands them from its own process environment at
runtime, so credentials never need to be stored in the database.

```yaml
# docker-compose.yml sketch
services:
  postgres:
    image: postgres:18
    # Both pg_ripple and pg-trickle extensions installed

  relay:
    image: ghcr.io/grove/pgtrickle-relay:0.29.0
    environment:
      PGTRICKLE_RELAY_POSTGRES_URL: postgres://relay:pw@postgres/hub
      # All pipeline config (topics, subjects, poll intervals) lives in the DB.
      # Broker addresses can use ${env:VAR} refs — set them here as env vars.
      # Register pipelines with pgtrickle.set_relay_outbox() / set_relay_inbox().
      KAFKA_BROKERS: kafka:9092   # expanded by ${env:KAFKA_BROKERS} in pipeline config
      NATS_URL: nats://nats:4222  # similarly available via ${env:NATS_URL}
    ports:
      - "9090:9090"   # Prometheus metrics + /health endpoint

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

For Kubernetes deployments, the relay's `/health` endpoint integrates with
readiness probes. See [Kubernetes & Helm](kubernetes.md) for a full Helm
chart example.

---

## Things to watch out for

### Outbox growing faster than it drains (backpressure)

When a single inbound event triggers many inferred triples — for example, an
`owl:sameAs` merge that touches hundreds of related facts — the outbox can
accumulate rows faster than the relay drains them. Three controls help:

- Use **pg-trickle's retention drain** to cap outbox size and drop the oldest
  rows once a maximum depth is reached.
- Use the relay's `/health` endpoint as a Kubernetes readiness probe. When the
  relay falls behind, it signals not-ready, letting the cluster apply
  back-pressure to inbound sources.
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
