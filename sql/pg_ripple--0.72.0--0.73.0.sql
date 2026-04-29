-- Migration 0.72.0 → 0.73.0: SPARQL 1.2 Tracking, Live Subscriptions, and Ecosystem Hardening
--
-- Schema changes:
--
-- SUB-01: Live SPARQL subscription catalog
--   _pg_ripple.sparql_subscriptions (subscription_id, query, graph_iri, created_at)
--   SQL functions: subscribe_sparql(), unsubscribe_sparql(), list_sparql_subscriptions()
--   HTTP endpoint: GET /subscribe/:subscription_id (Server-Sent Events)
--
-- JSON-MAPPING-01: Named bidirectional JSON<->RDF mapping registry
--   _pg_ripple.json_mappings (name, context, shape_iri, created_at)
--   _pg_ripple.json_mapping_warnings (mapping_name, kind, detail, recorded_at)
--   SQL functions: register_json_mapping(), ingest_json(), export_json_node()
--
-- JSONLD-INGEST-02: Multi-subject JSON-LD document ingest
--   SQL function: json_ld_load(document JSONB, default_graph TEXT DEFAULT NULL) RETURNS BIGINT
--
-- FEATURE-STATUS-02: Added llm_sparql_repair, kge_embeddings, sparql_nl_to_sparql,
--   sparql_12, sparql_subscription, json_ld_multi_ingest, json_mapping to feature_status()
--
-- CONTROL-01: pg_ripple.control comment updated to describe v0.73.0 highlights
--
-- Other deliverables (no SQL changes):
--   SPARQL12-01: plans/sparql12_tracking.md design document
--   TAXONOMY-01: docs/src/reference/feature-status-taxonomy.md
--   CONTRIB-01: CONTRIBUTING.md at repository root
--   HELM-01: charts/pg_ripple/values.yaml pins pg_ripple_http to release tag
--   R2RML-DOC-01: docs/src/features/r2rml.md + plans/r2rml_virtual.md

-- v0.73.0 SUB-01: Live SPARQL subscription catalog.
CREATE TABLE IF NOT EXISTS _pg_ripple.sparql_subscriptions (
    subscription_id  TEXT        NOT NULL PRIMARY KEY,
    query            TEXT        NOT NULL,
    graph_iri        TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.sparql_subscriptions IS
    'Registered SPARQL SELECT subscriptions for live query change notifications (v0.73.0 SUB-01)';

-- v0.73.0 JSON-MAPPING-01: Named bidirectional JSON-LD mapping registry.
CREATE TABLE IF NOT EXISTS _pg_ripple.json_mappings (
    name       TEXT        NOT NULL PRIMARY KEY,
    context    JSONB       NOT NULL,
    shape_iri  TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.json_mappings IS
    'Named bidirectional JSON<->RDF mapping registry (v0.73.0 JSON-MAPPING-01)';

-- v0.73.0 JSON-MAPPING-01: SHACL consistency check warnings.
CREATE TABLE IF NOT EXISTS _pg_ripple.json_mapping_warnings (
    mapping_name  TEXT        NOT NULL,
    kind          TEXT        NOT NULL,
    detail        TEXT        NOT NULL,
    recorded_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (mapping_name, kind, detail)
);
COMMENT ON TABLE _pg_ripple.json_mapping_warnings IS
    'SHACL consistency check warnings from register_json_mapping() (v0.73.0 JSON-MAPPING-01)';

-- Bump schema version stamp.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.73.0', '0.72.0', clock_timestamp());

SELECT pg_ripple_version();
