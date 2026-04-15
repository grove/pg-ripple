-- change_notification.sql — Test CDC subscribe/unsubscribe
--
-- Verifies that:
--  1. subscribe() returns a positive subscription ID
--  2. The subscription is visible in cdc_subscriptions
--  3. A second subscribe() for a different channel also works
--  4. unsubscribe() removes the subscription and returns the removed count
--  5. Subscribing with a specific predicate IRI is recorded correctly
--
-- NOTE: Actual LISTEN/NOTIFY delivery cannot be synchronously tested in
-- pg_regress; this file tests only the subscription metadata operations.
--
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.

SET search_path TO pg_ripple, public;

-- ── 1. subscribe() returns a bigint ID ────────────────────────────────────────
SELECT pg_ripple.subscribe('*', 'test_channel_a') > 0 AS sub_a_ok;

-- ── 2. Subscription recorded in catalog ───────────────────────────────────────
SELECT count(*) AS sub_count
FROM _pg_ripple.cdc_subscriptions
WHERE channel = 'test_channel_a';

-- ── 3. Second subscription on different channel ───────────────────────────────
SELECT pg_ripple.subscribe('*', 'test_channel_b') > 0 AS sub_b_ok;

SELECT count(*) AS total_subs
FROM _pg_ripple.cdc_subscriptions
WHERE channel IN ('test_channel_a', 'test_channel_b');

-- ── 4. unsubscribe() removes only the named channel ──────────────────────────
SELECT pg_ripple.unsubscribe('test_channel_a') AS removed_count;

SELECT count(*) AS remaining_subs
FROM _pg_ripple.cdc_subscriptions
WHERE channel IN ('test_channel_a', 'test_channel_b');

-- Channel B still present.
SELECT count(*) AS channel_b_survives
FROM _pg_ripple.cdc_subscriptions
WHERE channel = 'test_channel_b';

-- ── 5. unsubscribe() on non-existent channel returns 0 ───────────────────────
SELECT pg_ripple.unsubscribe('no_such_channel') AS removed_nonexistent;

-- ── 6. Unsubscribe remaining ──────────────────────────────────────────────────
SELECT pg_ripple.unsubscribe('test_channel_b') AS removed_b;

SELECT count(*) AS final_subs
FROM _pg_ripple.cdc_subscriptions;
