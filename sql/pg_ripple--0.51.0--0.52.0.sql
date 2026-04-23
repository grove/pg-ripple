-- Migration 0.51.0 → 0.52.0: pg-trickle Relay Integration
-- ─────────────────────────────────────────────────────────────────────────────
--
-- New SQL-visible features in v0.52.0:
--   * pg_ripple.trickle_available()                (pg-trickle runtime detection)
--   * pg_ripple.enable_cdc_bridge_trigger()        (install CDC bridge trigger)
--   * pg_ripple.disable_cdc_bridge_trigger()       (drop CDC bridge trigger)
--   * pg_ripple.cdc_bridge_triggers()              (list registered triggers, SRF)
--   * pg_ripple.statement_dedup_key()              (relay-compatible dedup key)
--   * pg_ripple.json_to_ntriples()                 (JSON → N-Triples conversion)
--   * pg_ripple.json_to_ntriples_and_load()        (JSON → N-Triples → store)
--   * pg_ripple.triple_to_jsonld()                 (single triple → JSON-LD)
--   * pg_ripple.triples_to_jsonld()                (star-pattern → JSON-LD)
--   * pg_ripple.load_vocab_template()              (load built-in vocab rules)
--
-- New GUCs:
--   * pg_ripple.cdc_bridge_enabled     (bool, default off)
--   * pg_ripple.cdc_bridge_batch_size  (int,  default 100)
--   * pg_ripple.cdc_bridge_flush_ms    (int,  default 200)
--   * pg_ripple.cdc_bridge_outbox_table (text, default null)
--   * pg_ripple.trickle_integration    (bool, default on)
--
-- Schema changes:
--   * CREATE TABLE _pg_ripple.cdc_bridge_triggers  (CDC bridge trigger catalog)
--   * CREATE OR REPLACE FUNCTION _pg_ripple.cdc_bridge_trigger_fn()

-- ── _pg_ripple.cdc_bridge_triggers ───────────────────────────────────────────
-- Catalog of registered CDC bridge triggers.  One row per trigger installed via
-- pg_ripple.enable_cdc_bridge_trigger().

CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_bridge_triggers (
    name         TEXT        NOT NULL PRIMARY KEY,
    predicate_id BIGINT      NOT NULL,
    outbox_table TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── _pg_ripple.cdc_bridge_trigger_fn ─────────────────────────────────────────
-- PL/pgSQL trigger function used by per-predicate CDC bridge triggers.
-- Encodes each inserted (s,p,o) as a JSON-LD event and writes it to the
-- configured outbox table within the same transaction.
-- TG_ARGV[0] = predicate_id (bigint text)
-- TG_ARGV[1] = outbox table name

CREATE OR REPLACE FUNCTION _pg_ripple.cdc_bridge_trigger_fn()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    pred_id    BIGINT  := TG_ARGV[0]::bigint;
    outbox_tbl TEXT    := TG_ARGV[1];
    s_iri      TEXT;
    p_iri      TEXT;
    o_iri      TEXT;
    payload    JSONB;
    dedup_key  TEXT;
    sid        BIGINT;
BEGIN
    SELECT value INTO s_iri FROM _pg_ripple.dictionary WHERE id = NEW.s;
    SELECT value INTO p_iri FROM _pg_ripple.dictionary WHERE id = pred_id;
    SELECT value INTO o_iri FROM _pg_ripple.dictionary WHERE id = NEW.o;

    sid := NEW.i;
    dedup_key := 'ripple:' || sid::text;

    payload := jsonb_build_object(
        '@context',   'https://schema.org/',
        '@id',        COALESCE(s_iri, '_:' || NEW.s::text),
        p_iri,        COALESCE(o_iri, NEW.o::text),
        '_dedup_key', dedup_key
    );

    EXECUTE format(
        'INSERT INTO %I (event_id, payload) VALUES ($1, $2) ON CONFLICT DO NOTHING',
        outbox_tbl
    ) USING dedup_key, payload;

    RETURN NEW;
END;
$$;

-- ── Schema version ────────────────────────────────────────────────────────────
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.52.0', '0.51.0', now())
ON CONFLICT DO NOTHING;
