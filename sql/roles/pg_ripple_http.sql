-- Least-privilege role for the pg_ripple_http companion (v0.134.0).
-- Run as a database administrator after CREATE EXTENSION pg_ripple. Set a
-- password or configure certificate authentication before using the role.
-- The companion calls the stable pg_ripple SQL API; it must not receive
-- direct access to the private _pg_ripple schema.

DO $$
BEGIN
    CREATE ROLE pg_ripple_http
        LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT;
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

DO $$
BEGIN
    EXECUTE format(
        'GRANT CONNECT ON DATABASE %I TO pg_ripple_http',
        current_database()
    );
END
$$;
GRANT USAGE ON SCHEMA pg_ripple TO pg_ripple_http;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO pg_ripple_http;

-- Keep future public API functions callable without exposing internal tables.
ALTER DEFAULT PRIVILEGES IN SCHEMA pg_ripple
    GRANT EXECUTE ON FUNCTIONS TO pg_ripple_http;

-- HTTP health/diagnostic handlers use these catalog views. Do not grant any
-- table, sequence, or schema privileges on _pg_ripple here.
GRANT SELECT ON pg_catalog.pg_extension, pg_catalog.pg_settings TO pg_ripple_http;

COMMENT ON ROLE pg_ripple_http IS
    'Least-privilege database role for the pg_ripple_http companion';
