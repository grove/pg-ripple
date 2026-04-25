-- Migration 0.54.0 → 0.55.0: Security hardening, observability, and scalability
-- See CHANGELOG.md for the full feature list.
--
-- Schema changes:
--   • _pg_ripple.tombstones_cleared_at — tracks when tombstone GC last ran
--     per-predicate (used by tombstone_retention_seconds = 0 logic)
--
-- New GUCs (no SQL DDL required; compiled from Rust):
--   • pg_ripple.federation_endpoint_policy (text, default 'default-deny')
--   • pg_ripple.federation_allowed_endpoints (text, default NULL)
--   • pg_ripple.tombstone_retention_seconds (int, default 0)
--   • pg_ripple.read_replica_dsn (text, default NULL)
--   • pg_ripple.normalize_iris (bool, default true)
--   • pg_ripple.copy_rdf_allowed_paths (text, default NULL)
--
-- New SQL functions (compiled from Rust):
--   • pg_ripple.federation_call_stats() → TABLE(endpoint, calls, p50_ms, p95_ms, errors, last_error)
--   • pg_ripple.grant_graph_access(graph_iri TEXT, role NAME, privilege TEXT DEFAULT 'SELECT')
--   • pg_ripple.revoke_graph_access(graph_iri TEXT, role NAME)
--   • pg_ripple.erase_subject(iri TEXT) → BIGINT
--
-- Security improvements:
--   • G-1/H-1: SSRF allowlist for federation endpoints (PT606)
--   • C-2: path allowlist for copy_rdf_from (PT403)
--   • L-8.1: named-graph RLS grant/revoke helpers
--
-- Quality improvements:
--   • A-1: shacl/mod.rs split into parser/validator/af_rules/spi submodules
--   • A-2: datalog/mod.rs trimmed (seminaive + coordinator moved to submodules)
--   • A-3: SAFETY comments on all unsafe blocks in export.rs, shmem.rs, lib.rs
--   • C-1: NFC/NFD unicode normalization for IRIs
--   • D-2: SHACL async validation snapshot LSN field
--   • E-1: execute_with_savepoint wired into parallel coordinator
--   • F-2: tombstone GC after merge cycles
--   • F-4: advisory lock on rare-predicate promotion
--   • G-4: federation call stats SRF
--   • L-5.1: read-replica routing for read-only SPARQL queries

-- Add tombstone GC tracking column to predicates catalog
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS tombstones_cleared_at TIMESTAMPTZ;

COMMENT ON COLUMN _pg_ripple.predicates.tombstones_cleared_at IS
    'Timestamp of the last tombstone GC cycle for this predicate (v0.55.0). '
    'NULL means no GC has run yet.';
