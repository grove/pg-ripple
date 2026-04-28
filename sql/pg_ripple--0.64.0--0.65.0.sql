-- Migration 0.64.0 → 0.65.0: CONSTRUCT Writeback Correctness Closure
--
-- Changes in this release:
--   - CWB-FIX-01: Delta maintenance kernel for derived triples
--   - CWB-FIX-02: Source graph write hooks — insert_triple/delete_triple trigger
--                 incremental CONSTRUCT rule maintenance in the same transaction
--   - CWB-FIX-03: HTAP-aware promoted-predicate retraction (delta + tombstone paths)
--   - CWB-FIX-04: Exact provenance capture via INSERT...RETURNING CTEs
--   - CWB-FIX-05: Parameterized SPI catalog writes; mode validation
--   - CWB-FIX-06: Shared-target reference-count semantics (existing)
--   - CWB-FIX-07: Observability columns added to _pg_ripple.construct_rules
--   - CWB-FIX-08: Full CWB behavior test matrix in construct_rules.sql
--   - CWB-FIX-09: SHACL rule bridge foundation (feature_status updated)
--   - CWB-FIX-10: construct_pipeline_status() introspection API
--
-- New SQL functions:
--   - pg_ripple.construct_pipeline_status() → JSONB
--   - pg_ripple.apply_construct_rules_for_graph(graph_iri TEXT) → BIGINT
--
-- Schema changes:
--   ALTER TABLE _pg_ripple.construct_rules ADD COLUMN last_incremental_run TIMESTAMPTZ;
--   ALTER TABLE _pg_ripple.construct_rules ADD COLUMN successful_run_count BIGINT DEFAULT 0;
--   ALTER TABLE _pg_ripple.construct_rules ADD COLUMN failed_run_count BIGINT DEFAULT 0;
--   ALTER TABLE _pg_ripple.construct_rules ADD COLUMN last_error TEXT;
--   ALTER TABLE _pg_ripple.construct_rules ADD COLUMN derived_triple_count BIGINT DEFAULT 0;

ALTER TABLE IF EXISTS _pg_ripple.construct_rules
    ADD COLUMN IF NOT EXISTS last_incremental_run  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS successful_run_count  BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS failed_run_count      BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_error            TEXT,
    ADD COLUMN IF NOT EXISTS derived_triple_count  BIGINT NOT NULL DEFAULT 0;
