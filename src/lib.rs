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
mod framing;
mod fts;
mod shacl;
mod shmem;
mod sparql;
mod storage;
mod views;
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

// v0.14.0: Graph-level RLS and administrative catalog tables.
pgrx::extension_sql!(
    r#"
-- Graph access control mapping (v0.14.0)
-- permission: 'read', 'write', or 'admin'
CREATE TABLE IF NOT EXISTS _pg_ripple.graph_access (
    role_name  TEXT   NOT NULL,
    graph_id   BIGINT NOT NULL,
    permission TEXT   NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
    PRIMARY KEY (role_name, graph_id, permission)
);
CREATE INDEX IF NOT EXISTS idx_graph_access_role
    ON _pg_ripple.graph_access (role_name);
CREATE INDEX IF NOT EXISTS idx_graph_access_graph
    ON _pg_ripple.graph_access (graph_id);

-- Live schema summary (v0.14.0 pg_trickle optional)
-- Populated by enable_schema_summary(); used by schema_summary().
CREATE TABLE IF NOT EXISTS _pg_ripple.inferred_schema (
    class_iri    TEXT   NOT NULL,
    property_iri TEXT   NOT NULL,
    cardinality  BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY  (class_iri, property_iri)
);
"#,
    name = "rls_schema_setup",
    requires = ["views_schema_setup"]
);

