-- Migration 0.81.0 → 0.82.0: Assessment 12 Performance & Observability
--
-- New schema objects required:
--
--  MERGE-HBEAT-01  : _pg_ripple.merge_worker_status — merge worker heartbeat table
--  FED-COST-01a    : _pg_ripple.federation_stats — federation endpoint latency stats
--  STATS-CACHE-01a : _pg_ripple.predicate_stats_cache — predicate triple-count cache
--
-- Pure Rust function additions (no DDL required):
--   CACHE-CAP-01       — pg_ripple.plan_cache_capacity GUC
--   DECODE-BIND-01     — ANY($1::bigint[]) bind in batch_decode
--   DECODE-WARN-01     — WARNING on missing dictionary IDs
--   MERGE-PRED-01      — merge worker predicate cache with 60s TTL
--   MERGE-LOCK-GUC-01  — pg_ripple.merge_lock_timeout_ms GUC
--   GUC-BOUNDS-01      — min/max validators for vp_promotion_threshold,
--                        dictionary_cache_size, merge_batch_size
--   PGSS-NORM-01       — pg_ripple.sparql_normalise() SQL function
--   EXPLAIN-ALG-01     — SPARQL algebra tree in sparql_explain() output
--   FED-BODY-STREAM-01 — Content-Length pre-check before buffering
--   FED-COUNTER-ORDER-01 — FED_CALL_COUNT after policy check
--   EXPORT-JSONLD-OOM-01 — NOTICE for >1M triples in export_jsonld()
--   PROPPATH-UNBOUNDED-01 — pg_ripple.all_nodes_predicate_limit GUC
--   VACUUM-DICT-BATCH-01  — batched UNION ALL, pg_ripple.vacuum_dict_batch_size GUC
--   STATS-DOC-01       — pg_ripple.stats_scan_limit GUC
--   DATALOG-SILENT-01  — cleanup failures logged, not silently swallowed
--   TENANT-NAME-01     — ^[A-Za-z0-9_]{1,63}$ allowlist for tenant_name
--   ROLE-UNICODE-01    — SPI fallback in quote_ident_safe() for Unicode names
--   SHMEM-SAFE-01      — checked_mul for shared-memory size calculation
--   RUSTSEC-01         — audit.toml: RUSTSEC-2023-0071 exemption added
--   SPARQL-COMPLEX-01  — sparql_max_algebra_depth GUC (already enforced from v0.81)
--   EMBED-MODEL-01     — all paths use pg_ripple.embedding_model GUC

-- ── MERGE-HBEAT-01: Merge worker heartbeat status table ──────────────────────
-- Tracks the last heartbeat from each merge background worker.
-- Written by emit_merge_worker_heartbeat() in src/worker.rs.
CREATE TABLE IF NOT EXISTS _pg_ripple.merge_worker_status (
    worker_idx          INT          PRIMARY KEY,
    last_heartbeat_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    predicates_total    BIGINT       NOT NULL DEFAULT 0,
    delta_rows_pending  BIGINT       NOT NULL DEFAULT 0
);

COMMENT ON TABLE _pg_ripple.merge_worker_status IS
    'Per-worker merge heartbeat status. Updated every pg_ripple.merge_heartbeat_interval_seconds seconds. '
    'Used for operational monitoring of the HTAP merge background worker. (v0.82.0 MERGE-HBEAT-01)';

-- ── FED-COST-01a: Federation endpoint statistics table ───────────────────────
-- Updated after each successful or failed federation HTTP call.
-- Written by update_federation_stats() in src/sparql/federation.rs.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_stats (
    endpoint_url        TEXT         PRIMARY KEY,
    call_count          BIGINT       NOT NULL DEFAULT 0,
    error_count         BIGINT       NOT NULL DEFAULT 0,
    total_latency_ms    FLOAT8       NOT NULL DEFAULT 0,
    max_latency_ms      FLOAT8       NOT NULL DEFAULT 0,
    p50_ms              FLOAT8,
    p95_ms              FLOAT8,
    row_estimate        BIGINT       NOT NULL DEFAULT 0,
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.federation_stats IS
    'Per-endpoint federation call statistics. p50_ms ≈ running average latency; p95_ms ≈ max latency. '
    'Updated after every federation HTTP call. Used by the cost model for SERVICE clause planning. '
    '(v0.82.0 FED-COST-01)';

-- ── STATS-CACHE-01a: Predicate stats cache table ─────────────────────────────
-- Materialised cache of per-predicate triple counts to avoid scanning VP tables
-- on every graph_stats() call under high concurrency.
-- Refreshed by pg_ripple.refresh_stats_cache() and by the merge background worker.
CREATE TABLE IF NOT EXISTS _pg_ripple.predicate_stats_cache (
    predicate_id    BIGINT       PRIMARY KEY,
    triple_count    BIGINT       NOT NULL DEFAULT 0,
    refreshed_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

COMMENT ON TABLE _pg_ripple.predicate_stats_cache IS
    'Materialised per-predicate triple counts, updated by pg_ripple.refresh_stats_cache() '
    'and by the background merge worker every pg_ripple.stats_refresh_interval_seconds seconds. '
    'Avoids VP-table scans on every graph_stats() call under high concurrency. '
    '(v0.82.0 STATS-CACHE-01)';
