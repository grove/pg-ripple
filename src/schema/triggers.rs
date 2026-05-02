//! Schema additions and trigger infrastructure (v0.56.0 -- v0.73.0).
//!
//! Split from `schema.rs` in v0.85.0 (Q13-02).

pgrx::extension_sql!(
    r#"
-- SPARQL audit log (v0.56.0).
-- Records SPARQL UPDATE / DELETE DATA / DROP / CLEAR / COPY / MOVE operations
-- when pg_ripple.audit_log_enabled = on.
CREATE TABLE IF NOT EXISTS _pg_ripple.audit_log (
    id                    BIGSERIAL    NOT NULL PRIMARY KEY,
    ts                    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    role                  NAME         NOT NULL DEFAULT current_user,
    txid                  BIGINT       NOT NULL DEFAULT txid_current(),
    operation             TEXT         NOT NULL DEFAULT '',
    query                 TEXT         NOT NULL DEFAULT '',
    affected_predicate_ids BIGINT[]    NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON _pg_ripple.audit_log (ts);

-- DDL event trigger catalog (v0.56.0).
-- Records DROP TABLE / DROP INDEX events on _pg_ripple.vp_* objects.
CREATE TABLE IF NOT EXISTS _pg_ripple.catalog_events (
    id           BIGSERIAL    NOT NULL PRIMARY KEY,
    ts           TIMESTAMPTZ  NOT NULL DEFAULT now(),
    op           TEXT         NOT NULL DEFAULT '',
    objname      TEXT         NOT NULL DEFAULT '',
    blocked_by_ripple BOOL    NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_catalog_events_ts ON _pg_ripple.catalog_events (ts);

-- L-2.4 (v0.56.0): Enable lz4 page-level TOAST compression on the dictionary
-- value column to reduce table size for long IRIs and literal strings.
-- PG18 supports lz4 compression natively; existing data is recompressed lazily
-- on next VACUUM or rewrite.  Silently ignored if lz4 is unavailable.
DO $$
BEGIN
    ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4;
EXCEPTION WHEN OTHERS THEN
    -- lz4 may not be compiled into this PostgreSQL build; not fatal.
    RAISE NOTICE 'pg_ripple: lz4 compression not available for dictionary.value: %', SQLERRM;
END;
$$;

-- I-2 (v0.56.0): DDL event trigger to warn when _pg_ripple.vp_* objects are
-- dropped outside pg_ripple maintenance functions.
-- The trigger is suppressed when pg_ripple.maintenance_mode = 'on' so that
-- the merge worker and vacuum functions can drop/rename VP tables freely.
CREATE OR REPLACE FUNCTION _pg_ripple.ddl_guard_vp_tables()
    RETURNS event_trigger
    LANGUAGE plpgsql
    SECURITY DEFINER -- SECURITY-JUSTIFY: event trigger needs SECURITY DEFINER to call
    -- pg_event_trigger_dropped_objects(), which requires elevated privilege; the
    -- function only reads the event trigger context and raises a WARNING/ERROR
    -- to protect VP tables from accidental DDL drops outside maintenance mode.
AS $$
DECLARE
    _obj record;
    _in_maintenance bool;
BEGIN
    -- Skip if we are inside a pg_ripple maintenance operation.
    _in_maintenance := coalesce(
        current_setting('pg_ripple.maintenance_mode', true) = 'on',
        false
    );
    IF _in_maintenance THEN
        RETURN;
    END IF;

    FOR _obj IN
        SELECT schema_name, object_name
        FROM pg_event_trigger_dropped_objects()
        WHERE object_type IN ('table', 'index')
          AND schema_name = '_pg_ripple'
          AND object_name LIKE 'vp_%'
    LOOP
        RAISE WARNING 'PT511: _pg_ripple relation % dropped outside pg_ripple maintenance function; '
                      'run pg_ripple.vacuum() to maintain consistent state', _obj.object_name;
        INSERT INTO _pg_ripple.catalog_events (op, objname, blocked_by_ripple)
        VALUES (tg_tag, _obj.schema_name || '.' || _obj.object_name, false);
    END LOOP;
END;
$$;

CREATE EVENT TRIGGER _pg_ripple_ddl_guard
    ON sql_drop
    EXECUTE FUNCTION _pg_ripple.ddl_guard_vp_tables();
"#,
    name = "v056_audit_and_catalog_events",
    requires = ["v055_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.56.0', '0.55.0', clock_timestamp());",
    name = "v056_schema_version_stamp",
    requires = ["v056_audit_and_catalog_events"]
);

// ─── v0.57.0: KGE embeddings + multi-tenant catalog ──────────────────────────

pgrx::extension_sql!(
    r#"
-- KGE embeddings table (v0.57.0 L-4.1).
-- Uses double precision[] to avoid a hard dependency on pgvector.
CREATE TABLE IF NOT EXISTS _pg_ripple.kge_embeddings (
    entity_id   BIGINT      NOT NULL PRIMARY KEY,
    embedding   double precision[],
    model       TEXT        NOT NULL DEFAULT 'transe',
    trained_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Multi-tenant catalog (v0.57.0 L-5.3).
CREATE TABLE IF NOT EXISTS _pg_ripple.tenants (
    tenant_name    TEXT        NOT NULL PRIMARY KEY,
    graph_iri      TEXT        NOT NULL,
    quota_triples  BIGINT      NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "v057_kge_tenants_setup",
    requires = ["v056_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.57.0', '0.56.0', clock_timestamp());",
    name = "v057_schema_version_stamp",
    requires = ["v057_kge_tenants_setup"]
);

// ─── v0.58.0 schema additions ─────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- Temporal RDF statement ID timeline (v0.58.0 L-1.3).
-- Maps statement IDs to wall-clock insertion timestamps for point-in-time
-- queries.  An AFTER INSERT trigger on vp_rare and every VP delta table
-- keeps this table current.
CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline (
    sid         BIGINT      NOT NULL PRIMARY KEY,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_statement_id_timeline_ts
    ON _pg_ripple.statement_id_timeline USING BRIN (inserted_at);

-- Trigger function that records each new SID with its insertion timestamp.
CREATE OR REPLACE FUNCTION _pg_ripple.record_statement_timestamp()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO _pg_ripple.statement_id_timeline (sid, inserted_at)
    VALUES (NEW.i, now())
    ON CONFLICT (sid) DO NOTHING;
    RETURN NEW;
END;
$$;

-- Attach to vp_rare so non-promoted predicates are also tracked.
DO $do$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger t
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_rare'
          AND t.tgname = 'trg_timeline_vp_rare'
    ) THEN
        EXECUTE 'CREATE TRIGGER trg_timeline_vp_rare
                 AFTER INSERT ON _pg_ripple.vp_rare
                 FOR EACH ROW
                 EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()';
    END IF;
END
$do$;

-- PROV-O provenance catalog (v0.58.0 L-8.4).
-- Tracks the source, activity IRI and triple count for every bulk ingest
-- operation when pg_ripple.prov_enabled = on.
CREATE TABLE IF NOT EXISTS _pg_ripple.prov_catalog (
    source        TEXT        NOT NULL PRIMARY KEY,
    activity_iri  TEXT        NOT NULL,
    triple_count  BIGINT      NOT NULL DEFAULT 0,
    last_updated  TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "v058_temporal_prov_setup",
    requires = ["v057_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.58.0', '0.57.0', clock_timestamp());",
    name = "v058_schema_version_stamp",
    requires = ["v058_temporal_prov_setup"]
);

// ─── v0.59.0 schema additions ─────────────────────────────────────────────────
// v0.59.0 adds no new tables or columns.  All new behaviour (shard-pruning,
// rebalance NOTIFY, explain_sparql citus section, citus_rebalance_progress) is
// compiled into the Rust shared library.  We only stamp the schema_version table.

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.59.0', '0.58.0', clock_timestamp());",
    name = "v059_schema_version_stamp",
    requires = ["v058_schema_version_stamp"]
);

// ─── v0.61.0 schema additions ─────────────────────────────────────────────────
// New tables: graph_shard_affinity (CITUS-22), rule_firing_log (inference explainability)
// Note: brin_summarize_failures column is already in the predicates CREATE TABLE above.

pgrx::extension_sql!(
    "-- v0.61.0: Citus named-graph shard affinity reference table (CITUS-22).
     CREATE TABLE IF NOT EXISTS _pg_ripple.graph_shard_affinity (
         graph_id    BIGINT      NOT NULL PRIMARY KEY,
         shard_id    INT         NOT NULL DEFAULT 0,
         worker_node TEXT        NOT NULL DEFAULT '',
         created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
     );

     -- v0.61.0: Datalog rule-firing log for inference explainability (6.6).
     CREATE TABLE IF NOT EXISTS _pg_ripple.rule_firing_log (
         id          BIGSERIAL   PRIMARY KEY,
         fired_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
         rule_id     TEXT        NOT NULL,
         rule_set    TEXT        NOT NULL DEFAULT '',
         output_sid  BIGINT,
         source_sids BIGINT[]    NOT NULL DEFAULT '{}',
         session_pid INT         NOT NULL DEFAULT pg_backend_pid()
     );
     CREATE INDEX IF NOT EXISTS rule_firing_log_output_sid_idx
         ON _pg_ripple.rule_firing_log (output_sid);

     INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
         VALUES ('0.61.0', '0.59.0', clock_timestamp());",
    name = "v061_schema_additions",
    requires = ["v059_schema_version_stamp"]
);

// ─── v0.62.0 schema additions ─────────────────────────────────────────────────
// Schema change: access_count column on _pg_ripple.dictionary (CITUS-26 tiered dict).
// All other v0.62.0 changes are pure Rust (WCOJ planner, Arrow Flight, Citus SRFs).

pgrx::extension_sql!(
    "-- v0.62.0: Tiered dictionary — access_count column (CITUS-26).
     ALTER TABLE _pg_ripple.dictionary
         ADD COLUMN IF NOT EXISTS access_count BIGINT NOT NULL DEFAULT 0;

     INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
         VALUES ('0.62.0', '0.61.0', clock_timestamp());",
    name = "v062_schema_additions",
    requires = ["v061_schema_additions"]
);

// ─── v0.63.0 schema additions ─────────────────────────────────────────────────
// New tables: construct_rules (CWB-07), construct_rule_triples (CWB-11).
// All CITUS-30–37 improvements are pure Rust (no schema changes).

pgrx::extension_sql!(
    "-- v0.63.0: SPARQL CONSTRUCT writeback rules catalog (CWB-07).
     CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rules (
         name            TEXT PRIMARY KEY,
         sparql          TEXT NOT NULL,
         generated_sql   TEXT,
         target_graph    TEXT NOT NULL,
         target_graph_id BIGINT NOT NULL,
         mode            TEXT NOT NULL DEFAULT 'incremental',
         source_graphs   TEXT[],
         rule_order      INT,
         created_at      TIMESTAMPTZ DEFAULT now(),
         last_refreshed  TIMESTAMPTZ
     );
     COMMENT ON TABLE _pg_ripple.construct_rules IS
         'Registered SPARQL CONSTRUCT writeback rules (v0.63.0+)';

     -- v0.63.0: Per-rule provenance for derived triples (CWB-11).
     CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rule_triples (
         rule_name TEXT   NOT NULL,
         pred_id   BIGINT NOT NULL,
         s         BIGINT NOT NULL,
         o         BIGINT NOT NULL,
         g         BIGINT NOT NULL,
         PRIMARY KEY (rule_name, pred_id, s, o, g)
     );
     COMMENT ON TABLE _pg_ripple.construct_rule_triples IS
         'Per-rule provenance for derived triples; enables safe multi-rule shared target graphs (v0.63.0+)';

     INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
         VALUES ('0.63.0', '0.62.0', clock_timestamp());",
    name = "v063_schema_additions",
    requires = ["v062_schema_additions"]
);

// v0.64.0: Release Truth and Safety Freeze.
// No schema DDL changes — stamp only so that diagnostic_report() returns
// schema_version = '0.64.0' on a fresh CREATE EXTENSION.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.64.0', '0.63.0', clock_timestamp());",
    name = "v064_schema_version_fresh_install_stamp",
    requires = ["v063_schema_additions"]
);

// v0.65.0: CONSTRUCT Writeback Correctness Closure.
// Schema changes: add v0.65.0 observability columns to _pg_ripple.construct_rules
// (last_incremental_run, successful_run_count, failed_run_count, last_error,
// derived_triple_count).  Added via IF NOT EXISTS for idempotency.
pgrx::extension_sql!(
    "ALTER TABLE IF EXISTS _pg_ripple.construct_rules
       ADD COLUMN IF NOT EXISTS last_incremental_run  TIMESTAMPTZ,
       ADD COLUMN IF NOT EXISTS successful_run_count  BIGINT NOT NULL DEFAULT 0,
       ADD COLUMN IF NOT EXISTS failed_run_count      BIGINT NOT NULL DEFAULT 0,
       ADD COLUMN IF NOT EXISTS last_error            TEXT,
       ADD COLUMN IF NOT EXISTS derived_triple_count  BIGINT NOT NULL DEFAULT 0;
     INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
       VALUES ('0.65.0', '0.64.0', clock_timestamp());",
    name = "v065_schema_additions",
    requires = ["v064_schema_version_fresh_install_stamp"]
);

// v0.66.0: Streaming and Distributed Reality.
// No schema DDL changes — stamp only so that diagnostic_report() returns
// schema_version = '0.66.0' on a fresh CREATE EXTENSION.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.66.0', '0.65.0', clock_timestamp());",
    name = "v066_schema_version_stamp",
    requires = ["v065_schema_additions"]
);

// v0.67.0: Production Hardening and Assessment 9 Remediation.
// No schema DDL changes — stamp only so that diagnostic_report() returns
// schema_version = '0.67.0' on a fresh CREATE EXTENSION.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.67.0', '0.66.0', clock_timestamp());",
    name = "v067_schema_version_stamp",
    requires = ["v066_schema_version_stamp"]
);

// v0.68.0: STREAM-01, CITUS-HLL-01, CITUS-SVC-01, PROMO-01, FUZZ-01.
// predicates.promotion_status column is already in the CREATE TABLE above
// so no ALTER TABLE needed here for fresh installs.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.68.0', '0.67.0', clock_timestamp());",
    name = "v068_schema_version_stamp",
    requires = ["v067_schema_version_stamp"]
);

// v0.69.0: ARCH-01..ARCH-05 module-restructuring (pure code layout, no SQL changes).
// Stamp only so that diagnostic_report() returns schema_version = '0.69.0'
// on a fresh CREATE EXTENSION.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.69.0', '0.68.0', clock_timestamp());",
    name = "v069_schema_version_stamp",
    requires = ["v068_schema_version_stamp"]
);

// v0.70.0: Assessment 10 critical remediation.
// No schema DDL changes — stamp only.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.70.0', '0.69.0', clock_timestamp());",
    name = "v070_schema_version_stamp",
    requires = ["v069_schema_version_stamp"]
);

