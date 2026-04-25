-- Migration 0.55.0 → 0.56.0: Audit log, DDL event trigger, lz4 compression
--
-- New SQL objects in this release:
--   • _pg_ripple.audit_log        — SPARQL UPDATE audit trail (H-3)
--   • _pg_ripple.catalog_events   — DDL event catalog (I-2)
--   • _pg_ripple.ddl_guard_vp_tables() — DDL event trigger function (I-2)
--   • _pg_ripple_ddl_guard event trigger (I-2)
--   • lz4 TOAST compression on _pg_ripple.dictionary.value (L-2.4)
--
-- New Rust-compiled SQL functions (no SQL migration needed):
--   • pg_ripple.sid_runway()           — SID sequence runway monitor (F-3)
--   • pg_ripple.audit_log()            — read audit log (H-3)
--   • pg_ripple.purge_audit_log(ts)    — purge old audit entries (H-3)
--   • pg_ripple.r2rml_load(iri)        — R2RML/RML direct mapping (L-7.3)
--
-- New GUCs:
--   • pg_ripple.audit_log_enabled           (bool, default off)
--   • pg_ripple.federation_circuit_breaker_threshold (int, default 5)
--   • pg_ripple.federation_circuit_breaker_reset_seconds (int, default 60)

-- SPARQL audit log.
-- Populated automatically when pg_ripple.audit_log_enabled = on.
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

-- DDL event trigger catalog.
-- Records DROP TABLE / DROP INDEX events on _pg_ripple.vp_* objects.
CREATE TABLE IF NOT EXISTS _pg_ripple.catalog_events (
    id           BIGSERIAL    NOT NULL PRIMARY KEY,
    ts           TIMESTAMPTZ  NOT NULL DEFAULT now(),
    op           TEXT         NOT NULL DEFAULT '',
    objname      TEXT         NOT NULL DEFAULT '',
    blocked_by_ripple BOOL    NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_catalog_events_ts ON _pg_ripple.catalog_events (ts);

-- L-2.4: Enable lz4 TOAST compression on the dictionary value column.
-- Silently skipped if lz4 is not compiled into this PostgreSQL build.
DO $$
BEGIN
    ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'pg_ripple: lz4 compression not available for dictionary.value: %', SQLERRM;
END;
$$;

-- I-2: DDL event trigger to warn on accidental VP table drops.
CREATE OR REPLACE FUNCTION _pg_ripple.ddl_guard_vp_tables()
    RETURNS event_trigger
    LANGUAGE plpgsql
    SECURITY DEFINER
AS $$
DECLARE
    _obj record;
BEGIN
    FOR _obj IN
        SELECT schema_name, object_name, command_tag
        FROM pg_event_trigger_dropped_objects()
        WHERE object_type IN ('table', 'index')
          AND schema_name = '_pg_ripple'
          AND object_name LIKE 'vp_%'
    LOOP
        RAISE WARNING 'PT511: _pg_ripple relation % dropped outside pg_ripple maintenance function; '
                      'run pg_ripple.vacuum() to maintain consistent state', _obj.object_name;
        INSERT INTO _pg_ripple.catalog_events (op, objname, blocked_by_ripple)
        VALUES (_obj.command_tag, _obj.schema_name || '.' || _obj.object_name, false);
    END LOOP;
END;
$$;

DO $$
BEGIN
    CREATE EVENT TRIGGER _pg_ripple_ddl_guard
        ON sql_drop
        EXECUTE FUNCTION _pg_ripple.ddl_guard_vp_tables();
EXCEPTION WHEN duplicate_object THEN
    -- already exists (e.g., re-running migration); not fatal
    NULL;
END;
$$;

-- Record migration in schema_version.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.56.0', '0.55.0', clock_timestamp());
