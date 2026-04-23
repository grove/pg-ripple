//! GUC (Grand Unified Configuration) parameter declarations for pg_ripple.
//!
//! All statics are `pub` so that `lib.rs` can re-export them with
//! `pub(crate) use gucs::*;` and callers can refer to them as `crate::SOME_GUC`.

// ─── GUC parameters ───────────────────────────────────────────────────────────

/// GUC: default named-graph identifier (empty string → default graph 0).
pub static DEFAULT_GRAPH: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: minimum triple count before a rare predicate gets its own VP table.
pub static VPP_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: when true, add a `(g, s, o)` index to every dedicated VP table for
/// fast named-graph–scoped queries.  Off by default to avoid index bloat.
pub static NAMED_GRAPH_OPTIMIZED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum number of cached SPARQL→SQL plan translations per backend.
/// Set to 0 to disable the plan cache.
pub static PLAN_CACHE_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(256);

/// GUC: maximum recursion depth for SPARQL property path queries (`+`, `*`).
/// Prevents runaway recursive CTEs on cyclic or very deep graphs.
pub static MAX_PATH_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: DESCRIBE algorithm — 'cbd' (Concise Bounded Description, default),
/// 'scbd' (Symmetric CBD, includes incoming arcs), 'simple' (one-hop only).
pub static DESCRIBE_STRATEGY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.6.0 GUCs ─────────────────────────────────────────────────────────────

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

// ─── v0.7.0 GUCs ─────────────────────────────────────────────────────────────

/// GUC: SHACL validation mode — 'off', 'sync', or 'async'.
/// 'sync' rejects violating triples inline; 'async' queues them for the
/// background validation worker; 'off' disables all SHACL enforcement.
pub static SHACL_MODE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: when true, the HTAP generation merge deduplicates `(s, o, g)` rows
/// using DISTINCT ON, keeping the row with the lowest SID.
/// Zero insert-time overhead; effective after the next merge cycle.
pub static DEDUP_ON_MERGE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum number of entries in the shared-memory dictionary encode cache.
/// Rounded down to the nearest multiple of `ENCODE_CACHE_CAPACITY` (4096).
/// This is a startup-only GUC (read at `_PG_init`); changing it requires a
/// PostgreSQL restart.
///
/// The shared-memory cache is split across 4 shards of 1024 slots each.
/// This GUC documents the effective size; the actual shard sizes are compiled
/// in at build time.  Set to 0 to note that only the backend-local cache is active.
pub static DICTIONARY_CACHE_SIZE: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(crate::shmem::ENCODE_CACHE_CAPACITY as i32);

/// GUC: shared-memory budget cap in megabytes.
///
/// Bulk loads check the encode-cache utilization against this budget and
/// reduce their batch size when utilization exceeds 90% to prevent OOM.
/// Set to 0 to disable back-pressure.  Startup-only GUC.
pub static CACHE_BUDGET_MB: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

// ─── v0.10.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: Datalog inference execution mode.
/// 'off' — inference disabled.
/// 'on_demand' — derived predicates compiled as inline CTEs at query time.
/// 'materialized' — derived predicates materialised as pg_trickle stream tables.
pub static INFERENCE_MODE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: Datalog constraint enforcement mode.
/// 'off' — violations are detected but ignored.
/// 'warn' — log a WARNING for each violation.
/// 'error' — reject the transaction when a violation is detected.
pub static ENFORCE_CONSTRAINTS: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: graph scope for unscoped body atoms (atoms without GRAPH clause).
/// 'default' — match only g = 0 (the default graph); recommended.
/// 'all' — match triples in any graph; useful for ontology-level rules.
pub static RULE_GRAPH_SCOPE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.13.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: enable BGP join reordering based on pg_stats selectivity estimates.
/// When true, triple patterns in a BGP are reordered before SQL generation
/// so the most selective pattern (fewest estimated rows) is evaluated first.
/// Also emits `SET LOCAL join_collapse_limit = 1` and `enable_mergejoin = on`
/// before each SPARQL SELECT execution to lock the computed join order.
pub static BGP_REORDER: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum number of VP table joins in a query before trying to exploit
/// PostgreSQL parallel query workers.  Queries with fewer joins use serial plans.
pub static PARALLEL_QUERY_MIN_JOINS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3);

