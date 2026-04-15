-- 01-init.sql
-- Runs against the default "postgres" database on first container start.
-- Creates the pg_ripple extension so the default database is ready to use.

CREATE EXTENSION pg_ripple;
