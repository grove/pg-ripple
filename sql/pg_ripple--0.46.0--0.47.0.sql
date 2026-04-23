-- pg_ripple upgrade script: 0.46.0 → 0.47.0
--
-- No schema changes in this release.
--
-- New features implemented in Rust/pgrx (no SQL DDL required):
--   - sh:lessThanOrEquals SHACL constraint (src/shacl/constraints/shape_based.rs)
--   - 6 GUC check_hook validators for federation_on_error, federation_on_partial,
--     sparql_overflow_action, tracing_exporter, embedding_index_type,
--     embedding_precision (src/lib.rs)
--   - plan_cache_stats(), dictionary_cache_stats(), federation_cache_stats()
--     individual cache hit-rate SRFs — replace JSONB plan_cache_stats() (src/sparql_api.rs)
--   - preallocate_sid_ranges() wired into parallel Datalog stratum evaluation
--     (src/datalog/mod.rs + src/datalog/parallel.rs)
--   - 5 new cargo-fuzz targets: sparql_parser, turtle_parser, datalog_parser,
--     shacl_parser, dictionary_hash (fuzz/fuzz_targets/)
--
-- Migration is a no-op: extension handles the version string update.

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.47.0', '0.46.0')
ON CONFLICT DO NOTHING;
