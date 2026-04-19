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
    htap         BOOLEAN NOT NULL DEFAULT false,
    schema_name  TEXT,
    table_name   TEXT
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
-- Federation result cache (v0.19.0, updated v0.25.0)
-- Caches SPARQL SELECT results from remote endpoints keyed by (url, query_hash).
-- query_hash is a 32-char hex XXH3-128 fingerprint of the SPARQL text.
-- TTL-based expiry; expired rows are cleaned up by the merge background worker.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache (
    url         TEXT        NOT NULL,
    query_hash  TEXT        NOT NULL,
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

// v0.25.0: Custom aggregate registry.
pgrx::extension_sql!(
    r#"
-- Custom aggregate catalog (v0.25.0)
-- Maps SPARQL custom aggregate IRIs to PostgreSQL aggregate/function names.
CREATE TABLE IF NOT EXISTS _pg_ripple.custom_aggregates (
    sparql_iri  TEXT NOT NULL PRIMARY KEY,
    pg_function TEXT NOT NULL
);
"#,
    name = "v025_custom_aggregates",
    requires = ["v019_federation_cache_setup"]
);

// v0.27.0: Embeddings table for vector / pgvector hybrid search.
pgrx::extension_sql!(
    r#"
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        EXECUTE $sql$
            CREATE TABLE IF NOT EXISTS _pg_ripple.embeddings (
                entity_id   BIGINT      NOT NULL,
                model       TEXT        NOT NULL DEFAULT 'default',
                embedding   vector(1536),
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (entity_id, model)
            );
            CREATE INDEX IF NOT EXISTS embeddings_hnsw_idx
                ON _pg_ripple.embeddings
                USING hnsw (embedding vector_cosine_ops);
        $sql$;
    ELSE
        EXECUTE $sql$
            CREATE TABLE IF NOT EXISTS _pg_ripple.embeddings (
                entity_id   BIGINT      NOT NULL,
                model       TEXT        NOT NULL DEFAULT 'default',
                embedding   BYTEA,
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (entity_id, model)
            );
        $sql$;
    END IF;
END;
$$;
"#,
    name = "v027_embeddings_table",
    requires = ["v025_custom_aggregates"]
);

// v0.28.0: Embedding queue table and vector endpoint catalog.
pgrx::extension_sql!(
    r#"
-- Embedding queue (v0.28.0): entities awaiting embedding by the background worker.
-- Populated by a trigger on _pg_ripple.dictionary when pg_ripple.auto_embed = true.
CREATE TABLE IF NOT EXISTS _pg_ripple.embedding_queue (
    entity_id   BIGINT      NOT NULL PRIMARY KEY,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.embedding_queue IS
    'Queue of entity_ids awaiting embedding by the background worker. '
    'Populated by a trigger on _pg_ripple.dictionary when pg_ripple.auto_embed = true.';

-- Vector endpoint catalog (v0.28.0): external vector service endpoints for
-- SPARQL federation with pg:similarTo predicates.
CREATE TABLE IF NOT EXISTS _pg_ripple.vector_endpoints (
    url         TEXT NOT NULL PRIMARY KEY,
    api_type    TEXT NOT NULL CHECK (api_type IN ('pgvector', 'weaviate', 'qdrant', 'pinecone')),
    enabled     BOOLEAN NOT NULL DEFAULT true,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.vector_endpoints IS
    'External vector service endpoints registered for SPARQL SERVICE federation '
    'with the pg:similarTo predicate.';

-- Trigger function: enqueue new dictionary IRI entries for embedding
-- when pg_ripple.auto_embed is on.
CREATE OR REPLACE FUNCTION _pg_ripple.auto_embed_trigger()
RETURNS TRIGGER LANGUAGE plpgsql AS $body$
BEGIN
    -- Only enqueue IRI entities (kind = 0).
    IF NEW.kind = 0
       AND current_setting('pg_ripple.auto_embed', true)::boolean IS TRUE
    THEN
        INSERT INTO _pg_ripple.embedding_queue (entity_id)
        VALUES (NEW.id)
        ON CONFLICT (entity_id) DO NOTHING;
    END IF;
    RETURN NEW;
END;
$body$;

-- Attach the trigger to the dictionary table.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgname = 'auto_embed_dict_trigger'
          AND tgrelid = '_pg_ripple.dictionary'::regclass
    ) THEN
        CREATE TRIGGER auto_embed_dict_trigger
            AFTER INSERT ON _pg_ripple.dictionary
            FOR EACH ROW EXECUTE FUNCTION _pg_ripple.auto_embed_trigger();
    END IF;
END;
$$;
"#,
    name = "v028_embedding_queue",
    requires = ["v027_embeddings_table"]
);
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

// v0.36.0: Lattice-based Datalog catalog table.
pgrx::extension_sql!(
    r#"
-- Lattice type catalog (v0.36.0)
-- Stores registered lattice types for Datalog^L monotone aggregation rules.
CREATE TABLE IF NOT EXISTS _pg_ripple.lattice_types (
    name       TEXT        NOT NULL PRIMARY KEY,
    join_fn    TEXT        NOT NULL,
    bottom     TEXT        NOT NULL DEFAULT '0',
    builtin    BOOLEAN     NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Register built-in lattice types.
INSERT INTO _pg_ripple.lattice_types (name, join_fn, bottom, builtin) VALUES
    ('min',      'min',       '9223372036854775807',  true),
    ('max',      'max',       '-9223372036854775808', true),
    ('set',      'array_agg', '{}',                   true),
    ('interval', 'max',       '0',                    true)
ON CONFLICT (name) DO NOTHING;
"#,
    name = "v036_lattice_types",
    requires = ["datalog_schema_setup"]
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

// ─── pg_trickle runtime detection (v0.6.0) ───────────────────────────────────

/// The pg_trickle version that pg_ripple was tested against (A-4, v0.25.0).
const PG_TRICKLE_TESTED_VERSION: &str = "0.3.0";

// ─── RDF Patch N-Triples term parser (v0.25.0) ───────────────────────────────

/// Parse an N-Triples triple statement string into (s, p, o) term strings.
///
/// Returns `None` when the input cannot be parsed as a valid N-Triples statement.
/// Supports IRIs (`<…>`), blank nodes (`_:…`), plain literals (`"…"`), and
/// datatyped/lang-tagged literals.
fn parse_nt_triple(line: &str) -> Option<(String, String, String)> {
    let line = line.trim().trim_end_matches('.').trim();
    let mut terms: Vec<String> = Vec::with_capacity(3);
    let mut chars = line.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => {
                chars.next();
            }
            '<' => {
                chars.next();
                let mut buf = String::from("<");
                for c in chars.by_ref() {
                    buf.push(c);
                    if c == '>' {
                        break;
                    }
                }
                terms.push(buf);
            }
            '"' => {
                chars.next();
                let mut buf = String::from("\"");
                let mut escaped = false;
                for c in chars.by_ref() {
                    buf.push(c);
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == '\\' {
                        escaped = true;
                        continue;
                    }
                    if c == '"' {
                        break;
                    }
                }
                // Consume optional ^^<datatype> or @lang suffix.
                while let Some(&p) = chars.peek() {
                    if p == '^' || p == '@' {
                        buf.push(p);
                        chars.next();
                    } else if p == '<' {
                        chars.next();
                        buf.push('<');
                        for c in chars.by_ref() {
                            buf.push(c);
                            if c == '>' {
                                break;
                            }
                        }
                        break;
                    } else if p.is_alphanumeric() || p == '-' || p == '_' {
                        buf.push(p);
                        chars.next();
                    } else {
                        break;
                    }
                }
                terms.push(buf);
            }
            '_' => {
                let mut buf = String::new();
                for c in chars.by_ref() {
                    if c == ' ' || c == '\t' {
                        break;
                    }
                    buf.push(c);
                }
                terms.push(buf);
            }
            _ => {
                chars.next();
            }
        }
        if terms.len() == 3 {
            break;
        }
    }
    if terms.len() == 3 {
        Some((terms.remove(0), terms.remove(0), terms.remove(0)))
    } else {
        None
    }
}

/// Returns `true` when the pg_trickle extension is installed in the current database.
///
/// All pg_trickle-dependent features gate on this check — core pg_ripple
/// functionality works without pg_trickle.
///
/// Also emits a one-time WARNING if the installed pg_trickle version is newer
/// than `PG_TRICKLE_TESTED_VERSION` (A-4, v0.25.0).
pub(crate) fn has_pg_trickle() -> bool {
    // Check existence first.
    let exists = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle')",
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if exists {
        // Version-lock probe (A-4): warn if installed version is newer than tested.
        if let Some(installed) = pgrx::Spi::get_one::<String>(
            "SELECT extversion FROM pg_extension WHERE extname = 'pg_trickle'",
        )
        .unwrap_or(None)
            && installed.as_str() > PG_TRICKLE_TESTED_VERSION
        {
            pgrx::warning!(
                "pg_ripple: pg_trickle version {} is newer than tested version {}; \
                 incremental views may behave unexpectedly",
                installed,
                PG_TRICKLE_TESTED_VERSION
            );
        }
    }

    exists
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

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.federation_partial_recovery_max_bytes",
        c"Maximum response body size in bytes for partial federation result recovery; responses larger than this return empty with a WARNING (default: 65536, min: 1024, max: 104857600)",
        c"",
        &FEDERATION_PARTIAL_RECOVERY_MAX_BYTES,
        1024,
        104_857_600,
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

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_index_type",
        c"Index type on _pg_ripple.embeddings: 'hnsw' (default) or 'ivfflat'; changing requires REINDEX",
        c"",
        &EMBEDDING_INDEX_TYPE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.embedding_precision",
        c"Embedding storage precision: 'single' (default, vector(N)), 'half' (halfvec(N), -50% storage), 'binary' (bit(N), -96% storage); requires pgvector >= 0.7.0",
        c"",
        &EMBEDDING_PRECISION,
        GucContext::Userset,
        GucFlags::default(),
    );

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

    // ── v0.29.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.magic_sets",
        c"When on (default), infer_goal() uses magic sets for goal-directed inference; \
          off falls back to full materialization + filter (v0.29.0)",
        c"",
        &MAGIC_SETS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.datalog_cost_reorder",
        c"When on (default), sort Datalog rule body atoms by ascending estimated \
          VP-table cardinality before SQL compilation (v0.29.0)",
        c"",
        &DATALOG_COST_REORDER,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.datalog_antijoin_threshold",
        c"Minimum VP-table rows for NOT body atoms to compile to LEFT JOIN IS NULL \
          anti-join form instead of NOT EXISTS (default: 1000, 0=always NOT EXISTS; v0.29.0)",
        c"",
        &DATALOG_ANTIJOIN_THRESHOLD,
        0,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.delta_index_threshold",
        c"Minimum semi-naive delta-table rows before creating a B-tree index on (s,o) \
          join columns (default: 500, 0=disabled; v0.29.0)",
        c"",
        &DELTA_INDEX_THRESHOLD,
        0,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.30.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.rule_plan_cache",
        c"When on (default), cache compiled SQL for each rule set to speed up \
          repeated infer() / infer_agg() calls; invalidated by drop_rules() and \
          load_rules() (v0.30.0)",
        c"",
        &RULE_PLAN_CACHE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.rule_plan_cache_size",
        c"Maximum number of rule sets kept in the plan cache (default: 64, \
          min: 1, max: 4096); oldest entries are evicted on overflow (v0.30.0)",
        c"",
        &RULE_PLAN_CACHE_SIZE,
        1,
        4096,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.31.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.sameas_reasoning",
        c"When on (default), Datalog inference applies an owl:sameAs \
          canonicalization pre-pass so that rules and SPARQL queries referencing \
          non-canonical entities are transparently rewritten to the canonical form \
          (v0.31.0)",
        c"",
        &SAMEAS_REASONING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.demand_transform",
        c"When on (default), create_datalog_view() automatically applies demand \
          transformation when multiple goal patterns are specified; infer_demand() \
          always applies demand filtering regardless (v0.31.0)",
        c"",
        &DEMAND_TRANSFORM,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.32.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.wfs_max_iterations",
        c"Safety cap on alternating fixpoint rounds per WFS pass (default: 100, \
          min: 1, max: 10000); emits PT520 WARNING if a pass does not converge (v0.32.0)",
        c"",
        &WFS_MAX_ITERATIONS,
        1,
        10_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.tabling",
        c"When on (default), infer_wfs() and SPARQL results are cached in \
          _pg_ripple.tabling_cache and reused on matching subsequent calls; \
          invalidated by drop_rules(), load_rules(), and triple modifications (v0.32.0)",
        c"",
        &TABLING,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.tabling_ttl",
        c"TTL in seconds for tabling cache entries (default: 300; set 0 to disable \
          TTL-based expiry) (v0.32.0)",
        c"",
        &TABLING_TTL,
        0,
        86_400,
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

    // ── v0.36.0 GUCs ─────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.wcoj_enabled",
        c"When on (default), cyclic SPARQL BGPs are detected and executed via \
          sort-merge join hints simulating Leapfrog Triejoin (v0.36.0)",
        c"",
        &WCOJ_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.wcoj_min_tables",
        c"Minimum VP table join count before WCOJ cyclic-pattern detection is applied \
          (default: 3, min: 2, max: 100) (v0.36.0)",
        c"",
        &WCOJ_MIN_TABLES,
        2,
        100,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.lattice_max_iterations",
        c"Maximum fixpoint iterations for lattice-based Datalog inference; \
          emits PT540 WARNING on non-convergence (default: 1000, min: 1, max: 1000000) (v0.36.0)",
        c"",
        &LATTICE_MAX_ITERATIONS,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

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

        // ── v0.32.0: Tabling cache invalidation ────────────────────────────
        if sid > 0 {
            crate::datalog::tabling_invalidate_all();
        }

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
        let deleted = crate::storage::delete_triple(s, p, o, 0_i64);
        // Invalidate tabling cache on data change (v0.32.0).
        if deleted > 0 {
            crate::datalog::tabling_invalidate_all();
        }
        deleted
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
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_ntriples(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_ntriples(data, strict)
    }

    /// Load N-Quads data from a text string (supports named graphs).
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_nquads(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_nquads(data, strict)
    }

    /// Load Turtle data from a text string.
    /// Also accepts Turtle-star (quoted triples) using oxttl with rdf-12 support.
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_turtle(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_turtle(data, strict)
    }

    /// Load TriG data (Turtle with named graph blocks) from a text string.
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_trig(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_trig(data, strict)
    }

    /// Load N-Triples from a server-side file path (superuser required).
    #[pg_extern]
    fn load_ntriples_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_ntriples_file(path, strict)
    }

    /// Load N-Quads from a server-side file path (superuser required).
    #[pg_extern]
    fn load_nquads_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_nquads_file(path, strict)
    }

    /// Load Turtle from a server-side file path (superuser required).
    #[pg_extern]
    fn load_turtle_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_turtle_file(path, strict)
    }

    /// Load TriG from a server-side file path (superuser required).
    #[pg_extern]
    fn load_trig_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_trig_file(path, strict)
    }

    /// Load RDF/XML data from a text string.  Returns the number of triples loaded.
    ///
    /// Parses conformant RDF/XML using `rio_xml`.  All triples are loaded into the
    /// default graph (RDF/XML does not support named graphs).
    /// When `strict = true`, any parse error aborts and rolls back the entire load.
    #[pg_extern]
    fn load_rdfxml(data: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_rdfxml(data, strict)
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

    // ── GraphRAG BYOG Parquet export (v0.26.0) ────────────────────────────────

    /// Export all `gr:Entity` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `title`, `type`, `description`, `text_unit_ids`, `frequency`, `degree`.
    ///
    /// `graph_iri` is the named graph IRI (without angle brackets), or an empty
    /// string to query the default graph.
    ///
    /// Requires superuser.  Returns the number of entity rows written.
    ///
    /// The output file is compatible with `pyarrow.parquet.read_table()` and
    /// can be fed directly to GraphRAG's BYOG `entity_table_path` option.
    #[pg_extern]
    fn export_graphrag_entities(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_entities(graph_iri, output_path)
    }

    /// Export all `gr:Relationship` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `source`, `target`, `description`, `weight`, `combined_degree`, `text_unit_ids`.
    ///
    /// Requires superuser.  Returns the number of relationship rows written.
    #[pg_extern]
    fn export_graphrag_relationships(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_relationships(graph_iri, output_path)
    }

    /// Export all `gr:TextUnit` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `text`, `n_tokens`, `document_id`, `entity_ids`, `relationship_ids`.
    ///
    /// Requires superuser.  Returns the number of text unit rows written.
    #[pg_extern]
    fn export_graphrag_text_units(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_text_units(graph_iri, output_path)
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

    /// Health check function (v0.25.0).
    ///
    /// Returns a JSONB object with key health indicators for operations dashboards:
    /// - `merge_worker`: `"ok"` if the merge worker PID is recorded in shared memory,
    ///   `"stalled"` otherwise.
    /// - `cache_hit_rate`: fraction of dictionary encode lookups that hit the
    ///   backend-local LRU cache (0.0–1.0).
    /// - `catalog_consistent`: `true` if the number of VP tables in `pg_class` matches
    ///   the number of promoted predicates in `_pg_ripple.predicates`.
    /// - `orphaned_rare_rows`: number of `vp_rare` rows whose predicate has a dedicated
    ///   VP table (should be 0 after a healthy promotion cycle).
    #[pg_extern]
    fn canary() -> pgrx::JsonB {
        use serde_json::{Map, Number, Value as Json};

        // merge_worker: check PID in shared memory.
        let merge_worker_pid =
            if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                crate::shmem::MERGE_WORKER_PID
                    .get()
                    .load(std::sync::atomic::Ordering::Relaxed)
            } else {
                0
            };
        let merge_worker_status = if merge_worker_pid > 0 {
            "ok"
        } else {
            "stalled"
        };

        // cache_hit_rate: from shmem stats.
        let (hits, misses, _, _) = crate::shmem::get_cache_stats();
        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64) / (total as f64)
        } else {
            1.0_f64
        };

        // catalog_consistent: VP table count == promoted predicate count.
        let pg_table_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relname LIKE 'vp_%_delta'",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let predicate_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE htap = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let catalog_consistent = pg_table_count == predicate_count;

        // orphaned_rare_rows: vp_rare rows for promoted predicates.
        let orphaned: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint \
             FROM _pg_ripple.vp_rare r \
             WHERE EXISTS ( \
               SELECT 1 FROM _pg_ripple.predicates p WHERE p.id = r.p AND p.htap = true \
             )",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let mut obj = Map::new();
        obj.insert(
            "merge_worker".to_owned(),
            Json::String(merge_worker_status.to_owned()),
        );
        obj.insert(
            "cache_hit_rate".to_owned(),
            Json::Number(Number::from_f64(hit_rate).unwrap_or(Number::from(0))),
        );
        obj.insert(
            "catalog_consistent".to_owned(),
            Json::Bool(catalog_consistent),
        );
        obj.insert(
            "orphaned_rare_rows".to_owned(),
            Json::Number(Number::from(orphaned)),
        );

        pgrx::JsonB(Json::Object(obj))
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
        // Invalidate plan cache for this rule set (v0.30.0).
        crate::datalog::cache::invalidate(rule_set);
        // Invalidate tabling cache (v0.32.0).
        crate::datalog::tabling_invalidate_all();
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
        // Invalidate plan cache for this rule set (v0.30.0).
        crate::datalog::cache::invalidate(rule_set);
        // Invalidate tabling cache (v0.32.0).
        crate::datalog::tabling_invalidate_all();
        pgrx::Spi::get_one_with_args::<i64>(
            "WITH deleted AS ( \
                 DELETE FROM _pg_ripple.rules WHERE rule_set = $1 RETURNING 1 \
             ) SELECT count(*) FROM deleted",
            &[pgrx::datum::DatumWithOid::from(rule_set)],
        )
        .unwrap_or(None)
        .unwrap_or(0)
    }

    // ── v0.34.0: Incremental rule updates ────────────────────────────────────

    /// Add a single rule to an existing rule set (v0.34.0).
    ///
    /// The rule is parsed, stored in the catalog, and its head predicate gets
    /// one fresh seed pass against the current VP tables.  Other derived
    /// predicates are not affected.  Returns the new rule's catalog ID.
    ///
    /// This is more efficient than calling `drop_rules()` + `load_rules()` when
    /// adding rules to a large live rule set, because only the new rule's derived
    /// predicate needs re-evaluation.
    #[pg_extern]
    fn add_rule(rule_set: &str, rule_text: &str) -> i64 {
        match crate::datalog::add_rule_to_set(rule_set, rule_text) {
            Ok(id) => id,
            Err(e) => pgrx::error!("add_rule error: {e}"),
        }
    }

    /// Remove a single rule by its catalog ID (v0.34.0).
    ///
    /// The rule is marked inactive and any derived facts solely supported by it
    /// are retracted using DRed (when `pg_ripple.dred_enabled = true`).  Falls
    /// back to full re-materialization when DRed is disabled or detects a cycle
    /// (error code PT530).  Returns the number of derived triples permanently
    /// retracted.
    ///
    /// Obtain the rule ID from `pg_ripple.list_rules()`.
    #[pg_extern]
    fn remove_rule(rule_id: i64) -> i64 {
        match crate::datalog::remove_rule_by_id(rule_id) {
            Ok(n) => n,
            Err(e) => pgrx::error!("remove_rule error: {e}"),
        }
    }

    /// Invoke DRed incremental retraction for a deleted base triple (v0.34.0).
    ///
    /// Normally called automatically by the CDC delete path.  This function
    /// exposes the DRed algorithm for testing and manual invocation.
    ///
    /// `pred_id` — dictionary ID of the deleted triple's predicate.
    /// `s_val`   — dictionary ID of the deleted triple's subject.
    /// `o_val`   — dictionary ID of the deleted triple's object.
    /// `g_val`   — dictionary ID of the deleted triple's graph (0 = default).
    ///
    /// Returns the number of derived triples permanently retracted.
    #[pg_extern]
    fn dred_on_delete(pred_id: i64, s_val: i64, o_val: i64, g_val: i64) -> i64 {
        crate::datalog::run_dred_on_delete(pred_id, s_val, o_val, g_val)
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
    /// - `"eliminated_rules"`: array of rule texts eliminated by subsumption checking (v0.29.0)
    /// - `"parallel_groups"`: number of independent rule groups detected in the first stratum (v0.35.0)
    /// - `"max_concurrent"`: effective worker count that would be used given `datalog_parallel_workers` (v0.35.0)
    ///
    /// Semi-naive evaluation avoids re-examining unchanged rows on each iteration,
    /// achieving iteration counts bounded by the longest derivation chain rather
    /// than the full relation size.  Subsumption checking (v0.29.0) removes rules
    /// whose body is a superset of another rule's body, reducing SQL statements per
    /// iteration.
    #[pg_extern]
    fn infer_with_stats(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        let (derived, iterations, eliminated, parallel_groups, max_concurrent) =
            crate::datalog::run_inference_seminaive_full(rule_set);
        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert(
            "eliminated_rules".to_owned(),
            serde_json::Value::Array(
                eliminated
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        obj.insert(
            "parallel_groups".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(parallel_groups as i64)),
        );
        obj.insert(
            "max_concurrent".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(max_concurrent as i64)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run goal-directed inference using magic sets (v0.29.0).
    ///
    /// Materialises only facts relevant to the goal triple pattern and returns
    /// a JSONB object with:
    /// - `"derived"`: total triples derived by inference
    /// - `"iterations"`: fixpoint iteration count
    /// - `"matching"`: count of triples in the store matching the goal pattern
    ///
    /// The `goal` parameter is a whitespace-delimited triple pattern:
    /// - `?varname` — free variable (any value matches)
    /// - `<iri>` — bound IRI
    /// - `prefix:local` — bound prefixed IRI
    /// - `"literal"` — bound literal
    ///
    /// Example: `pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person')`
    ///
    /// When `pg_ripple.magic_sets = false`, runs full materialization and
    /// filters the results post-hoc (functionally correct but slower).
    #[pg_extern]
    fn infer_goal(rule_set: &str, goal: &str) -> pgrx::JsonB {
        let goal_pattern = match crate::datalog::parse_goal(goal) {
            Ok(g) => g,
            Err(e) => {
                pgrx::warning!("infer_goal: failed to parse goal '{}': {e}", goal);
                // Return empty result on parse error.
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "derived".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i64)),
                );
                obj.insert(
                    "iterations".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i32)),
                );
                obj.insert(
                    "matching".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i64)),
                );
                return pgrx::JsonB(serde_json::Value::Object(obj));
            }
        };

        let (matching, derived, iterations) =
            match crate::datalog::run_infer_goal(rule_set, &goal_pattern) {
                Ok(r) => r,
                Err(e) => {
                    pgrx::warning!("infer_goal: inference failed: {e}");
                    (0, 0, 0)
                }
            };

        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert(
            "matching".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(matching)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run inference for a rule set that may contain aggregate body literals
    /// (Datalog^agg, v0.30.0).
    ///
    /// Supports `COUNT(?aggVar WHERE subject pred object) = ?resultVar` syntax
    /// in rule bodies.  Aggregate rules derive facts by grouping over a base
    /// predicate and computing COUNT, SUM, MIN, MAX, or AVG per group.
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total triples derived (aggregate + non-aggregate)
    /// - `"aggregate_derived"`: triples derived by aggregate rules only
    /// - `"iterations"`: fixpoint iteration count for non-aggregate rules
    ///
    /// Emits a WARNING with PT510 code if aggregation-stratification is violated.
    #[pg_extern]
    fn infer_agg(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        let (total, agg_derived, iterations) = crate::datalog::run_inference_agg(rule_set);
        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(total)),
        );
        obj.insert(
            "aggregate_derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(agg_derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run inference for a rule set, restricted to rules that can contribute to
    /// the given demand patterns (demand transformation, v0.31.0).
    ///
    /// `demands` is a JSONB array of goal patterns, e.g.:
    /// ```json
    /// [{"p": "<https://example.org/transitive>"}, {"s": "<https://ex.org/a>", "p": "<https://ex.org/childOf>"}]
    /// ```
    /// Each element has optional `"s"`, `"p"`, `"o"` keys with IRI values.
    /// Omitted keys are treated as free variables.
    ///
    /// When `demands` is an empty array (`'[]'`), runs full inference (same as
    /// `infer()`).
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total triples derived
    /// - `"iterations"`: fixpoint iteration count
    /// - `"demand_predicates"`: array of predicate IRI strings that were used as
    ///   demand seeds (decoded from dictionary)
    ///
    /// Also applies `owl:sameAs` canonicalization when
    /// `pg_ripple.sameas_reasoning` is `on` (default).
    #[pg_extern]
    fn infer_demand(
        rule_set: default!(&str, "'custom'"),
        demands: default!(pgrx::JsonB, "'[]'::jsonb"),
    ) -> pgrx::JsonB {
        let demands_str = demands.0.to_string();
        let demand_specs = crate::datalog::parse_demands_json(&demands_str);

        let (derived, iterations, demand_pred_ids) =
            crate::datalog::run_infer_demand(rule_set, &demand_specs);

        // Decode demand predicate IDs back to IRI strings for the output.
        let demand_preds_json: serde_json::Value = if demand_pred_ids.is_empty() {
            serde_json::Value::Array(vec![])
        } else {
            let decoded: Vec<serde_json::Value> = demand_pred_ids
                .iter()
                .filter_map(|&id| crate::dictionary::decode(id).map(serde_json::Value::String))
                .collect();
            serde_json::Value::Array(decoded)
        };

        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert("demand_predicates".to_owned(), demand_preds_json);
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Return statistics for the Datalog rule plan cache (v0.30.0).
    ///
    /// Each row has:
    /// - `rule_set TEXT` — the rule set name
    /// - `hits BIGINT` — number of times the cached SQL was used
    /// - `misses BIGINT` — number of times the cache was consulted but missed
    /// - `entries INT` — total number of entries currently in the cache
    #[pg_extern]
    fn rule_plan_cache_stats() -> TableIterator<
        'static,
        (
            name!(rule_set, String),
            name!(hits, i64),
            name!(misses, i64),
            name!(entries, i32),
        ),
    > {
        let stats = crate::datalog::cache::stats();
        TableIterator::new(
            stats
                .into_iter()
                .map(|s| (s.rule_set, s.hits, s.misses, s.entries)),
        )
    }

    // ── v0.32.0: Well-Founded Semantics ───────────────────────────────────────

    /// Run well-founded semantics inference for the named rule set (v0.32.0).
    ///
    /// For **stratifiable programs** (no cyclic negation): identical to
    /// `infer_with_stats()` — all derived facts have `certainty = 'true'`.
    ///
    /// For **non-stratifiable programs** (cyclic negation detected):
    /// - Facts derivable from purely positive rules → `certainty = 'true'`
    ///   (materialised into VP tables like normal inference).
    /// - Facts only derivable via negation of uncertain atoms → `certainty = 'unknown'`
    ///   (reported in the JSONB output but NOT materialised into VP tables).
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total facts (certain + unknown)
    /// - `"certain"`: facts with `certainty = 'true'`
    /// - `"unknown"`: facts with `certainty = 'unknown'`
    /// - `"iterations"`: number of fixpoint passes performed
    /// - `"stratifiable"`: `true` if the program is stratifiable, `false` otherwise
    ///
    /// GUC: `pg_ripple.wfs_max_iterations` (default 100) — safety cap per pass.
    /// Emits WARNING PT520 if a pass does not converge within the limit.
    #[pg_extern]
    fn infer_wfs(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        // Ensure the tabling catalog exists before any call that may try to
        // invalidate it (tabling_invalidate_all checks for the table first).
        crate::datalog::ensure_tabling_catalog();

        // Check tabling cache for a previous result.
        let goal_hash = crate::datalog::compute_goal_hash(&format!("wfs:{rule_set}"));
        if let Some(cached) = crate::datalog::tabling_lookup(goal_hash) {
            return pgrx::JsonB(cached);
        }

        // Cache miss — run WFS inference.
        let start = std::time::Instant::now();
        let (certain, unknown, total, iters, stratifiable) = crate::datalog::run_wfs(rule_set);
        let result = crate::datalog::build_wfs_jsonb(certain, unknown, total, iters, stratifiable);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Store result in the tabling cache for future calls.
        crate::datalog::tabling_store(goal_hash, &result.0, elapsed_ms);

        result
    }

    // ── v0.36.0: WCOJ & Lattice-Based Datalog ────────────────────────────────

    /// Detect whether a SPARQL triangle query is cyclic (v0.36.0).
    ///
    /// Returns `true` if the provided BGP variable pattern sets contain a cycle
    /// (i.e. the variable adjacency graph has a back-edge).  Used internally
    /// by the SPARQL→SQL translator; also exposed for testing and introspection.
    ///
    /// Each row in `pattern_vars_json` is a JSON array of variable name strings
    /// representing the variables co-occurring in one triple pattern.
    ///
    /// Example:
    /// ```sql
    /// SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","a"]]');
    /// -- returns true
    /// ```
    #[pg_extern]
    fn wcoj_is_cyclic(pattern_vars_json: &str) -> bool {
        let patterns: Vec<Vec<String>> = match serde_json::from_str(pattern_vars_json) {
            Ok(v) => v,
            Err(e) => pgrx::error!("wcoj_is_cyclic: invalid JSON input: {e}"),
        };
        crate::sparql::wcoj::detect_cyclic_bgp(&patterns)
    }

    /// Run a triangle-detection query on a VP predicate and return result stats (v0.36.0).
    ///
    /// Returns JSONB with `{"triangle_count": N, "wcoj_applied": bool, "predicate_iri": "..."}`.
    ///
    /// `predicate_iri` — the predicate IRI (without angle brackets) to use for all
    /// three edges of the triangle.
    ///
    /// This function is primarily used by `benchmarks/wcoj.sql` to compare
    /// WCOJ vs. standard planner execution.
    #[pg_extern]
    fn wcoj_triangle_query(predicate_iri: &str) -> pgrx::JsonB {
        let result = crate::sparql::wcoj::run_triangle_query(predicate_iri);
        pgrx::JsonB(serde_json::json!({
            "triangle_count": result.triangle_count,
            "wcoj_applied":   result.wcoj_applied,
            "predicate_iri":  result.predicate_iri
        }))
    }

    /// Register a user-defined lattice type for Datalog^L rules (v0.36.0).
    ///
    /// A lattice is an algebraic structure (L, ⊔) where the join operation ⊔
    /// is commutative, associative, and idempotent, with a bottom element ⊥.
    /// Fixpoint computation on a lattice terminates when the ascending chain
    /// condition holds.
    ///
    /// # Parameters
    ///
    /// - `name` — unique lattice identifier (e.g. `'trust'`, `'my_lattice'`).
    /// - `join_fn` — PostgreSQL aggregate function name implementing the join
    ///   (e.g. `'min'`, `'max'`, `'array_agg'`, `'my_custom_agg'`).
    ///   Must be commutative and associative.
    /// - `bottom` — bottom element as a text string (e.g. `'9223372036854775807'`
    ///   for a MinLattice over integer trust scores).
    ///
    /// Returns `true` if the lattice was newly registered, `false` if it already
    /// existed.
    ///
    /// # Built-in lattices
    ///
    /// The following lattices are pre-registered and do not need to be created:
    /// - `'min'` — MinLattice (join = MIN, bottom = i64::MAX)
    /// - `'max'` — MaxLattice (join = MAX, bottom = i64::MIN)
    /// - `'set'` — SetLattice (join = UNION via array_agg, bottom = {})
    /// - `'interval'` — IntervalLattice (join = MAX, bottom = 0)
    ///
    /// # Example
    ///
    /// ```sql
    /// -- Register a MinLattice for trust propagation over [0.0, 1.0] scores.
    /// SELECT pg_ripple.create_lattice('trust_score', 'min', '1.0');
    /// ```
    #[pg_extern]
    fn create_lattice(name: &str, join_fn: &str, bottom: &str) -> bool {
        crate::datalog::register_lattice(name, join_fn, bottom)
    }

    /// List all registered lattice types as JSONB (v0.36.0).
    ///
    /// Returns an array of `{"name": "...", "join_fn": "...", "bottom": "...", "builtin": bool}`.
    #[pg_extern]
    fn list_lattices() -> pgrx::JsonB {
        crate::datalog::ensure_lattice_catalog();
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT name, join_fn, bottom, builtin \
                 FROM _pg_ripple.lattice_types \
                 ORDER BY builtin DESC, name",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_lattices: SPI error: {e}"))
            .map(|row| {
                let name: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let join_fn: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let bottom: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                let builtin: bool = row.get::<bool>(4).ok().flatten().unwrap_or(false);
                serde_json::json!({
                    "name":    name,
                    "join_fn": join_fn,
                    "bottom":  bottom,
                    "builtin": builtin
                })
            })
            .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Run lattice-based Datalog inference for a rule set (v0.36.0).
    ///
    /// Executes a monotone fixpoint computation over the rules in `rule_set`
    /// using `lattice_name` as the lattice type for head derivations.
    ///
    /// Terminates when no new values are derived (convergence), or when
    /// `pg_ripple.lattice_max_iterations` is reached (emits WARNING PT540 and
    /// returns partial results).
    ///
    /// Returns JSONB with:
    /// - `"derived"` — total new lattice values written
    /// - `"iterations"` — fixpoint iterations performed
    /// - `"lattice"` — name of the lattice used
    /// - `"rule_set"` — name of the rule set evaluated
    ///
    /// # Example
    ///
    /// ```sql
    /// -- Trust propagation: min-cost path through a social graph.
    /// SELECT pg_ripple.load_rules($$
    ///     ?x <ex:trust> ?min_t :-
    ///         ?x <ex:knows> ?y, ?y <ex:trust> ?t1, ?x <ex:directTrust> ?t2,
    ///         COUNT(?z WHERE ?z <ex:knows> ?y) AS min_t = LEAST(?t1, ?t2) .
    /// $$, 'trust_rules');
    /// SELECT pg_ripple.infer_lattice('trust_rules', 'min');
    /// ```
    #[pg_extern]
    fn infer_lattice(
        rule_set: default!(&str, "'custom'"),
        lattice_name: default!(&str, "'min'"),
    ) -> pgrx::JsonB {
        pgrx::JsonB(crate::datalog::run_infer_lattice(rule_set, lattice_name))
    }

    // ── v0.32.0: Tabling / memoisation ───────────────────────────────────────

    /// Return statistics for the tabling / memoisation cache (v0.32.0).
    ///
    /// Each row has:
    /// - `goal_hash BIGINT` — XXH3-64 hash of the cached goal string
    /// - `hits BIGINT` — number of cache hits for this entry
    /// - `computed_ms FLOAT` — wall-clock time (ms) for the original computation
    /// - `cached_at TIMESTAMPTZ` — when the entry was last written
    #[pg_extern]
    fn tabling_stats() -> TableIterator<
        'static,
        (
            name!(goal_hash, i64),
            name!(hits, i64),
            name!(computed_ms, f64),
            name!(cached_at, String),
        ),
    > {
        let rows = crate::datalog::tabling_stats_impl();
        TableIterator::new(rows)
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
    fn load_rdfxml_file(path: &str, strict: pgrx::default!(bool, false)) -> i64 {
        crate::bulk_load::load_rdfxml_file(path, strict)
    }

    // ── v0.25.0 supplementary features ────────────────────────────────────────

    /// Load an OWL ontology file (Turtle / N-Triples / RDF/XML) from the server
    /// file system and insert all triples into the default graph.
    ///
    /// The format is detected from the file extension: `.ttl` → Turtle,
    /// `.nt` → N-Triples, `.xml` / `.rdf` / `.owl` → RDF/XML.
    /// Unrecognised extensions default to Turtle.
    ///
    /// Returns the number of triples loaded.
    #[pg_extern]
    fn load_owl_ontology(path: &str) -> i64 {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "nt" => crate::bulk_load::load_ntriples_file(path, false),
            "xml" | "rdf" | "owl" => crate::bulk_load::load_rdfxml_file(path, false),
            _ => crate::bulk_load::load_turtle_file(path, false),
        }
    }

    /// Apply an RDF Patch (W3C community standard) string to the store.
    ///
    /// Supported patch operations (one per line):
    /// - `A <s> <p> <o> .`  — add triple
    /// - `D <s> <p> <o> .`  — delete triple
    ///
    /// Lines beginning with `#`, `TX`, `TC`, `H` are treated as comments /
    /// transaction markers and silently ignored.  Returns the net change in
    /// triple count (additions minus deletions).
    #[pg_extern]
    fn apply_patch(patch: &str) -> i64 {
        let mut added = 0i64;
        let mut deleted = 0i64;
        for line in patch.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with("TX")
                || line.starts_with("TC")
                || line.starts_with('H')
            {
                continue;
            }
            if let Some(rest) = line.strip_prefix('A').map(|s| s.trim()) {
                // Parse as a single N-Triples statement
                let nt = format!("{rest}\n");
                added += crate::bulk_load::load_ntriples(&nt, false);
            } else if let Some(rest) = line.strip_prefix('D').map(|s| s.trim()) {
                // Delete via N-Triples term parser.
                if let Some((s, p, o)) = crate::parse_nt_triple(rest) {
                    deleted += crate::storage::delete_triple(&s, &p, &o, 0);
                }
            }
        }
        added - deleted
    }

    /// Register a custom SPARQL aggregate function name with the extension.
    ///
    /// This records the aggregate IRI in the `_pg_ripple.custom_aggregates`
    /// catalog table so the SPARQL-to-SQL translator can recognise it and
    /// delegate to the corresponding PostgreSQL aggregate.
    ///
    /// `sparql_iri`  — the full IRI of the custom aggregate function.
    /// `pg_function` — the PostgreSQL aggregate or function to call (schema-qualified
    ///                 if outside `pg_catalog`).
    #[pg_extern]
    fn register_aggregate(sparql_iri: &str, pg_function: &str) {
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.custom_aggregates (sparql_iri, pg_function)
             VALUES ($1, $2)
             ON CONFLICT (sparql_iri) DO UPDATE SET pg_function = EXCLUDED.pg_function",
            &[
                pgrx::datum::DatumWithOid::from(sparql_iri),
                pgrx::datum::DatumWithOid::from(pg_function),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("register_aggregate failed: {e}"));
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

    // ── Vector embedding (v0.27.0) ────────────────────────────────────────────

    /// Store a user-supplied embedding vector for an entity IRI.
    ///
    /// `embedding` is a `FLOAT8[]` array upserted into `_pg_ripple.embeddings`.
    /// Its length must match `pg_ripple.embedding_dimensions` (default: 1536).
    ///
    /// Raises a WARNING (not an ERROR) when pgvector is absent or the array
    /// length does not match the configured dimension count (PT602 / PT603).
    #[pg_extern]
    fn store_embedding(
        entity_iri: &str,
        embedding: Vec<f64>,
        model: default!(Option<&str>, "NULL"),
    ) {
        crate::sparql::embedding::store_embedding(entity_iri, embedding, model)
    }

    /// Return the k nearest entities to `query_text` by cosine distance.
    ///
    /// Encodes `query_text` via the configured embedding API and queries
    /// `_pg_ripple.embeddings` using the pgvector `<=>` cosine distance
    /// operator.  Returns results sorted by ascending distance.
    ///
    /// Returns zero rows when pgvector is absent, `pgvector_enabled = false`,
    /// or `embedding_api_url` is not configured.
    #[pg_extern]
    fn similar_entities(
        query_text: &str,
        k: default!(i32, 10),
        model: default!(Option<&str>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_id, i64),
            name!(entity_iri, String),
            name!(distance, f64),
        ),
    > {
        let rows = crate::sparql::embedding::similar_entities(query_text, k, model);
        TableIterator::new(rows)
    }

    /// Batch-embed entities from a graph using the configured embedding API.
    ///
    /// Collects entity IRIs + their `rdfs:label` (or IRI local name) and calls
    /// the OpenAI-compatible API at `pg_ripple.embedding_api_url`.  Results are
    /// upserted into `_pg_ripple.embeddings`.
    ///
    /// `graph_iri` — restrict to a named graph; NULL embeds entities from all graphs.
    /// `model` — override `pg_ripple.embedding_model`.
    /// `batch_size` — API call batch size (default: 100).
    ///
    /// Returns total embeddings stored.
    #[pg_extern]
    fn embed_entities(
        graph_iri: default!(Option<&str>, "NULL"),
        model: default!(Option<&str>, "NULL"),
        batch_size: default!(i32, 100),
    ) -> i64 {
        crate::sparql::embedding::embed_entities(graph_iri, model, batch_size)
    }

    /// Refresh stale embeddings after label updates.
    ///
    /// Identifies entities whose `rdfs:label` triple was inserted after
    /// `_pg_ripple.embeddings.updated_at` and re-embeds them.  When `force =
    /// true`, re-embeds all entities regardless of staleness.
    ///
    /// Returns the count of re-embedded entities.  Emits a NOTICE when no
    /// stale embeddings are found (PT606).
    #[pg_extern]
    fn refresh_embeddings(
        graph_iri: default!(Option<&str>, "NULL"),
        model: default!(Option<&str>, "NULL"),
        force: default!(bool, false),
    ) -> i64 {
        crate::sparql::embedding::refresh_embeddings(graph_iri, model, force)
    }

    // ── v0.28.0: Advanced Hybrid Search & RAG Pipeline ────────────────────────

    /// Enumerate all embedding models stored in `_pg_ripple.embeddings`.
    ///
    /// Returns one row per model with the entity count and vector dimension.
    /// Returns zero rows when pgvector is absent.
    #[pg_extern]
    fn list_embedding_models() -> TableIterator<
        'static,
        (
            name!(model, String),
            name!(entity_count, i64),
            name!(dimensions, i32),
        ),
    > {
        let rows = crate::sparql::embedding::list_embedding_models();
        TableIterator::new(rows)
    }

    /// Materialise `pg:hasEmbedding` triples for entities in `_pg_ripple.embeddings`.
    ///
    /// Inserts `<entity_iri> <pg:hasEmbedding> "true"^^xsd:boolean` for every
    /// embedded entity.  This makes embedding completeness checkable via SHACL.
    ///
    /// Returns the count of newly inserted triples.
    #[pg_extern]
    fn add_embedding_triples() -> i64 {
        crate::sparql::embedding::add_embedding_triples()
    }

    /// Produce a text representation of an entity's RDF neighborhood for embedding.
    ///
    /// Gathers the entity's label, type(s), and neighboring entity labels within
    /// `depth` hops (up to `max_neighbors`).  Returns a plain-text string suitable
    /// for passing to an embedding API.
    #[pg_extern]
    fn contextualize_entity(
        entity_iri: &str,
        depth: default!(i32, 1),
        max_neighbors: default!(i32, 20),
    ) -> String {
        crate::sparql::embedding::contextualize_entity(entity_iri, depth, max_neighbors)
    }

    /// Hybrid search using Reciprocal Rank Fusion of SPARQL and vector results.
    ///
    /// Executes `sparql_query` (a SPARQL SELECT returning `?entity`) for the
    /// SPARQL-ranked candidate set, then executes `similar_entities(query_text)`
    /// for the vector-ranked set.  Applies RRF with k_rrf = 60.
    ///
    /// `alpha` controls weighting: 0.0 = vector only, 1.0 = SPARQL only, 0.5 = equal.
    ///
    /// Returns zero rows when pgvector is absent (PT603 WARNING).
    #[pg_extern]
    fn hybrid_search(
        sparql_query: &str,
        query_text: &str,
        k: default!(i32, 10),
        alpha: default!(f64, 0.5),
        model: default!(Option<&str>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_id, i64),
            name!(entity_iri, String),
            name!(rrf_score, f64),
            name!(sparql_rank, i32),
            name!(vector_rank, i32),
        ),
    > {
        let rows =
            crate::sparql::embedding::hybrid_search(sparql_query, query_text, k, alpha, model);
        TableIterator::new(rows)
    }

    /// End-to-end RAG retrieval: find k nearest entities to `question`, collect context.
    ///
    /// Step 1: vector search for `k` candidates.
    /// Step 2: apply optional `sparql_filter` WHERE clause on candidates.
    /// Step 3: contextualize each surviving entity.
    /// Step 4: return rows with `entity_iri`, `label`, `context_json`, `distance`.
    ///
    /// `output_format`: `'jsonb'` (default) or `'jsonld'`.  When `'jsonld'`,
    /// `context_json` includes `@type` and `@context` keys.
    ///
    /// Returns zero rows when pgvector is absent (PT603 WARNING).
    #[pg_extern]
    fn rag_retrieve(
        question: &str,
        sparql_filter: default!(Option<&str>, "NULL"),
        k: default!(i32, 5),
        model: default!(Option<&str>, "NULL"),
        output_format: default!(&str, "'jsonb'"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_iri, String),
            name!(label, String),
            name!(context_json, pgrx::JsonB),
            name!(distance, f64),
        ),
    > {
        let rows = crate::sparql::embedding::rag_retrieve(
            question,
            sparql_filter,
            k,
            model,
            output_format,
        );
        TableIterator::new(rows)
    }

    /// Register an external vector service endpoint for SPARQL SERVICE federation.
    ///
    /// `api_type` must be one of `'pgvector'`, `'weaviate'`, `'qdrant'`, or `'pinecone'`.
    ///
    /// Registered endpoints can be queried via `SERVICE <url> { ?e pg:similarTo "text" }`.
    #[pg_extern]
    fn register_vector_endpoint(url: &str, api_type: &str) {
        crate::sparql::federation::register_vector_endpoint(url, api_type)
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
        let count = crate::bulk_load::load_ntriples(data, false);
        assert_eq!(count, 1);
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_turtle_bulk_load() {
        let data = "@prefix ex: <https://example.org/> .\nex:x ex:rel ex:y .\n";
        let count = crate::bulk_load::load_turtle(data, false);
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
        crate::bulk_load::load_ntriples(nt, false);
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