// ─── v0.14.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: superuser override to bypass graph-level Row-Level Security policies.
/// When `on`, the current session ignores graph_access restrictions.
/// Only effective for superusers — regular users cannot set this.
pub static RLS_BYPASS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ─── v0.16.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: per-SERVICE-call wall-clock timeout in seconds (default: 30).
/// When the remote endpoint does not respond within this window the call fails.
pub static FEDERATION_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(30);

/// GUC: maximum number of rows accepted from a single remote SERVICE call (default: 10,000).
/// Rows beyond this limit are silently dropped.
pub static FEDERATION_MAX_RESULTS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: behaviour when a SERVICE call fails.
/// `'warning'` (default) — emit a WARNING and return empty results.
/// `'error'` — raise an ERROR and abort the query.
/// `'empty'` — silently return empty results.
pub static FEDERATION_ON_ERROR: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.19.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: number of idle connections to keep per remote endpoint in the
/// thread-local ureq connection pool (default: 4, range: 1–32).
pub static FEDERATION_POOL_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: TTL in seconds for cached SERVICE results in `_pg_ripple.federation_cache`.
/// 0 (default) disables caching.  When > 0, successful remote results are cached
/// and reused for this many seconds before the remote endpoint is re-queried.
pub static FEDERATION_CACHE_TTL: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: behaviour when a SERVICE call delivers rows then fails.
/// `'empty'` (default) — discard all partial results, return empty.
/// `'use'` — use however many rows were received before the failure.
pub static FEDERATION_ON_PARTIAL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: when `on`, derive the effective per-endpoint timeout from P95 latency
/// observed in `_pg_ripple.federation_health` instead of using the fixed
/// `pg_ripple.federation_timeout` value (default: off).
pub static FEDERATION_ADAPTIVE_TIMEOUT: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(false);

/// Maximum body size in bytes for partial federation result recovery (H-13, v0.25.0).
pub static FEDERATION_PARTIAL_RECOVERY_MAX_BYTES: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(65_536);

// ─── v0.21.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: when `on` (default), a FILTER expression that uses an unsupported
/// SPARQL built-in function raises `ERRCODE_FEATURE_NOT_SUPPORTED` with a
/// message naming the function.  When `off`, the legacy warn-and-drop behaviour
/// is preserved for backward compatibility.
pub static SPARQL_STRICT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.24.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: maximum recursion depth for SPARQL property path queries (`+`, `*`).
/// Aligns with the v0.24.0 naming convention; equivalent to `max_path_depth`.
/// Default: 64 (conservative default to prevent runaway recursion).
pub static PROPERTY_PATH_MAX_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

/// GUC: when `on` (default), the background merge worker runs `ANALYZE` on
/// each VP main table immediately after a successful merge cycle, keeping
/// planner statistics current without requiring manual `VACUUM ANALYZE`.
/// Set `off` if you manage statistics manually.
pub static AUTO_ANALYZE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: number of triples fetched per cursor batch when streaming export
/// (Turtle / N-Triples / JSON-LD).  Peak memory is bounded by
/// `export_batch_size × average_triple_size` per export call.
pub static EXPORT_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.27.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: embedding model name tag stored in the `model` column of `_pg_ripple.embeddings`.
pub static EMBEDDING_MODEL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: vector dimension count; must match the actual model output (default: 1536).
pub static EMBEDDING_DIMENSIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1536);

