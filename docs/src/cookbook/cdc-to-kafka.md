# Cookbook: CDC → Kafka via JSON-LD Outbox

**Goal.** Push a stream of structured graph-change events into Kafka (or NATS, RabbitMQ, AWS SNS — anywhere your event bus lives), without polling the database, and using a JSON-LD payload that downstream consumers can validate against the same SHACL shapes the database uses.

**Why pg_ripple.** Combines CDC subscriptions (push, no polling), JSON-LD framing (schema-shaped payloads), and the transactional-outbox pattern (no lost events).

**Time to first result.** ~30 minutes (most of it is wiring the Kafka producer).

---

## Architecture

```
   triple INSERT/DELETE
            │
            ▼
   ┌────────────────────────┐
   │ CDC subscription       │  pg_ripple.create_subscription('out_persons',
   │ filter: SHACL shape    │       filter_shape := '<…/PersonShape>')
   └─────────┬──────────────┘
             │ NOTIFY  ──── JSON  ─────┐
             ▼                          │
   ┌────────────────────────┐           │
   │ outbox table           │           │  alternatively, a small
   │ INSERT trigger writes  │           │  Rust/Python LISTENer
   │ JSON-LD framed payload │           │
   └─────────┬──────────────┘           │
             ▼                          ▼
   ┌────────────────────────┐  ┌────────────────────────┐
   │ Debezium / outbox      │  │ asyncpg LISTENer       │
   │ connector → Kafka      │  │ → Kafka producer       │
   └────────────────────────┘  └────────────────────────┘
```

Two valid implementations:

- **Outbox + Debezium** (recommended for at-least-once durability).
- **Direct LISTEN** (recommended for low-latency, fire-and-forget streams).

The recipe shows both.

---

## Step 1 — Define the change shape

The simplest payload is *the entire updated entity* in JSON-LD framed shape, so downstream consumers see a self-contained document. Define the frame once:

```sql
SELECT pg_ripple.register_jsonld_frame('person_event', $JSON$
{
  "@context": {
    "name":  "http://xmlns.com/foaf/0.1/name",
    "email": "http://xmlns.com/foaf/0.1/mbox",
    "knows": { "@id": "http://xmlns.com/foaf/0.1/knows", "@type": "@id" }
  },
  "@type": "http://xmlns.com/foaf/0.1/Person"
}
$JSON$);
```

## Step 2 — Create a CDC subscription

```sql
-- Optional: create a SHACL shape so the subscription only fires for valid Persons.
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

<https://shapes.example.org/PersonShape> a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    sh:property [ sh:path foaf:name ; sh:minCount 1 ] .
$TTL$);

SELECT pg_ripple.create_subscription(
    'persons_out',
    filter_sparql := 'SELECT ?s ?p ?o WHERE { ?s a <http://xmlns.com/foaf/0.1/Person> ; ?p ?o }'
);
```

## Step 3a — Direct-LISTEN producer

The lowest-latency path: a tiny LISTENer reads the NOTIFY stream and produces straight to Kafka.

```python
import asyncio, json, asyncpg
from aiokafka import AIOKafkaProducer

async def main():
    conn  = await asyncpg.connect("postgresql://…")
    kafka = AIOKafkaProducer(bootstrap_servers="kafka:9092")
    await kafka.start()
    queue: asyncio.Queue = asyncio.Queue()

    def callback(_conn, _pid, _channel, payload):
        queue.put_nowait(payload)

    await conn.add_listener("pg_ripple_cdc_persons_out", callback)

    while True:
        raw = await queue.get()
        evt = json.loads(raw)
        # Optionally re-frame the changed entity as JSON-LD.
        await kafka.send_and_wait("graph.persons", raw.encode())

asyncio.run(main())
```

The CDC payload already includes `op`, `s`, `p`, `o`, `g` — see [CDC subscriptions](../features/live-views-and-subscriptions.md#cdc-subscriptions).

For full-entity payloads (rather than per-triple events), call `sparql_construct_jsonld()` inside the LISTENer to fetch the framed entity at the time of the change.

## Step 3b — Outbox + Debezium

For at-least-once durability you want the events in a *table*, replicated by Debezium. Add an outbox trigger that materialises the JSON-LD payload at write time:

```sql
CREATE TABLE outbox (
    id          BIGSERIAL PRIMARY KEY,
    aggregate   TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    payload     JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION enqueue_person_event() RETURNS trigger AS $$
DECLARE
    framed JSONB;
BEGIN
    framed := pg_ripple.sparql_construct_jsonld(
        format(
            'CONSTRUCT { %s ?p ?o } WHERE { %s ?p ?o }',
            NEW.s, NEW.s
        ),
        frame_name := 'person_event'
    );
    INSERT INTO outbox (aggregate, event_type, payload)
    VALUES (NEW.s, 'PersonChanged', framed);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- pg_ripple.cdc_events is the catch-up table written by every subscription.
CREATE TRIGGER outbox_persons
AFTER INSERT ON _pg_ripple.cdc_events
FOR EACH ROW
WHEN (NEW.subscription = 'persons_out')
EXECUTE FUNCTION enqueue_person_event();
```

Point Debezium at the `outbox` table; configure it to delete rows after publishing. The pattern is canonical and is exactly what Debezium's "outbox event router" SMT was designed for.

## Step 4 — Validate consumers

The same SHACL shape that filters the CDC subscription can be shipped to downstream consumers as the contract. Consumers that read the JSON-LD payload validate it with any standard SHACL library (Python `pyshacl`, Java `topbraid-shacl`, etc.). If the contract changes, only one document changes — the `<https://shapes.example.org/PersonShape>` definition.

---

## Failure modes and how to handle them

| Failure | Direct LISTEN | Outbox + Debezium |
|---|---|---|
| Subscriber crashes | Events lost | Events persisted, replayed on restart |
| NOTIFY queue overflows | Events dropped | Outbox grows; backpressure handled |
| Consumer slow | Producer backpressures | Outbox grows; cleanup lags |
| Schema drift | Consumer parses garbage | Outbox + SHACL catches it before publish |

The outbox path is more code; in return you get the durability guarantees most production event buses expect.

---

## See also

- [Live Views and Subscriptions](../features/live-views-and-subscriptions.md)
- [Exporting and Sharing — JSON-LD framing](../features/exporting-and-sharing.md)
- [APIs and Integration — `pg_ripple_http`](../features/apis-and-integration.md)
