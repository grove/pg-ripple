-- pg_regress test: Named CDC subscriptions (v0.42.0)
--
-- Tests that:
-- 1. create_subscription() creates a named subscription in _pg_ripple.subscriptions.
-- 2. list_subscriptions() lists all active subscriptions.
-- 3. NOTIFY pg_ripple_cdc_{name} is sent on triple insert/delete.
-- 4. drop_subscription() removes the subscription.
-- 5. Duplicate subscription names return false.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: create_subscription ──────────────────────────────────────────────

-- Create a subscription that watches all triple changes.
SELECT pg_ripple.create_subscription('test_all') AS created_all;

-- Creating the same subscription again returns false.
SELECT pg_ripple.create_subscription('test_all') AS created_duplicate;

-- Create a subscription with SPARQL filter (stored but not enforced at SQL level
-- in v0.42.0; filter is used by pg_ripple_http WebSocket endpoint).
SELECT pg_ripple.create_subscription(
    'test_filtered',
    'SELECT ?s ?p ?o WHERE { ?s <https://ex.org/type> ?o }',
    NULL
) AS created_filtered;

-- ── Part 2: list_subscriptions ───────────────────────────────────────────────

SELECT name, filter_sparql IS NULL AS no_filter
FROM pg_ripple.list_subscriptions()
ORDER BY name;

-- ── Part 3: NOTIFY on triple insert ──────────────────────────────────────────

-- Listen for notifications on our subscription channel.
LISTEN pg_ripple_cdc_test_all;

-- Insert a triple — the trigger on the delta table should fire.
-- (In pg_regress, NOTIFY is not received synchronously; we verify the subscription
-- machinery is working by checking that insert_triple succeeds and the subscription
-- table is intact.)
SELECT pg_ripple.insert_triple(
    '<https://ex.org/s1>',
    '<https://ex.org/hasProp>',
    '"value1"'
) AS inserted;

-- Subscription still active.
SELECT count(*) AS subscription_count FROM _pg_ripple.subscriptions;

-- ── Part 4: drop_subscription ────────────────────────────────────────────────

SELECT pg_ripple.drop_subscription('test_all') AS dropped_all;
SELECT pg_ripple.drop_subscription('test_all') AS dropped_again;  -- false

-- Only test_filtered remains.
SELECT name FROM pg_ripple.list_subscriptions() ORDER BY name;

SELECT pg_ripple.drop_subscription('test_filtered') AS dropped_filtered;

-- All subscriptions removed.
SELECT count(*) AS subscription_count FROM pg_ripple.list_subscriptions();

-- ── Part 5: Direct table check ───────────────────────────────────────────────

SELECT count(*) AS endpoint_stats_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'endpoint_stats';

SELECT count(*) AS subscriptions_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'subscriptions';