/// GUC: base URL for an OpenAI-compatible embedding API
/// (e.g. `https://api.openai.com/v1`, local Ollama, vLLM).
pub static EMBEDDING_API_URL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: API key for the embedding endpoint.  Superuser-only; value is masked
/// in `pg_settings` via the `NOT_IN_SAMPLE` GUC flag.
pub static EMBEDDING_API_KEY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: runtime switch; set to `false` to disable all pgvector-dependent code
/// paths without uninstalling the extension (default: `true`).
pub static PGVECTOR_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: index type created on `_pg_ripple.embeddings` — `'hnsw'` (default)
/// or `'ivfflat'`.  Changing this requires `REINDEX`.
pub static EMBEDDING_INDEX_TYPE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: embedding storage precision — `'single'` (default, `vector(N)`),
/// `'half'` (`halfvec(N)`, 50% storage reduction), or `'binary'` (`bit(N)`,
/// ~96% storage reduction, Hamming distance).  Requires pgvector ≥ 0.7.0.
pub static EMBEDDING_PRECISION: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.28.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: master switch for trigger-based auto-embedding of new dictionary entries.
/// When `true`, a trigger on `_pg_ripple.dictionary` enqueues new entity IDs
/// for the background embedding worker.  Off by default to avoid surprise API charges.
pub static AUTO_EMBED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: number of entities dequeued and embedded per background worker batch.
pub static EMBEDDING_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: when `true`, `embed_entities()` serializes each entity's RDF neighborhood
/// before embedding instead of using only the IRI local name.
/// Produces richer vectors but requires a SPARQL query per entity.
pub static USE_GRAPH_CONTEXT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: HTTP timeout in milliseconds for calls to external vector service endpoints
/// registered via `pg_ripple.register_vector_endpoint()`.
pub static VECTOR_FEDERATION_TIMEOUT_MS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(5000);

// ─── v0.29.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: master switch for magic sets goal-directed inference (v0.29.0).
///
/// When `true` (default), `infer_goal()` uses a simplified magic sets
/// transformation to derive only facts relevant to the goal pattern.
/// When `false`, falls back to full materialization + post-hoc filtering.
pub static MAGIC_SETS: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: when `true` (default), sort Datalog rule body atoms by ascending estimated
/// VP-table cardinality before SQL compilation (cost-based join reordering, v0.29.0).
pub static DATALOG_COST_REORDER: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum VP-table row count for negated body atoms to use `LEFT JOIN … IS NULL`
/// anti-join form instead of `NOT EXISTS` (v0.29.0).  Default: 1000.
pub static DATALOG_ANTIJOIN_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: minimum semi-naive delta temp-table row count before creating a B-tree index
/// on `(s, o)` join columns prior to the next fixpoint iteration (v0.29.0).
/// Set to `0` to disable delta table indexing.  Default: 500.
pub static DELTA_INDEX_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(500);

// ─── v0.30.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: master switch for the Datalog rule plan cache (v0.30.0).
///
/// When `true` (default), `infer()`, `infer_with_stats()`, and `infer_agg()`
/// cache the compiled SQL for each rule set so that repeated calls on the same
/// rule set skip the parse + compile step.  Invalidated by `drop_rules()` and
/// `load_rules()`.
pub static RULE_PLAN_CACHE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of rule sets whose compiled SQL is kept in the plan cache
/// (v0.30.0).  When the cache is full, the entry with the fewest hits is evicted.
/// Default: 64.
pub static RULE_PLAN_CACHE_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

// ─── v0.31.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: master switch for `owl:sameAs` entity canonicalization (v0.31.0).
///
/// When `true` (default), the Datalog inference engine performs a pre-pass
/// before each fixpoint iteration that computes equivalence classes of
/// `owl:sameAs` triples and rewrites rule-body constants to their canonical
/// (lowest dictionary ID) representative.  Queries that reference non-canonical
/// entity IRIs are transparently redirected to the canonical form.
pub static SAMEAS_REASONING: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: master switch for demand transformation (v0.31.0).
///
/// When `true` (default), `create_datalog_view()` automatically applies demand
/// transformation when multiple goal patterns are specified.  The
/// `infer_demand()` function always applies demand filtering regardless of this
/// GUC.
pub static DEMAND_TRANSFORM: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ─── v0.32.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: safety cap on alternating fixpoint rounds for well-founded semantics (v0.32.0).
///
/// `pg_ripple.infer_wfs()` runs two fixpoint passes (positive closure + full
/// inference).  Each pass terminates early when no new facts are derived; this
/// GUC bounds the maximum iteration count per pass.  If either pass reaches the
/// limit without converging a WARNING with code PT520 is emitted and the partial
/// results are returned.
pub static WFS_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: master switch for the Datalog / SPARQL tabling cache (v0.32.0).
///
/// When `true` (default), `infer_wfs()` results and SPARQL query results are
/// cached in `_pg_ripple.tabling_cache` and reused on subsequent calls with the
/// same goal hash.  The cache is invalidated on any triple insert/delete or
/// `drop_rules()` call.
pub static TABLING: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: TTL in seconds for tabling cache entries (v0.32.0).
///
/// Entries older than this value are ignored on lookup and overwritten on the
/// next call.  Set to `0` to disable TTL-based expiry (entries survive until
/// explicit invalidation).  Default: `300` seconds (5 minutes).
pub static TABLING_TTL: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(300);

