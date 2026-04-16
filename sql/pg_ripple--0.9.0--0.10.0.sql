-- Migration 0.9.0 → 0.10.0: Datalog Reasoning Engine
--
-- Schema changes:
--
--   1. _pg_ripple.rules — new catalog table for Datalog rules
--   2. _pg_ripple.rule_sets — new catalog table for named rule sets
--   3. _pg_ripple.predicates — ADD COLUMN derived BOOLEAN DEFAULT FALSE
--   4. _pg_ripple.predicates — ADD COLUMN rule_set TEXT
--   5. _pg_ripple.dictionary_hot — new UNLOGGED table for hot-path IRIs
--   6. VP tables — ADD COLUMN source SMALLINT DEFAULT 0 (if not already present)
--   7. _pg_ripple.vp_rare — ADD COLUMN source SMALLINT DEFAULT 0 (if not already present)
--
-- New GUC parameters (registered at _PG_init):
--   pg_ripple.inference_mode    ('off' | 'on_demand' | 'materialized')
--   pg_ripple.enforce_constraints ('off' | 'warn' | 'error')
--   pg_ripple.rule_graph_scope  ('default' | 'all')
--
-- New SQL functions (compiled from Rust):
--   pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom') RETURNS BIGINT
--   pg_ripple.load_rules_builtin(name TEXT) RETURNS BIGINT
--   pg_ripple.list_rules() RETURNS JSONB
--   pg_ripple.drop_rules(rule_set TEXT) RETURNS BIGINT
--   pg_ripple.enable_rule_set(name TEXT) RETURNS VOID
--   pg_ripple.disable_rule_set(name TEXT) RETURNS VOID
--   pg_ripple.infer(rule_set TEXT DEFAULT 'custom') RETURNS BIGINT
--   pg_ripple.check_constraints(rule_set TEXT DEFAULT NULL) RETURNS JSONB
--   pg_ripple.prewarm_dictionary_hot() RETURNS BIGINT
--
-- No existing data is modified or deleted.
-- All ALTER TABLE operations use ADD COLUMN IF NOT EXISTS for zero-downtime
-- upgrades (PostgreSQL fast-path adds column with stored default without
-- rewriting the table).

-- ── Datalog rules catalog ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS _pg_ripple.rules (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    rule_set      TEXT NOT NULL,
    rule_text     TEXT NOT NULL,
    head_pred     BIGINT,
    stratum       INT NOT NULL DEFAULT 0,
    is_recursive  BOOLEAN NOT NULL DEFAULT false,
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_rules_rule_set
    ON _pg_ripple.rules (rule_set);
CREATE INDEX IF NOT EXISTS idx_rules_head_pred
    ON _pg_ripple.rules (head_pred);

-- ── Rule sets catalog ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS _pg_ripple.rule_sets (
    name          TEXT NOT NULL PRIMARY KEY,
    rule_hash     BYTEA,
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Predicates table: add derived and rule_set columns ────────────────────────
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS derived BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS rule_set TEXT;

-- ── Hot dictionary table ──────────────────────────────────────────────────────
CREATE UNLOGGED TABLE IF NOT EXISTS _pg_ripple.dictionary_hot (
    id       BIGINT   NOT NULL PRIMARY KEY,
    hash     BYTEA    NOT NULL,
    value    TEXT     NOT NULL,
    kind     SMALLINT NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hot_hash
    ON _pg_ripple.dictionary_hot (hash);

-- ── VP tables: add source column if missing ───────────────────────────────────
-- The source column (0 = explicit, 1 = derived) was added to new VP tables
-- starting with v0.10.0. Existing tables created before this migration may
-- not have the column yet. This DO block adds it safely.
DO $$
DECLARE
    tbl TEXT;
BEGIN
    FOR tbl IN
        SELECT c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple'
          AND c.relkind IN ('r', 'p')
          AND c.relname LIKE 'vp_%'
          AND c.relname NOT LIKE '%_delta'
          AND c.relname NOT LIKE '%_main'
          AND c.relname NOT LIKE '%_tombstones'
          AND NOT EXISTS (
              SELECT 1 FROM pg_attribute a
              WHERE a.attrelid = c.oid
                AND a.attname = 'source'
                AND a.attnum > 0
                AND NOT a.attisdropped
          )
    LOOP
        EXECUTE format(
            'ALTER TABLE _pg_ripple.%I ADD COLUMN IF NOT EXISTS source SMALLINT NOT NULL DEFAULT 0',
            tbl
        );
    END LOOP;
END $$;

-- Add source to _delta and _main tables as well.
DO $$
DECLARE
    tbl TEXT;
BEGIN
    FOR tbl IN
        SELECT c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple'
          AND c.relkind IN ('r', 'p')
          AND (c.relname LIKE 'vp_%_delta' OR c.relname LIKE 'vp_%_main')
          AND NOT EXISTS (
              SELECT 1 FROM pg_attribute a
              WHERE a.attrelid = c.oid
                AND a.attname = 'source'
                AND a.attnum > 0
                AND NOT a.attisdropped
          )
    LOOP
        EXECUTE format(
            'ALTER TABLE _pg_ripple.%I ADD COLUMN IF NOT EXISTS source SMALLINT NOT NULL DEFAULT 0',
            tbl
        );
    END LOOP;
END $$;

-- vp_rare: add source column if missing (idempotent).
ALTER TABLE _pg_ripple.vp_rare
    ADD COLUMN IF NOT EXISTS source SMALLINT NOT NULL DEFAULT 0;
