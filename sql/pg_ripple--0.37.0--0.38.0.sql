-- Migration 0.37.0 → 0.38.0: Architecture Refactoring & Query Completeness
-- Schema changes:
--   - CREATE TABLE _pg_ripple.shape_hints
--   - New GUC: predicate_cache_enabled
-- Data-rewrite cost: Low (new catalog table only; no VP table data changes)
-- Downgrade: Drop shape_hints table; no data loss.

-- SHACL-to-SPARQL planner hints catalog
-- Populated automatically when shapes are loaded via pg_ripple.load_shapes()
CREATE TABLE IF NOT EXISTS _pg_ripple.shape_hints (
    predicate_id  BIGINT  NOT NULL,
    hint_type     TEXT    NOT NULL,  -- 'max_count_1' | 'min_count_1'
    shape_iri_id  BIGINT  NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (predicate_id, hint_type)
);

CREATE INDEX IF NOT EXISTS shape_hints_pred_idx
    ON _pg_ripple.shape_hints (predicate_id);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.38.0', '0.37.0')
ON CONFLICT DO NOTHING;
