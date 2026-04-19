-- telemetry.sql — Tracing GUC toggle tests (v0.40.0)
--
-- Verifies that the tracing-related GUCs exist and can be toggled without error.

SET search_path TO pg_ripple, public;

CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library explicitly before SHOW (needed so GUCs are registered).
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

-- ── 1. GUC existence ──────────────────────────────────────────────────────────

-- 1a. tracing_enabled defaults to off.
SHOW pg_ripple.tracing_enabled;

-- 1b. tracing_exporter GUC exists.
SELECT count(*) = 1 AS tracing_exporter_guc_exists
FROM pg_settings WHERE name = 'pg_ripple.tracing_exporter';

-- ── 2. Toggle tracing_enabled ─────────────────────────────────────────────────

SET pg_ripple.tracing_enabled = on;
SHOW pg_ripple.tracing_enabled;

SET pg_ripple.tracing_enabled = off;
SHOW pg_ripple.tracing_enabled;

-- ── 3. Set tracing_exporter ───────────────────────────────────────────────────

SET pg_ripple.tracing_exporter = 'stdout';
SELECT current_setting('pg_ripple.tracing_exporter') = 'stdout' AS tracing_exporter_set;

-- ── 4. SPARQL query executes with tracing on (zero-overhead check) ────────────

SET pg_ripple.tracing_enabled = on;
SELECT pg_ripple.triple_count() >= 0 AS query_with_tracing_ok;
SET pg_ripple.tracing_enabled = off;
SELECT pg_ripple.triple_count() >= 0 AS query_with_tracing_off;

-- ── 5. datalog_max_derived GUC exists ─────────────────────────────────────────

SHOW pg_ripple.datalog_max_derived;
SET pg_ripple.datalog_max_derived = 1000000;
SHOW pg_ripple.datalog_max_derived;
SET pg_ripple.datalog_max_derived = 0;
SHOW pg_ripple.datalog_max_derived;
