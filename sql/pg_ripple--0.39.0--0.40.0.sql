-- Migration 0.39.0 → 0.40.0: Streaming Results, Explain & Observability
-- Schema changes: None (new Rust functions and GUCs only; no VP table schema changes)
-- Data-rewrite cost: None
-- Downgrade: Remove GUC settings from postgresql.conf; no schema rollback needed.
--
-- New functions:
--   pg_ripple.sparql_cursor(text)          RETURNS SETOF JSONB
--   pg_ripple.sparql_cursor_turtle(text)   RETURNS SETOF TEXT
--   pg_ripple.sparql_cursor_jsonld(text)   RETURNS SETOF TEXT
--   pg_ripple.explain_sparql(text, bool)   RETURNS JSONB   [new JSONB overload]
--   pg_ripple.explain_datalog(text)        RETURNS JSONB
--   pg_ripple.cache_stats()                RETURNS JSONB   [replaces plan_cache_stats + dict_cache_stats]
--   pg_ripple.reset_cache_stats()          RETURNS VOID
--
-- New GUCs:
--   pg_ripple.sparql_max_rows          INTEGER  DEFAULT 0  (unlimited)
--   pg_ripple.datalog_max_derived      INTEGER  DEFAULT 0  (unlimited)
--   pg_ripple.export_max_rows          INTEGER  DEFAULT 0  (unlimited)
--   pg_ripple.sparql_overflow_action   TEXT     DEFAULT '' (warn)
--   pg_ripple.tracing_enabled          BOOL     DEFAULT off
--   pg_ripple.tracing_exporter         TEXT     DEFAULT '' (stdout)
--
-- Bug fixes:
--   - OPTIONAL {} inside GRAPH {} now correctly scopes the optional join
--     to the named graph; previously the graph filter was dropped.
--   - Property path expressions (e.g. p+, p*) inside GRAPH {} now correctly
--     filter CTE anchor and recursive steps to the named graph.
--
-- New views:
--   pg_ripple.stat_statements_decoded  (requires pg_stat_statements extension)

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.40.0', '0.39.0')
ON CONFLICT DO NOTHING;