// ─── v0.34.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: maximum depth for bounded-depth Datalog fixpoint termination (v0.34.0).
///
/// When `> 0`, recursive CTEs compiled from Datalog rules include a depth counter
/// column that terminates the recursion when `depth >= datalog_max_depth`.  This
/// produces 20–50% speedups for bounded hierarchies (e.g. class hierarchies capped
/// at 5 levels by SHACL `sh:maxDepth` constraints).
///
/// `0` (default) — unlimited; the CYCLE clause provides cycle safety.
/// SPARQL property path queries also respect this bound when the path predicate
/// has a SHACL `sh:maxDepth` constraint.
pub static DATALOG_MAX_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: master switch for the Delete-Rederive (DRed) incremental retraction
/// algorithm (v0.34.0).
///
/// When `true` (default), deleting a base triple surgically retracts only the
/// affected derived facts and re-derives any that survive via alternative paths.
/// When `false`, falls back to full re-materialization on delete (safe but slow).
pub static DRED_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of deleted base triples to process in a single DRed
/// transaction (v0.34.0).
///
/// Batching prevents lock contention and transaction bloat when deleting many
/// triples at once.  Default: `1000`.
pub static DRED_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ─── v0.35.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: maximum number of parallel background workers for Datalog stratum
/// evaluation (v0.35.0).
///
/// Within a single stratum, rules deriving different predicates with no shared
/// body dependencies are independent and can execute concurrently.  This GUC
/// caps the concurrency at the given number.  Set to `1` (default) to use the
/// serial path.  Higher values enable parallelism analysis and group-aware
/// scheduling.  Maximum: `max_worker_processes - 3`.
pub static DATALOG_PARALLEL_WORKERS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: minimum estimated total row count for a stratum before parallel group
/// analysis is applied (v0.35.0).
///
/// When the estimated total row count across all derived predicates in a stratum
/// is below this threshold, the serial evaluation path is used to avoid the
/// overhead of dependency analysis.  Default: `10000`.
pub static DATALOG_PARALLEL_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ─── v0.36.0 GUCs ────────────────────────────────────────────────────────────

/// GUC: master switch for Worst-Case Optimal Join (WCOJ) optimisation (v0.36.0).
///
/// When `true` (default), cyclic SPARQL BGPs (triangle queries and other
/// cyclic join patterns) are detected at translation time and routed through
/// the Leapfrog Triejoin execution path, which forces sort-merge joins over
/// the existing B-tree `(s, o)` indices on VP tables.
///
/// Set `false` to fall back to the standard PostgreSQL planner for all queries.
pub static WCOJ_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: minimum number of VP table joins before WCOJ analysis is applied (v0.36.0).
///
/// Queries with fewer VP table joins than this value use the standard planner
/// even when cyclic.  Setting to `3` (default) means only triangle or larger
/// cyclic patterns trigger WCOJ optimisation.  Set to `2` to also optimise
/// 2-table cyclic patterns (uncommon in practice).
pub static WCOJ_MIN_TABLES: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3);

/// GUC: maximum fixpoint iterations for lattice-based Datalog inference (v0.36.0).
///
/// `pg_ripple.infer_lattice()` runs a monotone fixpoint loop over lattice rules.
/// Termination is guaranteed when the lattice satisfies the ascending chain
/// condition.  This GUC provides a safety cap — if a user-defined lattice's
/// join function is not properly monotone, the fixpoint may not converge;
/// after `lattice_max_iterations` rounds a WARNING is emitted with error code
/// PT540 and the partial results are returned.
///
/// Default: `1000`.  Set higher for very large lattice computations.
pub static LATTICE_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

// ── v0.37.0 GUC statics ───────────────────────────────────────────────────────

