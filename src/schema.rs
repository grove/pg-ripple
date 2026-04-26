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
    tombstones_cleared_at TIMESTAMPTZ
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

// v0.37.0: Schema version tracking table.
pgrx::extension_sql!(
    r#"
-- Schema version tracking (v0.37.0)
-- Stamped at CREATE EXTENSION time and on every ALTER EXTENSION ... UPDATE.
CREATE TABLE IF NOT EXISTS _pg_ripple.schema_version (
    version       TEXT        NOT NULL,
    installed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    upgraded_from TEXT
);

-- Stamp initial install version.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.37.0', NULL)
ON CONFLICT DO NOTHING;
"#,
    name = "v037_schema_version",
    requires = ["v036_lattice_types"]
);

// v0.38.0: SHACL-to-SPARQL planner hints catalog.
// Populated automatically when shapes are loaded via pg_ripple.load_shacl().
pgrx::extension_sql!(
    r#"
CREATE TABLE IF NOT EXISTS _pg_ripple.shape_hints (
    predicate_id  BIGINT  NOT NULL,
    hint_type     TEXT    NOT NULL,  -- 'max_count_1' | 'min_count_1'
    shape_iri_id  BIGINT  NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (predicate_id, hint_type)
);
CREATE INDEX IF NOT EXISTS shape_hints_pred_idx
    ON _pg_ripple.shape_hints (predicate_id);
"#,
    name = "v038_shape_hints",
    requires = ["v037_schema_version"]
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

// v0.40.0: stat_statements_decoded view.
// Wraps pg_stat_statements with a helper column for decoded query text.
// Only created when pg_stat_statements extension is installed.
pgrx::extension_sql!(
    r#"
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements'
    ) THEN
        EXECUTE $view$
            CREATE OR REPLACE VIEW pg_ripple.stat_statements_decoded AS
            SELECT
                pss.userid,
                pss.dbid,
                pss.queryid,
                pss.query,
                pss.calls,
                pss.total_exec_time,
                pss.mean_exec_time,
                pss.rows,
                pss.query AS query_decoded
            FROM pg_stat_statements pss
        $view$;
    END IF;
END;
$$;
"#,
    name = "stat_statements_decoded_view",
    requires = ["predicate_stats_view"]
);

// Stamp the compiled (CARGO_PKG_VERSION) version at fresh-install time so that
// diagnostic_report() returns a matching schema_version on a clean CREATE EXTENSION.
// Uses clock_timestamp() so this row is inserted after the v0.37.0 init row
// (both share the same transaction-start now() value) and is therefore returned
// first by "ORDER BY installed_at DESC LIMIT 1".
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.48.0', NULL, clock_timestamp());",
    name = "v048_schema_version_fresh_install_stamp",
    requires = ["v037_schema_version"]
);

// v0.49.0: LLM few-shot examples table.
pgrx::extension_sql!(
    r#"
-- Few-shot question → SPARQL examples for the NL-to-SPARQL LLM integration (v0.49.0).
-- Rows are loaded by sparql_from_nl() on each call to provide context to the LLM.
CREATE TABLE IF NOT EXISTS _pg_ripple.llm_examples (
    question    TEXT        NOT NULL PRIMARY KEY,
    sparql      TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.llm_examples IS
    'Few-shot question/SPARQL examples for the NL-to-SPARQL LLM integration. '
    'Populated via pg_ripple.add_llm_example().';
"#,
    name = "v049_llm_examples",
    requires = ["v048_schema_version_fresh_install_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.49.0', NULL, clock_timestamp());",
    name = "v049_schema_version_fresh_install_stamp",
    requires = ["v049_llm_examples"]
);

// v0.50.0: Developer Experience & GraphRAG Polish.
// New Rust functions: explain_sparql(query, analyze) extended with cache_status +
// actual_rows; rag_context(question, k) full RAG pipeline.
// No schema changes — stamp only.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.50.0', NULL, clock_timestamp());",
    name = "v050_schema_version_fresh_install_stamp",
    requires = ["v049_schema_version_fresh_install_stamp"]
);

// v0.51.0: Security Hardening & Production Readiness.
// New SQL-visible features: sparql_max_algebra_depth / sparql_max_triple_patterns GUCs,
// sparql_csv() / sparql_tsv(), predicate_workload_stats().
// No schema changes for fresh install — predicate_stats is created on-demand
// by enable_live_statistics() via pg_trickle, and by the upgrade migration.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.51.0', NULL, clock_timestamp());",
    name = "v051_schema_version_fresh_install_stamp",
    requires = ["v050_schema_version_fresh_install_stamp"]
);

