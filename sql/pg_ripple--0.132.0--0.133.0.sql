-- Migration 0.132.0 → 0.133.0: crash recovery, backup, failover, and operations.
-- No destructive schema changes. The release adds health diagnostics,
-- deterministic test-only fault injection, and resilience qualification tooling.

CREATE TABLE IF NOT EXISTS _pg_ripple.merge_worker_status (
    worker_idx          INTEGER PRIMARY KEY,
    pid                 BIGINT NOT NULL DEFAULT 0,
    last_heartbeat_at   TIMESTAMPTZ,
    restart_count       BIGINT NOT NULL DEFAULT 0,
    last_error          TEXT,
    predicates_total    BIGINT NOT NULL DEFAULT 0,
    delta_rows_pending  BIGINT NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'starting',
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.merge_worker_status IS
    'Durable merge-worker heartbeat and restart diagnostics (v0.133.0).';

DO $$
DECLARE
    relation_name REGCLASS;
BEGIN
    IF EXISTS (SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'pg_ripple') THEN
        FOR relation_name IN
            SELECT c.oid::regclass
            FROM pg_catalog.pg_class AS c
            JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace
            WHERE n.nspname IN ('_pg_ripple', 'pg_ripple')
              AND c.relkind IN ('r', 'p')
              AND NOT (n.nspname = '_pg_ripple'
                      AND c.relname IN ('lattice_types', 'schema_version'))
        LOOP
            PERFORM pg_catalog.pg_extension_config_dump(relation_name, '');
        END LOOP;
    END IF;
END;
$$;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.133.0', '0.132.0', clock_timestamp());