/// GUC: enable automatic tombstone VACUUM after merge cycles (v0.37.0).
pub static TOMBSTONE_GC_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: tombstone/main ratio threshold for triggering VACUUM (stored as string, v0.37.0).
pub static TOMBSTONE_GC_THRESHOLD_STR: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ── v0.38.0 GUC statics ───────────────────────────────────────────────────────

/// GUC: enable the backend-local predicate OID cache (v0.38.0).
///
/// When `true` (default) the first SPARQL query that references a given
/// predicate performs one SPI lookup to determine which VP table to use;
/// subsequent queries for the same predicate skip the SPI call entirely.
/// Set `false` to disable for debugging or in environments with very frequent
/// DDL changes to `_pg_ripple.predicates`.
pub static PREDICATE_CACHE_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

// ── v0.40.0 GUC statics ───────────────────────────────────────────────────────

/// GUC: maximum rows returned by a SPARQL SELECT or CONSTRUCT query (v0.40.0).
///
/// `0` (default) means unlimited.  When a query exceeds this limit, the
/// behaviour is controlled by `pg_ripple.sparql_overflow_action`.
pub static SPARQL_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: maximum derived facts produced by a single `infer()` call (v0.40.0).
///
/// `0` (default) means unlimited.  When exceeded, a PT602 WARNING is emitted
/// and partial results are returned.
pub static DATALOG_MAX_DERIVED: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: maximum rows returned by export functions (Turtle/N-Triples/JSON-LD) (v0.40.0).
///
/// `0` (default) means unlimited.  When exceeded, a PT603 WARNING is emitted.
pub static EXPORT_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(0);

/// GUC: action when `sparql_max_rows` is exceeded (v0.40.0).
///
/// `'warn'` (default) — emit a WARNING with error code PT601 and truncate the result.
/// `'error'` — raise an ERROR and abort the query.
pub static SPARQL_OVERFLOW_ACTION: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: master switch for OpenTelemetry tracing (v0.40.0).
///
/// When `true`, spans are emitted for SPARQL parse/translate/execute, merge
/// cycles, federation calls, and Datalog inference.  When `false` (default),
/// the tracing facade is a no-op with zero overhead.
pub static TRACING_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: OpenTelemetry exporter backend (v0.40.0).
///
/// `'stdout'` (default) — write spans as JSON lines to the PostgreSQL log.
/// `'otlp'` — export via OTLP gRPC; reads the `OTEL_EXPORTER_OTLP_ENDPOINT`
/// environment variable for the collector address.
pub static TRACING_EXPORTER: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ── v0.42.0 GUCs ─────────────────────────────────────────────────────────────

/// GUC: number of background merge worker processes (v0.42.0).
///
/// Each worker manages a disjoint round-robin subset of VP table predicates.
/// `pg_advisory_lock` ensures no two workers race on the same VP table.
/// Default: `1` (single worker, original behaviour). Range: 1–16.
/// Startup-only GUC — requires PostgreSQL restart to take effect.
pub static MERGE_WORKERS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1);

/// GUC: maximum `owl:sameAs` equivalence-class size before emitting PT550 WARNING (v0.42.0).
///
/// When canonicalization detects a cluster with more members than this threshold
/// the full Tarjan-SCC traversal is replaced by a sampling approximation and a
/// PT550 WARNING is emitted.  Default: `100_000`.  Set `0` to disable the check.
pub static SAMEAS_MAX_CLUSTER_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100_000);

/// GUC: TTL in seconds for cached VoID statistics per federation endpoint (v0.42.0).
///
/// When > 0, the endpoint's VoID description is re-fetched at most every N seconds.
/// Default: `3600` (1 hour).  Set `0` to disable caching (fetch on every registration).
pub static FEDERATION_STATS_TTL_SECS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(3600);

/// GUC: enable cost-based federation source selection (v0.42.0).
///
/// When `true` (default), the FedX-style planner uses VoID statistics to rank
/// endpoints by estimated selectivity and assigns each BGP atom to its best source.
/// Independent atoms with no shared variables are scheduled for parallel execution.
pub static FEDERATION_PLANNER_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: maximum number of parallel SERVICE clause workers (v0.42.0).
///
/// Independent SERVICE clauses (no shared variables) are dispatched concurrently
/// up to this limit.  Default: `4`.
pub static FEDERATION_PARALLEL_MAX: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(4);

