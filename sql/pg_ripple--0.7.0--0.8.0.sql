-- Migration 0.7.0 → 0.8.0: SHACL Advanced
--
-- New Rust-compiled SQL functions (no schema changes required for async pipeline):
--
--   pg_ripple.process_validation_queue(batch_size BIGINT DEFAULT 1000) → BIGINT
--       Manually drain up to batch_size items from _pg_ripple.validation_queue.
--       Violations are moved to _pg_ripple.dead_letter_queue.
--
--   pg_ripple.validation_queue_length() → BIGINT
--       Return count of pending items in the async validation queue.
--
--   pg_ripple.dead_letter_count() → BIGINT
--       Return count of violation entries in _pg_ripple.dead_letter_queue.
--
--   pg_ripple.dead_letter_queue() → JSONB
--       Return all dead-letter entries as a JSON array.
--
--   pg_ripple.drain_dead_letter_queue() → BIGINT
--       Delete all dead-letter entries; returns count deleted.
--
-- SHACL constraint engine extended with:
--   - sh:node   — value nodes validated against a nested shape
--   - sh:or     — logical OR over a list of shapes
--   - sh:and    — logical AND over a list of shapes
--   - sh:not    — logical NOT of a shape
--   - sh:qualifiedValueShape with sh:qualifiedMinCount / sh:qualifiedMaxCount
--
-- Background worker (merge worker) now also processes the async validation
-- queue when pg_ripple.shacl_mode = 'async'.
--
-- The _pg_ripple.validation_queue and _pg_ripple.dead_letter_queue tables
-- were already created in v0.7.0; no DDL changes are required for those.
--
-- ─── pg_trickle multi-shape DAG validation (v0.8.0) ─────────────────────────
--
-- New catalog table for DAG monitor metadata:

CREATE TABLE IF NOT EXISTS _pg_ripple.shacl_dag_monitors (
    shape_iri          TEXT        NOT NULL PRIMARY KEY,
    stream_table_name  TEXT        NOT NULL,
    constraint_summary TEXT        NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- New SQL functions for pg_trickle DAG validation
-- (optional at runtime — pg_trickle does not need to be installed):
--
--   pg_ripple.enable_shacl_dag_monitors() → BIGINT
--       Compile all active, compilable SHACL shapes into per-shape pg_trickle
--       stream tables.  Supported constraint types: sh:minCount, sh:maxCount,
--       sh:datatype, sh:class.  Creates violation_summary_dag as the DAG leaf.
--       Returns count of per-shape stream tables created.
--       Returns 0 (with a warning) when pg_trickle is not installed.
--
--   pg_ripple.disable_shacl_dag_monitors() → BIGINT
--       Drop all per-shape stream tables and violation_summary_dag.
--       Returns count of tables dropped.
--
--   pg_ripple.list_shacl_dag_monitors()
--         → TABLE(shape_iri TEXT, stream_table TEXT, constraints TEXT)
--       List all active DAG monitor stream tables and their compiled constraints.
