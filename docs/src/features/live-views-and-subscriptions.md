# Live Views and Subscriptions

Two features cover the *push* side of pg_ripple — getting data **out** as it changes, instead of polling for it.

| Feature | Best for | Backed by |
|---|---|---|
| **Materialized SPARQL / Datalog views** | Always-fresh dashboards, denormalised tables | [pg_trickle](https://github.com/grove/pg-trickle) |
| **CDC subscriptions** | Streaming events to applications, Kafka, WebSocket clients | PostgreSQL `LISTEN/NOTIFY` |

If you want a snapshot of *what is true now*, use a view. If you want a stream of *what changed since I last looked*, use CDC.

---

## Materialized SPARQL and Datalog views

A SPARQL view compiles a SPARQL `SELECT` (or a Datalog goal) into a pg_trickle stream table that is incrementally maintained as triples change. The view always reflects the latest data without you running the query yourself.

> Requires the optional [`pg_trickle`](https://github.com/grove/pg-trickle) extension. pg_ripple loads and runs without it; view functions detect its absence at call time and return a clear error with an install hint.

```sql
-- Check availability before using.
SELECT pg_ripple.pg_trickle_available();   -- true / false

-- Create a view of all people and their names, refreshed every second.
SELECT pg_ripple.create_sparql_view(
    name     := 'people_names',
    sparql   := 'SELECT ?p ?name WHERE { ?p <http://xmlns.com/foaf/0.1/name> ?name }',
    schedule := '1s',
    decode   := true
);

-- Query the view as a regular table.
SELECT * FROM pg_ripple.people_names;

-- Drop when you are done.
SELECT pg_ripple.drop_sparql_view('people_names');
```

The `decode` flag controls whether the stream table stores raw dictionary IDs (`BIGINT`, fastest) or decoded strings (`TEXT`, easiest to consume). Decoded views are a touch slower to refresh but trivial to point a BI tool at.

You can build the same kind of view from a Datalog goal:

```sql
SELECT pg_ripple.create_datalog_view(
    name     := 'indirect_managers',
    goal     := '?x <https://example.org/indirectManager> ?y',
    schedule := '5s'
);
```

These views integrate with the materialised inference pipeline — the view stays correct after `infer()`, `retract_rule()`, and `clear_graph()`.

---

## CDC subscriptions

A subscription publishes a JSON message on a named PostgreSQL `NOTIFY` channel every time a triple is inserted or deleted that matches a SPARQL filter. Listeners receive changes with no polling.

> Available since v0.42.0.

### Create

```sql
-- Subscribe to all changes.
SELECT pg_ripple.create_subscription('all_changes');

-- Subscribe with a SPARQL pattern filter.
SELECT pg_ripple.create_subscription(
    'person_changes',
    filter_sparql := 'SELECT ?s ?p ?o WHERE { ?s a <https://schema.org/Person> ; ?p ?o }'
);

-- Subscribe with a SHACL-shape filter — only triples that *violate* the shape are published.
SELECT pg_ripple.create_subscription(
    'shape_violations',
    filter_shape := '<https://shapes.example.org/PersonShape>'
);
```

### Listen

```sql
LISTEN pg_ripple_cdc_person_changes;
```

In your application (Python, Node.js, Go, …) connect, issue the same `LISTEN`, and read the notification stream:

```python
import psycopg
import json

with psycopg.connect("...") as conn:
    conn.set_isolation_level(0)  # AUTOCOMMIT
    with conn.cursor() as cur:
        cur.execute("LISTEN pg_ripple_cdc_person_changes;")
    for notify in conn.notifies():
        event = json.loads(notify.payload)
        print(event["op"], event["s"], event["p"], event["o"])
```

### Payload

```json
{
  "op": "add",
  "s": "<https://example.org/alice>",
  "p": "<https://schema.org/name>",
  "o": "\"Alice\"",
  "g": ""
}
```

| Field | Meaning |
|---|---|
| `op` | `"add"` or `"remove"` |
| `s` / `p` / `o` | N-Triples-formatted subject / predicate / object |
| `g` | Named-graph IRI, or empty string for the default graph |

### Manage

```sql
SELECT name, has_filter, created_at FROM pg_ripple.list_subscriptions();
SELECT pg_ripple.drop_subscription('person_changes');
```

### WebSocket access via `pg_ripple_http`

When the [`pg_ripple_http`](apis-and-integration.md) companion service is running, every subscription is automatically exposed as a WebSocket endpoint:

```
ws://<host>:8080/ws/subscriptions/{name}
```

The HTTP service handles backpressure, reconnection, and authentication — you point a browser-side EventSource or a server-side stream consumer at the URL.

### Lifecycle telemetry

The `_pg_ripple.cdc_lifecycle_events` table records every subscription create / drop / error, with timestamps. Useful for alerting on dropped subscriptions in production.

---

## Choosing between views and subscriptions

| If you want… | Use |
|---|---|
| A table that always shows the latest aggregate / projection | **View** |
| A push notification per change to drive an external system | **Subscription** |
| A WebSocket stream to a browser | **Subscription** + `pg_ripple_http` |
| A denormalised cache of derived facts | **Datalog view** |
| To trigger Kafka / SQS / SNS messages on change | **Subscription** + an outbox worker |

The two compose: build a view of *what is currently true*, and a subscription of *what just changed*, and let your application choose per use case.

---

## See also

- [APIs and Integration](apis-and-integration.md) — `pg_ripple_http` and WebSocket access.
- [Cookbook: CDC → Kafka via JSON-LD outbox](../cookbook/cdc-to-kafka.md)
