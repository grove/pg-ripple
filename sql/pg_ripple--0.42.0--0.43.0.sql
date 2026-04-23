-- Migration 0.42.0 → 0.43.0: WatDiv + Jena Conformance Suite
--
-- No schema changes in this release.  v0.43.0 is a test infrastructure
-- release that adds:
--   • Apache Jena test adapter (tests/jena/)
--   • WatDiv benchmark harness (tests/watdiv/)
--   • Unified conformance runner (tests/conformance/)
--   • Extended test data download script (scripts/fetch_conformance_tests.sh)
--
-- All new functionality is in the test layer; the pg_ripple SQL/Rust API
-- is unchanged from v0.42.0.

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.43.0', '0.42.0')
ON CONFLICT DO NOTHING;
