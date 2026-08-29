-- Migration 0.133.0 → 0.134.0: performance, scale, and true streaming qualification.
-- Adds the internal portal-backed streaming entry points and advances the
-- schema ledger for databases upgraded from v0.133.0.

CREATE OR REPLACE FUNCTION _pg_ripple.sparql_stream_triples(
    query TEXT
)
RETURNS TABLE (triple TEXT)
STRICT
LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_stream_triples_wrapper';

CREATE OR REPLACE FUNCTION _pg_ripple.sparql_stream_metadata(
    query TEXT
)
RETURNS JSONB
STRICT
LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_stream_metadata_wrapper';

CREATE OR REPLACE FUNCTION _pg_ripple.sparql_stream_bindings(
    query TEXT
)
RETURNS TABLE (result JSONB)
STRICT
LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_stream_bindings_wrapper';

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.134.0', '0.133.0', clock_timestamp());
