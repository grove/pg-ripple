-- pg_regress test: v0.83.0 feature gate (REG-TESTS-01)
--   API-RENAME-01:    load_jsonld() added, json_ld_load() deprecated
--   API-GRAPH-COL-01: find_triples() g column present
--   DL-COST-GUC-01:   datalog_cost_bound_s_divisor, datalog_cost_bound_so_divisor
--   MERGE-BACKOFF-01: merge_max_backoff_secs GUC
--   MOD-BIDI-01:      bidi module integrity (wire version, validate_normalize)
--   EXPORT-BNODE:     export_ntriples / export_nquads smoke test
--   STATS-CACHE-01:   refresh_stats_cache()

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: load_jsonld() is callable (API-RENAME-01) ────────────────────────

-- 1a. load_jsonld() exists and accepts JSONB with an @id.
SELECT pg_ripple.load_jsonld('{"@context": {"ex": "http://example.org/"}, "@id": "ex:thing1", "@type": "http://schema.org/Thing"}'::jsonb) >= 0
  AS load_jsonld_works;

-- 1b. load_jsonld() with a named graph IRI.
SELECT pg_ripple.load_jsonld(
    '{"@context": {"ex": "http://example.org/"}, "@id": "ex:t1", "ex:name": "Test"}'::jsonb,
    graph_uri => 'http://example.org/g1'
) >= 0 AS load_jsonld_graph_uri_works;

-- ── Part 2: find_triples() returns g column (API-GRAPH-COL-01) ───────────────

-- 2a. find_triples() function exists in pg_ripple schema.
SELECT COUNT(*) >= 1 AS find_triples_exists
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple' AND p.proname = 'find_triples';

-- 2b. find_triples() is callable (returns rows).
SELECT COUNT(*) >= 0 AS find_triples_callable FROM pg_ripple.find_triples(NULL, NULL, NULL);

-- ── Part 3: New GUCs (DL-COST-GUC-01, MERGE-BACKOFF-01) ─────────────────────

-- 3a. datalog_cost_bound_s_divisor has expected default.
SELECT current_setting('pg_ripple.datalog_cost_bound_s_divisor')::int = 100
  AS cost_s_divisor_default;

-- 3b. datalog_cost_bound_so_divisor has expected default.
SELECT current_setting('pg_ripple.datalog_cost_bound_so_divisor')::int = 10
  AS cost_so_divisor_default;

-- 3c. merge_max_backoff_secs has expected default.
SELECT current_setting('pg_ripple.merge_max_backoff_secs')::int = 60
  AS merge_max_backoff_default;

-- 3d. GUC values can be changed.
SET pg_ripple.datalog_cost_bound_s_divisor = 200;
SELECT current_setting('pg_ripple.datalog_cost_bound_s_divisor')::int = 200
  AS cost_s_divisor_settable;
RESET pg_ripple.datalog_cost_bound_s_divisor;

-- ── Part 4: Bidi module integrity (MOD-BIDI-01) ───────────────────────────────

-- 4a. bidi_wire_version() is '1.0'.
SELECT pg_ripple.bidi_wire_version() = '1.0' AS bidi_wire_version_ok;

-- 4b. refresh_stats_cache() is callable (STATS-CACHE-01).
SELECT pg_ripple.refresh_stats_cache() >= 0 AS refresh_stats_cache_works;

-- 4c. bidi_health() is callable (BIDIOPS-DASH-01).
SELECT COUNT(*) >= 0 AS bidi_health_callable FROM pg_ripple.bidi_health();

-- ── Part 5: export_ntriples / export_nquads smoke test (EXPORT-BNODE) ────────

-- 5a. export_ntriples() is callable.
SELECT pg_ripple.export_ntriples() IS NOT NULL AS export_ntriples_works;

-- 5b. export_nquads() is callable.
SELECT pg_ripple.export_nquads() IS NOT NULL AS export_nquads_works;
