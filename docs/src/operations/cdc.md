# Change Data Capture (CDC) with pg_ripple

pg_ripple integrates with PostgreSQL's logical replication infrastructure to provide real-time notifications when triples are inserted, updated, or deleted. This feature is called Change Data Capture (CDC).

## Overview

CDC subscriptions allow applications to react immediately when the RDF graph changes — for example, to invalidate caches, trigger downstream processing, or stream updates to external systems.

pg_ripple CDC builds on:
- PostgreSQL logical replication slots (`pg_logical_slot_get_changes`)
- The `_pg_ripple.cdc_queue` table (populated by triggers on VP delta tables)
- The `pg_ripple.cdc_subscribe()` / `pg_ripple.cdc_poll()` API

## Configuration

```sql
-- Enable CDC (creates the queue table and triggers if not already present)
SELECT pg_ripple.cdc_enable();

-- Subscribe to a specific predicate IRI (NULL = all predicates)
SELECT pg_ripple.cdc_subscribe(
    subscriber_id := 'my-app',
    predicate_iri := 'http://schema.org/name'
);
```

## Polling for changes

```sql
-- Poll up to 100 changes for subscriber 'my-app'
SELECT * FROM pg_ripple.cdc_poll('my-app', max_events := 100);
```

Returns rows of `(event_type TEXT, s TEXT, p TEXT, o TEXT, graph_iri TEXT, event_time TIMESTAMPTZ)`.

## Tuning the CDC queue

| Parameter | Default | Description |
|-----------|---------|-------------|
| `pg_ripple.cdc_queue_max_age` | `'7 days'` | Rows older than this are auto-purged |
| `pg_ripple.cdc_batch_size` | `1000` | Maximum events processed per vacuum cycle |
| `pg_ripple.cdc_slow_subscriber_timeout` | `'1 hour'` | Disconnect subscriber if it hasn't polled in this window |

```sql
-- Increase queue retention to 30 days
ALTER SYSTEM SET pg_ripple.cdc_queue_max_age = '30 days';
SELECT pg_reload_conf();
```

## Handling a slow subscriber

If a subscriber is slow to poll, the CDC queue can grow unboundedly. pg_ripple logs a warning when a subscriber exceeds `cdc_slow_subscriber_timeout`:

```
WARNING: CDC subscriber 'my-app' has not polled in 2h 15m (threshold: 1h);
         consider increasing poll frequency or raising cdc_slow_subscriber_timeout
```

To force-disconnect a stale subscriber:

```sql
SELECT pg_ripple.cdc_unsubscribe('my-app');
```

## Monitoring queue depth

```sql
SELECT
    subscriber_id,
    count(*) AS pending_events,
    min(event_time) AS oldest_event,
    max(event_time) AS newest_event
FROM _pg_ripple.cdc_queue
GROUP BY subscriber_id
ORDER BY pending_events DESC;
```

## Example: react to new triples via LISTEN/NOTIFY

pg_ripple can optionally send a PostgreSQL `NOTIFY` on the `pg_ripple_cdc` channel whenever a CDC event is enqueued:

```sql
SELECT pg_ripple.cdc_enable_notify();

-- In your application:
LISTEN pg_ripple_cdc;
-- When NOTIFY arrives, call cdc_poll() to retrieve the actual events.
```

## Limitations

- CDC captures DML on VP delta tables; main-table rows added by the merge worker are not re-captured (they were captured at insert time via the delta).
- `pg_dump` does not include CDC queue contents; restore starts with an empty queue.
- Logical replication slots must be managed separately if using `pg_logical` directly.
