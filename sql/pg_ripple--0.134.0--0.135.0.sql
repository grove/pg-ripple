-- Migration 0.134.0 -> 0.135.0: typed query bindings and governed prefixes.

CREATE FUNCTION pg_ripple.sparql(text, jsonb)
RETURNS TABLE (result jsonb)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_with_bindings_wrapper';

CREATE FUNCTION pg_ripple.sparql_construct(text, jsonb)
RETURNS TABLE (result jsonb)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_construct_with_bindings_wrapper';

CREATE FUNCTION pg_ripple.sparql_describe(text, jsonb, text DEFAULT 'cbd')
RETURNS TABLE (result jsonb)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_describe_with_bindings_wrapper';

CREATE FUNCTION pg_ripple.sparql_cursor(text, jsonb)
RETURNS TABLE (result jsonb)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_cursor_with_bindings_wrapper';

CREATE FUNCTION _pg_ripple.sparql_stream_bindings_with_bindings(text, jsonb)
RETURNS TABLE (result jsonb)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_stream_bindings_with_bindings_wrapper';

CREATE FUNCTION _pg_ripple.sparql_stream_triples_with_bindings(text, jsonb)
RETURNS TABLE (triple text)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'sparql_stream_triples_with_bindings_wrapper';

CREATE FUNCTION pg_ripple.drop_prefix(text)
RETURNS boolean
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'drop_prefix_wrapper';

CREATE FUNCTION pg_ripple.prefix_registry_generation()
RETURNS bigint
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'prefix_registry_generation_wrapper';

CREATE FUNCTION pg_ripple.supported_surface(text)
RETURNS TABLE (
    feature_name text,
    optional_dependency text,
    unsupported_combination text,
    evidence_artifact text
)
STRICT LANGUAGE C
AS 'MODULE_PATHNAME', 'supported_surface_wrapper';

ALTER TABLE _pg_ripple.prefixes
    ADD COLUMN IF NOT EXISTS owner_oid OID,
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;

UPDATE _pg_ripple.prefixes
SET owner_oid = COALESCE(owner_oid, (current_user::regrole)::oid),
    created_at = COALESCE(created_at, now()),
    updated_at = COALESCE(updated_at, now())
WHERE owner_oid IS NULL OR created_at IS NULL OR updated_at IS NULL;

ALTER TABLE _pg_ripple.prefixes
    ALTER COLUMN owner_oid SET DEFAULT (current_user::regrole)::oid,
    ALTER COLUMN created_at SET DEFAULT now(),
    ALTER COLUMN updated_at SET DEFAULT now(),
    ALTER COLUMN owner_oid SET NOT NULL,
    ALTER COLUMN created_at SET NOT NULL,
    ALTER COLUMN updated_at SET NOT NULL;

CREATE TABLE IF NOT EXISTS _pg_ripple.prefix_registry_state (
    singleton BOOLEAN NOT NULL PRIMARY KEY DEFAULT true CHECK (singleton),
    generation BIGINT NOT NULL DEFAULT 1
);

INSERT INTO _pg_ripple.prefix_registry_state (singleton, generation)
VALUES (true, 1)
ON CONFLICT (singleton) DO NOTHING;

REVOKE INSERT, UPDATE, DELETE ON _pg_ripple.prefixes FROM PUBLIC;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.135.0', '0.134.0', clock_timestamp());
