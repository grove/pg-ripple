-- Migration 0.6.0 → 0.7.0: SHACL Core Validation + Deduplication
--
-- New features in v0.7.0:
--   - SHACL Core validation engine (load_shacl, validate, list_shapes, drop_shape)
--   - Synchronous validation mode (pg_ripple.shacl_mode = 'sync')
--   - Explicit deduplication functions (deduplicate_predicate, deduplicate_all)
--   - Merge-time deduplication GUC (pg_ripple.dedup_on_merge)
--
-- Schema changes:
--   - New table _pg_ripple.shacl_shapes  — SHACL shape catalog
--   - New table _pg_ripple.validation_queue — async validation inbox
--   - New table _pg_ripple.dead_letter_queue — async validation violations
--
-- No existing tables are altered; all new tables and indexes are created
-- only if they do not already exist (idempotent).

-- SHACL shapes catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.shacl_shapes (
    shape_iri  TEXT        NOT NULL PRIMARY KEY,
    shape_json JSONB       NOT NULL,
    active     BOOLEAN     NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_shacl_shapes_active
    ON _pg_ripple.shacl_shapes (active);

-- Async validation queue (populated when shacl_mode = 'async')
CREATE TABLE IF NOT EXISTS _pg_ripple.validation_queue (
    id         BIGSERIAL   PRIMARY KEY,
    s_id       BIGINT      NOT NULL,
    p_id       BIGINT      NOT NULL,
    o_id       BIGINT      NOT NULL,
    g_id       BIGINT      NOT NULL DEFAULT 0,
    stmt_id    BIGINT      NOT NULL,
    queued_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_validation_queue_queued
    ON _pg_ripple.validation_queue (queued_at);

-- Dead-letter queue for async SHACL violations
CREATE TABLE IF NOT EXISTS _pg_ripple.dead_letter_queue (
    id            BIGSERIAL   PRIMARY KEY,
    s_id          BIGINT      NOT NULL,
    p_id          BIGINT      NOT NULL,
    o_id          BIGINT      NOT NULL,
    g_id          BIGINT      NOT NULL DEFAULT 0,
    stmt_id       BIGINT      NOT NULL,
    violation     JSONB       NOT NULL,
    detected_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dead_letter_shape
    ON _pg_ripple.dead_letter_queue ((violation->>'shapeIRI'));
