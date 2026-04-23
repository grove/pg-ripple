-- Migration 0.40.0 → 0.41.0: Full W3C SPARQL 1.1 Test Suite
--
-- This is a test-infrastructure release.  No SQL schema changes are required.
--
-- What this version adds:
--   - tests/w3c/           — Rust integration test harness for the W3C SPARQL 1.1
--                            test suite (~3 000 tests, 13 sub-suites)
--   - tests/w3c_smoke.rs   — 180-test smoke subset (optional, aggregates, grouping)
--   - tests/w3c_suite.rs   — full suite runner with parallel execution
--   - scripts/fetch_w3c_tests.sh  — download & verify the official W3C test data
--   - tests/w3c/known_failures.txt — curated list of expected failures
--   - CI jobs: w3c-smoke (required), w3c-suite (informational)
--
-- No VP table schema changes, no new GUC parameters, no new SQL functions.

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.41.0', '0.40.0')
ON CONFLICT DO NOTHING;
