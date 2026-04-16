-- Migration 0.13.0 → 0.14.0: Administrative & Operational Readiness
--
-- New SQL objects (schema changes):
--   _pg_ripple.graph_access     — graph-level RLS mapping (role, graph, permission)
--   _pg_ripple.inferred_schema  — live schema summary table (optional pg_trickle)
--
-- New compiled functions (no DDL required; compiled into the extension library):
--   pg_ripple.vacuum()                  — merge + ANALYZE all VP tables
--   pg_ripple.reindex()                 — REINDEX all VP tables
--   pg_ripple.vacuum_dictionary()       — remove orphaned dictionary entries
--   pg_ripple.dictionary_stats()        — cache and dictionary metrics
--   pg_ripple.enable_graph_rls()        — activate RLS policies on vp_rare
--   pg_ripple.grant_graph(role, graph, permission)
--   pg_ripple.revoke_graph(role, graph [, permission])
--   pg_ripple.list_graph_access()
--   pg_ripple.enable_schema_summary()   — optional pg_trickle integration
--   pg_ripple.schema_summary()
--
-- New GUC:
--   pg_ripple.rls_bypass  BOOL  default off  (superuser-only override)

SET LOCAL allow_system_table_mods = on;

-- Graph access control mapping table
CREATE TABLE IF NOT EXISTS _pg_ripple.graph_access (
    role_name  TEXT   NOT NULL,
    graph_id   BIGINT NOT NULL,
    permission TEXT   NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
    PRIMARY KEY (role_name, graph_id, permission)
);
CREATE INDEX IF NOT EXISTS idx_graph_access_role
    ON _pg_ripple.graph_access (role_name);
CREATE INDEX IF NOT EXISTS idx_graph_access_graph
    ON _pg_ripple.graph_access (graph_id);

-- Live schema summary placeholder table
CREATE TABLE IF NOT EXISTS _pg_ripple.inferred_schema (
    class_iri    TEXT   NOT NULL,
    property_iri TEXT   NOT NULL,
    cardinality  BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY  (class_iri, property_iri)
);
