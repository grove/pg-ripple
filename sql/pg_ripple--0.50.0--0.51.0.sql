-- Migration 0.50.0 → 0.51.0: Security Hardening & Production Readiness
-- ─────────────────────────────────────────────────────────────────────────────
--
-- New SQL-visible features in v0.51.0:
--   * pg_ripple.sparql_max_algebra_depth GUC  (SPARQL DoS protection)
--   * pg_ripple.sparql_max_triple_patterns GUC
--   * pg_ripple.tracing_otlp_endpoint GUC     (OTLP endpoint override)
--   * pg_ripple.predicate_workload_stats()    (per-predicate workload SRF)
--   * pg_ripple.sparql_csv()                  (W3C CSV output)
--   * pg_ripple.sparql_tsv()                  (W3C TSV output)
--
-- Schema changes:
--   * CREATE TABLE _pg_ripple.predicate_stats
--     (backing table for predicate_workload_stats())

-- ── _pg_ripple.predicate_stats ────────────────────────────────────────────────
-- Per-predicate workload counters.  Populated by the query planner and merge
-- worker; read via pg_ripple.predicate_workload_stats().

CREATE TABLE IF NOT EXISTS _pg_ripple.predicate_stats (
    predicate_id BIGINT  PRIMARY KEY,
    query_count  BIGINT  NOT NULL DEFAULT 0,
    merge_count  BIGINT  NOT NULL DEFAULT 0,
    last_merged  TIMESTAMPTZ
);

-- ── Deprecation notice ────────────────────────────────────────────────────────
-- GUC pg_ripple.property_path_max_depth is superseded by pg_ripple.max_path_depth
-- introduced in v0.42.0.  The old GUC still works but will be removed in v1.0.0.
-- Users relying on it should migrate to:
--   SET pg_ripple.max_path_depth = <value>;

-- ── Schema version ────────────────────────────────────────────────────────────
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.51.0', '0.50.0', now())
ON CONFLICT DO NOTHING;
