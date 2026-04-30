-- BIDIOPS-PERF-01: Bidirectional Integration Operations performance benchmark (v0.78.0).
--
-- Measures the cost of:
--   1. Queue depth check (pg_class.reltuples estimation).
--   2. event_audit insert per side-band mutating call (target: ≤ 0.2 ms).
--   3. Per-request scope check (cached path, target: ≤ 0.05 ms).
--   4. Frame redaction render at bridge-write time (≤ 0.5 ms per 10-predicate frame).
--   5. bidi_status() over 16 subscriptions (target: ≤ 50 ms).
--
-- Usage:
--   pgbench -n -f benchmarks/bidiops_throughput.sql -c 4 -j 2 -T 60 <dbname>
--
-- Note: All timings are wall-clock estimates; actual CI gate uses ci_benchmark.sh.

\set sub_name 'perf_test_sub'
\set eid gen_random_uuid()

-- Ensure the subscription exists.
SELECT pg_ripple.create_subscription(:'sub_name') WHERE NOT EXISTS (
    SELECT 1 FROM _pg_ripple.subscriptions WHERE name = :'sub_name'
);

-- Bench 1: Queue depth estimate (non-blocking reltuples check).
SELECT reltuples::bigint AS estimated_depth
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = '_pg_ripple'
  AND c.relname = 'event_dead_letters';

-- Bench 2: event_audit insert (one row per side-band call).
INSERT INTO _pg_ripple.event_audit
    (event_id, subscription_name, resource_type, action, actor_session)
VALUES
    (gen_random_uuid(), :'sub_name', 'event', 'linkback', session_user);

-- Bench 3: Scope check (index lookup on subscription_tokens).
SELECT COUNT(*) AS matching_tokens
FROM _pg_ripple.subscription_tokens
WHERE subscription_name = :'sub_name'
  AND revoked_at IS NULL
  AND 'linkback' = ANY(scopes);

-- Bench 4: Frame redaction render (simulated: count predicates with @redact).
SELECT COUNT(*) AS redacted_predicates
FROM (
    SELECT jsonb_object_keys(
        '{"ex:name": {}, "ex:phone": {"@redact": true}, "ex:email": {},
          "ex:taxId": {"@redact": true}, "ex:ssn": {"@redact": true},
          "ex:dob": {}, "ex:address": {}, "ex:city": {}, "ex:state": {},
          "ex:zip": {}}'::jsonb
    ) AS key
) t
JOIN jsonb_each(
    '{"ex:name": {}, "ex:phone": {"@redact": true}, "ex:email": {},
      "ex:taxId": {"@redact": true}, "ex:ssn": {"@redact": true},
      "ex:dob": {}, "ex:address": {}, "ex:city": {}, "ex:state": {},
      "ex:zip": {}}'::jsonb
) e ON e.key = t.key
WHERE (e.value->>'@redact')::boolean IS TRUE;

-- Bench 5: bidi_status() (fast path — subscriptions table scan).
SELECT COUNT(*) AS status_rows FROM pg_ripple.bidi_status();
