//! GUC registration for storage and HTAP (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

unsafe extern "C-unwind" fn check_embedding_index_type(
    newval: *mut *mut std::ffi::c_char,
    _extra: *mut *mut std::ffi::c_void,
    _source: pgrx::pg_sys::GucSource::Type,
) -> bool {
    if newval.is_null() {
        return true;
    }
    // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
    // the duration of this call and the string has at least a NUL terminator.
    let s = unsafe {
        if (*newval).is_null() {
            return true;
        }
        std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
    };
    matches!(s, "hnsw" | "ivfflat")
}

/// Validate `embedding_precision`: `single`, `half`, or `binary`.
#[pg_guard]
unsafe extern "C-unwind" fn check_embedding_precision(
    newval: *mut *mut std::ffi::c_char,
    _extra: *mut *mut std::ffi::c_void,
    _source: pgrx::pg_sys::GucSource::Type,
) -> bool {
    if newval.is_null() {
        return true;
    }
    // SAFETY: newval is a GUC check-hook argument; the pointer is valid for
    // the duration of this call and the string has at least a NUL terminator.
    let s = unsafe {
        if (*newval).is_null() {
            return true;
        }
        std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
    };
    matches!(s, "single" | "half" | "binary")
}

/// v0.55.0 H-2: Assign hook for `pg_ripple.llm_api_key_env`.
///
/// Emits a WARNING if the value looks like a raw API key (i.e., contains
/// non-identifier characters such as hyphens, dots, slashes, or lowercase
/// letters mixed with digits) rather than an environment-variable name.
/// Environment variable names are conventionally ALL_CAPS with underscores.
/// Register all GUCs for this domain.
pub fn register() {
    // ── v0.6.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_threshold",
        c"Minimum rows in a delta table before triggering a background merge (default: 10000)",
        c"",
        &MERGE_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_interval_secs",
        c"Maximum seconds between merge worker polling cycles (default: 60)",
        c"",
        &MERGE_INTERVAL_SECS,
        1,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_retention_seconds",
        c"Seconds to keep the previous main table after a merge before dropping it (default: 60)",
        c"",
        &MERGE_RETENTION_SECONDS,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.latch_trigger_threshold",
        c"Rows written in one batch before poking the merge worker latch (default: 10000)",
        c"",
        &LATCH_TRIGGER_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.worker_database",
        c"Database the background merge worker connects to (default: postgres)",
        c"",
        &WORKER_DATABASE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_watchdog_timeout",
        c"Seconds of merge worker inactivity before a WARNING is logged (default: 300)",
        c"",
        &MERGE_WATCHDOG_TIMEOUT,
        10,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.27.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_string_guc(
    c"pg_ripple.embedding_model",
    c"Embedding model name tag (e.g. 'text-embedding-3-small'); stored in the model column of _pg_ripple.embeddings",
    c"",
    &EMBEDDING_MODEL,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_int_guc(
    c"pg_ripple.embedding_dimensions",
    c"Vector dimension count; must match the actual model output (default: 1536, range: 1-16000)",
    c"",
    &EMBEDDING_DIMENSIONS,
    1,
    16_000,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_api_url",
        c"Base URL for an OpenAI-compatible embedding API (e.g. https://api.openai.com/v1)",
        c"",
        &EMBEDDING_API_URL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_api_key",
        c"API key for the embedding endpoint (superuser-only; masked in pg_settings)",
        c"",
        &EMBEDDING_API_KEY,
        GucContext::Suset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
    c"pg_ripple.pgvector_enabled",
    c"When off, disable all pgvector-dependent code paths without uninstalling the extension (default: on)",
    c"",
    &PGVECTOR_ENABLED,
    GucContext::Userset,
    GucFlags::default(),
);

    // v0.47.0: validated embedding_index_type and embedding_precision
    // SAFETY: define_string_guc_with_hooks requires an unsafe block;
    // the hook function pointers are valid extern "C" function pointers.
    unsafe {
        pgrx::GucRegistry::define_string_guc_with_hooks(
        c"pg_ripple.embedding_index_type",
        c"Index type on _pg_ripple.embeddings: 'hnsw' (default) or 'ivfflat'; changing requires REINDEX",
        c"",
        &EMBEDDING_INDEX_TYPE,
        GucContext::Userset,
        GucFlags::default(),
        Some(check_embedding_index_type),
        None,
        None,
    );

        pgrx::GucRegistry::define_string_guc_with_hooks(
        c"pg_ripple.embedding_precision",
        c"Embedding storage precision: 'single' (default, vector(N)), 'half' (halfvec(N), -50% storage), 'binary' (bit(N), -96% storage); requires pgvector >= 0.7.0",
        c"",
        &EMBEDDING_PRECISION,
        GucContext::Userset,
        GucFlags::default(),
        Some(check_embedding_precision),
        None,
        None,
    );
    }

    // ── v0.28.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
    c"pg_ripple.auto_embed",
    c"When on, a trigger on _pg_ripple.dictionary enqueues new entity IDs for automatic embedding (default: off)",
    c"",
    &AUTO_EMBED,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_int_guc(
    c"pg_ripple.embedding_batch_size",
    c"Number of entities dequeued and embedded per background worker batch (default: 100, range: 1–10000)",
    c"",
    &EMBEDDING_BATCH_SIZE,
    1,
    10_000,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_bool_guc(
    c"pg_ripple.use_graph_context",
    c"When on, embed_entities() serializes each entity's RDF neighborhood for richer vectors (default: off)",
    c"",
    &USE_GRAPH_CONTEXT,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_int_guc(
    c"pg_ripple.vector_federation_timeout_ms",
    c"HTTP timeout in milliseconds for external vector service endpoint calls (default: 5000, range: 100–300000)",
    c"",
    &VECTOR_FEDERATION_TIMEOUT_MS,
    100,
    300_000,
    GucContext::Userset,
    GucFlags::default(),
);

    // ── v0.34.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
    c"pg_ripple.datalog_max_depth",
    c"Maximum depth for bounded-depth Datalog fixpoint termination; 0 = unlimited (default: 0, min: 0, max: 100000) (v0.34.0)",
    c"",
    &DATALOG_MAX_DEPTH,
    0,
    100_000,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.dred_enabled",
        c"When on (default), deleting a base triple uses DRed incremental retraction \
      to surgically remove only affected derived facts; off falls back to full \
      re-materialization (v0.34.0)",
        c"",
        &DRED_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.dred_batch_size",
        c"Maximum number of deleted base triples processed in a single DRed \
      transaction (default: 1000, min: 1, max: 1000000) (v0.34.0)",
        c"",
        &DRED_BATCH_SIZE,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.35.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_parallel_workers",
        c"Maximum parallel worker count for Datalog stratum evaluation; 1 = serial \
      (default: 4, min: 1, max: 32) (v0.35.0)",
        c"",
        &DATALOG_PARALLEL_WORKERS,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_parallel_threshold",
        c"Minimum estimated total-row count for a stratum before parallel group \
      analysis is applied (default: 10000, min: 0) (v0.35.0)",
        c"",
        &DATALOG_PARALLEL_THRESHOLD,
        0,
        100_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.37.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.tombstone_gc_enabled",
        c"When on (default), automatically VACUUM VP tombstone tables after merge \
      when the tombstone/main ratio exceeds tombstone_gc_threshold (v0.37.0)",
        c"",
        &TOMBSTONE_GC_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.tombstone_gc_threshold",
        c"Tombstone-to-main-row ratio that triggers automatic VACUUM after merge \
      (default: '0.05' = 5%; accepts a decimal string, range: 0.0–1.0) (v0.37.0)",
        c"",
        &TOMBSTONE_GC_THRESHOLD_STR,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.38.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.predicate_cache_enabled",
        c"When on (default), cache VP table OID lookups per backend so repeated \
      SPARQL queries on the same predicates avoid SPI catalog round-trips \
      (v0.38.0)",
        c"",
        &PREDICATE_CACHE_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.42.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sameas_max_cluster_size",
        c"Maximum owl:sameAs equivalence-class size before emitting PT550 WARNING and \
      switching to sampling approximation. 0 = disabled (v0.42.0)",
        c"",
        &SAMEAS_MAX_CLUSTER_SIZE,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_stats_ttl_secs",
        c"TTL in seconds for cached VoID statistics per federation endpoint. \
      0 = disabled (v0.42.0)",
        c"",
        &FEDERATION_STATS_TTL_SECS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_planner_enabled",
        c"Enable cost-based FedX-style federation source selection using VoID statistics. \
      On by default (v0.42.0)",
        c"",
        &FEDERATION_PLANNER_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_parallel_max",
        c"Maximum number of parallel SERVICE clause workers for independent atoms. \
      Default: 4 (v0.42.0)",
        c"",
        &FEDERATION_PARALLEL_MAX,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_parallel_timeout",
        c"Wall-clock timeout in seconds for parallel federation workers. \
      Default: 60 (v0.42.0)",
        c"",
        &FEDERATION_PARALLEL_TIMEOUT,
        1,
        3600,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_inline_max_rows",
        c"SERVICE responses exceeding this row count are spooled to a temp table \
      instead of VALUES clause inline. Emits PT620 INFO. Default: 10000 (v0.42.0)",
        c"",
        &FEDERATION_INLINE_MAX_ROWS,
        0,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_allow_private",
        c"Allow federation endpoints with RFC-1918/loopback/link-local IP addresses. \
      Off by default (PT621 emitted when rejected). (v0.42.0)",
        c"",
        &FEDERATION_ALLOW_PRIVATE,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.52.0 GUCs — pg-trickle Relay Integration ───────────────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.cdc_bridge_enabled",
        c"Enable the CDC → pg-trickle outbox bridge worker (default: off). \
      Requires pg-trickle to be installed. (v0.52.0)",
        c"",
        &CDC_BRIDGE_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_bridge_batch_size",
        c"Maximum CDC notifications batched before a bridge worker flush \
      (default: 100, min: 1, max: 10000). (v0.52.0)",
        c"",
        &CDC_BRIDGE_BATCH_SIZE,
        1,
        10_000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_bridge_flush_ms",
        c"Maximum milliseconds between bridge worker flush cycles \
      (default: 200, min: 10, max: 60000). (v0.52.0)",
        c"",
        &CDC_BRIDGE_FLUSH_MS,
        10,
        60_000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.cdc_bridge_outbox_table",
        c"Target outbox table for the CDC bridge worker (default: 'enriched_events'). \
      Must have (event_id TEXT, payload JSONB) columns. (v0.52.0)",
        c"",
        &CDC_BRIDGE_OUTBOX_TABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.trickle_integration",
        c"Enable pg-trickle integration features; set off to disable bridge even \
      when pg-trickle is installed (default: on). (v0.52.0)",
        c"",
        &TRICKLE_INTEGRATION,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.54.0 GUCs — High Availability & Logical Replication ───────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.replication_enabled",
        c"Enable the RDF logical replication consumer (logical_apply_worker). \
      When on, a background worker subscribes to the pg_ripple_pub publication \
      and applies N-Triples batches to the local store (default: off). (v0.54.0)",
        c"",
        &REPLICATION_ENABLED,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.replication_conflict_strategy",
        c"Conflict resolution strategy for the logical apply worker: \
      'last_writer_wins' (default) — keeps the row with the highest SID. (v0.54.0)",
        c"",
        &REPLICATION_CONFLICT_STRATEGY,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // ── v0.58.0 GUCs — Citus sharding, merge fence, PROV-O ──────────────────
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_sharding_enabled",
        c"Enable Citus horizontal sharding of VP tables on predicate promotion. \
      Requires the Citus extension. Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::CITUS_SHARDING_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_trickle_compat",
        c"When on, VP delta tables use colocate_with='none' for pg-trickle CDC compatibility. \
      Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::CITUS_TRICKLE_COMPAT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
    c"pg_ripple.merge_fence_timeout_ms",
    c"Milliseconds the merge worker waits for an advisory fence lock during Citus rebalancing. \
      0 = no fence. (v0.58.0)",
    c"",
    &crate::gucs::storage::MERGE_FENCE_TIMEOUT_MS,
    0,
    300_000,
    GucContext::Userset,
    GucFlags::default(),
);

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.prov_enabled",
        c"Emit PROV-O provenance triples for all bulk ingest operations. Default off. (v0.58.0)",
        c"",
        &crate::gucs::storage::PROV_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.62.0 GUCs — Arrow Flight, Citus scalability ──────────────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.dictionary_tier_threshold",
        c"Dictionary tier threshold for Citus cold-entry offload (v0.62.0). \
      Terms with access_count < N are eligible for cold tier. \
      -1 = disabled (default); only active when citus_sharding_enabled = on.",
        c"",
        &crate::gucs::storage::DICTIONARY_TIER_THRESHOLD,
        -1,
        1_000_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.citus_prune_carry_max",
        c"Maximum carry-forward set size for multi-hop shard pruning (v0.62.0 CITUS-29). \
      Above this threshold, falls back to full fan-out. Default 1000.",
        c"",
        &crate::gucs::storage::CITUS_PRUNE_CARRY_MAX,
        0,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.66.0 Arrow Flight GUCs (FLIGHT-01).
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.arrow_flight_secret",
        c"HMAC-SHA256 secret for signing Arrow Flight tickets (v0.66.0 FLIGHT-01). \
      Empty = unsigned tickets (rejected by default in pg_ripple_http).",
        c"",
        &crate::gucs::storage::ARROW_FLIGHT_SECRET,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.arrow_flight_expiry_secs",
        c"Arrow Flight ticket validity in seconds (v0.66.0 FLIGHT-01). Default: 3600.",
        c"",
        &crate::gucs::storage::ARROW_FLIGHT_EXPIRY_SECS,
        60,
        86400,
        GucContext::Userset,
        GucFlags::default(),
    );

    // v0.67.0 Arrow Flight GUCs (FLIGHT-SEC-01, FLIGHT-SEC-02).
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.arrow_unsigned_tickets_allowed",
        c"When on, unsigned Arrow Flight tickets (sig=\"unsigned\") are accepted for \
      local development. Default off — production must use a signed secret. \
      (v0.67.0 FLIGHT-SEC-01)",
        c"",
        &crate::gucs::storage::ARROW_UNSIGNED_TICKETS_ALLOWED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.arrow_batch_size",
        c"Number of rows per Arrow record batch when streaming export (v0.67.0 FLIGHT-SEC-02). \
      Default: 1000.",
        c"",
        &crate::gucs::storage::ARROW_BATCH_SIZE,
        1,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.68.0 Citus/scalability GUCs ───────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.approx_distinct",
        c"When on, route SPARQL COUNT(DISTINCT …) through Citus HLL when available. \
      Falls back to exact COUNT(DISTINCT …) when hll extension is absent. \
      Default off. (v0.68.0 CITUS-HLL-01)",
        c"",
        &crate::gucs::storage::APPROX_DISTINCT,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.citus_service_pruning",
        c"When on, the SPARQL federation translator rewrites SERVICE subqueries targeting \
      Citus workers to include shard-constraint annotations to prune irrelevant shards. \
      Default off. (v0.68.0 CITUS-SVC-01)",
        c"",
        &crate::gucs::storage::CITUS_SERVICE_PRUNING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vp_promotion_batch_size",
        c"Batch size for nonblocking VP promotion background copy phase. \
      Number of rows copied from vp_rare to shadow tables per iteration. \
      Default: 10000. (v0.68.0 PROMO-01)",
        c"",
        &crate::gucs::storage::VP_PROMOTION_BATCH_SIZE,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.datalog_citus_dispatch",
        c"When on, wrap Datalog stratum-iteration INSERT…SELECT in \
      run_command_on_all_nodes for parallel worker execution (v0.62.0 CITUS-27). \
      Requires citus_sharding_enabled = on. Default off.",
        c"",
        &crate::gucs::datalog::DATALOG_CITUS_DISPATCH,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.81.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.strict_dictionary",
        c"When on, decode() returns an error for missing dictionary IDs instead of \
      the _unknown_<id> placeholder string. Useful for strict data-quality contexts. \
      Default off. (v0.81.0 DICT-STRICT-01)",
        c"",
        &crate::gucs::storage::STRICT_DICTIONARY,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.strict_sparql_filters",
        c"When on, unknown built-in function names in FILTER expressions raise \
      ERROR (PT422) rather than evaluating to UNDEF. Default off. \
      (v0.81.0 FILTER-STRICT-01)",
        c"",
        &crate::gucs::sparql::STRICT_SPARQL_FILTERS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cdc_slot_idle_timeout_seconds",
        c"Seconds of no LSN advance before the CDC slot cleanup worker drops an \
      orphaned replication slot. Default: 3600. (v0.81.0 CDC-SLOT-01)",
        c"",
        &crate::gucs::storage::CDC_SLOT_IDLE_TIMEOUT_SECONDS,
        60,
        86400,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.82.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.plan_cache_capacity",
        c"Maximum number of cached SPARQL-to-SQL plan translations (default: 1024, range: 64–65536). \
      Replaces the hardcoded constant in plan_cache.rs. (v0.82.0 CACHE-CAP-01)",
        c"",
        &crate::gucs::sparql::PLAN_CACHE_CAPACITY,
        64,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_lock_timeout_ms",
        c"Milliseconds to wait for the merge fence lock before skipping this cycle \
      (default: 5000, range: 100–60000). (v0.82.0 MERGE-LOCK-GUC-01)",
        c"",
        &crate::gucs::storage::MERGE_LOCK_TIMEOUT_MS,
        100,
        60000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_heartbeat_interval_seconds",
        c"Seconds between merge worker heartbeat log lines (default: 60, range: 10–3600). \
      (v0.82.0 MERGE-HBEAT-01)",
        c"",
        &crate::gucs::storage::MERGE_HEARTBEAT_INTERVAL_SECONDS,
        10,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.stats_scan_limit",
        c"Maximum number of VP tables scanned per graph_stats() call \
      (default: 1000, range: 1–100000). (v0.82.0 STATS-DOC-01)",
        c"",
        &crate::gucs::storage::STATS_SCAN_LIMIT,
        1,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.stats_refresh_interval_seconds",
        c"Seconds between background refreshes of predicate_stats_cache \
      (default: 300, range: 10–86400). (v0.82.0 STATS-CACHE-01)",
        c"",
        &crate::gucs::storage::STATS_REFRESH_INTERVAL_SECONDS,
        10,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vacuum_dict_batch_size",
        c"Number of predicates processed per batch in vacuum_dictionary() \
      (default: 200, range: 10–10000). (v0.82.0 VACUUM-DICT-BATCH-01)",
        c"",
        &crate::gucs::storage::VACUUM_DICT_BATCH_SIZE,
        10,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.all_nodes_predicate_limit",
        c"Maximum number of predicates in a wildcard property-path UNION ALL expansion \
      (default: 500, range: 10–50000). Excess predicates sorted by triple count and truncated. \
      (v0.82.0 PROPPATH-UNBOUNDED-01)",
        c"",
        &crate::gucs::sparql::ALL_NODES_PREDICATE_LIMIT,
        10,
        50000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // GUC-BOUNDS-01 (v0.82.0): merge_batch_size — controls merge worker batch size.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_batch_size",
        c"Maximum rows processed per merge worker INSERT…SELECT batch (default: 1000000, \
          range: 100–100,000,000). (v0.82.0 GUC-BOUNDS-01)",
        c"",
        &crate::gucs::storage::MERGE_BATCH_SIZE,
        100,
        100_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // MERGE-BACKOFF-01 (v0.83.0): merge worker exponential backoff cap.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_max_backoff_secs",
        c"Maximum backoff duration in seconds for the merge worker exponential backoff \
      after consecutive errors (default: same as merge_interval_secs=60, range: 10–3600). \
      First error doubles the wait; each subsequent error doubles again; capped here. \
      (v0.83.0 MERGE-BACKOFF-01)",
        c"",
        &crate::gucs::storage::MERGE_MAX_BACKOFF_SECS,
        10,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // PGC_POSTMASTER GUCs can only be registered during shared_preload_libraries
    // loading.  `process_shared_preload_libraries_in_progress` is the correct
    // flag — `IsPostmasterEnvironment` is true in every server process and
    // cannot be used to distinguish this case.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.dictionary_cache_size",
            c"Shared-memory encode-cache capacity in entries (default: 4096; startup only; range: 1024–1,073,741,824)",
            c"",
            &DICTIONARY_CACHE_SIZE,
            1024,
            1_073_741_824,
            GucContext::Postmaster,
            GucFlags::default(),
        );

        pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cache_budget",
        c"Shared-memory budget cap in MB; bulk loads throttle when >90% utilised (default: 64; startup only)",
        c"",
        &CACHE_BUDGET_MB,
        0,
        65536,
        GucContext::Postmaster,
        GucFlags::default(),
    );

        pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_workers",
        c"Number of parallel background merge worker processes (default: 1, max: 16; startup only). \
          Each worker handles a round-robin subset of VP table predicates (v0.42.0)",
        c"",
        &MERGE_WORKERS,
        1,
        16,
        GucContext::Postmaster,
        GucFlags::default(),
    );

        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.audit_retention",
            c"Retention period in days for _pg_ripple.event_audit rows (v0.78.0). \
          0 disables automatic pruning.",
            c"",
            &crate::gucs::observability::AUDIT_RETENTION_DAYS,
            0,
            3650,
            GucContext::Suset,
            GucFlags::default(),
        );
    }

    // C13-11 (v0.85.0): DESCRIBE CBD recursion depth cap.
    // NOTE: Userset GUCs must be registered unconditionally, not inside the
    // process_shared_preload_libraries_in_progress block (which only runs at
    // postmaster startup). Placing Userset GUCs in that block would cause
    // "unrecognized configuration parameter" errors in regular sessions.
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.describe_max_depth",
        c"Maximum recursion depth for DESCRIBE CBD traversal. \
          Prevents runaway traversal on cyclic or deep graphs (v0.85.0 C13-11). \
          Default: 16. Range: 1–256.",
        c"",
        &crate::gucs::storage::DESCRIBE_MAX_DEPTH,
        1,
        256,
        GucContext::Userset,
        GucFlags::default(),
    );
}
