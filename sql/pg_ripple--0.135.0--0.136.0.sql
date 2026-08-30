-- Migration 0.135.0 -> 0.136.0: external-audit readiness and hardened candidate.
-- No catalog changes are required. The Rust implementation keeps the frozen
-- v0.135.0 public query contract and advances the schema ledger.

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.136.0', '0.135.0', clock_timestamp());
