//! Foundation schema tables, sequences, and indexes (v0.1.0 -- v0.28.0).
//!
//! Split from `schema.rs` in v0.85.0 (Q13-02).

//! pg_ripple schema DDL — all `extension_sql!` blocks that create
//! internal tables, sequences, views, and helper functions at
//! CREATE EXTENSION time.

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
    id                    BIGINT      NOT NULL PRIMARY KEY,
    table_oid             OID,
    triple_count          BIGINT      NOT NULL DEFAULT 0,
    htap                  BOOLEAN     NOT NULL DEFAULT false,
    schema_name           TEXT,
    table_name            TEXT,
    tombstones_cleared_at TIMESTAMPTZ,
    brin_summarize_failures INT       NOT NULL DEFAULT 0,
    promotion_status      TEXT
);

-- Rare-predicate consolidation table
CREATE TABLE IF NOT EXISTS _pg_ripple.vp_rare (
    p      BIGINT   NOT NULL,
    s      BIGINT   NOT NULL,
    o      BIGINT   NOT NULL,
    g      BIGINT   NOT NULL DEFAULT 0,
    i      BIGINT   NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'),
    source SMALLINT NOT NULL DEFAULT 0,
    CONSTRAINT vp_rare_psog_unique UNIQUE (p, s, o, g)
);
CREATE INDEX IF NOT EXISTS idx_vp_rare_p_s_o   ON _pg_ripple.vp_rare (p, s, o);
CREATE INDEX IF NOT EXISTS idx_vp_rare_s_p     ON _pg_ripple.vp_rare (s, p);
CREATE INDEX IF NOT EXISTS idx_vp_rare_g_p_s_o ON _pg_ripple.vp_rare (g, p, s, o);
-- v0.37.0: (o, s) index eliminates seq-scans on object-leading patterns
CREATE INDEX IF NOT EXISTS vp_rare_os_idx      ON _pg_ripple.vp_rare (o, s);

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

-- Named-graph registry (v0.43.0)
-- Tracks named graph IRIs that have been explicitly loaded, even if the
-- graph has zero triples (needed for GRAPH ?var { COUNT(*) } queries).
CREATE TABLE IF NOT EXISTS _pg_ripple.named_graphs (
    graph_id BIGINT NOT NULL PRIMARY KEY
);
CREATE INDEX IF NOT EXISTS idx_named_graphs_id ON _pg_ripple.named_graphs (graph_id);


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
-- Federation endpoint allowlist (v0.16.0, extended v0.19.0, v0.42.0)
-- Only endpoints with enabled = true are contacted via SERVICE clauses.
-- local_view_name: when set, SERVICE is rewritten to scan the named stream table.
-- complexity (v0.19.0): 'fast', 'normal', or 'slow' — used to order multi-endpoint queries.
-- graph_iri (v0.42.0): when set, SERVICE is satisfied locally by querying that named graph
--   instead of making an HTTP call.  Enables mock/local endpoint support for testing.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_endpoints (
    url             TEXT    NOT NULL PRIMARY KEY,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    local_view_name TEXT,
    complexity      TEXT    NOT NULL DEFAULT 'normal'
                    CHECK (complexity IN ('fast', 'normal', 'slow')),
    graph_iri       TEXT
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

// v0.42.0: VoID statistics catalog and CDC subscription registry.
pgrx::extension_sql!(
    r#"
-- VoID statistics catalog (v0.42.0)
-- Caches per-endpoint VoID statistics used by the cost-based federation planner.
CREATE TABLE IF NOT EXISTS _pg_ripple.endpoint_stats (
    endpoint_url         TEXT        NOT NULL PRIMARY KEY,
    total_triples        BIGINT      NOT NULL DEFAULT 0,
    predicate_stats_json TEXT        NOT NULL DEFAULT '{}',
    distinct_subjects    BIGINT      NOT NULL DEFAULT 0,
    distinct_objects     BIGINT      NOT NULL DEFAULT 0,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Named subscription registry (v0.42.0)
-- Stores named CDC subscriptions created via pg_ripple.create_subscription().
CREATE TABLE IF NOT EXISTS _pg_ripple.subscriptions (
    name            TEXT        NOT NULL PRIMARY KEY,
    filter_sparql   TEXT,
    filter_shape    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "v042_endpoint_stats_subscriptions",
    requires = ["v019_federation_cache_setup"]
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
    requires = ["v042_endpoint_stats_subscriptions"]
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
