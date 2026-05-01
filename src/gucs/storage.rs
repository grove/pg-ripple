//! GUC parameters for the storage subsystem (VP tables, HTAP, merge worker,
//! dictionary cache, CDC bridge, and misc storage knobs).

/// GUC: default named-graph identifier (empty string → default graph 0).
pub static DEFAULT_GRAPH: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: minimum triple count before a rare predicate gets its own VP table.
pub static VPP_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: when true, add a `(g, s, o)` index to every dedicated VP table for
/// fast named-graph–scoped queries.  Off by default to avoid index bloat.
pub static NAMED_GRAPH_OPTIMIZED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.6.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: minimum rows in a delta table before triggering a merge.
pub static MERGE_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: maximum seconds between merge worker polling intervals.
pub static MERGE_INTERVAL_SECS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: seconds to keep the old main table after a merge before dropping it.
pub static MERGE_RETENTION_SECONDS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: number of rows written in one batch before poking the merge worker.
pub static LATCH_TRIGGER_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: database the merge background worker connects to.
pub static WORKER_DATABASE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: seconds before the merge worker watchdog logs a WARNING for inactivity.
pub static MERGE_WATCHDOG_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(300);

// ─── v0.7.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: when true, the HTAP generation merge deduplicates `(s, o, g)` rows
/// using DISTINCT ON, keeping the row with the lowest SID.
pub static DEDUP_ON_MERGE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum number of entries in the shared-memory dictionary encode cache.
pub static DICTIONARY_CACHE_SIZE: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(crate::shmem::ENCODE_CACHE_CAPACITY as i32);

/// GUC: shared-memory budget cap in megabytes.
pub static CACHE_BUDGET_MB: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

// ─── v0.14.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: superuser override to bypass graph-level Row-Level Security policies.
pub static RLS_BYPASS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.24.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: when `on` (default), the background merge worker runs `ANALYZE` on
/// each VP main table immediately after a successful merge cycle.
pub static AUTO_ANALYZE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: number of triples fetched per cursor batch when streaming export.
pub static EXPORT_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.37.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: enable automatic tombstone VACUUM after merge cycles (v0.37.0).
pub static TOMBSTONE_GC_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: tombstone/main ratio threshold for triggering VACUUM (stored as string, v0.37.0).
pub static TOMBSTONE_GC_THRESHOLD_STR: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.38.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: enable the backend-local predicate OID cache (v0.38.0).
pub static PREDICATE_CACHE_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.42.0 storage GUCs ─────────────────────────────────────────────────────

/// GUC: number of background merge worker processes (v0.42.0).
pub static MERGE_WORKERS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1);

// ─── v0.52.0 CDC bridge GUCs ─────────────────────────────────────────────────

/// GUC: master switch for the CDC → pg-trickle outbox bridge worker (v0.52.0).
pub static CDC_BRIDGE_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum number of CDC notifications batched before a flush (v0.52.0).
pub static CDC_BRIDGE_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: maximum milliseconds between bridge worker flush cycles (v0.52.0).
pub static CDC_BRIDGE_FLUSH_MS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(200);

/// GUC: outbox table that the CDC bridge worker writes JSON-LD events to (v0.52.0).
pub static CDC_BRIDGE_OUTBOX_TABLE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: master switch for pg-trickle integration features (v0.52.0).
pub static TRICKLE_INTEGRATION: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.54.0 logical replication GUCs ────────────────────────────────────────

/// GUC: enable the RDF logical replication consumer worker (v0.54.0).
pub static REPLICATION_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: conflict resolution strategy for the logical apply worker (v0.54.0).
/// Values: `last_writer_wins` (default).
pub static REPLICATION_CONFLICT_STRATEGY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.55.0 storage and security GUCs ───────────────────────────────────────

/// GUC: number of seconds to retain tombstones after a merge cycle (v0.55.0).
/// When 0 (default), tombstones are truncated immediately after a merge cycle
/// that consumes all tombstones for a predicate.
pub static TOMBSTONE_RETENTION_SECONDS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: when on (default), normalize IRI strings to NFC before dictionary encoding (v0.55.0).
/// Ensures that semantically equivalent IRIs differing only in Unicode normalization
/// form map to the same dictionary entry.
pub static NORMALIZE_IRIS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: comma-separated list of allowed path prefixes for copy_rdf_from() (v0.55.0).
/// When set, copy_rdf_from() rejects paths that do not start with one of the listed
/// prefixes (PT403).  When NULL/empty, superusers bypass the check; non-superusers
/// are always restricted to this list.
pub static COPY_RDF_ALLOWED_PATHS: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: DSN for the read replica to route read-only SPARQL queries to (v0.55.0).
/// When set, SELECT/CONSTRUCT/ASK/DESCRIBE queries are routed to this replica.
/// Falls back to primary on connection failure (PT530 WARNING emitted).
pub static READ_REPLICA_DSN: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.82.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: merge fence lock timeout in milliseconds (v0.82.0 MERGE-LOCK-GUC-01).
/// Replaces the hardcoded `SET LOCAL lock_timeout = '5s'` in merge.rs.
pub static MERGE_LOCK_TIMEOUT_MS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(5000);

/// GUC: seconds between merge worker heartbeat log lines (v0.82.0 MERGE-HBEAT-01).
pub static MERGE_HEARTBEAT_INTERVAL_SECONDS: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(60);

