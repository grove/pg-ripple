-- v0.133.0: operational health API and durable worker diagnostics.

LOAD '$libdir/pg_ripple';

\pset format unaligned
\pset tuples_only on

SELECT (pg_ripple.health() ? 'status') AS health_has_status;
SELECT (pg_ripple.health() ? 'worker') AS health_has_worker;
SELECT to_regclass('_pg_ripple.merge_worker_status') IS NOT NULL AS worker_catalog_exists;
SELECT (pg_ripple.health() -> 'migration' ->> 'status')
       IN ('current', 'pending', 'unknown') AS migration_status_is_known;
