-- Migration 0.38.0 → 0.39.0: Streaming Results, Explain & Observability
-- Schema changes:
--   - New GUCs: sparql_max_rows, datalog_max_derived, export_max_rows,
--               sparql_overflow_action, tracing_enabled, tracing_exporter
-- Data-rewrite cost: None (pure Rust function additions; no SQL schema changes)
-- Downgrade: No schema changes to revert; remove GUC settings from postgresql.conf.

-- No DDL changes required.
-- New pg_extern functions and GUCs are registered by _PG_init() on library load:
--   pg_ripple.sparql_cursor(query TEXT)            RETURNS SETOF RECORD
--   pg_ripple.sparql_cursor_turtle(query TEXT)     RETURNS SETOF TEXT
--   pg_ripple.sparql_cursor_jsonld(query TEXT)     RETURNS SETOF TEXT
--   pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN DEFAULT false) RETURNS JSONB
--   pg_ripple.explain_datalog(rule_set_name TEXT)  RETURNS JSONB
--   pg_ripple.cache_stats()                        RETURNS JSONB
--   pg_ripple.reset_cache_stats()                  RETURNS VOID
--   pg_ripple.stat_statements_decoded              VIEW

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.39.0', '0.38.0')
ON CONFLICT DO NOTHING;
