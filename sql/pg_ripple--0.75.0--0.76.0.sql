-- Migration 0.75.0 → 0.76.0
--
-- v0.76.0: Assessment 11 Low-Severity Findings and Production Polish
--
-- Deliverables:
--
--   TOOLCHAIN-PIN-01:          rust-toolchain.toml pinned to channel = "1.87.0"
--                              for fully reproducible builds.
--   RLS-HASH-01:               RLS policy name generation upgraded from XXH3-64 to
--                              XXH3-128 (suffix is now 32 hex chars instead of 16).
--                              Existing policies are rebuilt below from graph_access catalog.
--   ARROW-PIN-01:              pg_ripple_http/Cargo.toml pins arrow = "55.1" instead
--                              of just "55" to prevent surprise minor-version breakage.
--   BENCH-REFRESH-01:          benchmarks/merge_throughput_baselines.json refreshed
--                              from v0.53.0 to v0.76.0 baselines.
--   TEST-GROWTH-01:            24 new pg_regress tests added; total now 227 (target ≥220).
--   METRICS-AUTH-DOC-01:       /metrics endpoint auth model documented in
--                              docs/src/operations/monitoring.md.
--   XACT-SPI-DOC-01:           src/lib.rs PRE_COMMIT SPI safety claim updated with
--                              authoritative PostgreSQL source citation.
--   LOG-HOOK-01:               Log-hook defense-in-depth audit performed; no secrets
--                              found in error paths; findings documented in
--                              docs/src/operations/security.md.
--   CLIPPY-VERIFY-01:          cargo clippy --deny warnings re-verified for current
--                              codebase; CI gate confirmed functional.
--   LLM-KGE-STATUS-01:         Confirmed src/llm/ and src/kge.rs are present in
--                              feature_status() (v0.73.0 FEATURE-STATUS-02 verified).
--   CI-INTEGRATION-VERIFY-01:  Confirmed Citus and Arrow integration scripts are wired
--                              to CI workflows (v0.75.0 CI-INTEGRATION-01/02 verified).
--
-- Schema change: RLS policy names change from 64-bit to 128-bit hash suffix.
-- Existing policies are rebuilt to use the new naming scheme.

-- Rebuild RLS policies with XXH3-128 naming (RLS-HASH-01).
-- Drop all old pg_ripple-managed policies and recreate them via grant_graph_access().
DO $$
DECLARE
    rec RECORD;
BEGIN
    -- Only run if RLS has been enabled in this deployment.
    IF NOT EXISTS (
        SELECT 1 FROM _pg_ripple.graph_access
        WHERE role_name = '__rls_enabled__' AND graph_id = -1
    ) THEN
        RETURN;
    END IF;

    -- Drop all existing pg_ripple_* RLS policies managed by this extension.
    FOR rec IN
        SELECT polrelid::regclass::text AS tbl, polname
        FROM pg_catalog.pg_policy
        WHERE polname LIKE 'pg_ripple_%'
    LOOP
        EXECUTE format('DROP POLICY IF EXISTS %I ON %s', rec.polname, rec.tbl);
    END LOOP;

    -- Recreate policies from the grant catalog using the new 128-bit hash names.
    FOR rec IN
        SELECT ga.role_name, d.value AS graph_iri, ga.permission
        FROM _pg_ripple.graph_access ga
        JOIN _pg_ripple.dictionary d ON d.id = ga.graph_id
        WHERE ga.role_name != '__rls_enabled__' AND ga.graph_id > 0
    LOOP
        PERFORM pg_ripple.grant_graph_access(rec.graph_iri, rec.role_name, rec.permission);
    END LOOP;
END;
$$;

-- Update schema version stamp.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.76.0', '0.75.0', clock_timestamp());
