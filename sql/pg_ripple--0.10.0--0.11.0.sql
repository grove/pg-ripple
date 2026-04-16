-- Migration 0.10.0 → 0.11.0: Incremental SPARQL Views, Datalog Views & ExtVP
--
-- New features in v0.11.0:
--   - pg_ripple.pg_trickle_available() — check whether pg_trickle is installed
--   - pg_ripple.create_sparql_view()  — compile SPARQL SELECT to a pg_trickle stream table
--   - pg_ripple.drop_sparql_view()    — drop a SPARQL view
--   - pg_ripple.list_sparql_views()   — list registered SPARQL views
--   - pg_ripple.create_datalog_view() — compile Datalog rules + goal to a pg_trickle stream table
--   - pg_ripple.create_datalog_view_from_rule_set() — same using a named rule set
--   - pg_ripple.drop_datalog_view()   — drop a Datalog view
--   - pg_ripple.list_datalog_views()  — list registered Datalog views
--   - pg_ripple.create_extvp()        — create an ExtVP semi-join stream table
--   - pg_ripple.drop_extvp()          — drop an ExtVP table
--   - pg_ripple.list_extvp()          — list ExtVP tables
--
-- All view-management functions are soft-dependent on pg_trickle; the core
-- triple store functions work without it.
--
-- Schema changes:
--   - _pg_ripple.sparql_views (new catalog table)
--   - _pg_ripple.datalog_views (new catalog table)
--   - _pg_ripple.extvp_tables (new catalog table)

-- SPARQL views catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.sparql_views (
    name          TEXT        NOT NULL PRIMARY KEY,
    sparql        TEXT        NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    decode        BOOLEAN     NOT NULL DEFAULT false,
    stream_table  TEXT        NOT NULL,
    variables     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Datalog views catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.datalog_views (
    name          TEXT        NOT NULL PRIMARY KEY,
    rules         TEXT,
    rule_set      TEXT        NOT NULL,
    goal          TEXT        NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    decode        BOOLEAN     NOT NULL DEFAULT false,
    stream_table  TEXT        NOT NULL,
    variables     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ExtVP semi-join tables catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.extvp_tables (
    name          TEXT        NOT NULL PRIMARY KEY,
    pred1_iri     TEXT        NOT NULL,
    pred2_iri     TEXT        NOT NULL,
    pred1_id      BIGINT      NOT NULL,
    pred2_id      BIGINT      NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    stream_table  TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_extvp_pred1 ON _pg_ripple.extvp_tables (pred1_id);
CREATE INDEX IF NOT EXISTS idx_extvp_pred2 ON _pg_ripple.extvp_tables (pred2_id);