/// GUC: wall-clock timeout in seconds for parallel federation workers (v0.42.0).
///
/// If a parallel SERVICE worker does not complete within this window its result
/// is dropped and an empty set is used.  Default: `60` seconds.
pub static FEDERATION_PARALLEL_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: maximum inline rows for federation results (v0.42.0).
///
/// SERVICE responses with more rows than this threshold are spooled into a
/// temporary table instead of being inlined as a `VALUES` clause.  Emits PT620
/// INFO when spooling is triggered.  Default: `10_000`.
pub static FEDERATION_INLINE_MAX_ROWS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: allow federation endpoints with private/loopback IP addresses (v0.42.0).
///
/// When `false` (default), `register_endpoint()` resolves the hostname and rejects
/// addresses in RFC 1918 (10.x, 172.16–31.x, 192.168.x), link-local (169.254.x.x),
/// loopback (127.x.x.x), and IPv6 link-local ranges.  Emits PT621 when a private-IP
/// endpoint is rejected.  Set `true` in trusted internal deployments.
pub static FEDERATION_ALLOW_PRIVATE: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

// ── v0.46.0 GUCs ─────────────────────────────────────────────────────────────

/// GUC: enable TopN push-down for `ORDER BY … LIMIT N` queries (v0.46.0).
///
/// When `true` (default), SPARQL SELECT queries that contain both `ORDER BY` and
/// `LIMIT N` (with no `OFFSET > 0`) emit the SQL as `… ORDER BY … LIMIT N` rather
/// than fetching all rows and discarding after decoding.  Skipped when `DISTINCT`
/// is in scope.  Disable for debugging or if incorrect results are suspected.
pub static TOPN_PUSHDOWN: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: SID range reserved per parallel Datalog worker per batch (v0.46.0).
///
/// Before launching N parallel strata workers, the coordinator calls
/// `SELECT setval(seq, currval(seq) + N * batch_size)` once to reserve a
/// contiguous SID range; each worker uses its slice without touching the sequence.
/// Default: `10000`.  Range: 100–1 000 000.
pub static DATALOG_SEQUENCE_BATCH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

// ── v0.48.0 GUCs ─────────────────────────────────────────────────────────────

/// GUC: maximum federation response body in bytes (v0.48.0).
///
/// When a remote SERVICE endpoint returns a JSON body larger than this value,
/// the response is refused with error code PT543 and an ERROR is raised.
/// Default: 100 MiB (`104_857_600`).  Set `-1` to disable the limit (not
/// recommended for untrusted deployments).
pub static FEDERATION_MAX_RESPONSE_BYTES: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(104_857_600);

// ── v0.49.0 GUCs — AI & LLM Integration ──────────────────────────────────────

/// GUC: LLM API base URL for natural-language → SPARQL generation (v0.49.0).
///
/// When empty (the default), `sparql_from_nl()` raises PT700 immediately.
/// Set to `'mock'` to use the built-in test mock (returns a canned SELECT).
/// Otherwise, the value must be an OpenAI-compatible base URL
/// (e.g. `https://api.openai.com/v1`, a local Ollama endpoint, or vLLM).
pub static LLM_ENDPOINT: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: LLM model identifier used for NL → SPARQL generation (v0.49.0).
///
/// Passed as the `model` field in the OpenAI-compatible `/v1/chat/completions`
/// request body.  Default: `gpt-4o`.
pub static LLM_MODEL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: name of the environment variable that holds the LLM API key (v0.49.0).
///
/// The key is never stored inline.  At call time, `sparql_from_nl()` reads
/// `std::env::var(llm_api_key_env)` to obtain the Bearer token.
/// Default: `PG_RIPPLE_LLM_API_KEY`.
pub static LLM_API_KEY_ENV: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: when `on` (default), include active SHACL shapes as semantic context
/// in the prompt sent to the LLM endpoint (v0.49.0).
///
/// Shapes are appended as a Turtle snippet after the VoID description.
/// Disable when shapes are large or the LLM context window is limited.
pub static LLM_INCLUDE_SHAPES: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);
