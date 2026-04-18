-- Migration 0.24.0 → 0.25.0: GeoSPARQL & Architectural Polish
--
-- New features (compiled from Rust — SQL schema changes below):
--   • GeoSPARQL 1.1 subset: geo:sfIntersects, geo:sfContains, geo:sfWithin,
--     geo:sfTouches, geo:sfCrosses, geo:sfOverlaps, geof:distance, geof:area,
--     geof:boundary — requires PostGIS (graceful no-op when absent)
--   • Federation: scheme validation (http/https only) at register_endpoint()
--   • Bulk load: strict BOOLEAN parameter; blank-node prefix via nextval()
--   • CDC: decode BOOLEAN parameter for pg_ripple.cdc_changes()
--   • Catalog: schema_name and table_name columns added to _pg_ripple.predicates
--   • pg_trickle version-lock probe at _PG_init
--   • Supplementary: load_owl_ontology(), apply_patch(), register_aggregate()
--   • Federation cache key upgraded from XXH3-64 (BIGINT) to XXH3-128 (TEXT)
--   • File-path security: load_*_file() restricted to pg data directory

-- Add schema_name and table_name columns to the predicate catalog.
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS schema_name NAME DEFAULT '_pg_ripple',
    ADD COLUMN IF NOT EXISTS table_name  NAME;

-- Populate table_name for all existing predicates.
-- Delta partition is the canonical mutable table; the name is derivable from the id.
UPDATE _pg_ripple.predicates
SET table_name = 'vp_' || id || '_delta'
WHERE table_name IS NULL;

-- H-12: Upgrade federation_cache query_hash from XXH3-64 (BIGINT) to XXH3-128 (TEXT).
-- Cache rows are ephemeral and can be safely discarded during the upgrade.
TRUNCATE _pg_ripple.federation_cache;
ALTER TABLE _pg_ripple.federation_cache DROP CONSTRAINT IF EXISTS federation_cache_pkey;
ALTER TABLE _pg_ripple.federation_cache ALTER COLUMN query_hash TYPE TEXT;
ALTER TABLE _pg_ripple.federation_cache ADD PRIMARY KEY (url, query_hash);

-- Custom aggregates catalog for register_aggregate().
CREATE TABLE IF NOT EXISTS _pg_ripple.custom_aggregates (
    sparql_iri  TEXT NOT NULL PRIMARY KEY,
    pg_function TEXT NOT NULL
);

-- Revoke PUBLIC access to the internal schema (defence-in-depth; mirrors v0.22.0 REVOKE
-- in case the 0.21.0→0.22.0 migration was applied before this column was added).
-- These statements are idempotent.
REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL TABLES    IN SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA _pg_ripple FROM PUBLIC;
