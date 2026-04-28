-- Migration 0.67.0 → 0.68.0
--
-- v0.68.0: Distributed scalability, streaming completion, and fuzz hardening.
--
-- Schema changes:
--   - PROMO-01: Add promotion_status column to _pg_ripple.predicates to track
--     nonblocking VP promotion state ('promoting' / 'promoted' / NULL).
--     This column is used by the crash recovery path in _PG_init and by the
--     promote_predicate() function to set progress markers.
--
-- New GUCs (registered by the updated Rust binary, no SQL needed):
--   pg_ripple.approx_distinct (BOOL, default off) — CITUS-HLL-01
--     Route SPARQL COUNT(DISTINCT …) through Citus HLL when available.
--   pg_ripple.citus_service_pruning (BOOL, default off) — CITUS-SVC-01
--     Rewrite SERVICE subqueries for Citus workers to add shard annotations.
--   pg_ripple.vp_promotion_batch_size (INT, 1–1000000, default 10000) — PROMO-01
--     Batch size for the nonblocking VP promotion copy phase.
--
-- Fuzz CI (FUZZ-01):
--   Added .github/workflows/fuzz.yml with nightly schedule for all 12 targets.
--
-- CONSTRUCT cursor streaming (STREAM-01):
--   sparql_cursor_turtle() and sparql_cursor_jsonld() now use ConstructCursorIter
--   — a portal-based lazy iterator that applies the CONSTRUCT template per page
--   without materializing the full result set.

-- Add promotion_status column for nonblocking VP promotion tracking.
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS promotion_status TEXT;

-- Stamp the schema_version so diagnostic_report() reflects the upgrade.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.68.0', '0.67.0', clock_timestamp());
