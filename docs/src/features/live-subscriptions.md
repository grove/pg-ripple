# Live SPARQL Subscriptions

**Status**: Experimental — v0.73.0 (SUB-01)  
**API**: `pg_ripple.subscribe_sparql()` / `pg_ripple.unsubscribe_sparql()`  
**HTTP**: `GET /subscribe/:subscription_id` (Server-Sent Events)  

---

## Overview

Live SPARQL subscriptions allow applications to receive real-time notifications
when the result of a SPARQL SELECT query changes.  Whenever a graph write or
delete touches a graph that a registered subscription monitors, pg_ripple calls
`pg_notify('pg_ripple_subscription_<id>', ...)` with the updated query result as
a JSONB payload.

The `pg_ripple_http` companion service exposes a `GET /subscribe/:id` endpoint
that translates PostgreSQL `LISTEN` notifications into Server-Sent Events (SSE)
that any HTTP client can consume.

---

## Quick start

### 1. Register a subscription

```sql
-- Register a subscription that fires whenever a triple is written to
-- the default graph and the query over it produces a different result.
SELECT pg_ripple.subscribe_sparql(
    'my-sub-01',
    'SELECT ?s ?label WHERE { ?s <https://schema.org/name> ?label }',
    NULL   -- NULL = monitor all graphs
);
```

### 2. Listen for changes in any PostgreSQL client

```sql
LISTEN pg_ripple_subscription_my-sub-01;
-- Each NOTIFY payload is a JSONB string of the updated result set.
```

### 3. Listen via HTTP SSE

```bash
curl -N http://localhost:7878/subscribe/my-sub-01 \
     -H "Authorization: Bearer $TOKEN"
```

Each Server-Sent Event has `event: sparql_result` and `data: <jsonb>` fields.

### 4. Unregister

```sql
SELECT pg_ripple.unsubscribe_sparql('my-sub-01');
```

---

## SQL API reference

### `subscribe_sparql(subscription_id, query, graph_iri)`

```sql
pg_ripple.subscribe_sparql(
    subscription_id TEXT,
    query           TEXT,
    graph_iri       TEXT DEFAULT NULL
) RETURNS VOID
```

Registers a subscription.  Raises an error if a subscription with the same ID
already exists.

- `subscription_id` — unique identifier; used in the channel name
  `pg_ripple_subscription_<id>`.
- `query` — SPARQL SELECT query to re-evaluate on change.
- `graph_iri` — if set, the subscription fires only when this named graph is
  written or deleted.  `NULL` fires for any graph write.

### `unsubscribe_sparql(subscription_id)`

```sql
pg_ripple.unsubscribe_sparql(subscription_id TEXT) RETURNS VOID
```

Removes the subscription.  Silently succeeds if the ID does not exist.

---

## HTTP SSE endpoint

`GET /subscribe/:subscription_id`

| Header | Value |
|---|---|
| `Authorization` | `Bearer <token>` (when auth is configured) |
| `Accept` | `text/event-stream` |

**Event format**:

```
event: sparql_result
id: <notification_id>
data: {"bindings": [...]}

event: keepalive
data: {}
```

A `keepalive` event is sent every 15 seconds to keep the TCP connection alive
through proxies and load balancers.

---

## Limitations

- **Payload size**: `pg_notify` has an 8 KB payload limit. When the updated
  result set exceeds this limit, pg_ripple sends a `{"changed": true}` signal
  instead of the full result. The client must then re-query to obtain the new
  result.
- **At-least-once delivery**: SSE is a fire-and-forget protocol. If the client
  disconnects and reconnects, it will receive the next notification but may miss
  intermediate ones.
- **Prototype**: This is an experimental implementation. The subscription
  mechanism is synchronous in the mutation path; very high write rates on a
  subscribed graph may add latency.

---

## Implementation notes

Subscriptions are stored in `_pg_ripple.sparql_subscriptions`:

```sql
SELECT * FROM _pg_ripple.sparql_subscriptions;
-- subscription_id | query | graph_iri | created_at
```

The mutation journal (`src/storage/mutation_journal.rs`) calls
`crate::subscriptions::notify_affected_subscriptions()` after flushing CWB
hooks.  This function queries the subscriptions catalog, re-executes the SPARQL
query for any matching subscription, and calls `pg_notify`.

---

## See also

- [CDC Subscriptions](cdc-subscriptions.md) — pg-trickle-based change data
  capture for CDC relay patterns.
- [Live Views](live-views-and-subscriptions.md) — SPARQL CONSTRUCT live views
  that automatically refresh derived graphs.
