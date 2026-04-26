-- Migration 0.57.0 → 0.58.0
-- Features: Temporal RDF queries (L-1.3), SPARQL-DL (L-1.4),
--           Citus horizontal sharding (L-5.4), PROV-O provenance (L-8.4),
--           v1 readiness integration test suite (J-6), CI gate flip (J-5).
--
-- Schema changes:
--   1. _pg_ripple.statement_id_timeline — maps SID → insertion timestamp
--   2. _pg_ripple.prov_catalog — PROV-O load provenance catalog
--   3. _pg_ripple.record_statement_timestamp() trigger function
--
-- New GUCs (registered in Rust _PG_init, no SQL required):
--   pg_ripple.citus_sharding_enabled  BOOL DEFAULT false
--   pg_ripple.citus_trickle_compat    BOOL DEFAULT false
--   pg_ripple.merge_fence_timeout_ms  INT  DEFAULT 0
--   pg_ripple.prov_enabled            BOOL DEFAULT false
--
-- New SQL functions (compiled from Rust):
--   pg_ripple.point_in_time(timestamptz)
--   pg_ripple.clear_point_in_time()
--   pg_ripple.point_in_time_info()
--   pg_ripple.sparql_dl_subclasses(text)
--   pg_ripple.sparql_dl_superclasses(text)
--   pg_ripple.enable_citus_sharding()
--   pg_ripple.citus_rebalance()
--   pg_ripple.citus_cluster_status()
--   pg_ripple.citus_available()
--   pg_ripple.prov_stats()
--   pg_ripple.prov_enabled()

-- ── 1. Statement ID timeline ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline (
    sid         BIGINT      NOT NULL PRIMARY KEY,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_statement_id_timeline_ts
    ON _pg_ripple.statement_id_timeline USING BRIN (inserted_at);

-- ── 2. PROV-O provenance catalog ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS _pg_ripple.prov_catalog (
    source        TEXT        NOT NULL PRIMARY KEY,
    activity_iri  TEXT        NOT NULL,
    triple_count  BIGINT      NOT NULL DEFAULT 0,
    last_updated  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── 3. Timeline trigger function ─────────────────────────────────────────────

CREATE OR REPLACE FUNCTION _pg_ripple.record_statement_timestamp()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO _pg_ripple.statement_id_timeline (sid, inserted_at)
    VALUES (NEW.i, now())
    ON CONFLICT (sid) DO NOTHING;
    RETURN NEW;
END;
$$;

-- Attach the trigger to all existing VP delta tables.
DO $$
DECLARE
    r RECORD;
    trigger_name TEXT;
BEGIN
    FOR r IN
        SELECT c.relname, n.nspname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple'
          AND c.relname LIKE 'vp_%_delta'
          AND c.relkind = 'r'
    LOOP
        trigger_name := 'trg_timeline_' || r.relname;
        IF NOT EXISTS (
            SELECT 1 FROM pg_trigger t
            JOIN pg_class tc ON tc.oid = t.tgrelid
            WHERE tc.relname = r.relname
              AND t.tgname = trigger_name
        ) THEN
            EXECUTE format(
                'CREATE TRIGGER %I AFTER INSERT ON %I.%I '
                'FOR EACH ROW '
                'EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()',
                trigger_name, r.nspname, r.relname
            );
        END IF;
    END LOOP;
END
$$;

-- Also attach the trigger to vp_rare for non-promoted predicates.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger t
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_rare'
          AND t.tgname = 'trg_timeline_vp_rare'
    ) THEN
        EXECUTE 'CREATE TRIGGER trg_timeline_vp_rare '
                'AFTER INSERT ON _pg_ripple.vp_rare '
                'FOR EACH ROW '
                'EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()';
    END IF;
END
$$;