/// GUC: maximum number of VP tables scanned per `graph_stats()` call (v0.82.0 STATS-DOC-01).
pub static STATS_SCAN_LIMIT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: seconds between background refreshes of `_pg_ripple.predicate_stats_cache`
/// (v0.82.0 STATS-CACHE-01).
pub static STATS_REFRESH_INTERVAL_SECONDS: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(300);

/// GUC: batch size for `vacuum_dictionary()` UNION ALL construction (v0.82.0 VACUUM-DICT-BATCH-01).
pub static VACUUM_DICT_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(200);

// ─── v0.57.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: triple count threshold above which the HTAP merge converts vp_{id}_main
/// from heap to columnar storage (via pg_columnar). -1 = disabled (default). (v0.57.0)
pub static COLUMNAR_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(-1);

/// GUC: enable automatic adaptive index creation based on query access patterns (v0.57.0).
pub static ADAPTIVE_INDEXING_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.58.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: enable Citus horizontal sharding of VP tables (v0.58.0).
/// When on, new VP tables get `REPLICA IDENTITY FULL` + `create_distributed_table(s)`.
pub static CITUS_SHARDING_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: when on, create_distributed_table uses `colocate_with = 'none'` for
/// pg-trickle / CDC compatibility — prevents cross-shard tombstone deletes (v0.58.0).
pub static CITUS_TRICKLE_COMPAT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: milliseconds the merge worker waits for an advisory fence lock before
/// proceeding with a merge cycle during Citus rebalancing (v0.58.0).
/// 0 = no fence (default non-Citus behaviour).
pub static MERGE_FENCE_TIMEOUT_MS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: when on, emit PROV-O provenance triples for all ingest operations (v0.58.0).
pub static PROV_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.62.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: dictionary tier threshold for Citus cold-entry offload (v0.62.0).
/// Terms with access_count < N are eligible for the cold tier.
/// -1 = disabled (default); only active when citus_sharding_enabled = on.
pub static DICTIONARY_TIER_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(-1);

/// GUC: maximum number of subject IDs to carry forward for multi-hop shard
/// pruning (v0.62.0 CITUS-29). Above this threshold, falls back to full fan-out.
pub static CITUS_PRUNE_CARRY_MAX: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ─── v0.66.0 Arrow Flight GUCs ───────────────────────────────────────────────

/// GUC: HMAC-SHA256 secret for signing Arrow Flight tickets (v0.66.0 FLIGHT-01).
/// Empty string = tickets are unsigned (rejected by default in pg_ripple_http).
/// Set to a long random value in production.
pub static ARROW_FLIGHT_SECRET: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: Arrow Flight ticket validity in seconds (v0.66.0 FLIGHT-01).
/// Default: 3600 (1 hour).
pub static ARROW_FLIGHT_EXPIRY_SECS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3600);

// ─── v0.67.0 Arrow Flight GUCs ───────────────────────────────────────────────

/// GUC: when `on`, unsigned Arrow Flight tickets (sig = "unsigned") are accepted
/// for local development. Default `off` — production must have a signed secret.
/// (v0.67.0 FLIGHT-SEC-01)
pub static ARROW_UNSIGNED_TICKETS_ALLOWED: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(false);

/// GUC: number of rows per Arrow record batch when streaming export.
/// Default: 1000. (v0.67.0 FLIGHT-SEC-02)
pub static ARROW_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ─── v0.68.0 Citus/scalability GUCs ─────────────────────────────────────────

/// GUC: when `on`, route SPARQL `COUNT(DISTINCT …)` aggregates through
/// Citus HLL (hll_add_agg) when the `hll` extension is available.
/// Provides approximate but highly scalable distinct counts on distributed VP
/// tables.  Falls back to exact `COUNT(DISTINCT …)` when HLL is absent or
/// when this GUC is `off`.  Default: `off`. (v0.68.0 CITUS-HLL-01)
pub static APPROX_DISTINCT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: when `on`, the SPARQL federation translator rewrites SERVICE subqueries
/// targeting Citus workers to include shard-constraint annotations, pruning
/// irrelevant shards.  Automatically set to `on` when Citus is detected unless
/// overridden.  Default: `off`. (v0.68.0 CITUS-SVC-01)
pub static CITUS_SERVICE_PRUNING: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: batch size for background VP promotion copy phase. Number of rows
/// copied from vp_rare to the shadow tables per iteration.  Default: 10000.
/// (v0.68.0 PROMO-01)
pub static VP_PROMOTION_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.81.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: when `on`, `decode()` returns an error for missing dictionary IDs instead
/// of the `_unknown_<id>` placeholder string. Useful for strict data-quality
/// contexts where missing entries indicate incomplete bulk loads. Default: `off`.
/// (v0.81.0 DICT-STRICT-01)
pub static STRICT_DICTIONARY: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: seconds of no LSN advance before the CDC slot cleanup worker drops an
/// orphaned replication slot. Default: 3600. (v0.81.0 CDC-SLOT-01)
pub static CDC_SLOT_IDLE_TIMEOUT_SECONDS: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(3600);

// ─── v0.82.0 storage GUCs ────────────────────────────────────────────────────

/// GUC: maximum number of rows the merge worker processes in a single INSERT…SELECT
/// batch. Allows tuning merge pressure vs. transaction duration.
/// Default: 1,000,000. Min: 100. Max: 100,000,000. (v0.82.0 GUC-BOUNDS-01)
pub static MERGE_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1_000_000);
