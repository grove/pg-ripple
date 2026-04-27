-- Migration 0.62.0 → 0.63.0: SPARQL CONSTRUCT writeback rules + Citus scalability
--
-- Schema changes:
--   - _pg_ripple.construct_rules  (CWB-07)  — registered CONSTRUCT writeback rules
--   - _pg_ripple.construct_rule_triples (CWB-11) — per-rule derived-triple provenance
--
-- No schema changes are required for the Citus improvements (CITUS-30–37);
-- those are pure Rust additions to the shared library.

CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rules (
    name            TEXT PRIMARY KEY,
    sparql          TEXT NOT NULL,
    generated_sql   TEXT,
    target_graph    TEXT NOT NULL,
    target_graph_id BIGINT NOT NULL,
    mode            TEXT NOT NULL DEFAULT 'incremental',
    source_graphs   TEXT[],
    rule_order      INT,
    created_at      TIMESTAMPTZ DEFAULT now(),
    last_refreshed  TIMESTAMPTZ
);

COMMENT ON TABLE _pg_ripple.construct_rules IS
    'Registered SPARQL CONSTRUCT writeback rules (v0.63.0+)';

CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rule_triples (
    rule_name TEXT   NOT NULL,
    pred_id   BIGINT NOT NULL,
    s         BIGINT NOT NULL,
    o         BIGINT NOT NULL,
    g         BIGINT NOT NULL,
    PRIMARY KEY (rule_name, pred_id, s, o, g)
);

COMMENT ON TABLE _pg_ripple.construct_rule_triples IS
    'Per-rule provenance for derived triples; enables safe multi-rule shared target graphs (v0.63.0+)';
