//! pg_ripple — High-performance RDF triple store for PostgreSQL 18.
//!
//! # Architecture
//!
//! Every IRI, blank node, and literal is encoded to `i64` via XXH3-128 hash
//! (see `src/dictionary/`) before being stored in vertical-partition (VP)
//! tables in the `_pg_ripple` schema (see `src/storage/`).  SPARQL queries
//! are parsed with `spargebra`, compiled to SQL, and executed via SPI
//! (see `src/sparql/`).
//!
//! In v0.6.0 (HTAP Architecture), VP tables are split into delta + main
//! partitions for non-blocking concurrent reads and writes.

use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

mod bulk_load;
mod cdc;
mod datalog;
mod dictionary;
mod error;
mod export;
mod fts;
mod shacl;
mod shmem;
mod sparql;
mod storage;
mod worker;

pgrx::pg_module_magic!();

// Allow creating the `pg_ripple` schema despite the `pg_` prefix restriction.
pgrx::extension_sql!(
    r#"SET LOCAL allow_system_table_mods = on;"#,
    name = "bootstrap_allow_system_mods",
    bootstrap
);

// Create all internal schema objects at CREATE EXTENSION time.
// This runs inside the extension transaction so SPI/DDL is available, unlike
// _PG_init() which may be called during shared_preload_libraries loading
// before any transaction context exists.
pgrx::extension_sql!(
    r#"
-- Internal schema
CREATE SCHEMA IF NOT EXISTS _pg_ripple;

-- Dictionary table (IRI / blank-node / literal → i64)
CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary (
    id       BIGINT   GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash     BYTEA    NOT NULL,
    value    TEXT     NOT NULL,
    kind     SMALLINT NOT NULL DEFAULT 0,
    datatype TEXT,
    lang     TEXT,
    qt_s     BIGINT,
    qt_p     BIGINT,
    qt_o     BIGINT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hash
    ON _pg_ripple.dictionary (hash);
CREATE INDEX IF NOT EXISTS idx_dictionary_value_kind
    ON _pg_ripple.dictionary (value, kind);

-- Sequences
CREATE SEQUENCE IF NOT EXISTS _pg_ripple.statement_id_seq
    START 1 INCREMENT 1 CACHE 64 NO CYCLE;
CREATE SEQUENCE IF NOT EXISTS _pg_ripple.load_generation_seq
    START 1 INCREMENT 1 NO CYCLE;

-- Predicate catalog
CREATE TABLE IF NOT EXISTS _pg_ripple.predicates (
    id           BIGINT  NOT NULL PRIMARY KEY,
    table_oid    OID,
    triple_count BIGINT  NOT NULL DEFAULT 0,
    htap         BOOLEAN NOT NULL DEFAULT false
);

-- Rare-predicate consolidation table
CREATE TABLE IF NOT EXISTS _pg_ripple.vp_rare (
    p      BIGINT   NOT NULL,
    s      BIGINT   NOT NULL,
    o      BIGINT   NOT NULL,
    g      BIGINT   NOT NULL DEFAULT 0,
    i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'),
    source SMALLINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_vp_rare_p_s_o   ON _pg_ripple.vp_rare (p, s, o);
CREATE INDEX IF NOT EXISTS idx_vp_rare_s_p     ON _pg_ripple.vp_rare (s, p);
CREATE INDEX IF NOT EXISTS idx_vp_rare_g_p_s_o ON _pg_ripple.vp_rare (g, p, s, o);

-- Statements range-mapping catalog (v0.2.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.statements (
    sid_min      BIGINT NOT NULL,
    sid_max      BIGINT NOT NULL,
    predicate_id BIGINT NOT NULL,
    table_oid    OID    NOT NULL,
    PRIMARY KEY  (sid_min)
);

-- IRI prefix registry
CREATE TABLE IF NOT EXISTS _pg_ripple.prefixes (
    prefix    TEXT NOT NULL PRIMARY KEY,
    expansion TEXT NOT NULL
);

-- HTAP star-pattern caches (v0.6.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.subject_patterns (
    s       BIGINT   NOT NULL PRIMARY KEY,
    pattern BIGINT[] NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_subject_patterns_gin
    ON _pg_ripple.subject_patterns USING GIN (pattern);

CREATE TABLE IF NOT EXISTS _pg_ripple.object_patterns (
    o       BIGINT   NOT NULL PRIMARY KEY,
    pattern BIGINT[] NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_object_patterns_gin
    ON _pg_ripple.object_patterns USING GIN (pattern);

-- CDC subscription registry (v0.6.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_subscriptions (
    id                BIGSERIAL PRIMARY KEY,
    channel           TEXT NOT NULL,
    predicate_id      BIGINT,
    predicate_pattern TEXT NOT NULL DEFAULT '*'
);
CREATE INDEX IF NOT EXISTS idx_cdc_subs_channel
    ON _pg_ripple.cdc_subscriptions (channel);
CREATE INDEX IF NOT EXISTS idx_cdc_subs_predicate
    ON _pg_ripple.cdc_subscriptions (predicate_id);

-- SHACL shapes catalog (v0.7.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.shacl_shapes (
    shape_iri  TEXT        NOT NULL PRIMARY KEY,
    shape_json JSONB       NOT NULL,
    active     BOOLEAN     NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_shacl_shapes_active
    ON _pg_ripple.shacl_shapes (active);

-- Async validation queue (v0.7.0 — populated when shacl_mode = 'async')
CREATE TABLE IF NOT EXISTS _pg_ripple.validation_queue (
    id         BIGSERIAL   PRIMARY KEY,
    s_id       BIGINT      NOT NULL,
    p_id       BIGINT      NOT NULL,
    o_id       BIGINT      NOT NULL,
    g_id       BIGINT      NOT NULL DEFAULT 0,
    stmt_id    BIGINT      NOT NULL,
    queued_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_validation_queue_queued
    ON _pg_ripple.validation_queue (queued_at);

-- Dead-letter queue for async SHACL violations (v0.7.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.dead_letter_queue (
    id            BIGSERIAL   PRIMARY KEY,
    s_id          BIGINT      NOT NULL,
    p_id          BIGINT      NOT NULL,
    o_id          BIGINT      NOT NULL,
    g_id          BIGINT      NOT NULL DEFAULT 0,
    stmt_id       BIGINT      NOT NULL,
    violation     JSONB       NOT NULL,
    detected_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dead_letter_shape
    ON _pg_ripple.dead_letter_queue ((violation->>'shapeIRI'));

-- SHACL DAG monitor catalog (v0.8.0)
-- Tracks which shapes have been compiled into pg_trickle stream tables.
CREATE TABLE IF NOT EXISTS _pg_ripple.shacl_dag_monitors (
    shape_iri          TEXT        NOT NULL PRIMARY KEY,
    stream_table_name  TEXT        NOT NULL,
    constraint_summary TEXT        NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- CDC notify trigger function (v0.6.0)
CREATE OR REPLACE FUNCTION _pg_ripple.notify_triple_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $body$
DECLARE
    pred_id BIGINT := TG_ARGV[0]::bigint;
    payload TEXT;
    sub     RECORD;
BEGIN
    IF TG_OP = 'INSERT' THEN
        payload := json_build_object(
            'op', 'insert',
            's', NEW.s, 'p', pred_id, 'o', NEW.o, 'g', NEW.g
        )::text;
    ELSE
        payload := json_build_object(
            'op', 'delete',
            's', OLD.s, 'p', pred_id, 'o', OLD.o, 'g', OLD.g
        )::text;
    END IF;
    FOR sub IN
        SELECT channel FROM _pg_ripple.cdc_subscriptions
        WHERE predicate_id = pred_id OR predicate_pattern = '*'
    LOOP
        PERFORM pg_notify(sub.channel, payload);
    END LOOP;
    RETURN NEW;
END;
$body$;
"#,
    name = "schema_setup",
    requires = ["bootstrap_allow_system_mods"]
);

// v0.10.0: Datalog reasoning catalog tables.
pgrx::extension_sql!(
    r#"
-- Datalog rules catalog (v0.10.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.rules (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    rule_set      TEXT NOT NULL,
    rule_text     TEXT NOT NULL,
    head_pred     BIGINT,
    stratum       INT NOT NULL DEFAULT 0,
    is_recursive  BOOLEAN NOT NULL DEFAULT false,
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_rules_rule_set
    ON _pg_ripple.rules (rule_set);
CREATE INDEX IF NOT EXISTS idx_rules_head_pred
    ON _pg_ripple.rules (head_pred);

-- Rule sets catalog (v0.10.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.rule_sets (
    name          TEXT NOT NULL PRIMARY KEY,
    rule_hash     BYTEA,
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Extend predicates table: mark derived predicates (v0.10.0)
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS derived BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS rule_set TEXT;

-- Hot dictionary table for frequently-accessed IRIs (v0.10.0)
CREATE UNLOGGED TABLE IF NOT EXISTS _pg_ripple.dictionary_hot (
    id       BIGINT   NOT NULL PRIMARY KEY,
    hash     BYTEA    NOT NULL,
    value    TEXT     NOT NULL,
    kind     SMALLINT NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hot_hash
    ON _pg_ripple.dictionary_hot (hash);
"#,
    name = "datalog_schema_setup",
    requires = ["schema_setup"]
);

// Create the predicate_stats view after the base tables exist.
pgrx::extension_sql!(
    r#"
CREATE OR REPLACE VIEW pg_ripple.predicate_stats AS
SELECT
    d.value       AS predicate_iri,
    p.triple_count,
    CASE WHEN p.table_oid IS NOT NULL THEN 'dedicated' ELSE 'rare' END AS storage
FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
ORDER BY p.triple_count DESC;
"#,
    name = "predicate_stats_view",
    requires = ["schema_setup"],
    finalize
);

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

// ─── pg_trickle runtime detection (v0.6.0) ───────────────────────────────────

/// Returns `true` when the pg_trickle extension is installed in the current database.
///
/// All pg_trickle-dependent features gate on this check — core pg_ripple
/// functionality works without pg_trickle.
pub(crate) fn has_pg_trickle() -> bool {
    pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle')",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Returns `true` when the pg_trickle live-statistics stream tables have been
/// created (i.e. `enable_live_statistics()` was previously called successfully).
pub(crate) fn has_live_statistics() -> bool {
    pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(
            SELECT 1 FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = '_pg_ripple'
              AND c.relname = 'predicate_stats'
        )",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

// ─── ExecutorEnd hook (v0.6.0) ────────────────────────────────────────────────

/// Register a PostgreSQL `ExecutorEnd_hook` that pokes the merge worker's latch
/// whenever the accumulated unmerged delta row count crosses
/// `pg_ripple.latch_trigger_threshold`.
///
/// Must only be called from `_PG_init` inside the postmaster context
/// (i.e. when loaded via `shared_preload_libraries`).
fn register_executor_end_hook() {
    unsafe {
        static mut PREV_EXECUTOR_END: pg_sys::ExecutorEnd_hook_type = None;

        PREV_EXECUTOR_END = pg_sys::ExecutorEnd_hook;
        pg_sys::ExecutorEnd_hook = Some(pg_ripple_executor_end);

        #[pg_guard]
        unsafe extern "C-unwind" fn pg_ripple_executor_end(query_desc: *mut pg_sys::QueryDesc) {
            // Call the previous hook first.
            unsafe {
                if let Some(prev) = PREV_EXECUTOR_END {
                    prev(query_desc);
                } else {
                    pg_sys::standard_ExecutorEnd(query_desc);
                }
            }

            // If shmem is ready, check whether delta growth exceeds the threshold.
            if !crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let threshold = crate::LATCH_TRIGGER_THRESHOLD.get() as i64;
            let delta_rows = crate::shmem::TOTAL_DELTA_ROWS
                .get()
                .load(std::sync::atomic::Ordering::Relaxed);
            if delta_rows >= threshold {
                crate::shmem::poke_merge_worker();
            }
        }
    }
}

/// Called once when the extension shared library is loaded.
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.default_graph",
        c"IRI of the default named graph (empty = built-in default graph)",
        c"",
        &DEFAULT_GRAPH,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vp_promotion_threshold",
        c"Minimum triple count before a predicate gets its own VP table (default: 1000)",
        c"",
        &VPP_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.named_graph_optimized",
        c"Add a (g, s, o) index to each VP table to speed up named-graph queries",
        c"",
        &NAMED_GRAPH_OPTIMIZED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.plan_cache_size",
        c"Maximum number of cached SPARQL-to-SQL plan translations per backend (0 = disabled)",
        c"",
        &PLAN_CACHE_SIZE,
        0,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.max_path_depth",
        c"Maximum recursion depth for SPARQL property path queries (+ and *); 0 = unlimited",
        c"",
        &MAX_PATH_DEPTH,
        0,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.describe_strategy",
        c"DESCRIBE algorithm: 'cbd' (Concise Bounded Description), 'scbd' (Symmetric CBD), or 'simple'",
        c"",
        &DESCRIBE_STRATEGY,
        GucContext::Userset,
        GucFlags::default(),
    );

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

    // ── v0.7.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.shacl_mode",
        c"SHACL validation mode: 'off' (default), 'sync' (reject violations inline), 'async' (queue for background worker)",
        c"",
        &SHACL_MODE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.dedup_on_merge",
        c"When true, the HTAP generation merge deduplicates (s,o,g) rows keeping the lowest SID (default: false)",
        c"",
        &DEDUP_ON_MERGE,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.10.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.inference_mode",
        c"Datalog inference mode: 'off' (default), 'on_demand', 'materialized'",
        c"",
        &INFERENCE_MODE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.enforce_constraints",
        c"Constraint rule enforcement: 'off' (default), 'warn', 'error'",
        c"",
        &ENFORCE_CONSTRAINTS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.rule_graph_scope",
        c"Graph scope for unscoped Datalog atoms: 'default' (g=0 only) or 'all' (any graph)",
        c"",
        &RULE_GRAPH_SCOPE,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── Postmaster-only GUCs and shared memory (v0.6.0) ─────────────────────
    // PGC_POSTMASTER GUCs can only be registered during shared_preload_libraries
    // loading.  `process_shared_preload_libraries_in_progress` is the correct
    // flag — `IsPostmasterEnvironment` is true in every server process and
    // cannot be used to distinguish this case.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        pgrx::GucRegistry::define_int_guc(
            c"pg_ripple.dictionary_cache_size",
            c"Shared-memory encode-cache capacity in entries (default: 4096; startup only)",
            c"",
            &DICTIONARY_CACHE_SIZE,
            0,
            1_000_000,
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
    }

    // ── Shared memory initialisation (v0.6.0) ────────────────────────────────
    // Only registers shmem hooks (pg_shmem_init!) when running in
    // shared_preload_libraries context.  When loaded via CREATE EXTENSION the
    // hooks have already fired; skip to avoid the "PgAtomic was not
    // initialized" panic.
    if unsafe { pg_sys::process_shared_preload_libraries_in_progress } {
        shmem::init();
        worker::register_merge_worker();
        // Register ExecutorEnd hook to poke the merge worker latch when the
        // accumulated unmerged delta row count crosses the trigger threshold.
        register_executor_end_hook();
    }
    // Schema and base tables are created by the `schema_setup` extension_sql!
    // block, which runs inside the CREATE EXTENSION transaction where SPI and
    // DDL are available.  Nothing to do here.
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

/// All user-visible SQL functions live in the `pg_ripple` schema.
#[pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Dictionary ────────────────────────────────────────────────────────────

    /// Encode a text IRI/blank-node/literal to its dictionary `i64` identifier.
    #[pg_extern]
    fn encode_term(term: &str, kind: i16) -> i64 {
        crate::dictionary::encode(term, kind)
    }

    /// Decode a dictionary `i64` back to its original text value.
    #[pg_extern]
    fn decode_id(id: i64) -> Option<String> {
        crate::dictionary::decode(id)
    }

    // ── RDF-star: quoted triple encoding (v0.4.0) ──────────────────────────

    /// Encode a quoted triple `(s, p, o)` into the dictionary.
    ///
    /// All three arguments must be N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or nested `<< … >>` quoted triples).
    /// Returns the dictionary ID of the quoted triple.
    #[pg_extern]
    fn encode_triple(s: &str, p: &str, o: &str) -> i64 {
        let s_id = crate::storage::encode_rdf_term(s);
        let p_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(p),
            crate::dictionary::KIND_IRI,
        );
        let o_id = crate::storage::encode_rdf_term(o);
        crate::dictionary::encode_quoted_triple(s_id, p_id, o_id)
    }

    /// Decode a quoted triple dictionary ID to its component terms as JSONB.
    ///
    /// Returns `{"s": "...", "p": "...", "o": "..."}` with N-Triples–formatted
    /// values, or NULL if `id` is not a quoted triple.
    #[pg_extern]
    fn decode_triple(id: i64) -> Option<pgrx::JsonB> {
        let (s_id, p_id, o_id) = crate::dictionary::decode_quoted_triple_components(id)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "s".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(s_id)),
        );
        obj.insert(
            "p".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(p_id)),
        );
        obj.insert(
            "o".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(o_id)),
        );
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    // ── Triple CRUD ───────────────────────────────────────────────────────────

    /// Insert a triple into the appropriate VP table.
    ///
    /// `s`, `p`, and `o` accept N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or `<< … >>` quoted triples).
    /// `g` is an optional named graph IRI; NULL uses the default graph.
    /// Returns the globally-unique statement identifier (SID).
    #[pg_extern]
    fn insert_triple(s: &str, p: &str, o: &str, g: default!(Option<&str>, "NULL")) -> i64 {
        let g_id = g.map_or(0_i64, |iri| {
            crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(iri),
                crate::dictionary::KIND_IRI,
            )
        });

        // ── v0.7.0: SHACL sync validation ──────────────────────────────────
        let shacl_mode = crate::SHACL_MODE.get();
        let shacl_mode_str = shacl_mode
            .as_ref()
            .and_then(|c| c.to_str().ok())
            .unwrap_or("off");

        if shacl_mode_str == "sync" {
            // Pre-encode the triple terms to check constraints.
            let s_id = crate::storage::encode_rdf_term(s);
            let p_id = crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(p),
                crate::dictionary::KIND_IRI,
            );
            let o_id = crate::storage::encode_rdf_term(o);
            if let Err(msg) = crate::shacl::validate_sync(s_id, p_id, o_id, g_id) {
                pgrx::error!("{msg}");
            }
        }

        let sid = crate::storage::insert_triple(s, p, o, g_id);

        // ── v0.7.0: SHACL async queue ───────────────────────────────────────
        if shacl_mode_str == "async" && sid > 0 {
            let s_id = crate::storage::encode_rdf_term(s);
            let p_id = crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(p),
                crate::dictionary::KIND_IRI,
            );
            let o_id = crate::storage::encode_rdf_term(o);
            let _ = pgrx::Spi::run_with_args(
                "INSERT INTO _pg_ripple.validation_queue (s_id, p_id, o_id, g_id, stmt_id) \
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    pgrx::datum::DatumWithOid::from(s_id),
                    pgrx::datum::DatumWithOid::from(p_id),
                    pgrx::datum::DatumWithOid::from(o_id),
                    pgrx::datum::DatumWithOid::from(g_id),
                    pgrx::datum::DatumWithOid::from(sid),
                ],
            );
        }

        sid
    }

    /// Look up a statement by its globally-unique statement identifier (SID).
    ///
    /// Returns `{"s": "...", "p": "...", "o": "...", "g": "..."}` as JSONB,
    /// or NULL if the SID is not found.
    #[pg_extern]
    fn get_statement(i: i64) -> Option<pgrx::JsonB> {
        let (s, p, o, g) = crate::storage::get_statement_by_sid(i)?;
        let mut obj = serde_json::Map::new();
        obj.insert("s".to_owned(), serde_json::Value::String(s));
        obj.insert("p".to_owned(), serde_json::Value::String(p));
        obj.insert("o".to_owned(), serde_json::Value::String(o));
        obj.insert("g".to_owned(), serde_json::Value::String(g));
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    /// Delete a triple.  Returns the number of rows removed (0 or 1).
    #[pg_extern]
    fn delete_triple(s: &str, p: &str, o: &str) -> i64 {
        crate::storage::delete_triple(s, p, o, 0_i64)
    }

    /// Return the total number of triples across all VP tables and vp_rare.
    #[pg_extern]
    fn triple_count() -> i64 {
        crate::storage::total_triple_count()
    }

    /// Pattern-match triples; any argument may be NULL to act as a wildcard.
    /// Queries both dedicated VP tables and vp_rare.
    /// Returns N-Triples–formatted `(s, p, o, g)` tuples.
    #[pg_extern]
    fn find_triples(
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
    ) -> TableIterator<
        'static,
        (
            name!(s, String),
            name!(p, String),
            name!(o, String),
            name!(g, String),
        ),
    > {
        let rows = crate::storage::find_triples(s, p, o, None);
        TableIterator::new(rows)
    }

    // ── Rare-predicate promotion ──────────────────────────────────────────────

    /// Promote all rare predicates that have reached the promotion threshold.
    /// Returns the number of predicates promoted.
    #[pg_extern]
    fn promote_rare_predicates() -> i64 {
        crate::storage::promote_rare_predicates()
    }

    // ── Bulk loaders ──────────────────────────────────────────────────────────

    /// Load N-Triples data from a text string.  Returns the number of triples loaded.
    /// Also accepts N-Triples-star (quoted triples as objects or subjects).
    #[pg_extern]
    fn load_ntriples(data: &str) -> i64 {
        crate::bulk_load::load_ntriples(data)
    }

    /// Load N-Quads data from a text string (supports named graphs).
    #[pg_extern]
    fn load_nquads(data: &str) -> i64 {
        crate::bulk_load::load_nquads(data)
    }

    /// Load Turtle data from a text string.
    /// Also accepts Turtle-star (quoted triples) using oxttl with rdf-12 support.
    #[pg_extern]
    fn load_turtle(data: &str) -> i64 {
        crate::bulk_load::load_turtle(data)
    }

    /// Load TriG data (Turtle with named graph blocks) from a text string.
    #[pg_extern]
    fn load_trig(data: &str) -> i64 {
        crate::bulk_load::load_trig(data)
    }

    /// Load N-Triples from a server-side file path (superuser required).
    #[pg_extern]
    fn load_ntriples_file(path: &str) -> i64 {
        crate::bulk_load::load_ntriples_file(path)
    }

    /// Load N-Quads from a server-side file path (superuser required).
    #[pg_extern]
    fn load_nquads_file(path: &str) -> i64 {
        crate::bulk_load::load_nquads_file(path)
    }

    /// Load Turtle from a server-side file path (superuser required).
    #[pg_extern]
    fn load_turtle_file(path: &str) -> i64 {
        crate::bulk_load::load_turtle_file(path)
    }

    /// Load TriG from a server-side file path (superuser required).
    #[pg_extern]
    fn load_trig_file(path: &str) -> i64 {
        crate::bulk_load::load_trig_file(path)
    }

    /// Load RDF/XML data from a text string.  Returns the number of triples loaded.
    ///
    /// Parses conformant RDF/XML using `rio_xml`.  All triples are loaded into the
    /// default graph (RDF/XML does not support named graphs).
    #[pg_extern]
    fn load_rdfxml(data: &str) -> i64 {
        crate::bulk_load::load_rdfxml(data)
    }

    // ── Named graph management ────────────────────────────────────────────────

    /// Register a named graph IRI.  Returns its dictionary id.
    /// This is idempotent — safe to call multiple times.
    #[pg_extern]
    fn create_graph(graph_iri: &str) -> i64 {
        crate::storage::create_graph(graph_iri)
    }

    /// Delete all triples in a named graph.  Returns the number of triples deleted.
    #[pg_extern]
    fn drop_graph(graph_iri: &str) -> i64 {
        crate::storage::drop_graph(graph_iri)
    }

    /// List all named graph IRIs (excludes the default graph).
    #[pg_extern]
    fn list_graphs() -> TableIterator<'static, (name!(graph_iri, String),)> {
        let graphs = crate::storage::list_graphs();
        TableIterator::new(graphs.into_iter().map(|g| (g,)))
    }

    // ── IRI prefix management ─────────────────────────────────────────────────

    /// Register (or update) an IRI prefix abbreviation.
    #[pg_extern]
    fn register_prefix(prefix: &str, expansion: &str) {
        crate::storage::register_prefix(prefix, expansion);
    }

    /// Return all registered prefix → expansion mappings.
    #[pg_extern]
    fn prefixes() -> TableIterator<'static, (name!(prefix, String), name!(expansion, String))> {
        let pfxs = crate::storage::list_prefixes();
        TableIterator::new(pfxs)
    }

    // ── Export ────────────────────────────────────────────────────────────────

    /// Export triples to N-Triples format.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_ntriples(graph: Option<&str>) -> String {
        crate::export::export_ntriples(graph)
    }

    /// Export triples to N-Quads format.
    /// Pass a graph IRI to export a specific graph, or NULL to export all graphs.
    #[pg_extern]
    fn export_nquads(graph: Option<&str>) -> String {
        crate::export::export_nquads(graph)
    }

    /// Export triples as Turtle text.
    ///
    /// Groups triples by subject and emits compact Turtle blocks.  Includes
    /// all `@prefix` declarations from the prefix registry.
    /// RDF-star quoted triples are serialized in Turtle-star `<< s p o >>` notation.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_turtle(graph: default!(Option<&str>, "NULL")) -> String {
        crate::export::export_turtle(graph)
    }

    /// Export triples as JSON-LD (expanded form).
    ///
    /// Returns a JSON-LD document as a JSONB array where each element represents
    /// one subject with all its predicates and objects.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_jsonld(graph: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        pgrx::JsonB(crate::export::export_jsonld(graph))
    }

    /// Streaming Turtle export — returns one `TEXT` row per triple.
    ///
    /// Yields `@prefix` declarations first, then one flat Turtle triple per line.
    /// Suitable for large graphs where buffering the full document would be too
    /// memory-intensive.
    #[pg_extern]
    fn export_turtle_stream(
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let lines = crate::export::export_turtle_stream(graph);
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    /// Streaming JSON-LD export — returns one NDJSON line per subject.
    ///
    /// Each row is a JSON string representing one subject's complete node object.
    /// Suitable for large graphs where buffering the full document is undesirable.
    #[pg_extern]
    fn export_jsonld_stream(
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let lines = crate::export::export_jsonld_stream(graph);
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    // ── SPARQL query engine ───────────────────────────────────────────────────

    /// Execute a SPARQL SELECT or ASK query.
    ///
    /// Returns one JSONB row per result binding for SELECT queries.
    /// For ASK returns a single row `{"result": "true"}` or `{"result": "false"}`.
    #[pg_extern]
    fn sparql(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL ASK query; returns TRUE if any results exist.
    #[pg_extern]
    fn sparql_ask(query: &str) -> bool {
        crate::sparql::sparql_ask(query)
    }

    /// Return the SQL generated for a SPARQL query (for debugging).
    /// Set `analyze := true` to EXPLAIN ANALYZE the generated SQL.
    #[pg_extern]
    fn sparql_explain(query: &str, analyze: bool) -> String {
        crate::sparql::sparql_explain(query, analyze)
    }

    /// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
    ///
    /// Each row is `{"s": "...", "p": "...", "o": "..."}` in N-Triples format.
    #[pg_extern]
    fn sparql_construct(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_construct(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL DESCRIBE query using the Concise Bounded Description algorithm.
    ///
    /// Returns one JSONB row per triple in the description.
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe(
        query: &str,
        strategy: default!(&str, "'cbd'"),
    ) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_describe(query, strategy);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL CONSTRUCT query; returns the result as Turtle text.
    ///
    /// Constructs triples according to the CONSTRUCT template and serializes them
    /// as a Turtle document.  RDF-star quoted triples are emitted in Turtle-star
    /// notation.
    #[pg_extern]
    fn sparql_construct_turtle(query: &str) -> String {
        let rows = crate::sparql::sparql_construct(query);
        let triples: Vec<(String, String, String)> = rows
            .into_iter()
            .filter_map(|jsonb| {
                let obj = jsonb.0.as_object()?;
                let s = obj.get("s")?.as_str()?.to_owned();
                let p = obj.get("p")?.as_str()?.to_owned();
                let o = obj.get("o")?.as_str()?.to_owned();
                Some((s, p, o))
            })
            .collect();
        crate::export::triples_to_turtle(&triples)
    }

    /// Execute a SPARQL CONSTRUCT query; returns the result as JSON-LD (JSONB).
    ///
    /// Constructs triples according to the CONSTRUCT template and serializes them
    /// as a JSON-LD expanded-form array.  Suitable for REST API responses.
    #[pg_extern]
    fn sparql_construct_jsonld(query: &str) -> pgrx::JsonB {
        let rows = crate::sparql::sparql_construct(query);
        let triples: Vec<(String, String, String)> = rows
            .into_iter()
            .filter_map(|jsonb| {
                let obj = jsonb.0.as_object()?;
                let s = obj.get("s")?.as_str()?.to_owned();
                let p = obj.get("p")?.as_str()?.to_owned();
                let o = obj.get("o")?.as_str()?.to_owned();
                Some((s, p, o))
            })
            .collect();
        pgrx::JsonB(crate::export::triples_to_jsonld(&triples))
    }

    /// Execute a SPARQL DESCRIBE query; returns the description as Turtle text.
    ///
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe_turtle(query: &str, strategy: default!(&str, "'cbd'")) -> String {
        let rows = crate::sparql::sparql_describe(query, strategy);
        let triples: Vec<(String, String, String)> = rows
            .into_iter()
            .filter_map(|jsonb| {
                let obj = jsonb.0.as_object()?;
                let s = obj.get("s")?.as_str()?.to_owned();
                let p = obj.get("p")?.as_str()?.to_owned();
                let o = obj.get("o")?.as_str()?.to_owned();
                Some((s, p, o))
            })
            .collect();
        crate::export::triples_to_turtle(&triples)
    }

    /// Execute a SPARQL DESCRIBE query; returns the description as JSON-LD (JSONB).
    ///
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe_jsonld(query: &str, strategy: default!(&str, "'cbd'")) -> pgrx::JsonB {
        let rows = crate::sparql::sparql_describe(query, strategy);
        let triples: Vec<(String, String, String)> = rows
            .into_iter()
            .filter_map(|jsonb| {
                let obj = jsonb.0.as_object()?;
                let s = obj.get("s")?.as_str()?.to_owned();
                let p = obj.get("p")?.as_str()?.to_owned();
                let o = obj.get("o")?.as_str()?.to_owned();
                Some((s, p, o))
            })
            .collect();
        pgrx::JsonB(crate::export::triples_to_jsonld(&triples))
    }

    /// Execute a SPARQL Update statement (`INSERT DATA` or `DELETE DATA`).
    ///
    /// Returns the total number of triples affected (inserted or deleted).
    #[pg_extern]
    fn sparql_update(query: &str) -> i64 {
        crate::sparql::sparql_update(query)
    }

    // ── Full-text search ─────────────────────────────────────────────────────

    /// Create a GIN tsvector index on the dictionary for the given predicate IRI.
    ///
    /// After indexing, SPARQL `CONTAINS()` and `REGEX()` FILTERs on triples
    /// using this predicate will be rewritten to use the GIN index for
    /// efficient text matching.  Returns the predicate dictionary id.
    #[pg_extern]
    fn fts_index(predicate: &str) -> i64 {
        crate::fts::fts_index(predicate)
    }

    /// Full-text search on literal objects of a given predicate.
    ///
    /// `query` is a `tsquery`-formatted search string (e.g. `'knowledge & graph'`).
    /// Returns matching triples as `(s TEXT, p TEXT, o TEXT)` in N-Triples format.
    #[pg_extern]
    fn fts_search(
        query: &str,
        predicate: &str,
    ) -> TableIterator<'static, (name!(s, String), name!(p, String), name!(o, String))> {
        let rows: Vec<(String, String, String)> =
            crate::fts::fts_search(query, predicate).collect();
        TableIterator::new(rows)
    }

    // ── HTAP maintenance (v0.6.0) ─────────────────────────────────────────────

    /// Trigger an immediate full merge of all HTAP VP tables.
    ///
    /// Moves all rows from delta into main, rebuilds subject_patterns and
    /// object_patterns, and runs ANALYZE on each merged table.
    /// Returns the total number of rows in all merged main tables.
    #[pg_extern]
    fn compact() -> i64 {
        crate::storage::merge::compact()
    }

    /// Migrate an existing flat VP table (pre-v0.6.0) to the HTAP partition split.
    ///
    /// Called automatically by the v0.5.1→v0.6.0 migration script, but can
    /// also be called manually if needed.  The predicate is specified by its
    /// dictionary integer ID.
    #[pg_extern]
    fn htap_migrate_predicate(pred_id: i64) {
        crate::storage::merge::migrate_flat_to_htap(pred_id);
    }

    // ── pg_trickle integration (v0.6.0, optional) ────────────────────────────

    /// Enable live statistics via pg_trickle stream tables.
    ///
    /// Creates `_pg_ripple.predicate_stats` and `_pg_ripple.graph_stats` stream
    /// tables using pg_trickle.  These let `pg_ripple.stats()` return results
    /// instantly (no full VP table scan) when pg_trickle is installed and
    /// `enable_live_statistics()` has been called.
    ///
    /// Returns `true` if stream tables were created, `false` if pg_trickle is
    /// not installed (no error is raised — pg_trickle is optional).
    ///
    /// ```sql
    /// SELECT pg_ripple.enable_live_statistics();
    /// ```
    #[pg_extern]
    fn enable_live_statistics() -> bool {
        // Check if pg_trickle is installed.
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; live statistics are unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_live_statistics() to enable."
            );
            return false;
        }

        // Create _pg_ripple.predicate_stats stream table via pg_trickle.
        // Refreshed every 5 seconds; reads from the predicates catalog +
        // dedicated VP table reltuples (fast, planner-statistics-based).
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.predicate_stats',
                $$
                    SELECT
                        p.id          AS predicate_id,
                        d.value       AS predicate_iri,
                        p.triple_count,
                        CASE WHEN p.table_oid IS NOT NULL THEN 'dedicated'
                             ELSE 'rare' END AS storage_type
                    FROM _pg_ripple.predicates p
                    JOIN _pg_ripple.dictionary d ON d.id = p.id
                    ORDER BY p.triple_count DESC
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.predicate_stats stream table: {}",
                e
            );
        });

        // Create _pg_ripple.graph_stats stream table via pg_trickle.
        // Refreshed every 10 seconds.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.graph_stats',
                $$
                    SELECT
                        g.id       AS graph_id,
                        d.value    AS graph_iri,
                        g.triple_count
                    FROM _pg_ripple.graphs g
                    JOIN _pg_ripple.dictionary d ON d.id = g.id
                    ORDER BY g.triple_count DESC
                $$,
                '10s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.graph_stats stream table: {}",
                e
            );
        });

        // Create _pg_ripple.vp_cardinality stream table — per-predicate live
        // row counts for BGP join reordering without waiting for ANALYZE.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.vp_cardinality',
                $$
                    SELECT
                        p.id     AS predicate_id,
                        c.reltuples::bigint AS estimated_rows
                    FROM _pg_ripple.predicates p
                    JOIN pg_class c ON c.oid = p.table_oid
                    WHERE p.table_oid IS NOT NULL
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.vp_cardinality stream table: {}",
                e
            );
        });

        // Create _pg_ripple.rare_predicate_candidates stream table with
        // IMMEDIATE mode — replaces the merge-worker GROUP BY polling for
        // VP promotion detection.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.rare_predicate_candidates',
                $$
                    SELECT p AS predicate_id, count(*) AS triple_count
                    FROM _pg_ripple.vp_rare
                    GROUP BY p
                    HAVING count(*) >= current_setting('pg_ripple.vp_promotion_threshold')::bigint
                $$,
                'IMMEDIATE'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.rare_predicate_candidates stream table: {}",
                e
            );
        });

        true
    }

    // ── pg_trickle SHACL violation monitors (v0.7.0, optional) ──────────────

    /// Enable SHACL violation monitors via pg_trickle stream tables.
    ///
    /// Creates the `_pg_ripple.violation_summary` stream table that aggregates
    /// `_pg_ripple.dead_letter_queue` by shape IRI and severity.  This avoids
    /// full `GROUP BY` scans of a potentially large dead-letter queue when
    /// monitoring dashboards or Prometheus `/metrics` poll for violation counts.
    ///
    /// The stream table is refreshed every 5 seconds by pg_trickle's IVM engine.
    ///
    /// Returns `true` if the stream table was created, `false` if pg_trickle is
    /// not installed.  No error is raised — pg_trickle is optional.
    ///
    /// ```sql
    /// SELECT pg_ripple.enable_shacl_monitors();
    /// -- Then query the summary:
    /// SELECT * FROM _pg_ripple.violation_summary;
    /// ```
    #[pg_extern]
    fn enable_shacl_monitors() -> bool {
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; SHACL violation monitors are unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_shacl_monitors() to enable."
            );
            return false;
        }

        // violation_summary — aggregate dead_letter_queue by shape + severity + graph.
        // Refreshed every 5 seconds via pg_trickle incremental view maintenance.
        // Reading the summary is an index scan on a small table rather than a
        // full GROUP BY over potentially millions of violation rows.
        pgrx::Spi::run(
            "SELECT pg_trickle.create_stream_table(
                '_pg_ripple.violation_summary',
                $$
                    SELECT
                        dlq.violation ->> 'shapeIRI'   AS shape_iri,
                        dlq.violation ->> 'severity'   AS severity,
                        dlq.g_id                       AS graph_id,
                        COUNT(*)                       AS violation_count,
                        MAX(dlq.detected_at)           AS last_seen
                    FROM _pg_ripple.dead_letter_queue dlq
                    GROUP BY 1, 2, 3
                $$,
                '5s'
            )",
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.violation_summary stream table: {}",
                e
            );
        });

        true
    }

    // ── pg_trickle SHACL DAG monitors (v0.8.0, optional) ────────────────────

    /// Enable multi-shape DAG validation via pg_trickle stream tables.
    ///
    /// For each active, compilable SHACL shape in `_pg_ripple.shacl_shapes`,
    /// creates a per-shape violation-detection stream table named
    /// `_pg_ripple.shacl_viol_{shape_suffix}` (refreshed in `IMMEDIATE` mode
    /// so violations are detected within the same transaction).  Supported
    /// constraint types: `sh:minCount`, `sh:maxCount`, `sh:datatype`,
    /// `sh:class`.  Complex combinators (`sh:or`, `sh:and`, `sh:not`,
    /// `sh:qualifiedValueShape`) are not compiled to stream tables; shapes
    /// that use only those constraints are skipped.
    ///
    /// After creating all per-shape tables, creates
    /// `_pg_ripple.violation_summary_dag` — a pg_trickle stream table (5 s
    /// refresh) that aggregates per-shape violation counts.  Because it reads
    /// from the per-shape stream tables, pg_trickle refreshes them in
    /// topological order (per-shape first, summary last).  When violations are
    /// resolved the summary automatically drops to zero — unlike the
    /// dead-letter-queue-based `_pg_ripple.violation_summary` from v0.7.0,
    /// which requires manual cleanup.
    ///
    /// Returns the number of per-shape stream tables created.  Returns 0 with
    /// a warning when pg_trickle is not installed.  No error is raised.
    ///
    /// ```sql
    /// -- Load shapes, then enable DAG monitors:
    /// SELECT pg_ripple.load_shacl('...');
    /// SELECT pg_ripple.enable_shacl_dag_monitors();
    /// -- Query the live summary:
    /// SELECT * FROM _pg_ripple.violation_summary_dag;
    /// ```
    #[pg_extern]
    fn enable_shacl_dag_monitors() -> i64 {
        crate::shacl::compile_dag_monitors()
    }

    /// Disable SHACL DAG monitors by dropping all per-shape violation stream
    /// tables and the `violation_summary_dag` aggregate table.
    ///
    /// Also clears the `_pg_ripple.shacl_dag_monitors` catalog.  Returns the
    /// number of per-shape stream tables dropped.
    ///
    /// ```sql
    /// SELECT pg_ripple.disable_shacl_dag_monitors();
    /// ```
    #[pg_extern]
    fn disable_shacl_dag_monitors() -> i64 {
        crate::shacl::drop_dag_monitors()
    }

    /// List all active SHACL DAG monitor stream tables.
    ///
    /// Returns one row per compiled shape with:
    /// - `shape_iri` — the shape's IRI
    /// - `stream_table` — fully-qualified name of the violation stream table
    /// - `constraints` — human-readable summary of compiled constraints
    ///
    /// ```sql
    /// SELECT * FROM pg_ripple.list_shacl_dag_monitors();
    /// ```
    #[pg_extern]
    fn list_shacl_dag_monitors() -> TableIterator<
        'static,
        (
            name!(shape_iri, String),
            name!(stream_table, String),
            name!(constraints, String),
        ),
    > {
        let rows = crate::shacl::list_dag_monitors();
        TableIterator::new(rows)
    }

    // ── Statistics (v0.6.0) ───────────────────────────────────────────────────

    /// Return extension statistics as JSONB.
    ///
    /// Includes total triple count, per-predicate storage sizes, delta/main
    /// split counts, and (when shared_preload_libraries is set) cache hit ratio.
    /// When pg_trickle is installed and `enable_live_statistics()` has been
    /// called, reads per-predicate counts from the `_pg_ripple.predicate_stats`
    /// stream table (instant, no full scan) instead of re-scanning VP tables.
    ///
    /// ```sql
    /// SELECT pg_ripple.stats();
    /// ```
    #[pg_extern]
    fn stats() -> pgrx::JsonB {
        // When pg_trickle live statistics are enabled, the total triple count
        // is read from the predicate_stats stream table (sum of triple_count
        // across all predicates) — this avoids a full VP table scan and
        // returns instantly.  Fall back to the full scan otherwise.
        let use_live_stats = crate::has_live_statistics();

        let total: i64 = if use_live_stats {
            pgrx::Spi::get_one::<i64>(
                "SELECT COALESCE(sum(triple_count), 0)::bigint \
                 FROM _pg_ripple.predicate_stats",
            )
            .unwrap_or(None)
            .unwrap_or_else(crate::storage::total_triple_count)
        } else {
            crate::storage::total_triple_count()
        };

        let pred_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let rare_count: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.vp_rare")
                .unwrap_or(None)
                .unwrap_or(0);

        let htap_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE htap = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let delta_rows: i64 =
            if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                crate::shmem::TOTAL_DELTA_ROWS
                    .get()
                    .load(std::sync::atomic::Ordering::Relaxed)
            } else {
                -1 // shmem not available (loaded without shared_preload_libraries)
            };

        let merge_pid: i32 = if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire)
        {
            crate::shmem::MERGE_WORKER_PID
                .get()
                .load(std::sync::atomic::Ordering::Relaxed)
        } else {
            0
        };

        let mut obj = serde_json::Map::new();
        obj.insert("total_triples".to_string(), serde_json::json!(total));
        obj.insert(
            "dedicated_predicates".to_string(),
            serde_json::json!(pred_count),
        );
        obj.insert("htap_predicates".to_string(), serde_json::json!(htap_count));
        obj.insert("rare_triples".to_string(), serde_json::json!(rare_count));
        obj.insert(
            "unmerged_delta_rows".to_string(),
            serde_json::json!(delta_rows),
        );
        obj.insert("merge_worker_pid".to_string(), serde_json::json!(merge_pid));
        obj.insert(
            "live_statistics_enabled".to_string(),
            serde_json::json!(use_live_stats),
        );

        // v0.6.0: encode cache statistics.
        let cache_utilization_pct = crate::shmem::cache_utilization_pct() as i64;
        let cache_capacity = crate::shmem::ENCODE_CACHE_CAPACITY as i64;
        obj.insert(
            "encode_cache_capacity".to_string(),
            serde_json::json!(cache_capacity),
        );
        obj.insert(
            "encode_cache_utilization_pct".to_string(),
            serde_json::json!(cache_utilization_pct),
        );

        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    // ── Change Data Capture (v0.6.0) ──────────────────────────────────────────

    /// Subscribe to triple change notifications on a NOTIFY channel.
    ///
    /// `pattern` is a predicate IRI (e.g. `<https://schema.org/name>`) or
    /// `'*'` for all predicates.  Notifications fire on INSERT and DELETE in
    /// the matching VP delta tables.  Returns the subscription ID.
    ///
    /// ```sql
    /// SELECT pg_ripple.subscribe('<https://schema.org/name>', 'my_channel');
    /// LISTEN my_channel;
    /// ```
    #[pg_extern]
    fn subscribe(pattern: &str, channel: &str) -> i64 {
        crate::cdc::subscribe(pattern, channel)
    }

    /// Remove all subscriptions for a notification channel.  Returns count removed.
    #[pg_extern]
    fn unsubscribe(channel: &str) -> i64 {
        crate::cdc::unsubscribe(channel)
    }

    // ── Subject / Object pattern index (v0.6.0) ───────────────────────────────

    /// Return the sorted array of predicate IDs for a given subject ID.
    ///
    /// Uses the `_pg_ripple.subject_patterns` index populated by the merge worker.
    /// Returns NULL if the subject has not been indexed yet (before first merge).
    #[pg_extern]
    fn subject_predicates(subject_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.subject_patterns WHERE s = $1",
            &[pgrx::datum::DatumWithOid::from(subject_id)],
        )
        .unwrap_or(None)
    }

    /// Return the sorted array of predicate IDs for a given object ID.
    ///
    /// Uses the `_pg_ripple.object_patterns` index populated by the merge worker.
    #[pg_extern]
    fn object_predicates(object_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.object_patterns WHERE o = $1",
            &[pgrx::datum::DatumWithOid::from(object_id)],
        )
        .unwrap_or(None)
    }

    // ── SHACL management (v0.7.0) ─────────────────────────────────────────────

    /// Load and store SHACL shapes from Turtle-formatted text.
    ///
    /// Parses the Turtle, extracts all NodeShape and PropertyShape definitions,
    /// and upserts them into `_pg_ripple.shacl_shapes`.  Returns the number of
    /// shapes loaded.
    #[pg_extern]
    fn load_shacl(data: &str) -> i32 {
        use crate::shacl::parse_and_store_shapes;
        parse_and_store_shapes(data)
    }

    /// Run a full SHACL validation report against all active shapes.
    ///
    /// `graph` selects which graph to validate:
    /// - NULL or empty string: default graph (id 0)
    /// - `'*'`: all graphs
    /// - An IRI: the named graph with that IRI
    ///
    /// Returns a SHACL validation report as JSONB with `conforms` and `violations`.
    #[pg_extern]
    fn validate(graph: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        crate::shacl::run_validate(graph)
    }

    /// Return a table of all loaded SHACL shapes.
    #[pg_extern]
    fn list_shapes() -> TableIterator<'static, (name!(shape_iri, String), name!(active, bool))> {
        let rows = pgrx::Spi::connect(|c| {
            let tup = c
                .select(
                    "SELECT shape_iri, active FROM _pg_ripple.shacl_shapes ORDER BY shape_iri",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_shapes SPI error: {e}"));
            let mut out: Vec<(String, bool)> = Vec::new();
            for row in tup {
                let iri: String = row.get::<&str>(1).unwrap_or(None).unwrap_or("").to_owned();
                let active: bool = row.get::<bool>(2).unwrap_or(None).unwrap_or(false);
                out.push((iri, active));
            }
            out
        });
        TableIterator::new(rows)
    }

    /// Deactivate and remove a SHACL shape by its IRI.
    ///
    /// Returns 1 if the shape was found and removed, 0 if not found.
    #[pg_extern]
    fn drop_shape(shape_uri: &str) -> i32 {
        let rows_deleted = pgrx::Spi::get_one_with_args::<i64>(
            "DELETE FROM _pg_ripple.shacl_shapes WHERE shape_iri = $1 RETURNING 1",
            &[pgrx::datum::DatumWithOid::from(shape_uri)],
        )
        .unwrap_or(None)
        .unwrap_or(0);
        rows_deleted as i32
    }

    // ── Async validation pipeline (v0.8.0) ────────────────────────────────────

    /// Manually process up to `batch_size` items from the async validation queue.
    ///
    /// Violations found are moved to `_pg_ripple.dead_letter_queue`.
    /// Returns the number of items processed.
    ///
    /// Normally the background worker processes the queue automatically when
    /// `pg_ripple.shacl_mode = 'async'`.  This function is useful for testing
    /// or for draining the queue on demand.
    #[pg_extern]
    fn process_validation_queue(batch_size: default!(i64, "1000")) -> i64 {
        crate::shacl::process_validation_batch(batch_size)
    }

    /// Return the number of items currently pending in the async validation queue.
    #[pg_extern]
    fn validation_queue_length() -> i64 {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.validation_queue")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    /// Return the number of items in the dead-letter queue (async violations).
    #[pg_extern]
    fn dead_letter_count() -> i64 {
        pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dead_letter_queue")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    /// Return all entries in the dead-letter queue as JSONB.
    ///
    /// Each row includes `s_id`, `p_id`, `o_id`, `g_id`, `stmt_id`,
    /// `violation` (JSONB), and `detected_at` (timestamptz).
    #[pg_extern]
    fn dead_letter_queue() -> pgrx::JsonB {
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            let tup = c
                .select(
                    "SELECT s_id, p_id, o_id, g_id, stmt_id, violation::text, detected_at::text \
                     FROM _pg_ripple.dead_letter_queue ORDER BY id ASC",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("dead_letter_queue SPI error: {e}"));
            let mut out: Vec<serde_json::Value> = Vec::new();
            for row in tup {
                let s_id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let p_id: i64 = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                let o_id: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                let g_id: i64 = row.get::<i64>(4).ok().flatten().unwrap_or(0);
                let stmt_id: i64 = row.get::<i64>(5).ok().flatten().unwrap_or(0);
                let violation_text: String =
                    row.get::<&str>(6).ok().flatten().unwrap_or("").to_owned();
                let detected_at: String =
                    row.get::<&str>(7).ok().flatten().unwrap_or("").to_owned();
                let violation_json: serde_json::Value =
                    serde_json::from_str(&violation_text).unwrap_or(serde_json::Value::Null);
                out.push(serde_json::json!({
                    "s_id": s_id,
                    "p_id": p_id,
                    "o_id": o_id,
                    "g_id": g_id,
                    "stmt_id": stmt_id,
                    "violation": violation_json,
                    "detected_at": detected_at
                }));
            }
            out
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Clear all entries from the dead-letter queue.
    ///
    /// Returns the number of rows deleted.
    #[pg_extern]
    fn drain_dead_letter_queue() -> i64 {
        pgrx::Spi::get_one::<i64>("DELETE FROM _pg_ripple.dead_letter_queue RETURNING 1")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    // ── Deduplication functions (v0.7.0) ──────────────────────────────────────

    /// Remove duplicate `(s, o, g)` rows for a single predicate, keeping the
    /// row with the lowest SID (oldest assertion).
    ///
    /// - In `_delta` tables: uses DELETE with a `ctid NOT IN (MIN(ctid) GROUP BY s,o,g)` pattern.
    /// - In `_main` tables: inserts tombstone rows for duplicate-SID rows so they are
    ///   filtered at query time and removed on the next merge cycle.
    /// - In `vp_rare`: uses the same MIN(ctid) pattern with a `p = pred_id` filter.
    ///
    /// Runs ANALYZE on all affected tables after deduplication.
    /// Returns the total count of rows removed.
    #[pg_extern]
    fn deduplicate_predicate(p_iri: &str) -> i64 {
        crate::storage::deduplicate_predicate(p_iri)
    }

    /// Remove duplicate `(s, o, g)` rows across all predicates and `vp_rare`.
    ///
    /// Applies `deduplicate_predicate` for each predicate with a dedicated VP table,
    /// then deduplicates `vp_rare`.
    /// Returns the total count of rows removed.
    #[pg_extern]
    fn deduplicate_all() -> i64 {
        crate::storage::deduplicate_all()
    }

    // ── Datalog Reasoning Engine (v0.10.0) ────────────────────────────────────

    /// Load Datalog rules from a text string.
    ///
    /// `rules` is a Turtle-flavoured Datalog rule set.
    /// `rule_set` is the name for this group of rules (default: 'custom').
    /// Returns the number of rules stored.
    #[pg_extern]
    fn load_rules(rules: &str, rule_set: default!(&str, "'custom'")) -> i64 {
        crate::datalog::builtins::register_standard_prefixes();
        let rule_set_ir = match crate::datalog::parse_rules(rules, rule_set) {
            Ok(rs) => rs,
            Err(e) => pgrx::error!("rule parse error: {e}"),
        };
        crate::datalog::store_rules(rule_set, &rule_set_ir.rules)
    }

    /// Load a built-in rule set by name.
    ///
    /// Supported names: `'rdfs'`, `'owl-rl'`.
    /// Returns the number of rules stored.
    #[pg_extern]
    fn load_rules_builtin(name: &str) -> i64 {
        crate::datalog::builtins::register_standard_prefixes();
        let text = match crate::datalog::builtins::get_builtin_rules(name) {
            Ok(t) => t,
            Err(e) => pgrx::error!("{e}"),
        };
        let rule_set_ir = match crate::datalog::parse_rules(text, name) {
            Ok(rs) => rs,
            Err(e) => pgrx::error!("built-in rule parse error: {e}"),
        };
        crate::datalog::store_rules(name, &rule_set_ir.rules)
    }

    /// List all stored Datalog rules as JSONB rows.
    ///
    /// Returns one row per rule with fields: id, rule_set, rule_text, head_pred,
    /// stratum, is_recursive, active.
    #[pg_extern]
    fn list_rules() -> pgrx::JsonB {
        crate::datalog::ensure_catalog();
        let rows = pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT id, rule_set, rule_text, head_pred, stratum, is_recursive, active \
                     FROM _pg_ripple.rules \
                     ORDER BY rule_set, stratum, id",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_rules SPI error: {e}"))
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "id".to_owned(),
                        row.get::<i64>(1)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::from)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "rule_set".to_owned(),
                        row.get::<String>(2)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "rule_text".to_owned(),
                        row.get::<String>(3)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "stratum".to_owned(),
                        row.get::<i32>(5)
                            .ok()
                            .flatten()
                            .map(|v| serde_json::Value::from(v as i64))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "is_recursive".to_owned(),
                        row.get::<bool>(6)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::Bool)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "active".to_owned(),
                        row.get::<bool>(7)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::Bool)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    serde_json::Value::Object(obj)
                })
                .collect::<Vec<_>>()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Drop all rules in the named rule set.
    ///
    /// Returns the number of rules deleted.
    #[pg_extern]
    fn drop_rules(rule_set: &str) -> i64 {
        crate::datalog::ensure_catalog();
        pgrx::Spi::get_one_with_args::<i64>(
            "WITH deleted AS ( \
                 DELETE FROM _pg_ripple.rules WHERE rule_set = $1 RETURNING 1 \
             ) SELECT count(*) FROM deleted",
            &[pgrx::datum::DatumWithOid::from(rule_set)],
        )
        .unwrap_or(None)
        .unwrap_or(0)
    }

    /// Enable a named rule set (set active = true).
    #[pg_extern]
    fn enable_rule_set(name: &str) {
        crate::datalog::ensure_catalog();
        let _ = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.rules SET active = true WHERE rule_set = $1; \
             UPDATE _pg_ripple.rule_sets SET active = true WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(name)],
        );
    }

    /// Disable a named rule set (set active = false) without dropping it.
    #[pg_extern]
    fn disable_rule_set(name: &str) {
        crate::datalog::ensure_catalog();
        let _ = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.rules SET active = false WHERE rule_set = $1; \
             UPDATE _pg_ripple.rule_sets SET active = false WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(name)],
        );
    }

    /// Run inference for the named rule set and materialise derived triples.
    ///
    /// Returns the number of triples derived.
    #[pg_extern]
    fn infer(rule_set: default!(&str, "'custom'")) -> i64 {
        crate::datalog::run_inference(rule_set)
    }

    /// Check all active constraint rules and return violations as JSONB.
    ///
    /// Each element has fields: `rule` (text), `violated` (bool).
    /// Pass `rule_set` to check only that rule set; pass NULL to check all.
    #[pg_extern]
    fn check_constraints(rule_set: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        let violations = crate::datalog::check_all_constraints(rule_set);
        pgrx::JsonB(serde_json::Value::Array(
            violations.into_iter().map(|v| v.0).collect(),
        ))
    }

    /// Prewarm the hot dictionary table by copying short IRIs and predicates.
    ///
    /// Returns the number of rows in the hot table after prewarm.
    #[pg_extern]
    fn prewarm_dictionary_hot() -> i64 {
        crate::dictionary::hot::ensure_hot_table();
        crate::dictionary::hot::prewarm_hot_table();
        pgrx::Spi::get_one::<i64>("SELECT count(*) FROM _pg_ripple.dictionary_hot")
            .unwrap_or(None)
            .unwrap_or(0)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_encode_decode_roundtrip() {
        let id = crate::dictionary::encode("https://example.org/subject", 0);
        let decoded = crate::dictionary::decode(id).expect("decode should succeed");
        assert_eq!(decoded, "https://example.org/subject");
    }

    #[pg_test]
    fn test_insert_and_count() {
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            0,
        );
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_typed_literal_roundtrip() {
        // xsd:integer is now inline-encoded (bit 63 = 1, no dictionary row).
        let id = crate::dictionary::encode_typed_literal(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        );
        assert!(
            crate::dictionary::inline::is_inline(id),
            "xsd:integer should be inline-encoded"
        );
        // decode() must still return the correct N-Triples literal string.
        let decoded = crate::dictionary::decode(id).expect("decode should succeed for inline");
        assert_eq!(
            decoded,
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[pg_test]
    fn test_lang_literal_roundtrip() {
        let id = crate::dictionary::encode_lang_literal("hello", "en");
        let full = crate::dictionary::decode_full(id).expect("decode_full should succeed");
        assert_eq!(full.value, "hello");
        assert_eq!(full.lang.as_deref(), Some("en"));
    }

    #[pg_test]
    fn test_ntriples_bulk_load() {
        let data =
            "<https://example.org/a> <https://example.org/knows> <https://example.org/b> .\n";
        let count = crate::bulk_load::load_ntriples(data);
        assert_eq!(count, 1);
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_turtle_bulk_load() {
        let data = "@prefix ex: <https://example.org/> .\nex:x ex:rel ex:y .\n";
        let count = crate::bulk_load::load_turtle(data);
        assert_eq!(count, 1);
    }

    #[pg_test]
    fn test_named_graph_drop() {
        let graph = "<https://example.org/mygraph>";
        let g_id = crate::storage::create_graph(graph);
        assert!(g_id > 0);
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            g_id,
        );
        let deleted = crate::storage::drop_graph(graph);
        assert!(deleted >= 1);
    }

    #[pg_test]
    fn test_export_ntriples_roundtrip() {
        let nt =
            "<https://example.org/ex> <https://example.org/pred> <https://example.org/obj> .\n";
        crate::bulk_load::load_ntriples(nt);
        let exported = crate::export::export_ntriples(None);
        assert!(exported.contains("<https://example.org/pred>"));
    }

    // ─── SPARQL engine tests (v0.3.0) ─────────────────────────────────────────

    /// A SELECT that returns no rows on an empty store must produce an empty set.
    #[pg_test]
    fn pg_test_sparql_select_empty() {
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 0, "expected no rows on empty store");
    }

    /// After loading one triple, SELECT ?s ?p ?o must return exactly one row.
    #[pg_test]
    fn pg_test_sparql_select_one_triple() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n",
        );
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 1, "expected exactly one row");
        // The row must contain a non-null ?s binding.
        let obj = rows[0].0.as_object().expect("row must be a JSON object");
        assert!(obj.contains_key("s"), "row must have ?s binding");
        assert!(obj.contains_key("p"), "row must have ?p binding");
        assert!(obj.contains_key("o"), "row must have ?o binding");
    }

    /// sparql_ask() on an empty store returns false.
    #[pg_test]
    fn pg_test_sparql_ask_empty() {
        let result = crate::sparql::sparql_ask("ASK { ?s ?p ?o }");
        assert!(!result, "ASK on empty store must be false");
    }

    /// sparql_ask() returns true after a matching triple is inserted.
    #[pg_test]
    fn pg_test_sparql_ask_match() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/x> <https://example.org/q> <https://example.org/y> .\n",
        );
        let result =
            crate::sparql::sparql_ask("ASK { <https://example.org/x> <https://example.org/q> ?o }");
        assert!(result, "ASK must be true after matching triple loaded");
    }

    /// sparql_explain() returns non-empty SQL for a simple SELECT.
    #[pg_test]
    fn pg_test_sparql_explain_returns_sql() {
        let plan = crate::sparql::sparql_explain(
            "SELECT ?s WHERE { ?s <https://example.org/p> ?o }",
            false,
        );
        assert!(
            plan.contains("Generated SQL"),
            "explain output must contain 'Generated SQL'"
        );
    }

    /// SPARQL LIMIT 1 must return at most one row.
    #[pg_test]
    fn pg_test_sparql_limit() {
        // Load two triples.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
        );
        let rows =
            crate::sparql::sparql("SELECT ?s ?o WHERE { ?s <https://example.org/p> ?o } LIMIT 1");
        assert!(rows.len() <= 1, "LIMIT 1 must return at most one row");
    }

    // ─── RDF-star / Statement Identifiers tests (v0.4.0) ──────────────────────

    /// N-Triples-star: loading an object-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_object_position() {
        let n = crate::bulk_load::load_ntriples(
            "<https://example.org/eve> <https://example.org/said> \
             << <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> .\n",
        );
        assert_eq!(n, 1, "object-position quoted triple must load as 1 triple");
    }

    /// N-Triples-star: loading a subject-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_subject_position() {
        let n = crate::bulk_load::load_ntriples(
            "<< <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> <https://example.org/certainty> \
             \"0.9\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n",
        );
        assert_eq!(n, 1, "subject-position quoted triple must load as 1 triple");
    }

    /// encode_quoted_triple / decode_quoted_triple_components round-trip.
    #[pg_test]
    fn pg_test_quoted_triple_encode_decode() {
        let s_id =
            crate::dictionary::encode("https://example.org/alice", crate::dictionary::KIND_IRI);
        let p_id =
            crate::dictionary::encode("https://example.org/knows", crate::dictionary::KIND_IRI);
        let o_id =
            crate::dictionary::encode("https://example.org/bob", crate::dictionary::KIND_IRI);
        let qt_id = crate::dictionary::encode_quoted_triple(s_id, p_id, o_id);
        assert!(qt_id != 0, "quoted triple must have a non-zero ID");
        let components = crate::dictionary::decode_quoted_triple_components(qt_id);
        assert!(components.is_some(), "decode must return Some");
        let (ds, dp, ob) = components.unwrap();
        assert_eq!(ds, s_id);
        assert_eq!(dp, p_id);
        assert_eq!(ob, o_id);
    }

    /// insert_triple returns a positive SID; get_statement can look it back up.
    #[pg_test]
    fn pg_test_statement_identifier_lifecycle() {
        let sid = crate::storage::insert_triple(
            "<https://example.org/subject1>",
            "<https://example.org/predicate1>",
            "<https://example.org/object1>",
            0,
        );
        assert!(sid > 0, "insert must return a positive SID");
    }

    /// SPARQL DISTINCT must deduplicate results.
    #[pg_test]
    fn pg_test_sparql_distinct() {
        // Two triples sharing the same predicate and object.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/same> <https://example.org/o> .\n\
             <https://example.org/s2> <https://example.org/same> <https://example.org/o> .\n",
        );
        // Select just ?o — should be deduplicated to 1 row.
        let rows =
            crate::sparql::sparql("SELECT DISTINCT ?o WHERE { ?s <https://example.org/same> ?o }");
        assert_eq!(rows.len(), 1, "DISTINCT ?o must collapse duplicates");
    }

    /// FILTER with a bound IRI constant must restrict results correctly.
    #[pg_test]
    fn pg_test_sparql_filter_bound() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
        );
        // Only one subject matches the binding of ?s to s1.
        let rows = crate::sparql::sparql(
            "SELECT ?o WHERE { <https://example.org/s1> <https://example.org/p> ?o }",
        );
        assert_eq!(rows.len(), 1, "bound subject must restrict to one row");
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["allow_system_table_mods = on"]
    }
}
