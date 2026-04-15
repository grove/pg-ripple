-- Migration: pg_ripple 0.5.1 → 0.6.0 — HTAP Architecture
--
-- This script migrates the pg_ripple schema from the flat single-table VP
-- storage model (v0.1.0–v0.5.1) to the HTAP dual-partition model.
--
-- Changes introduced in this migration:
--
-- 1. ALTER TABLE _pg_ripple.predicates ADD COLUMN htap BOOLEAN DEFAULT false
--    Tracks whether a predicate has been split into delta/main/tombstones.
--
-- 2. CREATE TABLE _pg_ripple.subject_patterns (s, pattern BIGINT[])
--    CREATE TABLE _pg_ripple.object_patterns  (o, pattern BIGINT[])
--    Pattern-index tables populated by the merge worker after each generation.
--
-- 3. CREATE TABLE _pg_ripple.cdc_subscriptions
--    CREATE OR REPLACE FUNCTION _pg_ripple.notify_triple_change()
--    Change data capture infrastructure for LISTEN/NOTIFY subscriptions.
--
-- 4. Per-predicate HTAP migration:
--    For each existing dedicated VP table _pg_ripple.vp_{id}:
--      a. Rename flat table → _pg_ripple.vp_{id}_pre_htap (backup)
--      b. CREATE TABLE _pg_ripple.vp_{id}_delta  AS SELECT * FROM backup
--      c. CREATE TABLE _pg_ripple.vp_{id}_main   (empty, BRIN indexed)
--      d. CREATE TABLE _pg_ripple.vp_{id}_tombstones (s, o, g)
--      e. CREATE VIEW  _pg_ripple.vp_{id}  = (main − tombstones) UNION ALL delta
--      f. UPDATE _pg_ripple.predicates SET htap = true
--      g. DROP TABLE _pg_ripple.vp_{id}_pre_htap
--
-- After migration all existing triples reside in delta tables.  An optional
-- call to pg_ripple.compact() promotes them to main in a single sorted merge.

-- ── Step 1: Add htap column ────────────────────────────────────────────────────

ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS htap BOOLEAN NOT NULL DEFAULT false;

