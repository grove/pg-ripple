-- Migration: pg_ripple 0.61.0 → 0.62.0
--
-- v0.62.0 — Query Frontier
--
-- New features in this release:
--
--   • Apache Arrow Flight bulk export: `pg_ripple.export_arrow_flight(graph_iri TEXT)`.
--   • WCOJ planner integration: automatic Leapfrog-Triejoin for cyclic SPARQL BGPs.
--   • Visual graph explorer at `/explorer` in pg_ripple_http.
--   • Citus CITUS-25: `pg_ripple.vacuum_vp_rare()` — remove dead vp_rare entries.
--   • Citus CITUS-26: tiered dictionary via `pg_ripple.dictionary_tier_threshold` GUC.
--     Schema change: `access_count BIGINT NOT NULL DEFAULT 0` column added to
--     `_pg_ripple.dictionary`.
--   • Citus CITUS-27: `pg_ripple.datalog_citus_dispatch` GUC for distributed inference.
--   • Citus CITUS-28: `pg_ripple.citus_live_rebalance()` — non-blocking rebalance.
--   • Citus CITUS-29: multi-hop shard-pruning carry-forward with
--     `pg_ripple.citus_prune_carry_max` GUC.
--   • CI quality: `cargo deny check` + `cargo audit` gates added.
--
-- SQL-visible schema changes:
--
--   1. ADD COLUMN `access_count BIGINT NOT NULL DEFAULT 0` to `_pg_ripple.dictionary`
--      (for tiered dictionary; existing rows initialised to 0).
--
-- Compiled-from-Rust function changes (no separate SQL required):
--   • `pg_ripple.export_arrow_flight(graph_iri TEXT) RETURNS BYTEA` — new.
--   • `pg_ripple.vacuum_vp_rare() RETURNS TABLE(predicate_id BIGINT, rows_removed BIGINT)` — new.
--   • `pg_ripple.citus_live_rebalance() RETURNS TABLE(source_node TEXT, target_node TEXT, shard_id BIGINT, shard_size_bytes BIGINT)` — new.

-- ── CITUS-26: tiered dictionary access_count column ──────────────────────────

-- Add the access_count column if it doesn't already exist (idempotent).
ALTER TABLE _pg_ripple.dictionary
    ADD COLUMN IF NOT EXISTS access_count BIGINT NOT NULL DEFAULT 0;

-- Existing rows are already DEFAULT 0 — no UPDATE needed (avoids table rewrite).