// v0.52.0: pg-trickle Relay Integration.
// New SQL-visible features: json_to_ntriples(), json_to_ntriples_and_load(),
// enable/disable_cdc_bridge_trigger(), cdc_bridge_triggers() SRF,
// triple_to_jsonld(), triples_to_jsonld(), statement_dedup_key(),
// load_vocab_template(), trickle_available().
// New catalog: _pg_ripple.cdc_bridge_triggers.
pgrx::extension_sql!(
    r#"
-- CDC bridge trigger catalog (v0.52.0).
-- One row per trigger installed via pg_ripple.enable_cdc_bridge_trigger().
CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_bridge_triggers (
    name         TEXT        NOT NULL PRIMARY KEY,
    predicate_id BIGINT      NOT NULL,
    outbox_table TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- PL/pgSQL trigger function used by per-predicate CDC bridge triggers.
-- TG_ARGV[0] = predicate_id (bigint text), TG_ARGV[1] = outbox table name.
CREATE OR REPLACE FUNCTION _pg_ripple.cdc_bridge_trigger_fn()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    pred_id    BIGINT  := TG_ARGV[0]::bigint;
    outbox_tbl TEXT    := TG_ARGV[1];
    s_iri      TEXT;
    p_iri      TEXT;
    o_iri      TEXT;
    payload    JSONB;
    dedup_key  TEXT;
    sid        BIGINT;
BEGIN
    SELECT value INTO s_iri FROM _pg_ripple.dictionary WHERE id = NEW.s;
    SELECT value INTO p_iri FROM _pg_ripple.dictionary WHERE id = pred_id;
    SELECT value INTO o_iri FROM _pg_ripple.dictionary WHERE id = NEW.o;
    sid := NEW.i;
    dedup_key := 'ripple:' || sid::text;
    payload := jsonb_build_object(
        '@context',   'https://schema.org/',
        '@id',        COALESCE(s_iri, '_:' || NEW.s::text),
        p_iri,        COALESCE(o_iri, NEW.o::text),
        '_dedup_key', dedup_key
    );
    EXECUTE format(
        'INSERT INTO %I (event_id, payload) VALUES ($1, $2) ON CONFLICT DO NOTHING',
        outbox_tbl
    ) USING dedup_key, payload;
    RETURN NEW;
END;
$$;
"#,
    name = "v052_cdc_bridge_schema",
    requires = ["v051_schema_version_fresh_install_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.52.0', NULL, clock_timestamp());",
    name = "v052_schema_version_fresh_install_stamp",
    requires = ["v052_cdc_bridge_schema"]
);

// ── v0.53.0 ───────────────────────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- RAG answer cache (v0.53.0)
-- Stores previously computed rag_context() results keyed by
-- (question_hash, k, schema_digest) to avoid redundant LLM round-trips.
CREATE TABLE IF NOT EXISTS _pg_ripple.rag_cache (
    question_hash TEXT         NOT NULL,
    k             INT          NOT NULL DEFAULT 10,
    schema_digest TEXT         NOT NULL DEFAULT '',
    result        TEXT         NOT NULL DEFAULT '',
    cached_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (question_hash, k, schema_digest)
);
CREATE INDEX IF NOT EXISTS idx_rag_cache_cached_at
    ON _pg_ripple.rag_cache (cached_at);
"#,
    name = "v053_rag_cache",
    requires = ["v052_schema_version_fresh_install_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.53.0', '0.52.0', clock_timestamp());",
    name = "v053_schema_version_stamp",
    requires = ["v053_rag_cache"]
);

// ── v0.54.0 ───────────────────────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- Replication status catalog (v0.54.0).
-- Tracks pending N-Triples batches delivered by the logical replication slot;
-- the logical_apply_worker reads and processes rows from this table.
CREATE TABLE IF NOT EXISTS _pg_ripple.replication_status (
    id           BIGSERIAL    NOT NULL PRIMARY KEY,
    slot_name    TEXT         NOT NULL DEFAULT 'pg_ripple_sub',
    batch_data   TEXT         NOT NULL DEFAULT '',
    received_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_replication_status_unprocessed
    ON _pg_ripple.replication_status (id)
    WHERE processed_at IS NULL;
"#,
    name = "v054_replication_status",
    requires = ["v053_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.54.0', '0.53.0', clock_timestamp());",
    name = "v054_schema_version_stamp",
    requires = ["v054_replication_status"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.55.0', '0.54.0', clock_timestamp());",
    name = "v055_schema_version_stamp",
    requires = ["v054_schema_version_stamp"]
);

// ── v0.56.0 ───────────────────────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- SPARQL audit log (v0.56.0).
-- Records SPARQL UPDATE / DELETE DATA / DROP / CLEAR / COPY / MOVE operations
-- when pg_ripple.audit_log_enabled = on.
CREATE TABLE IF NOT EXISTS _pg_ripple.audit_log (
    id                    BIGSERIAL    NOT NULL PRIMARY KEY,
    ts                    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    role                  NAME         NOT NULL DEFAULT current_user,
    txid                  BIGINT       NOT NULL DEFAULT txid_current(),
    operation             TEXT         NOT NULL DEFAULT '',
    query                 TEXT         NOT NULL DEFAULT '',
    affected_predicate_ids BIGINT[]    NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON _pg_ripple.audit_log (ts);

-- DDL event trigger catalog (v0.56.0).
-- Records DROP TABLE / DROP INDEX events on _pg_ripple.vp_* objects.
CREATE TABLE IF NOT EXISTS _pg_ripple.catalog_events (
    id           BIGSERIAL    NOT NULL PRIMARY KEY,
    ts           TIMESTAMPTZ  NOT NULL DEFAULT now(),
    op           TEXT         NOT NULL DEFAULT '',
    objname      TEXT         NOT NULL DEFAULT '',
    blocked_by_ripple BOOL    NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_catalog_events_ts ON _pg_ripple.catalog_events (ts);

-- L-2.4 (v0.56.0): Enable lz4 page-level TOAST compression on the dictionary
-- value column to reduce table size for long IRIs and literal strings.
-- PG18 supports lz4 compression natively; existing data is recompressed lazily
-- on next VACUUM or rewrite.  Silently ignored if lz4 is unavailable.
DO $$
BEGIN
    ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4;
EXCEPTION WHEN OTHERS THEN
    -- lz4 may not be compiled into this PostgreSQL build; not fatal.
    RAISE NOTICE 'pg_ripple: lz4 compression not available for dictionary.value: %', SQLERRM;
END;
$$;

-- I-2 (v0.56.0): DDL event trigger to warn when _pg_ripple.vp_* objects are
-- dropped outside pg_ripple maintenance functions.
-- The trigger is suppressed when pg_ripple.maintenance_mode = 'on' so that
-- the merge worker and vacuum functions can drop/rename VP tables freely.
CREATE OR REPLACE FUNCTION _pg_ripple.ddl_guard_vp_tables()
    RETURNS event_trigger
    LANGUAGE plpgsql
    SECURITY DEFINER
AS $$
DECLARE
    _obj record;
    _in_maintenance bool;
BEGIN
    -- Skip if we are inside a pg_ripple maintenance operation.
    _in_maintenance := coalesce(
        current_setting('pg_ripple.maintenance_mode', true) = 'on',
        false
    );
    IF _in_maintenance THEN
        RETURN;
    END IF;

    FOR _obj IN
        SELECT schema_name, object_name
        FROM pg_event_trigger_dropped_objects()
        WHERE object_type IN ('table', 'index')
          AND schema_name = '_pg_ripple'
          AND object_name LIKE 'vp_%'
    LOOP
        RAISE WARNING 'PT511: _pg_ripple relation % dropped outside pg_ripple maintenance function; '
                      'run pg_ripple.vacuum() to maintain consistent state', _obj.object_name;
        INSERT INTO _pg_ripple.catalog_events (op, objname, blocked_by_ripple)
        VALUES (tg_tag, _obj.schema_name || '.' || _obj.object_name, false);
    END LOOP;
END;
$$;

CREATE EVENT TRIGGER _pg_ripple_ddl_guard
    ON sql_drop
    EXECUTE FUNCTION _pg_ripple.ddl_guard_vp_tables();
"#,
    name = "v056_audit_and_catalog_events",
    requires = ["v055_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.56.0', '0.55.0', clock_timestamp());",
    name = "v056_schema_version_stamp",
    requires = ["v056_audit_and_catalog_events"]
);

// ─── v0.57.0: KGE embeddings + multi-tenant catalog ──────────────────────────

pgrx::extension_sql!(
    r#"
-- KGE embeddings table (v0.57.0 L-4.1).
-- Uses double precision[] to avoid a hard dependency on pgvector.
CREATE TABLE IF NOT EXISTS _pg_ripple.kge_embeddings (
    entity_id   BIGINT      NOT NULL PRIMARY KEY,
    embedding   double precision[],
    model       TEXT        NOT NULL DEFAULT 'transe',
    trained_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Multi-tenant catalog (v0.57.0 L-5.3).
CREATE TABLE IF NOT EXISTS _pg_ripple.tenants (
    tenant_name    TEXT        NOT NULL PRIMARY KEY,
    graph_iri      TEXT        NOT NULL,
    quota_triples  BIGINT      NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "v057_kge_tenants_setup",
    requires = ["v056_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.57.0', '0.56.0', clock_timestamp());",
    name = "v057_schema_version_stamp",
    requires = ["v057_kge_tenants_setup"]
);

// ─── v0.58.0 schema additions ─────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- Temporal RDF statement ID timeline (v0.58.0 L-1.3).
-- Maps statement IDs to wall-clock insertion timestamps for point-in-time
-- queries.  An AFTER INSERT trigger on vp_rare and every VP delta table
-- keeps this table current.
CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline (
    sid         BIGINT      NOT NULL PRIMARY KEY,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_statement_id_timeline_ts
    ON _pg_ripple.statement_id_timeline USING BRIN (inserted_at);

-- Trigger function that records each new SID with its insertion timestamp.
CREATE OR REPLACE FUNCTION _pg_ripple.record_statement_timestamp()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO _pg_ripple.statement_id_timeline (sid, inserted_at)
    VALUES (NEW.i, now())
    ON CONFLICT (sid) DO NOTHING;
    RETURN NEW;
END;
$$;

-- Attach to vp_rare so non-promoted predicates are also tracked.
DO $do$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger t
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_rare'
          AND t.tgname = 'trg_timeline_vp_rare'
    ) THEN
        EXECUTE 'CREATE TRIGGER trg_timeline_vp_rare
                 AFTER INSERT ON _pg_ripple.vp_rare
                 FOR EACH ROW
                 EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()';
    END IF;
END
$do$;

-- PROV-O provenance catalog (v0.58.0 L-8.4).
-- Tracks the source, activity IRI and triple count for every bulk ingest
-- operation when pg_ripple.prov_enabled = on.
CREATE TABLE IF NOT EXISTS _pg_ripple.prov_catalog (
    source        TEXT        NOT NULL PRIMARY KEY,
    activity_iri  TEXT        NOT NULL,
    triple_count  BIGINT      NOT NULL DEFAULT 0,
    last_updated  TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    name = "v058_temporal_prov_setup",
    requires = ["v057_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.58.0', '0.57.0', clock_timestamp());",
    name = "v058_schema_version_stamp",
    requires = ["v058_temporal_prov_setup"]
);