-- ── Step 2: Subject/Object pattern tables ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS _pg_ripple.subject_patterns (
    s       BIGINT   NOT NULL PRIMARY KEY,
    pattern BIGINT[] NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subject_patterns_gin
    ON _pg_ripple.subject_patterns USING GIN (pattern);

CREATE TABLE IF NOT EXISTS _pg_ripple.object_patterns (
    o       BIGINT   NOT NULL PRIMARY KEY,
    pattern BIGINT[] NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_object_patterns_gin
    ON _pg_ripple.object_patterns USING GIN (pattern);

-- ── Step 3: CDC infrastructure ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_subscriptions (
    id               BIGSERIAL PRIMARY KEY,
    channel          TEXT    NOT NULL,
    predicate_id     BIGINT,
    predicate_pattern TEXT   NOT NULL DEFAULT '*'
);

CREATE INDEX IF NOT EXISTS idx_cdc_subs_channel
    ON _pg_ripple.cdc_subscriptions (channel);

CREATE INDEX IF NOT EXISTS idx_cdc_subs_predicate
    ON _pg_ripple.cdc_subscriptions (predicate_id);

CREATE OR REPLACE FUNCTION _pg_ripple.notify_triple_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    pred_id BIGINT := TG_ARGV[0]::bigint;
    payload TEXT;
    sub RECORD;
BEGIN
    IF TG_OP = 'INSERT' THEN
        payload := json_build_object(
            'op', 'insert',
            's', NEW.s, 'p', pred_id, 'o', NEW.o, 'g', NEW.g
        )::text;
    ELSE
        payload := json_build_object(
            'op', 'delete',
            's', OLD.s, 'p', pred_id, 'o', OLD.o, 'g', OLD.g
        )::text;
    END IF;

    FOR sub IN
        SELECT channel FROM _pg_ripple.cdc_subscriptions
        WHERE predicate_id = pred_id OR predicate_pattern = '*'
    LOOP
        PERFORM pg_notify(sub.channel, payload);
    END LOOP;

    RETURN NEW;
END;
$$;

-- ── Step 4: Per-predicate HTAP migration ──────────────────────────────────────

DO $$
DECLARE
    rec RECORD;
    table_exists BOOLEAN;
    pred_id BIGINT;
BEGIN
    FOR rec IN
        SELECT id FROM _pg_ripple.predicates
        WHERE table_oid IS NOT NULL
          AND htap = false
        ORDER BY id
    LOOP
        pred_id := rec.id;

        -- Check if the flat table exists (not already migrated).
        SELECT EXISTS (
            SELECT 1 FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relname = 'vp_' || pred_id::text
              AND n.nspname  = '_pg_ripple'
              AND c.relkind  = 'r'   -- must be a regular table (r), not a view (v)
        ) INTO table_exists;

        IF NOT table_exists THEN
            CONTINUE;
        END IF;

        -- a. Rename flat table to backup.
        EXECUTE format(
            'ALTER TABLE _pg_ripple.vp_%s RENAME TO vp_%s_pre_htap',
            pred_id, pred_id
        );

        -- b. Create delta (copy all existing rows).
        EXECUTE format(
            'CREATE TABLE _pg_ripple.vp_%s_delta AS SELECT * FROM _pg_ripple.vp_%s_pre_htap',
            pred_id, pred_id
        );
        EXECUTE format(
            'CREATE INDEX idx_vp_%s_delta_s_o ON _pg_ripple.vp_%s_delta (s, o)',
            pred_id, pred_id
        );
        EXECUTE format(
            'CREATE INDEX idx_vp_%s_delta_o_s ON _pg_ripple.vp_%s_delta (o, s)',
            pred_id, pred_id
        );

        -- c. Create empty main (BRIN indexed).
        EXECUTE format(
            'CREATE TABLE _pg_ripple.vp_%s_main (
                 s      BIGINT   NOT NULL,
                 o      BIGINT   NOT NULL,
                 g      BIGINT   NOT NULL DEFAULT 0,
                 i      BIGINT   NOT NULL DEFAULT nextval(''_pg_ripple.statement_id_seq''),
                 source SMALLINT NOT NULL DEFAULT 0
             )',
            pred_id
        );
        EXECUTE format(
            'CREATE INDEX idx_vp_%s_main_brin ON _pg_ripple.vp_%s_main USING BRIN (s)',
            pred_id, pred_id
        );

        -- d. Create empty tombstones.
        EXECUTE format(
            'CREATE TABLE _pg_ripple.vp_%s_tombstones (
                 s BIGINT NOT NULL,
                 o BIGINT NOT NULL,
                 g BIGINT NOT NULL DEFAULT 0
             )',
            pred_id
        );
        EXECUTE format(
            'CREATE INDEX idx_vp_%s_tombs ON _pg_ripple.vp_%s_tombstones (s, o, g)',
            pred_id, pred_id
        );

        -- e. Create the read view.
        EXECUTE format(
            'CREATE VIEW _pg_ripple.vp_%s AS
             SELECT m.s, m.o, m.g, m.i, m.source
             FROM _pg_ripple.vp_%s_main m
             LEFT JOIN _pg_ripple.vp_%s_tombstones t
                 ON m.s = t.s AND m.o = t.o AND m.g = t.g
             WHERE t.s IS NULL
             UNION ALL
             SELECT d.s, d.o, d.g, d.i, d.source
             FROM _pg_ripple.vp_%s_delta d',
            pred_id, pred_id, pred_id, pred_id
        );

        -- f. Update predicates catalog.
        EXECUTE format(
            'UPDATE _pg_ripple.predicates
             SET table_oid = (''_pg_ripple.vp_%s''::regclass)::oid, htap = true
             WHERE id = %s',
            pred_id, pred_id
        );

        -- g. Drop the backup.
        EXECUTE format('DROP TABLE _pg_ripple.vp_%s_pre_htap', pred_id);

        -- Install CDC trigger on delta.
        EXECUTE format(
            'CREATE TRIGGER cdc_notify_%s
             AFTER INSERT OR DELETE ON _pg_ripple.vp_%s_delta
             FOR EACH ROW EXECUTE FUNCTION _pg_ripple.notify_triple_change(%s)',
            pred_id, pred_id, pred_id
        );

        RAISE NOTICE 'Migrated predicate % to HTAP', pred_id;
    END LOOP;
END
$$;
