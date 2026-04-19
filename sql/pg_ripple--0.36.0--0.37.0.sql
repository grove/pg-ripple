-- Migration 0.36.0 → 0.37.0: Storage Concurrency Hardening & Error Safety
-- Schema changes:
--   - CREATE INDEX vp_rare_os_idx ON _pg_ripple.vp_rare (o, s)
--   - CREATE TABLE _pg_ripple.schema_version
--   - New GUCs: tombstone_gc_enabled, tombstone_gc_threshold
-- Data-rewrite cost: Low (index build only; no VP table data changes)
-- Downgrade: Remove the (o, s) index and drop schema_version table; no data loss.

-- Add reverse (o, s) index to vp_rare for object-leading pattern performance
CREATE INDEX IF NOT EXISTS vp_rare_os_idx
    ON _pg_ripple.vp_rare (o, s);

-- Schema version tracking
CREATE TABLE IF NOT EXISTS _pg_ripple.schema_version (
    version       TEXT        NOT NULL,
    installed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    upgraded_from TEXT
);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.37.0', '0.36.0')
ON CONFLICT DO NOTHING;
