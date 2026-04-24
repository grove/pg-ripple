-- Migration 0.53.0 → 0.54.0: High Availability & Logical Replication
-- See CHANGELOG.md for the full feature list.
--
-- Schema changes:
--   • _pg_ripple.replication_status — tracks pending N-Triples batches
--     delivered by the logical replication slot; used by the
--     logical_apply_worker background worker when
--     pg_ripple.replication_enabled = on.
--
-- New GUCs (no SQL DDL required; compiled from Rust):
--   • pg_ripple.replication_enabled (bool, default off)
--   • pg_ripple.replication_conflict_strategy (text, default 'last_writer_wins')
--
-- New SQL functions (compiled from Rust):
--   • pg_ripple.replication_stats() → TABLE(slot_name, lag_bytes,
--       last_applied_lsn, last_applied_at)
--
-- New infrastructure (no SQL DDL required):
--   • docker/Dockerfile.batteries — batteries-included Docker image
--   • docker/Dockerfile.cnpg     — CloudNativePG extension volume image
--   • charts/pg_ripple/          — Kubernetes Helm chart
--   • examples/cloudnativepg_cluster.yaml — CNP Cluster manifest example
--   • benchmarks/vector_index_compare.sql — HNSW vs IVFFlat benchmark

CREATE TABLE IF NOT EXISTS _pg_ripple.replication_status (
    id           BIGSERIAL    NOT NULL PRIMARY KEY,
    slot_name    TEXT         NOT NULL DEFAULT 'pg_ripple_sub',
    batch_data   TEXT         NOT NULL DEFAULT '',
    received_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_replication_status_unprocessed
    ON _pg_ripple.replication_status (id)
    WHERE processed_at IS NULL;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.54.0', '0.53.0', clock_timestamp());
