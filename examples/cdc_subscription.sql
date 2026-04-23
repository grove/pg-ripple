-- examples/cdc_subscription.sql
-- Change Data Capture (CDC) subscription example for pg_ripple (v0.51.0)
--
-- This example shows how to subscribe to real-time triple changes and
-- process them in a downstream application.
--
-- Prerequisites:
--   CREATE EXTENSION pg_ripple CASCADE;

-- ── Step 1: Enable CDC ────────────────────────────────────────────────────────

-- Enable CDC tracking (creates queue table and VP table triggers).
SELECT pg_ripple.cdc_enable();

-- Optionally enable LISTEN/NOTIFY for push-style polling.
SELECT pg_ripple.cdc_enable_notify();

-- ── Step 2: Subscribe to specific predicates ─────────────────────────────────

-- Subscribe to all changes on the schema:name predicate.
SELECT pg_ripple.cdc_subscribe(
    subscriber_id := 'name-indexer',
    predicate_iri := 'http://schema.org/name'
);

-- Subscribe to all changes (NULL predicate = catch-all).
SELECT pg_ripple.cdc_subscribe(
    subscriber_id := 'audit-log',
    predicate_iri := NULL
);

-- ── Step 3: Insert some triples (these will appear in the CDC queue) ──────────

SELECT pg_ripple.insert_triple(
    '<http://example.org/Charlie>',
    '<http://schema.org/name>',
    '"Charlie Brown"'
);

SELECT pg_ripple.insert_triple(
    '<http://example.org/Charlie>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://schema.org/Person>'
);

-- ── Step 4: Poll for changes ──────────────────────────────────────────────────

-- Poll the name-indexer subscriber for up to 50 changes.
SELECT
    event_type,
    s AS subject,
    p AS predicate,
    o AS object,
    event_time
FROM pg_ripple.cdc_poll('name-indexer', max_events := 50)
ORDER BY event_time;

-- Poll the audit-log subscriber (catches all predicates).
SELECT event_type, s, p, o, graph_iri, event_time
FROM pg_ripple.cdc_poll('audit-log', max_events := 100)
ORDER BY event_time;

-- ── Step 5: Monitor queue depth ──────────────────────────────────────────────

-- Check how many events are pending for each subscriber.
SELECT
    subscriber_id,
    count(*)            AS pending_events,
    min(event_time)     AS oldest_event,
    now() - min(event_time) AS queue_lag
FROM _pg_ripple.cdc_queue
GROUP BY subscriber_id
ORDER BY pending_events DESC;

-- ── Step 6: Tuning for high-throughput workloads ──────────────────────────────

-- For high-insert workloads, tune the queue retention and batch size.
ALTER SYSTEM SET pg_ripple.cdc_queue_max_age = '1 day';
ALTER SYSTEM SET pg_ripple.cdc_batch_size = 5000;
SELECT pg_reload_conf();

-- ── Step 7: Unsubscribe when done ────────────────────────────────────────────

-- Remove a subscriber and purge its queue entries.
SELECT pg_ripple.cdc_unsubscribe('name-indexer');