// v0.71.0: Arrow Flight streaming, Citus integration test, compatibility matrix.
// No schema DDL changes — stamp only.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.71.0', '0.70.0', clock_timestamp());",
    name = "v071_schema_version_stamp",
    requires = ["v070_schema_version_stamp"]
);

// v0.72.0: Architecture and Protocol Hardening.
// No schema DDL changes — stamp only.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.72.0', '0.71.0', clock_timestamp());",
    name = "v072_schema_version_stamp",
    requires = ["v071_schema_version_stamp"]
);

// v0.73.0: SPARQL 1.2 tracking, live subscriptions, JSON mapping.
// Schema changes:
//   - _pg_ripple.sparql_subscriptions: live SPARQL subscription catalog (SUB-01)
//   - _pg_ripple.json_mappings: named bidirectional JSON-LD mapping registry (JSON-MAPPING-01)
//   - _pg_ripple.json_mapping_warnings: SHACL consistency check warnings
pgrx::extension_sql!(
    r#"
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

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.73.0', '0.72.0', clock_timestamp());
"#,
    name = "v073_schema_additions",
    requires = ["v072_schema_version_stamp"]
);

// ─── v0.74.0 schema additions ────────────────────────────────────────────────
// Schema Optimization: surrogate ids, split-hash dictionary, satellite tables,
