-- Migration 0.73.0 → 0.74.0
-- Assessment 11 Critical/High Remediation and Evidence Truthfulness
--
-- What's new in this migration:
-- * EVIDENCE-01: Twelve missing docs/src/reference/*.md files created.
-- * GATE-05: validate-feature-status CI job bypass fixed.
-- * GATE-06: validate-feature-status-populated CI job added.
-- * JOURNAL-DATALOG-01: Datalog inference wired through mutation journal (CF-D, HF-C).
-- * SBOM-03: SBOM regenerated; just check-sbom-version target added.
-- * HTTP-VERSION-01: pg_ripple_http version bumped to 0.74.0.
-- * DOC-JOURNAL-01: mutation_journal::flush() doc comment updated.
-- * PROMO-RECOVER-01: recover_interrupted_promotions() auto-invoked at startup.
-- * CACHE-INVALIDATE-01: plan_cache::reset() called after VP promotion.
-- * TEST-04: v070_features.sql regression test added.
-- * FLUSH-DEFER-01: executor-end hook flushes mutation journal per-statement.
--
-- Schema changes: None (all changes are in Rust implementation only).

-- Bump schema version stamp.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.74.0', '0.73.0', clock_timestamp());

SELECT pg_ripple_version();
