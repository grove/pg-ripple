-- Migration 0.16.0 → 0.17.0: JSON-LD Framing Engine
--
-- New features (compiled from Rust):
--   - pg_ripple.jsonld_frame_to_sparql(frame JSONB, graph TEXT) RETURNS TEXT
--   - pg_ripple.export_jsonld_framed(frame JSONB, ...) RETURNS JSONB
--   - pg_ripple.export_jsonld_framed_stream(frame JSONB, ...) RETURNS SETOF TEXT
--   - pg_ripple.jsonld_frame(input JSONB, frame JSONB, ...) RETURNS JSONB
--   - pg_ripple.create_framing_view(name TEXT, frame JSONB, ...) RETURNS void
--   - pg_ripple.drop_framing_view(name TEXT) RETURNS BOOLEAN
--   - pg_ripple.list_framing_views() RETURNS JSONB
--
-- Schema changes:
--   - _pg_ripple.framing_views catalog table (new)

CREATE TABLE IF NOT EXISTS _pg_ripple.framing_views (
    name              TEXT        PRIMARY KEY,
    frame             JSONB       NOT NULL,
    generated_construct TEXT      NOT NULL DEFAULT '',
    schedule          TEXT        NOT NULL DEFAULT '5s',
    output_format     TEXT        NOT NULL DEFAULT 'jsonld',
    decode            BOOLEAN     NOT NULL DEFAULT FALSE,
    stream_table_oid  OID,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.framing_views IS
    'Catalog of incrementally-maintained JSON-LD framing views (v0.17.0).';
