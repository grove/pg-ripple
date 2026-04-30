-- BIDI-PERF-01: Bidirectional relay throughput benchmark
--
-- Performance budget:
--   - Conflict-policied writes ≤ 2× baseline ingest throughput
--   - Outbound rewrite ≤ 5 ms median per emitted event
--
-- Usage (requires pg_ripple installed and a pgbench-compatible connection):
--   pgbench -f benchmarks/bidi_relay_throughput.sql -c 4 -j 4 -T 30 <dbname>
--
-- Baseline: standard ingest without conflict policy
--   pgbench -f benchmarks/insert_throughput.sql -c 4 -j 4 -T 30 <dbname>

\set subject_base 'https://crm.example.com/contacts/'
\set subject_id random(1, 100000)
\set graph_iri '<urn:source:crm>'
\set pred_name 'http://schema.org/name'
\set pred_email 'http://schema.org/email'
\set value_suffix random(1, 999)

BEGIN;

-- Upsert mode: simulate CRM contact update with conflict policy
SELECT pg_ripple.ingest_json(
    json_build_object(
        'ex:name',  'Contact_' || :value_suffix,
        'ex:email', 'contact_' || :value_suffix || '@crm.example.com'
    )::jsonb,
    :subject_base || :subject_id,
    'bidi_bench_crm',
    mode     => 'upsert',
    graph_iri => :graph_iri
);

COMMIT;