// v0.16.0: SPARQL federation endpoint allowlist and health monitoring.
pgrx::extension_sql!(
    r#"
-- Federation endpoint allowlist (v0.16.0, extended v0.19.0)
-- Only endpoints with enabled = true are contacted via SERVICE clauses.
-- local_view_name: when set, SERVICE is rewritten to scan the named stream table.
-- complexity (v0.19.0): 'fast', 'normal', or 'slow' — used to order multi-endpoint queries.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_endpoints (
    url             TEXT    NOT NULL PRIMARY KEY,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    local_view_name TEXT,
    complexity      TEXT    NOT NULL DEFAULT 'normal'
                    CHECK (complexity IN ('fast', 'normal', 'slow'))
);

-- Federation health log (v0.16.0, used when pg_trickle is installed)
-- Rolling probe log: executor writes here after each SERVICE call.
-- Used by is_endpoint_healthy() to skip endpoints with success_rate < 10%.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_health (
    id          BIGSERIAL   PRIMARY KEY,
    url         TEXT        NOT NULL,
    success     BOOLEAN     NOT NULL,
    latency_ms  BIGINT      NOT NULL DEFAULT 0,
    probed_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_federation_health_url_time
    ON _pg_ripple.federation_health (url, probed_at DESC);
"#,
    name = "federation_schema_setup",
    requires = ["rls_schema_setup"]
);

// v0.19.0: federation result cache table.
pgrx::extension_sql!(
    r#"
-- Federation result cache (v0.19.0)
-- Caches SPARQL SELECT results from remote endpoints keyed by (url, query_hash).
-- TTL-based expiry; expired rows are cleaned up by the merge background worker.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache (
    url         TEXT        NOT NULL,
    query_hash  BIGINT      NOT NULL,
    result_jsonb JSONB      NOT NULL,
    cached_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (url, query_hash)
);
CREATE INDEX IF NOT EXISTS idx_federation_cache_expires
    ON _pg_ripple.federation_cache (expires_at);
"#,
    name = "v019_federation_cache_setup",
    requires = ["federation_schema_setup"]
);

// v0.11.0: SPARQL views, Datalog views, and ExtVP catalog tables.
pgrx::extension_sql!(
    r#"
-- SPARQL views catalog (v0.11.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.sparql_views (
    name          TEXT        NOT NULL PRIMARY KEY,
    sparql        TEXT        NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    decode        BOOLEAN     NOT NULL DEFAULT false,
    stream_table  TEXT        NOT NULL,
    variables     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Datalog views catalog (v0.11.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.datalog_views (
    name          TEXT        NOT NULL PRIMARY KEY,
    rules         TEXT,
    rule_set      TEXT        NOT NULL,
    goal          TEXT        NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    decode        BOOLEAN     NOT NULL DEFAULT false,
    stream_table  TEXT        NOT NULL,
    variables     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ExtVP semi-join tables catalog (v0.11.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.extvp_tables (
    name          TEXT        NOT NULL PRIMARY KEY,
    pred1_iri     TEXT        NOT NULL,
    pred2_iri     TEXT        NOT NULL,
    pred1_id      BIGINT      NOT NULL,
    pred2_id      BIGINT      NOT NULL,
    generated_sql TEXT        NOT NULL,
    schedule      TEXT        NOT NULL,
    stream_table  TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_extvp_pred1 ON _pg_ripple.extvp_tables (pred1_id);
CREATE INDEX IF NOT EXISTS idx_extvp_pred2 ON _pg_ripple.extvp_tables (pred2_id);
"#,
    name = "views_schema_setup",
    requires = ["datalog_schema_setup"]
);

// v0.17.0: Framing views catalog table.
pgrx::extension_sql!(
    r#"
-- Framing views catalog (v0.17.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.framing_views (
    name               TEXT        NOT NULL PRIMARY KEY,
    frame              JSONB       NOT NULL,
    generated_construct TEXT       NOT NULL,
    schedule           TEXT        NOT NULL,
    output_format      TEXT        NOT NULL DEFAULT 'jsonld',
    decode             BOOLEAN     NOT NULL DEFAULT false,
    stream_table_oid   OID,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "framing_views_schema_setup",
    requires = ["views_schema_setup"]
);

// v0.18.0: CONSTRUCT, DESCRIBE, and ASK view catalog tables.
pgrx::extension_sql!(
    r#"
-- CONSTRUCT views catalog (v0.18.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.construct_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    decode         BOOLEAN     NOT NULL DEFAULT false,
    template_count BIGINT      NOT NULL DEFAULT 0,
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- DESCRIBE views catalog (v0.18.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.describe_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    decode         BOOLEAN     NOT NULL DEFAULT false,
    strategy       TEXT        NOT NULL DEFAULT 'cbd',
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ASK views catalog (v0.18.0)
CREATE TABLE IF NOT EXISTS _pg_ripple.ask_views (
    name           TEXT        NOT NULL PRIMARY KEY,
    sparql         TEXT        NOT NULL,
    generated_sql  TEXT        NOT NULL,
    schedule       TEXT        NOT NULL,
    stream_table   TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Helper function for DESCRIBE views: enumerate all triples for a resource.
-- For cbd (include_incoming=false): outgoing arcs only.
-- For scbd (include_incoming=true): outgoing + incoming arcs.
CREATE OR REPLACE FUNCTION _pg_ripple.triples_for_resource(
    resource_id     BIGINT,
    include_incoming BOOLEAN DEFAULT false
) RETURNS TABLE(s BIGINT, p BIGINT, o BIGINT)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    r RECORD;
BEGIN
    -- Outgoing arcs from rare predicates table.
    RETURN QUERY SELECT vr.s, vr.p, vr.o
                 FROM _pg_ripple.vp_rare vr
                 WHERE vr.s = resource_id;

    -- Outgoing arcs from dedicated VP tables.
    FOR r IN
        SELECT pc.id AS pred_id
        FROM _pg_ripple.predicates pc
        WHERE pc.table_oid IS NOT NULL
    LOOP
        RETURN QUERY EXECUTE format(
            'SELECT s, %L::bigint AS p, o FROM _pg_ripple.vp_%s WHERE s = $1',
            r.pred_id, r.pred_id
        ) USING resource_id;
    END LOOP;

    IF include_incoming THEN
        -- Incoming arcs from rare predicates table.
        RETURN QUERY SELECT vr.s, vr.p, vr.o
                     FROM _pg_ripple.vp_rare vr
                     WHERE vr.o = resource_id;

        -- Incoming arcs from dedicated VP tables.
        FOR r IN
            SELECT pc.id AS pred_id
            FROM _pg_ripple.predicates pc
            WHERE pc.table_oid IS NOT NULL
        LOOP
            RETURN QUERY EXECUTE format(
                'SELECT s, %L::bigint AS p, o FROM _pg_ripple.vp_%s WHERE o = $1',
                r.pred_id, r.pred_id
            ) USING resource_id;
        END LOOP;
    END IF;
END;
$$;
"#,
    name = "v018_views_schema_setup",
    requires = ["framing_views_schema_setup"]
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
        c"Minimum triple count before a predicate gets its own VP table (default: 1000, range: 10–10,000,000)",
        c"",
        &VPP_THRESHOLD,
        10,
        10_000_000,
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

    // ── v0.13.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.bgp_reorder",
        c"Reorder BGP triple patterns by estimated selectivity before SQL generation (default: on)",
        c"",
        &BGP_REORDER,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.parallel_query_min_joins",
        c"Minimum number of VP-table joins before enabling parallel query workers (default: 3)",
        c"",
        &PARALLEL_QUERY_MIN_JOINS,
        1,
        100,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.14.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.rls_bypass",
        c"Superuser override: when on, graph-level RLS policies are bypassed for this session (default: off)",
        c"",
        &RLS_BYPASS,
        GucContext::Suset,
        GucFlags::default(),
    );

    // ── v0.16.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_timeout",
        c"Per-SERVICE-call wall-clock timeout in seconds (default: 30)",
        c"",
        &FEDERATION_TIMEOUT,
        1,
        3600,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_max_results",
        c"Maximum rows accepted from a single remote SERVICE call (default: 10000)",
        c"",
        &FEDERATION_MAX_RESULTS,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.federation_on_error",
        c"Behaviour on SERVICE call failure: 'warning' (default), 'error', or 'empty'",
        c"",
        &FEDERATION_ON_ERROR,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.19.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_pool_size",
        c"Idle connections per remote endpoint kept in the thread-local HTTP pool (default: 4, range: 1-32)",
        c"",
        &FEDERATION_POOL_SIZE,
        1,
        32,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_cache_ttl",
        c"TTL in seconds for cached SERVICE results; 0 disables caching (default: 0, range: 0-86400)",
        c"",
        &FEDERATION_CACHE_TTL,
        0,
        86400,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.federation_on_partial",
        c"Behaviour on mid-stream SERVICE failure: 'empty' (default, discard) or 'use' (keep partial rows)",
        c"",
        &FEDERATION_ON_PARTIAL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.federation_adaptive_timeout",
        c"When on, derive per-endpoint timeout from P95 latency in federation_health (default: off)",
        c"",
        &FEDERATION_ADAPTIVE_TIMEOUT,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.21.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.sparql_strict",
        c"When on (default), unsupported SPARQL FILTER functions raise ERRCODE_FEATURE_NOT_SUPPORTED; \
          when off, they are silently dropped for backward compatibility",
        c"",
        &SPARQL_STRICT,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.24.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.property_path_max_depth",
        c"Maximum recursion depth for SPARQL property path queries (p+ and p*); default: 64, min: 1, max: 100000",
        c"",
        &PROPERTY_PATH_MAX_DEPTH,
        1,
        100_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.auto_analyze",
        c"When on (default), run ANALYZE on VP main tables after each merge cycle to keep planner statistics current",
        c"",
        &AUTO_ANALYZE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.export_batch_size",
        c"Number of triples fetched per cursor batch during streaming export (default: 10000, min: 100, max: 1000000)",
        c"",
        &EXPORT_BATCH_SIZE,
        100,
        1_000_000,
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

    // ── Transaction callbacks (v0.22.0) ───────────────────────────────────────
    // Register transaction callback to clear the dictionary cache on abort.
    // This ensures rolled-back dictionary entries (from INSERT INTO dictionary
    // during a failed transaction) do not persist in the backend-local cache,
    // preventing phantom references (v0.22.0 critical fix C-2).
    register_xact_callback();

    // Schema and base tables are created by the `schema_setup` extension_sql!
    // block, which runs inside the CREATE EXTENSION transaction where SPI and
    // DDL are available.  Nothing to do here.
}

// ─── Transaction callbacks (v0.22.0) ──────────────────────────────────────────

/// Register a transaction callback to clear the dictionary cache on abort.
///
/// This prevents rolled-back dictionary entries from persisting in the
/// backend-local cache, which would create phantom references in subsequent
/// transactions (critical fix C-2).
fn register_xact_callback() {
    unsafe {
        // SAFETY: RegisterXactCallback is a standard PostgreSQL callback mechanism
        // for transaction events. We register a C-compatible callback that will be
        // called at various transaction events. The callback uses only Rust code
        // (clear_caches) which has no dependencies on PG's signal handling, so it
        // is safe to call from a callback context.
        pg_sys::RegisterXactCallback(Some(xact_callback_c), std::ptr::null_mut());
    }
}

/// C-compatible transaction callback wrapper.
///
/// PostgreSQL calls this callback with XactEvent and an opaque arg pointer.
/// We forward to the Rust clear_caches function only on XACT_EVENT_ABORT and
/// XACT_EVENT_PARALLEL_ABORT events.
#[allow(non_snake_case)]
unsafe extern "C-unwind" fn xact_callback_c(event: u32, _arg: *mut std::ffi::c_void) {
    // XactEvent is an enum in PostgreSQL, and the event is passed as a u32.
    // We check against the enum discriminants for ABORT events:
    // XACT_EVENT_ABORT = 0, XACT_EVENT_PARALLEL_ABORT = 4
    // See src/include/access/xact.h in PostgreSQL source.
    if event == 0 || event == 4 {
        crate::dictionary::clear_caches();
    }
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

    /// Return shared-memory encode cache statistics (v0.22.0+).
    ///
    /// Returns (hits, misses, evictions, utilisation) where:
    /// - `hits`: count of cache hits since startup
    /// - `misses`: count of cache misses since startup
    /// - `evictions`: count of cache evictions since startup
    /// - `utilisation`: fraction of cache capacity in use (0.0–1.0)
    ///
    /// Returns (0, 0, 0, 0.0) when shmem is not initialized.
    #[pg_extern]
    fn cache_stats() -> pgrx::JsonB {
        let (hits, misses, evictions, utilisation) = crate::shmem::get_cache_stats();
        let mut obj = serde_json::Map::new();
        obj.insert("hits".to_string(), serde_json::json!(hits));
        obj.insert("misses".to_string(), serde_json::json!(misses));
        obj.insert("evictions".to_string(), serde_json::json!(evictions));
        obj.insert("utilisation".to_string(), serde_json::json!(utilisation));
        pgrx::JsonB(serde_json::Value::Object(obj))
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

    // ── JSON-LD Framing (v0.17.0) ─────────────────────────────────────────────

    /// Translate a JSON-LD frame to a SPARQL CONSTRUCT query string.
    ///
    /// Primary inspection and debugging tool: shows the generated CONSTRUCT
    /// query without executing it. `graph` restricts to a named graph when set.
    #[pg_extern]
    fn jsonld_frame_to_sparql(frame: pgrx::JsonB, graph: default!(Option<&str>, "NULL")) -> String {
        let val = &frame.0;
        crate::framing::frame_to_sparql(val, graph).unwrap_or_else(|e| pgrx::error!("{}", e))
    }

    /// Primary end-user function: translate a JSON-LD frame into a SPARQL
    /// CONSTRUCT query, execute it, apply the W3C embedding algorithm, compact
    /// with the frame's `@context`, and return the framed JSON-LD document.
    #[pg_extern]
    fn export_jsonld_framed(
        frame: pgrx::JsonB,
        graph: default!(Option<&str>, "NULL"),
        embed: default!(&str, "'@once'"),
        explicit: default!(bool, "false"),
        ordered: default!(bool, "false"),
    ) -> pgrx::JsonB {
        let val = &frame.0;
        let result = crate::framing::frame_and_execute(val, graph, embed, explicit, ordered)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        pgrx::JsonB(result)
    }

    /// Streaming variant of `export_jsonld_framed` — returns one NDJSON line
    /// per matched root node. Avoids buffering large framed documents in memory.
    #[pg_extern]
    fn export_jsonld_framed_stream(
        frame: pgrx::JsonB,
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let val = frame.0.clone();
        let lines = crate::framing::execute_framed_stream(&val, graph)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    /// General-purpose framing primitive: apply the W3C JSON-LD Framing
    /// embedding algorithm to any already-expanded JSON-LD JSONB document.
    ///
    /// `input` is expected to be a JSON-LD array of expanded node objects.
    /// Useful for framing SPARQL CONSTRUCT results obtained via other means.
    #[pg_extern]
    fn jsonld_frame(
        input: pgrx::JsonB,
        frame: pgrx::JsonB,
        embed: default!(&str, "'@once'"),
        explicit: default!(bool, "false"),
        ordered: default!(bool, "false"),
    ) -> pgrx::JsonB {
        let result = crate::framing::frame_jsonld(&input.0, &frame.0, embed, explicit, ordered)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        pgrx::JsonB(result)
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

    /// Explain a SPARQL query with flexible output format (v0.23.0).
    ///
    /// `format` may be one of:
    /// - `'sql'`             — return the generated SQL without executing it
    /// - `'text'` (default)  — run EXPLAIN (ANALYZE, FORMAT TEXT)
    /// - `'json'`            — run EXPLAIN (ANALYZE, FORMAT JSON)
    /// - `'sparql_algebra'`  — return the spargebra algebra tree
    #[pg_extern]
    fn explain_sparql(query: &str, format: default!(&str, "'text'")) -> String {
        crate::sparql::explain_sparql(query, format)
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

    // ── Plan cache monitoring (v0.13.0) ──────────────────────────────────────

    /// Return SPARQL plan cache statistics as JSONB.
    ///
    /// Returns `{"hits": N, "misses": N, "size": N, "capacity": N, "hit_rate": 0.xx}`.
    /// Counters accumulate from backend start; reset with `plan_cache_reset()`.
    #[pg_extern]
    fn plan_cache_stats() -> pgrx::JsonB {
        crate::sparql::plan_cache_stats()
    }

    /// Evict all cached SPARQL plan translations and reset hit/miss counters.
    #[pg_extern]
    fn plan_cache_reset() {
        crate::sparql::plan_cache_reset()
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

    // ── Administrative functions (v0.14.0) ───────────────────────────────────

    /// Force a full delta→main merge on all HTAP VP tables, then run
    /// PostgreSQL VACUUM on every VP table (delta, main, tombstones).
    ///
    /// Returns the number of VP tables vacuumed.
    #[pg_extern]
    fn vacuum() -> i64 {
        // Merge first so VACUUM sees the final row set.
        crate::storage::merge::compact();

        // Collect all HTAP predicate IDs.
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("vacuum: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        let mut vacuumed = 0i64;
        for p_id in &pred_ids {
            // VACUUM cannot run inside a transaction block, so we use
            // ANALYZE instead, which has the same effect on planner statistics
            // and can run inside a transaction.
            let _ = pgrx::Spi::run(&format!(
                "ANALYZE _pg_ripple.vp_{p_id}_delta; \
                 ANALYZE _pg_ripple.vp_{p_id}_main; \
                 ANALYZE _pg_ripple.vp_{p_id}_tombstones"
            ));
            vacuumed += 1;
        }

        // Analyze vp_rare as well.
        let _ = pgrx::Spi::run("ANALYZE _pg_ripple.vp_rare");

        pgrx::log!("pg_ripple.vacuum: analyzed {} VP table groups", vacuumed);
        vacuumed
    }

    /// Rebuild all indices on VP tables (delta, main, tombstones) and vp_rare.
    ///
    /// Uses `REINDEX TABLE CONCURRENTLY` to avoid locking out reads.
    /// Returns the number of tables reindexed.
    #[pg_extern]
    fn reindex() -> i64 {
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE htap = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("reindex: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        let mut reindexed = 0i64;
        for p_id in &pred_ids {
            // REINDEX CONCURRENTLY cannot run inside a transaction block;
            // use plain REINDEX instead (safe for maintenance windows).
            let _ = pgrx::Spi::run(&format!(
                "REINDEX TABLE _pg_ripple.vp_{p_id}_delta; \
                 REINDEX TABLE _pg_ripple.vp_{p_id}_main"
            ));
            reindexed += 1;
        }
        let _ = pgrx::Spi::run("REINDEX TABLE _pg_ripple.vp_rare");

        pgrx::log!("pg_ripple.reindex: reindexed {} VP table groups", reindexed);
        reindexed
    }

    /// Remove dictionary entries that are no longer referenced by any VP table.
    ///
    /// Scans all predicate VP tables and vp_rare to build a set of live s/o/p IDs,
    /// then deletes any dictionary rows not in that set.
    ///
    /// Uses an advisory lock (key 0x7269706c = ASCII 'ripl') to prevent
    /// concurrent runs.  Safe to run during normal operation — may miss very
    /// recently orphaned entries (cleaned on the next run).
    ///
    /// Returns the number of dictionary entries removed.
    #[pg_extern]
    fn vacuum_dictionary() -> i64 {
        // Advisory lock to prevent concurrent runs.
        let lock_acquired: bool =
            pgrx::Spi::get_one::<bool>("SELECT pg_try_advisory_xact_lock(0x7269706c::bigint)")
                .unwrap_or(None)
                .unwrap_or(false);

        if !lock_acquired {
            pgrx::warning!("vacuum_dictionary: another vacuum_dictionary is already running");
            return 0;
        }

        // Collect all live IDs referenced by VP tables and vp_rare.
        // Build a UNION ALL of all s,o,g columns from every VP table.
        let pred_ids: Vec<i64> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: predicates scan error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        // Build a temporary table of live IDs.
        pgrx::Spi::run(
            "CREATE TEMP TABLE IF NOT EXISTS _pg_ripple_live_ids (id BIGINT) ON COMMIT DROP",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: create temp table error: {e}"));

        pgrx::Spi::run("TRUNCATE _pg_ripple_live_ids")
            .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: truncate temp table error: {e}"));

        // Insert predicate IDs themselves.
        pgrx::Spi::run(
            "INSERT INTO _pg_ripple_live_ids \
             SELECT id FROM _pg_ripple.predicates",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: insert pred IDs error: {e}"));

        // Insert vp_rare IDs.
        pgrx::Spi::run(
            "INSERT INTO _pg_ripple_live_ids \
             SELECT p FROM _pg_ripple.vp_rare \
             UNION ALL SELECT s FROM _pg_ripple.vp_rare \
             UNION ALL SELECT o FROM _pg_ripple.vp_rare \
             UNION ALL SELECT g FROM _pg_ripple.vp_rare WHERE g <> 0",
        )
        .unwrap_or_else(|e| pgrx::error!("vacuum_dictionary: insert vp_rare IDs error: {e}"));

        // Insert IDs from each dedicated VP table.
        for p_id in &pred_ids {
            let _ = pgrx::Spi::run(&format!(
                "INSERT INTO _pg_ripple_live_ids \
                 SELECT s FROM _pg_ripple.vp_{p_id} \
                 UNION ALL SELECT o FROM _pg_ripple.vp_{p_id} \
                 UNION ALL SELECT g FROM _pg_ripple.vp_{p_id} WHERE g <> 0"
            ));
        }

        // Delete dictionary entries not referenced by any live ID.
        // Inline-encoded IDs (bit 63 set) have no dictionary row; skip them.
        let deleted: i64 = pgrx::Spi::get_one::<i64>(
            "WITH live AS (SELECT DISTINCT id FROM _pg_ripple_live_ids), \
              deleted AS ( \
                  DELETE FROM _pg_ripple.dictionary d \
                  WHERE d.id > 0 \
                    AND NOT EXISTS (SELECT 1 FROM live WHERE live.id = d.id) \
                  RETURNING 1 \
              ) \
              SELECT count(*)::bigint FROM deleted",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        pgrx::log!(
            "pg_ripple.vacuum_dictionary: removed {} orphaned dictionary entries",
            deleted
        );
        deleted
    }

    /// Return detailed dictionary cache and size metrics as JSONB.
    ///
    /// Fields:
    /// - `total_entries` — total rows in the dictionary
    /// - `hot_entries` — rows in the unlogged hot dictionary cache
    /// - `cache_capacity` — shared-memory encode cache capacity (entries)
    /// - `cache_budget_mb` — configured cache budget cap in MB
    /// - `shmem_ready` — whether shared memory is initialized
    #[pg_extern]
    fn dictionary_stats() -> pgrx::JsonB {
        let total: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dictionary")
                .unwrap_or(None)
                .unwrap_or(0);

        let hot: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.dictionary_hot")
                .unwrap_or(None)
                .unwrap_or(0);

        let cache_capacity = crate::DICTIONARY_CACHE_SIZE.get();
        let cache_budget_mb = crate::CACHE_BUDGET_MB.get();
        let shmem_ready = crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire);

        pgrx::JsonB(serde_json::json!({
            "total_entries":   total,
            "hot_entries":     hot,
            "cache_capacity":  cache_capacity,
            "cache_budget_mb": cache_budget_mb,
            "shmem_ready":     shmem_ready
        }))
    }

    // ── Graph-level Row-Level Security (v0.14.0) ─────────────────────────────

    /// Enable graph-level Row-Level Security on the current database.
    ///
    /// Creates RLS policies on `_pg_ripple.vp_rare` using the `g` column and
    /// the `_pg_ripple.graph_access` mapping table.  Dedicated VP tables
    /// created after this call also receive RLS policies.
    ///
    /// Set `pg_ripple.rls_bypass = on` in a superuser session to bypass all
    /// policies.  Default graph (g = 0) is always accessible.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn enable_graph_rls() -> bool {
        // Enable RLS on vp_rare — the consolidation table always exists.
        pgrx::Spi::run(
            "ALTER TABLE _pg_ripple.vp_rare ENABLE ROW LEVEL SECURITY; \
             DROP POLICY IF EXISTS pg_ripple_rls_read ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_read ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR SELECT \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('read', 'write', 'admin') \
                     ) \
                 ); \
             DROP POLICY IF EXISTS pg_ripple_rls_write ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_write ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR ALL \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('write', 'admin') \
                     ) \
                 )",
        )
        .unwrap_or_else(|e| pgrx::error!("enable_graph_rls: error creating policy: {e}"));

        // Record that RLS is enabled in the predicates catalog metadata.
        let _ = pgrx::Spi::run(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ('__rls_enabled__', -1, 'admin') \
             ON CONFLICT DO NOTHING",
        );

        true
    }

    /// Grant a permission on a named graph to a PostgreSQL role.
    ///
    /// `permission` must be `'read'`, `'write'`, or `'admin'`.
    /// The graph IRI is encoded in the dictionary automatically.
    /// Granting `'admin'` implies read and write.
    #[pg_extern]
    fn grant_graph(role: &str, graph: &str, permission: &str) {
        let valid = matches!(permission, "read" | "write" | "admin");
        if !valid {
            pgrx::error!(
                "grant_graph: permission must be 'read', 'write', or 'admin'; got '{permission}'"
            );
        }

        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        pgrx::Spi::run_with_args(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
            &[
                pgrx::datum::DatumWithOid::from(role),
                pgrx::datum::DatumWithOid::from(graph_id),
                pgrx::datum::DatumWithOid::from(permission),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("grant_graph: insert error: {e}"));
    }

    /// Revoke a permission on a named graph from a PostgreSQL role.
    ///
    /// Pass NULL for `permission` to revoke all permissions for the role on that graph.
    #[pg_extern]
    fn revoke_graph(role: &str, graph: &str, permission: default!(Option<&str>, "NULL")) {
        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        if let Some(perm) = permission {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2 AND permission = $3",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                    pgrx::datum::DatumWithOid::from(perm),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph: delete error: {e}"));
        } else {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph: delete error: {e}"));
        }
    }

    /// List all graph access control entries as JSONB.
    ///
    /// Returns one row per (role, graph, permission) entry with decoded graph IRIs.
    #[pg_extern]
    fn list_graph_access() -> pgrx::JsonB {
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT ga.role_name, d.value AS graph_iri, ga.permission \
                 FROM _pg_ripple.graph_access ga \
                 LEFT JOIN _pg_ripple.dictionary d ON d.id = ga.graph_id \
                 WHERE ga.role_name <> '__rls_enabled__' \
                 ORDER BY ga.role_name, ga.graph_id",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_graph_access: SPI error: {e}"))
            .map(|row| {
                let role: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let graph_iri: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let perm: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                serde_json::json!({
                    "role": role,
                    "graph": graph_iri,
                    "permission": perm
                })
            })
            .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    // ── Schema summary (v0.14.0, optional pg_trickle) ────────────────────────

    /// Enable the live schema summary stream table via pg_trickle.
    ///
    /// Creates `_pg_ripple.inferred_schema` as a pg_trickle stream table that
    /// maintains a live class→property→cardinality summary.  Used by tooling
    /// and SPARQL IDE auto-completion.
    ///
    /// Returns `true` if the stream table was created; `false` if pg_trickle
    /// is not installed (no error is raised).
    #[pg_extern]
    fn enable_schema_summary() -> bool {
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; schema summary is unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_schema_summary() to enable."
            );
            return false;
        }

        // The schema summary groups triples by predicate to give a rough
        // class→property→cardinality overview.  We use rdf:type as the
        // class link; predicates become properties; COUNT becomes cardinality.
        let rdf_type_id = crate::dictionary::encode(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            crate::dictionary::KIND_IRI,
        );

        let summary_sql = format!(
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.vp_rare vr \
             JOIN _pg_ripple.vp_rare type_row \
                 ON type_row.s = vr.s \
                AND type_row.p = {rdf_type_id} \
             JOIN _pg_ripple.dictionary dp ON dp.id = vr.p \
             LEFT JOIN _pg_ripple.dictionary dc ON dc.id = type_row.o \
             WHERE vr.p <> {rdf_type_id} \
             GROUP BY 1, 2"
        );

        pgrx::Spi::run_with_args(
            "SELECT pg_trickle.create_stream_table($1, $2, '30s')",
            &[
                pgrx::datum::DatumWithOid::from("_pg_ripple.inferred_schema"),
                pgrx::datum::DatumWithOid::from(summary_sql.as_str()),
            ],
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.inferred_schema stream table: {}",
                e
            );
        });

        true
    }

    /// Return the live schema summary as JSONB.
    ///
    /// Reads from `_pg_ripple.inferred_schema` if available (requires
    /// `enable_schema_summary()` to have been called), otherwise falls back
    /// to a direct scan.  Returns an array of `{class, property, cardinality}`.
    #[pg_extern]
    fn schema_summary() -> pgrx::JsonB {
        let has_stream_table = pgrx::Spi::get_one::<bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = '_pg_ripple' AND c.relname = 'inferred_schema' \
             )",
        )
        .unwrap_or(None)
        .unwrap_or(false);

        let query = if has_stream_table {
            "SELECT class_iri, property_iri, cardinality \
             FROM _pg_ripple.inferred_schema \
             ORDER BY class_iri, property_iri"
        } else {
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.predicates p \
             JOIN _pg_ripple.dictionary dp ON dp.id = p.id \
             CROSS JOIN LATERAL (SELECT 1 LIMIT 0) AS dummy(x) \
             GROUP BY 1, 2 \
             ORDER BY 1, 2 \
             LIMIT 0"
        };

        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(query, None, &[])
                .unwrap_or_else(|e| pgrx::error!("schema_summary: SPI error: {e}"))
                .map(|row| {
                    let class: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let prop: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    let card: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                    serde_json::json!({
                        "class": class,
                        "property": prop,
                        "cardinality": card
                    })
                })
                .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
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

        // v0.22.0: encode cache statistics (4-way set-associative).
        let (hits, misses, evictions, utilisation) = crate::shmem::get_cache_stats();
        let cache_capacity = crate::shmem::ENCODE_CACHE_CAPACITY as i64;
        let cache_utilization_pct = (utilisation * 100.0) as i64;
        obj.insert(
            "encode_cache_capacity".to_string(),
            serde_json::json!(cache_capacity),
        );
        obj.insert(
            "encode_cache_utilization_pct".to_string(),
            serde_json::json!(cache_utilization_pct),
        );
        obj.insert("encode_cache_hits".to_string(), serde_json::json!(hits));
        obj.insert("encode_cache_misses".to_string(), serde_json::json!(misses));
        obj.insert(
            "encode_cache_evictions".to_string(),
            serde_json::json!(evictions),
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

    /// Run semi-naive inference for the named rule set and materialise derived triples.
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total number of triples derived (i64)
    /// - `"iterations"`: number of fixpoint iterations performed (i32)
    ///
    /// Semi-naive evaluation avoids re-examining unchanged rows on each iteration,
    /// achieving iteration counts bounded by the longest derivation chain rather
    /// than the full relation size.
    #[pg_extern]
    fn infer_with_stats(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        let (derived, iterations) = crate::datalog::run_inference_seminaive(rule_set);
        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
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

    // ── v0.11.0: SPARQL Views, Datalog Views, ExtVP ───────────────────────────

    /// Return `true` when the pg_trickle extension is installed in the current database.
    ///
    /// SPARQL views, Datalog views, and ExtVP all require pg_trickle.
    /// Call this function to check availability before calling the view functions.
    #[pg_extern]
    fn pg_trickle_available() -> bool {
        crate::views::pg_trickle_available()
    }

    /// Create a named, incrementally-maintained SPARQL SELECT result table.
    ///
    /// Compiles the SPARQL query to SQL, registers a pg_trickle stream table
    /// under `pg_ripple.{name}`, and records the view in `_pg_ripple.sparql_views`.
    ///
    /// - `name` — view name (alphanumeric + underscores, ≤ 63 chars)
    /// - `sparql` — SPARQL SELECT query
    /// - `schedule` — pg_trickle schedule, e.g. `'1s'`, `'IMMEDIATE'`, `'30s'`
    /// - `decode` — when `false` (recommended), the stream table stores BIGINT IDs;
    ///              when `true`, stores decoded TEXT values
    ///
    /// Returns the number of projected variables (stream table columns).
    #[pg_extern]
    fn create_sparql_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_sparql_view(name, sparql, schedule, decode)
    }

    /// Drop a SPARQL view and its underlying pg_trickle stream table.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_sparql_view(name: &str) -> bool {
        crate::views::drop_sparql_view(name)
    }

    /// List all registered SPARQL views as a JSONB array.
    #[pg_extern]
    fn list_sparql_views() -> pgrx::JsonB {
        crate::views::list_sparql_views()
    }

    /// Create a Datalog view from inline rules and a SPARQL SELECT goal.
    ///
    /// The rules are parsed, stratified, and stored under `rule_set_name`.
    /// The goal query is compiled to SQL and registered as a pg_trickle stream
    /// table under `pg_ripple.{name}`.
    ///
    /// - `name` — view name
    /// - `rules` — Datalog rules in Turtle-flavoured Datalog syntax
    /// - `rule_set_name` — logical name for the stored rule set
    /// - `goal` — SPARQL SELECT query selecting from derived predicates
    /// - `schedule` — pg_trickle schedule
    /// - `decode` — as for `create_sparql_view`
    ///
    /// Returns the number of projected variables in the goal query.
    #[pg_extern]
    fn create_datalog_view(
        name: &str,
        rules: &str,
        goal: &str,
        rule_set_name: default!(&str, "'custom'"),
        schedule: default!(&str, "'10s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_datalog_view_from_rules(
            name,
            rules,
            rule_set_name,
            goal,
            schedule,
            decode,
        )
    }

    /// Create a Datalog view referencing an existing named rule set.
    ///
    /// The rule set must have been previously loaded with `load_rules`.
    /// The goal query is compiled to SQL and registered as a pg_trickle stream table.
    #[pg_extern]
    fn create_datalog_view_from_rule_set(
        name: &str,
        rule_set: &str,
        goal: &str,
        schedule: default!(&str, "'10s'"),
        decode: default!(bool, false),
    ) -> i64 {
        crate::views::create_datalog_view_from_rule_set(name, rule_set, goal, schedule, decode)
    }

    /// Drop a Datalog view and its underlying pg_trickle stream table.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_datalog_view(name: &str) -> bool {
        crate::views::drop_datalog_view(name)
    }

    /// List all registered Datalog views as a JSONB array.
    #[pg_extern]
    fn list_datalog_views() -> pgrx::JsonB {
        crate::views::list_datalog_views()
    }

    // ── v0.17.0: Framing views ────────────────────────────────────────────────

    /// Create an incrementally-maintained JSON-LD framing view (requires pg_trickle).
    ///
    /// Translates `frame` to a SPARQL CONSTRUCT query and registers it as a
    /// pg_trickle stream table `pg_ripple.framing_view_{name}` with schema
    /// `(subject_id BIGINT, frame_tree JSONB, refreshed_at TIMESTAMPTZ)`.
    ///
    /// When `decode = TRUE` a thin IRI-decoding view is also created.
    #[pg_extern]
    fn create_framing_view(
        name: &str,
        frame: pgrx::JsonB,
        schedule: default!(&str, "'5s'"),
        decode: default!(bool, "false"),
        output_format: default!(&str, "'jsonld'"),
    ) {
        crate::views::create_framing_view(name, &frame.0, schedule, decode, output_format)
    }

    /// Drop a framing view stream table and its catalog entry.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_framing_view(name: &str) -> bool {
        crate::views::drop_framing_view(name)
    }

    /// List all registered framing views as a JSONB array.
    #[pg_extern]
    fn list_framing_views() -> pgrx::JsonB {
        crate::views::list_framing_views()
    }

    // ── v0.18.0: SPARQL CONSTRUCT, DESCRIBE & ASK Views ──────────────────────

    /// Create a CONSTRUCT view — an incrementally-maintained stream table
    /// `pg_ripple.construct_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
    /// whose rows reflect the CONSTRUCT template output at all times.
    ///
    /// When `decode = TRUE`, a thin TEXT-decoding view
    /// `pg_ripple.construct_view_{name}_decoded` is also created.
    ///
    /// Returns the number of template triples registered.
    ///
    /// Errors if `sparql` is not a CONSTRUCT query, if template variables are
    /// unbound, or if the template contains blank nodes.
    #[pg_extern]
    fn create_construct_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, "false"),
    ) -> i64 {
        crate::views::create_construct_view(name, sparql, schedule, decode)
    }

    /// Drop a CONSTRUCT view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_construct_view(name: &str) {
        crate::views::drop_construct_view(name)
    }

    /// List all registered CONSTRUCT views as a JSONB array.
    #[pg_extern]
    fn list_construct_views() -> pgrx::JsonB {
        crate::views::list_construct_views()
    }

    /// Create a DESCRIBE view — an incrementally-maintained stream table
    /// `pg_ripple.describe_view_{name}(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`
    /// materialising the Concise Bounded Description (CBD) of the described resources.
    ///
    /// The `pg_ripple.describe_strategy` GUC controls CBD vs symmetric-CBD.
    ///
    /// When `decode = TRUE`, a thin TEXT-decoding view
    /// `pg_ripple.describe_view_{name}_decoded` is also created.
    ///
    /// Errors if `sparql` is not a DESCRIBE query.
    #[pg_extern]
    fn create_describe_view(
        name: &str,
        sparql: &str,
        schedule: default!(&str, "'1s'"),
        decode: default!(bool, "false"),
    ) {
        crate::views::create_describe_view(name, sparql, schedule, decode)
    }

    /// Drop a DESCRIBE view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_describe_view(name: &str) {
        crate::views::drop_describe_view(name)
    }

    /// List all registered DESCRIBE views as a JSONB array.
    #[pg_extern]
    fn list_describe_views() -> pgrx::JsonB {
        crate::views::list_describe_views()
    }

    /// Create an ASK view — an incrementally-maintained single-row stream table
    /// `pg_ripple.ask_view_{name}(result BOOLEAN, evaluated_at TIMESTAMPTZ)`
    /// that updates whenever the underlying pattern's satisfiability changes.
    ///
    /// Errors if `sparql` is not an ASK query.
    #[pg_extern]
    fn create_ask_view(name: &str, sparql: &str, schedule: default!(&str, "'1s'")) {
        crate::views::create_ask_view(name, sparql, schedule)
    }

    /// Drop an ASK view and its underlying pg_trickle stream table.
    #[pg_extern]
    fn drop_ask_view(name: &str) {
        crate::views::drop_ask_view(name)
    }

    /// List all registered ASK views as a JSONB array.
    #[pg_extern]
    fn list_ask_views() -> pgrx::JsonB {
        crate::views::list_ask_views()
    }

    ///
    /// Pre-computes subjects that appear in triples of both `pred1_iri` and
    /// `pred2_iri`.  The SPARQL query engine uses these tables to accelerate
    /// star-pattern queries that reference both predicates.
    ///
    /// - `name` — ExtVP name
    /// - `pred1_iri` — IRI of the first predicate
    /// - `pred2_iri` — IRI of the second predicate
    /// - `schedule` — pg_trickle schedule
    ///
    /// Returns the number of rows in the stream table after the first refresh.
    #[pg_extern]
    fn create_extvp(
        name: &str,
        pred1_iri: &str,
        pred2_iri: &str,
        schedule: default!(&str, "'10s'"),
    ) -> i64 {
        crate::views::create_extvp(name, pred1_iri, pred2_iri, schedule)
    }

    /// Drop an ExtVP table and remove it from the catalog.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn drop_extvp(name: &str) -> bool {
        crate::views::drop_extvp(name)
    }

    /// List all registered ExtVP tables as a JSONB array.
    #[pg_extern]
    fn list_extvp() -> pgrx::JsonB {
        crate::views::list_extvp()
    }

    // ── v0.15.0: Graph-aware bulk loaders ─────────────────────────────────────

    /// Load N-Triples data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_ntriples_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_ntriples_into_graph(data, g_id)
    }

    /// Load Turtle data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_turtle_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_turtle_into_graph(data, g_id)
    }

    /// Load RDF/XML data into a specific named graph.  Returns triples loaded.
    #[pg_extern]
    fn load_rdfxml_into_graph(data: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_rdfxml_into_graph(data, g_id)
    }

    /// Load N-Triples from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_ntriples_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_ntriples_file_into_graph(path, g_id)
    }

    /// Load Turtle from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_turtle_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_turtle_file_into_graph(path, g_id)
    }

    /// Load RDF/XML from a server-side file into a named graph (superuser required).
    #[pg_extern]
    fn load_rdfxml_file_into_graph(path: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::bulk_load::load_rdfxml_file_into_graph(path, g_id)
    }

    /// Load RDF/XML from a server-side file path (superuser required).
    #[pg_extern]
    fn load_rdfxml_file(path: &str) -> i64 {
        crate::bulk_load::load_rdfxml_file(path)
    }

    // ── v0.15.0: Graph-aware triple deletion ──────────────────────────────────

    /// Delete a specific triple from a named graph.  Returns 0 or 1.
    #[pg_extern]
    fn delete_triple_from_graph(s: &str, p: &str, o: &str, graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::delete_triple(s, p, o, g_id)
    }

    /// Delete all triples in a named graph without unregistering it.
    /// Returns the number of triples deleted.
    #[pg_extern]
    fn clear_graph(graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::clear_graph_by_id(g_id)
    }

    // ── v0.15.0: SQL API completeness gaps ────────────────────────────────────

    /// Pattern-match triples within a specific named graph (or default graph if NULL).
    #[pg_extern]
    fn find_triples_in_graph(
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
        graph: Option<&str>,
    ) -> TableIterator<
        'static,
        (
            name!(s, String),
            name!(p, String),
            name!(o, String),
            name!(g, String),
        ),
    > {
        let g_id = graph.map(|g| crate::dictionary::encode(g, crate::dictionary::KIND_IRI));
        let rows = crate::storage::find_triples(s, p, o, g_id);
        TableIterator::new(rows)
    }

    /// Return the number of triples in a specific named graph.
    #[pg_extern]
    fn triple_count_in_graph(graph_iri: &str) -> i64 {
        let g_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
        crate::storage::triple_count_in_graph(g_id)
    }

    /// Decode a dictionary ID to its full structured representation as JSONB.
    /// Returns {"kind": ..., "value": ..., "language": null|"...", "datatype": null|"..."}.
    #[pg_extern]
    fn decode_id_full(id: i64) -> Option<pgrx::JsonB> {
        crate::dictionary::decode_full(id).map(|info| {
            let kind_label = match info.kind {
                0 => "iri",
                1 => "blank_node",
                2 => "literal",
                3 => "typed_literal",
                4 => "lang_literal",
                5 => "quoted_triple",
                _ => "unknown",
            };
            let mut obj = serde_json::Map::new();
            obj.insert(
                "kind".to_owned(),
                serde_json::Value::String(kind_label.to_owned()),
            );
            obj.insert("value".to_owned(), serde_json::Value::String(info.value));
            obj.insert(
                "datatype".to_owned(),
                info.datatype
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            obj.insert(
                "language".to_owned(),
                info.lang
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            pgrx::JsonB(serde_json::Value::Object(obj))
        })
    }

    /// Look up an IRI in the dictionary without encoding it.
    /// Returns the dictionary ID if the IRI exists, NULL otherwise.
    #[pg_extern]
    fn lookup_iri(iri: &str) -> Option<i64> {
        crate::dictionary::lookup_iri(iri)
    }

    // ── v0.16.0: SPARQL Federation ────────────────────────────────────────────

    /// Register a remote SPARQL endpoint in the federation allowlist.
    ///
    /// Only registered endpoints can be contacted via SERVICE clauses.
    /// Attempting to call an unregistered endpoint raises an ERROR (SSRF protection).
    ///
    /// `local_view_name` — optional name of a pg_ripple SPARQL view stream table
    /// that pre-materialises the same data.  When set, SERVICE clauses targeting
    /// this URL are rewritten to scan the local table instead of making HTTP calls.
    ///
    /// `complexity` (v0.19.0) — optional hint for query planning: `'fast'`, `'normal'`
    /// (default), or `'slow'`.  Fast endpoints execute first in multi-endpoint queries.
    #[pg_extern]
    fn register_endpoint(
        url: &str,
        local_view_name: default!(Option<&str>, "NULL"),
        complexity: default!(Option<&str>, "NULL"),
    ) {
        // v0.22.0 M-13: Reject non-http/https URL schemes to prevent file://, gopher://, etc.
        let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
        if !scheme_ok {
            pgrx::error!(
                "register_endpoint: URL scheme must be http or https; got: {}",
                url
            );
        }
        let local_view = local_view_name.unwrap_or("");
        let cx = complexity.unwrap_or("normal");
        if local_view.is_empty() {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, complexity)
                 VALUES ($1, true, $2)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, complexity = $2",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(cx),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
        } else {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, local_view_name, complexity)
                 VALUES ($1, true, $2, $3)
                 ON CONFLICT (url) DO UPDATE SET enabled = true, local_view_name = $2, complexity = $3",
                &[
                    pgrx::datum::DatumWithOid::from(url),
                    pgrx::datum::DatumWithOid::from(local_view_name),
                    pgrx::datum::DatumWithOid::from(cx),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("register_endpoint failed: {e}"));
        }
    }

    /// Set the complexity hint for a registered endpoint (v0.19.0).
    ///
    /// Allowed values: `'fast'`, `'normal'`, `'slow'`.
    /// Fast endpoints execute first in queries with multiple SERVICE clauses
    /// targeting different endpoints, enabling earlier failure detection.
    #[pg_extern]
    fn set_endpoint_complexity(url: &str, complexity: &str) {
        Spi::run_with_args(
            "UPDATE _pg_ripple.federation_endpoints SET complexity = $2 WHERE url = $1",
            &[
                pgrx::datum::DatumWithOid::from(url),
                pgrx::datum::DatumWithOid::from(complexity),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("set_endpoint_complexity failed: {e}"));
    }

    /// Remove a remote SPARQL endpoint from the federation allowlist.
    ///
    /// After removal, SERVICE clauses targeting this URL will raise an ERROR.
    #[pg_extern]
    fn remove_endpoint(url: &str) {
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.federation_endpoints WHERE url = $1",
            &[pgrx::datum::DatumWithOid::from(url)],
        )
        .unwrap_or_else(|e| pgrx::error!("remove_endpoint failed: {e}"));
    }

    /// Disable a remote SPARQL endpoint without removing it.
    ///
    /// Disabled endpoints are excluded from SERVICE queries (like not being
    /// registered) but can be re-enabled with `register_endpoint()`.
    #[pg_extern]
    fn disable_endpoint(url: &str) {
        Spi::run_with_args(
            "UPDATE _pg_ripple.federation_endpoints SET enabled = false WHERE url = $1",
            &[pgrx::datum::DatumWithOid::from(url)],
        )
        .unwrap_or_else(|e| pgrx::error!("disable_endpoint failed: {e}"));
    }

    /// List all registered federation endpoints.
    ///
    /// Returns (url, enabled, local_view_name, complexity) for every endpoint in the allowlist.
    #[pg_extern]
    fn list_endpoints() -> TableIterator<
        'static,
        (
            name!(url, String),
            name!(enabled, bool),
            name!(local_view_name, Option<String>),
            name!(complexity, String),
        ),
    > {
        let mut rows: Vec<(String, bool, Option<String>, String)> = Vec::new();
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT url, enabled, local_view_name, complexity
                     FROM _pg_ripple.federation_endpoints
                     ORDER BY url",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_endpoints SPI error: {e}"));
            for row in result {
                let url: String = row.get(1).ok().flatten().unwrap_or_default();
                let enabled: bool = row.get(2).ok().flatten().unwrap_or(false);
                let local_view: Option<String> = row.get(3).ok().flatten();
                let cx: String = row
                    .get(4)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "normal".to_owned());
                rows.push((url, enabled, local_view, cx));
            }
        });
        TableIterator::new(rows)
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
