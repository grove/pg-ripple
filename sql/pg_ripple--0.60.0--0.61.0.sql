-- Migration: pg_ripple 0.60.0 → 0.61.0
-- Theme: Ecosystem Depth & Polish
--
-- New features delivered (Rust code changes only unless noted below):
--   - Per-named-graph RLS: grant_graph() / revoke_graph() helper functions
--   - GDPR right-to-erasure: erase_subject() SRF (returns per-relation counts)
--   - Inference explainability: explain_inference() SRF
--   - SHACL-AF sh:rule bridge: compiles sh:TripleRule to Datalog
--   - dbt adapter: clients/dbt-pg-ripple/ (Python package, no SQL changes)
--   - OTLP traceparent propagation: pg_ripple.tracing_traceparent GUC
--   - Federation call stats extended with p50_ms, p95_ms, last_error_at
--   - BRIN summarize failure tracking and NOTICE promotion
--   - Citus: prune_bound_term(), set_graph_shard_affinity(),
--             clear_graph_shard_affinity(), batch_insert_encoded_shard_direct()
--   - HTTP: PT404 JSON envelope for body-size rejection
--   - HTTP: traceparent header propagation end-to-end
--
-- SQL schema changes:
--   1. _pg_ripple.graph_shard_affinity — new table for Citus graph affinity
--   2. _pg_ripple.rule_firing_log      — new table for inference explainability
--   3. _pg_ripple.predicates           — new column brin_summarize_failures

-- ---------------------------------------------------------------------------
-- 1. Citus graph shard affinity table (CITUS-22)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS _pg_ripple.graph_shard_affinity (
    graph_id  BIGINT      NOT NULL PRIMARY KEY,
    shard_id  INT         NOT NULL,
    worker_node TEXT      NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.graph_shard_affinity IS
    'Maps named-graph IDs to their preferred Citus shard / worker node. '
    'Used by the SPARQL planner to restrict GRAPH-scoped queries to a single worker.';

-- ---------------------------------------------------------------------------
-- 2. Rule firing log table (inference explainability, v0.61.0 6.6)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS _pg_ripple.rule_firing_log (
    id          BIGSERIAL   PRIMARY KEY,
    fired_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    rule_id     TEXT        NOT NULL,
    rule_set    TEXT        NOT NULL DEFAULT '',
    output_sid  BIGINT,
    source_sids BIGINT[]    NOT NULL DEFAULT '{}',
    session_pid INT         NOT NULL DEFAULT pg_backend_pid()
);

CREATE INDEX IF NOT EXISTS rule_firing_log_output_sid_idx
    ON _pg_ripple.rule_firing_log (output_sid);

COMMENT ON TABLE _pg_ripple.rule_firing_log IS
    'Records each Datalog rule firing. Used by explain_inference() to reconstruct '
    'the derivation chain for inferred triples.';

-- ---------------------------------------------------------------------------
-- 3. Add brin_summarize_failures column to predicates (F7-3)
-- ---------------------------------------------------------------------------
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS brin_summarize_failures INT NOT NULL DEFAULT 0;

COMMENT ON COLUMN _pg_ripple.predicates.brin_summarize_failures IS
    'Counts consecutive brin_summarize_new_values() failures for this predicate''s '
    'main VP table. Promoted from debug1 to NOTICE after the second consecutive failure.';
