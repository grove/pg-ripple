-- Migration 0.17.0 → 0.18.0: SPARQL CONSTRUCT, DESCRIBE & ASK Views
--
-- New SQL functions (compiled from Rust, no DDL changes required for them):
--   pg_ripple.create_construct_view(name, sparql, schedule, decode) → BIGINT
--   pg_ripple.drop_construct_view(name) → void
--   pg_ripple.list_construct_views() → jsonb
--   pg_ripple.create_describe_view(name, sparql, schedule, decode) → void
--   pg_ripple.drop_describe_view(name) → void
--   pg_ripple.list_describe_views() → jsonb
--   pg_ripple.create_ask_view(name, sparql, schedule) → void
--   pg_ripple.drop_ask_view(name) → void
--   pg_ripple.list_ask_views() → jsonb
--
-- Schema changes: three new catalog tables and a helper function.

-- CONSTRUCT views catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.construct_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    decode         BOOLEAN     NOT NULL DEFAULT false,
    template_count BIGINT      NOT NULL DEFAULT 0,
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- DESCRIBE views catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.describe_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    decode         BOOLEAN     NOT NULL DEFAULT false,
    strategy       TEXT        NOT NULL DEFAULT 'cbd',
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ASK views catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.ask_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Helper function for DESCRIBE views: enumerate all triples for a resource.
-- For cbd (include_incoming=false): outgoing arcs only.
-- For scbd (include_incoming=true): outgoing + incoming arcs.
CREATE OR REPLACE FUNCTION _pg_ripple.triples_for_resource(
    resource_id     BIGINT,
    include_incoming BOOLEAN DEFAULT false
) RETURNS TABLE(s BIGINT, p BIGINT, o BIGINT)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    r RECORD;
BEGIN
    -- Outgoing arcs from rare predicates table.
    RETURN QUERY SELECT vr.s, vr.p, vr.o
                 FROM _pg_ripple.vp_rare vr
                 WHERE vr.s = resource_id;

    -- Outgoing arcs from dedicated VP tables.
    FOR r IN
        SELECT pc.id AS pred_id
        FROM _pg_ripple.predicates pc
        WHERE pc.table_oid IS NOT NULL
    LOOP
        RETURN QUERY EXECUTE format(
            'SELECT s, %L::bigint AS p, o FROM _pg_ripple.vp_%s WHERE s = $1',
            r.pred_id, r.pred_id
        ) USING resource_id;
    END LOOP;

    IF include_incoming THEN
        -- Incoming arcs from rare predicates table.
        RETURN QUERY SELECT vr.s, vr.p, vr.o
                     FROM _pg_ripple.vp_rare vr
                     WHERE vr.o = resource_id;

        -- Incoming arcs from dedicated VP tables.
        FOR r IN
            SELECT pc.id AS pred_id
            FROM _pg_ripple.predicates pc
            WHERE pc.table_oid IS NOT NULL
        LOOP
            RETURN QUERY EXECUTE format(
                'SELECT s, %L::bigint AS p, o FROM _pg_ripple.vp_%s WHERE o = $1',
                r.pred_id, r.pred_id
            ) USING resource_id;
        END LOOP;
    END IF;
END;
$$;
