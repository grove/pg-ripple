-- Migration: pg_ripple 0.44.0 → 0.45.0
--
-- Schema changes:    None (pure Rust function and constraint enhancements)
-- Data-rewrite cost: None
-- Downgrade strategy: No data migration is required; downgrading to 0.44.0
--                     is safe by restoring the previous .so / .dylib library
--                     file.  The two new ShapeConstraint variants (Equals,
--                     Disjoint) are Rust-only; no VP table or catalog changes
--                     are needed.
-- Test reference:    tests/pg_regress/sql/shacl_equals_disjoint.sql
--                    tests/pg_regress/sql/datalog_wfs_cap.sql
--                    tests/pg_regress/sql/datalog_parallel_rollback.sql
--
-- What this release provides (Rust-compiled; no SQL DDL required):
--   • sh:equals and sh:disjoint SHACL Core constraints
--   • decode_id_safe() helper ensures SHACL violation messages always include
--     the decoded focus-node IRI
--   • PT541 error code: LatticeJoinFnInvalid — raised when create_lattice()
--     receives a join_fn that cannot be resolved as a regprocedure
--   • Coordinated SAVEPOINT-based rollback for the Datalog seeding pass
--   • WFS iteration-cap test (wfs_max_iterations GUC)
--   • Standardised migration script headers (backfilled to key past scripts)
--   • Recovery procedure runbook in RELEASE.md

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.45.0', '0.44.0')
ON CONFLICT DO NOTHING;
