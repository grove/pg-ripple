//! Foundation schema tables, sequences, and indexes (v0.1.0 -- v0.28.0).
// @allow-large-file: versioned foundation DDL remains grouped in one schema module
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
    promotion_status      TEXT,
    tombstone_count       BIGINT      NOT NULL DEFAULT 0
);

-- Merge-worker lifecycle diagnostics.
CREATE TABLE IF NOT EXISTS _pg_ripple.merge_worker_status (
    worker_idx        INTEGER PRIMARY KEY,
    pid               BIGINT NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'starting',
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    restart_count     BIGINT NOT NULL DEFAULT 0,
    last_error        TEXT,
    last_heartbeat_at TIMESTAMPTZ,
    predicates_total  BIGINT NOT NULL DEFAULT 0,
    delta_rows_pending BIGINT NOT NULL DEFAULT 0
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
CREATE INDEX IF NOT EXISTS idx_vp_rare_i       ON _pg_ripple.vp_rare (i);

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
    expansion TEXT NOT NULL,
    owner_oid OID NOT NULL DEFAULT (current_user::regrole)::oid,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS _pg_ripple.prefix_registry_state (
    singleton  BOOLEAN NOT NULL PRIMARY KEY DEFAULT true CHECK (singleton),
    generation BIGINT NOT NULL DEFAULT 1
);
INSERT INTO _pg_ripple.prefix_registry_state (singleton, generation)
VALUES (true, 1) ON CONFLICT (singleton) DO NOTHING;
REVOKE INSERT, UPDATE, DELETE ON _pg_ripple.prefixes FROM PUBLIC;

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

// v0.100.0: Proof tree / derivation provenance table (PROOF-TREE-01).
pgrx::extension_sql!(
    r#"
-- Derivation provenance table (v0.100.0 PROOF-TREE-01)
-- Records why each inferred fact was derived: which rule fired and which base
-- triples (antecedents) satisfied the rule body.  Populated only when
-- pg_ripple.record_derivations = on; stays empty (zero overhead) otherwise.
--
-- Columns:
--   derived_sid     — statement ID (vp_rare.i) of the inferred triple
--   rule_name       — the raw Datalog rule text (used as the human-readable name)
--   rule_set        — the rule set this rule belongs to (e.g. 'rdfs', 'owl-rl')
--   antecedent_sids — array of statement IDs of the body-atom triples that
--                     satisfied the rule for this specific derivation
--   created_at      — when the derivation was recorded
CREATE TABLE IF NOT EXISTS _pg_ripple.derivations (
    id              BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    derived_sid     BIGINT      NOT NULL,
    rule_name       TEXT        NOT NULL,
    rule_set        TEXT        NOT NULL DEFAULT '',
    antecedent_sids BIGINT[]    NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT derivations_unique UNIQUE (derived_sid, rule_name)
);
CREATE INDEX IF NOT EXISTS idx_derivations_derived_sid
    ON _pg_ripple.derivations (derived_sid);
CREATE INDEX IF NOT EXISTS idx_derivations_antecedent
    ON _pg_ripple.derivations USING GIN (antecedent_sids);
COMMENT ON TABLE _pg_ripple.derivations IS
    'Proof provenance for Datalog-inferred facts. '
    'Populated when pg_ripple.record_derivations = on. '
    'Query with pg_ripple.justify(subject, predicate, object).';
"#,
    name = "v0100_derivations_table",
    requires = ["v028_embedding_queue"]
);

pgrx::extension_sql!(
    r#"
-- NL explanation cache (v0.101.0 NL-EXPLAIN-01)
-- Caches natural-language explanations of Datalog-inferred facts to avoid
-- repeated LLM calls.  One row per (fact SID, format, LLM model) combination.
-- Keyed on SID so dictionary churn does not create stale cache entries.
-- Expires after pg_ripple.explanation_cache_ttl seconds (default: 3600).
CREATE TABLE IF NOT EXISTS _pg_ripple.explanation_cache (
    sid         BIGINT      NOT NULL,
    format      TEXT        NOT NULL DEFAULT 'text',
    model       TEXT        NOT NULL DEFAULT '',
    explanation TEXT        NOT NULL,
    cached_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (sid, format, model)
);
CREATE INDEX IF NOT EXISTS idx_explanation_cache_cached_at
    ON _pg_ripple.explanation_cache (cached_at);
COMMENT ON TABLE _pg_ripple.explanation_cache IS
    'NL explanation cache for Datalog-inferred facts (v0.101.0). '
    'One row per (sid, format, model). TTL controlled by '
    'pg_ripple.explanation_cache_ttl (default 3600 seconds). '
    'Vacuum with pg_ripple.vacuum_explanation_cache().';
"#,
    name = "v0101_explanation_cache",
    requires = ["v0100_derivations_table"]
);

// v0.106.0: Temporal fact store — predicates registry and facts table.
pgrx::extension_sql!(
    r#"
-- Temporal predicates registry (v0.106.0, extended v0.118.0)
-- Marks which predicates are time-aware and what data model they use.
-- data_model: 'snapshot' — closes previous open interval on re-assertion;
--             'versioned' — always inserts a new row, never modifies existing rows.
-- default_tz (v0.118.0): optional default time zone for temporal queries.
CREATE TABLE IF NOT EXISTS _pg_ripple.temporal_predicates (
    predicate_id BIGINT NOT NULL PRIMARY KEY,
    data_model   TEXT   NOT NULL
                 CHECK (data_model IN ('snapshot', 'versioned')),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    default_tz    TEXT
);

-- Temporal facts table (v0.106.0)
-- Stores facts with validity intervals.  No changes to VP table schemas.
-- valid_to IS NULL means the fact is currently valid (open-ended interval).
CREATE TABLE IF NOT EXISTS _pg_ripple.temporal_facts (
    s          BIGINT      NOT NULL,
    p          BIGINT      NOT NULL,
    o          BIGINT      NOT NULL,
    g          BIGINT      NOT NULL DEFAULT 0,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_to   TIMESTAMPTZ
);

-- B-tree on (s, p, valid_from, valid_to) for subject-scoped temporal queries.
CREATE INDEX IF NOT EXISTS idx_temporal_facts_s_p_vf_vt
    ON _pg_ripple.temporal_facts (s, p, valid_from, valid_to);

-- B-tree on (p, valid_from, valid_to) for predicate-scoped temporal scans.
CREATE INDEX IF NOT EXISTS idx_temporal_facts_p_vf_vt
    ON _pg_ripple.temporal_facts (p, valid_from, valid_to);

-- Partial B-tree for currently-valid (open-ended interval) facts.
CREATE INDEX IF NOT EXISTS idx_temporal_facts_open
    ON _pg_ripple.temporal_facts (valid_from, valid_to)
    WHERE valid_to IS NULL;
"#,
    name = "v0106_temporal_store",
    requires = ["v0101_explanation_cache"]
);

// v0.108.0: Bayesian confidence updates — evidence log and stale-confidence table.
pgrx::extension_sql!(
    r#"
-- Evidence log (v0.108.0 BAYES-01)
-- Append-only log of every evidence event that updates a fact's confidence.
-- Rows older than pg_ripple.evidence_log_retention are pruned by the background
-- worker.
-- No FK constraint on sid — dictionary IDs are hash-based and do not support FK.
CREATE TABLE IF NOT EXISTS _pg_ripple.evidence_log (
    id                  BIGSERIAL   PRIMARY KEY,
    sid                 BIGINT      NOT NULL,
    event_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    source_iri          BIGINT,
    likelihood_ratio    FLOAT8      NOT NULL,
    prior_confidence    FLOAT8      NOT NULL,
    posterior_confidence FLOAT8     NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_evidence_log_sid
    ON _pg_ripple.evidence_log (sid);
CREATE INDEX IF NOT EXISTS idx_evidence_log_event_at
    ON _pg_ripple.evidence_log (event_at);
COMMENT ON TABLE _pg_ripple.evidence_log IS
    'Bayesian confidence update log (v0.108.0). '
    'One row per evidence event. TTL controlled by '
    'pg_ripple.evidence_log_retention (default 1 year). '
    'Vacuum with pg_ripple.vacuum_evidence_log().';

-- Confidence stale queue (v0.108.0 BAYES-02)
-- Derived facts whose confidence update was capped by
-- pg_ripple.confidence_propagation_max_depth are recorded here and
-- reprocessed by the confidence_reprocessing background worker.
CREATE TABLE IF NOT EXISTS _pg_ripple.confidence_stale (
    sid        BIGINT      NOT NULL PRIMARY KEY,
    marked_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.confidence_stale IS
    'Queue of derived facts whose confidence has not been propagated '
    'because the cascade exceeded pg_ripple.confidence_propagation_max_depth. '
    'Drained by the confidence_reprocessing background worker.';
"#,
    name = "v0108_bayesian_confidence",
    requires = ["v0106_temporal_store"]
);

// v0.110.0: NS-RL evaluation harness, monitoring, rule explainability.
pgrx::extension_sql!(
    r#"
-- Rule explanation cache (v0.110.0 EXPLAIN-01)
-- Caches plain-English explanations of Datalog rules generated by explain_rule().
-- One row per (rule_id, language, format).
-- TTL controlled by pg_ripple.rule_explanation_cache_ttl (default '24 hours').
CREATE TABLE IF NOT EXISTS _pg_ripple.rule_explanations (
    rule_id             BIGINT      NOT NULL,
    language            TEXT        NOT NULL DEFAULT 'en',
    format              TEXT        NOT NULL DEFAULT 'text',
    explanation         TEXT        NOT NULL,
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    rule_version_stamp  BIGINT      NOT NULL DEFAULT 0,
    PRIMARY KEY (rule_id, language, format)
);
COMMENT ON TABLE _pg_ripple.rule_explanations IS
    'Plain-English explanation cache for Datalog rules (v0.110.0). '
    'One row per (rule_id, language, format). '
    'TTL controlled by pg_ripple.rule_explanation_cache_ttl (default 24 hours). '
    'rule_version_stamp incremented on store_rules/update_rule to invalidate stale entries (M16-05 v0.116.0).';

-- owl:sameAs anomaly log (v0.110.0 ANOMALY-01)
-- Append-only log of any owl:sameAs assertion that would exceed
-- pg_ripple.sameas_max_cluster_size (PT550).
-- INSERT ONLY RLS policy enforces append-only semantics.
CREATE TABLE IF NOT EXISTS _pg_ripple.sameas_anomaly_log (
    id                   BIGSERIAL   PRIMARY KEY,
    detected_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    entity1              BIGINT      NOT NULL,
    entity2              BIGINT      NOT NULL,
    cluster_size_before  INT         NOT NULL DEFAULT 0,
    cluster_size_after   INT         NOT NULL DEFAULT 0,
    trigger              TEXT        NOT NULL DEFAULT '',
    transaction_xid      XID8        NOT NULL DEFAULT pg_current_xact_id()
);
CREATE INDEX IF NOT EXISTS idx_sameas_anomaly_log_detected_at
    ON _pg_ripple.sameas_anomaly_log (detected_at);
ALTER TABLE _pg_ripple.sameas_anomaly_log ENABLE ROW LEVEL SECURITY;
-- Only INSERT is allowed; no UPDATE or DELETE on this audit log.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = '_pg_ripple'
          AND tablename  = 'sameas_anomaly_log'
          AND policyname = 'insert_only'
    ) THEN
        CREATE POLICY insert_only ON _pg_ripple.sameas_anomaly_log
            FOR INSERT WITH CHECK (true);
    END IF;
END;
$$;
COMMENT ON TABLE _pg_ripple.sameas_anomaly_log IS
    'Append-only audit log of owl:sameAs assertions that would exceed '
    'pg_ripple.sameas_max_cluster_size (PT550). '
    'Rows older than pg_ripple.sameas_anomaly_log_retention (default 90 days) '
    'are pruned by the background worker.';
"#,
    name = "v0110_nsrl_eval_tables",
    requires = ["v0108_bayesian_confidence"]
);

// v0.118.0: Privacy budget registry and benchmark history table.
pgrx::extension_sql!(
    r#"
-- Privacy budget registry (v0.118.0 Feature 2)
-- Tracks per-dataset per-principal differential-privacy epsilon budgets.
-- dp_noisy_count() and dp_noisy_histogram() deduct epsilon from budget_spent on
-- each call, raising PT0490 when the budget is exhausted.
-- Automatic daily reset is controlled by pg_ripple.privacy_budget_reset_interval.
CREATE TABLE IF NOT EXISTS _pg_ripple.privacy_budget (
    dataset_id    BIGINT      NOT NULL,
    principal     TEXT        NOT NULL,
    budget_total  FLOAT8      NOT NULL,
    budget_spent  FLOAT8      NOT NULL DEFAULT 0,
    last_reset_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT privacy_budget_pk PRIMARY KEY (dataset_id, principal),
    CONSTRAINT privacy_budget_total_pos CHECK (budget_total > 0),
    CONSTRAINT privacy_budget_spent_nonneg CHECK (budget_spent >= 0)
);
COMMENT ON TABLE _pg_ripple.privacy_budget IS
    'Per-dataset per-principal differential-privacy epsilon budget registry (v0.118.0). '
    'dp_noisy_count() and dp_noisy_histogram() deduct epsilon from budget_spent; '
    'PT0490 is raised when budget would be exceeded. '
    'Automatic daily reset controlled by pg_ripple.privacy_budget_reset_interval.';

-- Benchmark history table (v0.118.0 Feature 1)
-- Records results from pg_ripple.bench_workload() runs.
CREATE TABLE IF NOT EXISTS _pg_ripple.bench_history (
    run_id              BIGSERIAL   PRIMARY KEY,
    profile             TEXT        NOT NULL,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    duration_ms         BIGINT,
    triples_processed   BIGINT,
    queries_per_second  FLOAT8
);
CREATE INDEX IF NOT EXISTS idx_bench_history_started_at
    ON _pg_ripple.bench_history (started_at DESC);
COMMENT ON TABLE _pg_ripple.bench_history IS
    'Benchmark run history for pg_ripple.bench_workload() (v0.118.0 Feature 1). '
    'Exposed via GET /admin/bench-history.';
"#,
    name = "v0118_privacy_bench_tables",
    requires = ["v0110_nsrl_eval_tables"]
);

// v0.119.0: Federation circuit breaker persistent state table.
pgrx::extension_sql!(
    r#"
-- Federation circuit breaker state table (v0.119.0 Feature 6)
-- Tracks per-endpoint circuit state for observability and Prometheus gauge.
-- State values: 'closed' (normal), 'open' (blocked), 'half_open' (probing).
-- Updated by circuit_sync_to_db() on state transitions.
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_circuit_state (
    endpoint_iri    TEXT        PRIMARY KEY,
    state           TEXT        NOT NULL DEFAULT 'closed'
                                CHECK (state IN ('closed', 'open', 'half_open')),
    last_failure_at TIMESTAMPTZ,
    failure_count   INT         NOT NULL DEFAULT 0
);
COMMENT ON TABLE _pg_ripple.federation_circuit_state IS
    'Per-endpoint federation circuit breaker state (v0.119.0 Feature 6). '
    'Populated on state transitions by the SPARQL federation layer. '
    'Prometheus gauge: pg_ripple_federation_circuit_state{endpoint}.';
"#,
    name = "v0119_federation_circuit_state",
    requires = ["v0118_privacy_bench_tables"]
);

// v0.120.0: Rule-Library Federation catalog table.
pgrx::extension_sql!(
    r#"
-- Rule-Library Federation table (v0.120.0 Feature 11)
-- Tracks published and subscribed rule libraries for federation.
-- published=true:  this instance exposes the library via HTTP streaming.
-- subscribed=true: this instance pulls rules from source_uri.
CREATE TABLE IF NOT EXISTS _pg_ripple.rule_library_federation (
    name            TEXT        PRIMARY KEY,
    endpoint_uri    TEXT        NOT NULL,
    published       BOOLEAN     NOT NULL DEFAULT FALSE,
    subscribed      BOOLEAN     NOT NULL DEFAULT FALSE,
    source_uri      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.rule_library_federation IS
    'Rule-Library Federation registry (v0.120.0 Feature 11). '
    'Tracks which rule libraries are published or subscribed on this instance.';
"#,
    name = "v0120_rule_library_federation",
    requires = ["v0119_federation_circuit_state"]
);

// v0.125.0: Temporal graph snapshots catalog (FEAT-02).
pgrx::extension_sql!(
    r#"
-- Snapshot ID sequence (v0.125.0 FEAT-02)
CREATE SEQUENCE IF NOT EXISTS _pg_ripple.snapshot_id_seq
    START 1 INCREMENT 1 NO CYCLE;

-- Temporal graph snapshots catalog (v0.125.0 FEAT-02)
-- One row per (graph_iri, snapshot_time) materialised snapshot.
-- snapshot_iri: deterministic URN: urn:snapshot:{iri_slug}:{iso_ts}
-- captured_at:  timestamp of the snapshot request (same as snapshot_time)
-- triple_count: number of temporal facts valid at captured_at
-- expires_at:   NULL = keep forever; otherwise pruned by background worker
--               when expires_at <= now() (retention controlled by
--               pg_ripple.snapshot_retention_days GUC, default 30).
CREATE TABLE IF NOT EXISTS _pg_ripple.graph_snapshots (
    snapshot_id  BIGINT      NOT NULL DEFAULT nextval('_pg_ripple.snapshot_id_seq') PRIMARY KEY,
    graph_iri    TEXT        NOT NULL,
    snapshot_iri TEXT        NOT NULL UNIQUE,
    captured_at  TIMESTAMPTZ NOT NULL,
    triple_count BIGINT      NOT NULL DEFAULT 0,
    expires_at   TIMESTAMPTZ
);

-- B-tree on graph_iri for per-graph snapshot listing.
CREATE INDEX IF NOT EXISTS idx_graph_snapshots_graph_iri
    ON _pg_ripple.graph_snapshots (graph_iri);

-- Partial B-tree for expiry pruning (only rows with an expires_at set).
CREATE INDEX IF NOT EXISTS idx_graph_snapshots_expires_at
    ON _pg_ripple.graph_snapshots (expires_at)
    WHERE expires_at IS NOT NULL;

COMMENT ON TABLE _pg_ripple.graph_snapshots IS
    'Temporal graph snapshot registry (v0.125.0 FEAT-02). '
    'Populated by pg_ripple.graph_at(); pruned by background worker when '
    'expires_at <= now(). Retention controlled by '
    'pg_ripple.snapshot_retention_days (default 30 days, 0 = keep forever).';
"#,
    name = "v0125_graph_snapshots",
    requires = ["v0120_rule_library_federation"]
);

// v0.126.0: Per-endpoint federation credentials (FEAT-03).
pgrx::extension_sql!(
    r#"
-- Per-endpoint federation credentials catalog (v0.126.0 FEAT-03)
-- Stores OAuth2 Bearer and API-key tokens encrypted at rest via
-- pgcrypto pgp_sym_encrypt using a server-managed symmetric key
-- (pg_ripple.federation_credential_key GUC, never visible via SHOW).
--
-- endpoint_iri: REFERENCES _pg_ripple.federation_endpoints(url) ON DELETE CASCADE
-- auth_type:    'bearer' | 'apikey' | 'none'
-- encrypted_token: pgp_sym_encrypt output; never returned in queries
-- header_name:  HTTP header to inject (default 'Authorization')
-- created_at:   timestamp of initial creation
-- rotated_at:   timestamp of last token rotation (NULL until first rotation)
-- last_used_at: timestamp of last SERVICE call that used this credential
CREATE TABLE IF NOT EXISTS _pg_ripple.federation_credentials (
    endpoint_iri    TEXT        NOT NULL PRIMARY KEY
                    REFERENCES _pg_ripple.federation_endpoints(url)
                    ON DELETE CASCADE ON UPDATE CASCADE,
    auth_type       TEXT        NOT NULL
                    CHECK (auth_type IN ('bearer', 'apikey', 'none')),
    encrypted_token BYTEA       NOT NULL DEFAULT ''::bytea,
    header_name     TEXT        NOT NULL DEFAULT 'Authorization',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at      TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ
);

COMMENT ON TABLE _pg_ripple.federation_credentials IS
    'Per-endpoint federation credentials (v0.126.0 FEAT-03). '
    'Tokens encrypted with pgp_sym_encrypt; decrypted in-process at query time. '
    'Use pg_ripple.set_federation_credential() and '
    'pg_ripple.rotate_federation_credential() to manage credentials. '
    'Never query encrypted_token directly — use federation_credential_audit() '
    'for operational metadata.';
"#,
    name = "v0126_federation_credentials",
    requires = ["v0125_graph_snapshots"]
);

// v0.128.0: JSON mapping relational writeback (JSON-WRITEBACK-01).
// Schema changes:
//   - Five new columns on _pg_ripple.json_mappings for writeback configuration
//   - _pg_ripple.json_writeback_queue: async queue for trigger-based writeback events
//   - _pg_ripple.json_writeback_enqueue_fn(): PL/pgSQL trigger function
pgrx::extension_sql!(
    r#"
-- v0.128.0 JSON-WRITEBACK-01: Extend json_mappings with writeback configuration.
ALTER TABLE _pg_ripple.json_mappings
    ADD COLUMN IF NOT EXISTS writeback_table        TEXT,
    ADD COLUMN IF NOT EXISTS writeback_schema       TEXT    NOT NULL DEFAULT 'public',
    ADD COLUMN IF NOT EXISTS writeback_key_columns  TEXT[]  NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS writeback_conflict_policy TEXT NOT NULL DEFAULT 'replace'
        CHECK (writeback_conflict_policy IN ('replace', 'skip', 'error')),
    ADD COLUMN IF NOT EXISTS writeback_enabled      BOOLEAN NOT NULL DEFAULT false;

-- v0.128.0 JSON-WRITEBACK-01: Async writeback queue.
CREATE TABLE IF NOT EXISTS _pg_ripple.json_writeback_queue (
    id            BIGSERIAL    PRIMARY KEY,
    mapping_name  TEXT         NOT NULL
        REFERENCES _pg_ripple.json_mappings(name) ON DELETE CASCADE,
    subject_id    BIGINT       NOT NULL,
    operation     TEXT         NOT NULL CHECK (operation IN ('upsert', 'delete')),
    queued_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    processed_at  TIMESTAMPTZ,
    error         TEXT
);

CREATE INDEX IF NOT EXISTS json_writeback_queue_pending_idx
    ON _pg_ripple.json_writeback_queue (mapping_name, queued_at)
    WHERE processed_at IS NULL;

COMMENT ON TABLE _pg_ripple.json_writeback_queue IS
    'Async queue for JSON-mapping relational writeback events (v0.128.0 JSON-WRITEBACK-01)';

-- v0.128.0 JSON-WRITEBACK-01: Trigger function for VP delta auto-enqueue.
-- TG_ARGV[0] = mapping_name
CREATE OR REPLACE FUNCTION _pg_ripple.json_writeback_enqueue_fn()
RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_mapping_name TEXT    := TG_ARGV[0];
    v_subject_id   BIGINT;
    v_operation    TEXT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        v_subject_id := NEW.s;
        v_operation  := 'upsert';
    ELSIF TG_OP = 'DELETE' THEN
        v_subject_id := OLD.s;
        v_operation  := 'delete';
    ELSE
        RETURN NULL;
    END IF;
    INSERT INTO _pg_ripple.json_writeback_queue
        (mapping_name, subject_id, operation)
    VALUES (v_mapping_name, v_subject_id, v_operation);
    RETURN NULL;
END;
$$;

COMMENT ON FUNCTION _pg_ripple.json_writeback_enqueue_fn() IS
    'Trigger function that enqueues VP delta changes into json_writeback_queue (v0.128.0 JSON-WRITEBACK-01)';
"#,
    name = "v0128_json_writeback_schema",
    requires = ["v0126_federation_credentials", "v073_schema_additions"]
);

// v0.129.0: A18 critical & high correctness remediation for JSON mapping
// relational writeback (C18-01 / H18-02). Schema changes:
//   - _pg_ripple.json_writeback_queue.retry_count: tracks drain-loop
//     retries when the status UPDATE itself fails (L18-02)
//   - _pg_ripple.json_mappings.writeback_column_casts: caches derived
//     column-type casts so writeback_json_row() need not re-derive them
//     from pg_attribute on every call (H18-02 / TYPED-PARAMS)
//   - json_writeback_enqueue_fn() gains an optional predicate-id filter
//     argument (for tables shared by multiple predicates, e.g. vp_rare)
//     and treats an INSERT on a *_tombstones table as a delete event for
//     main-resident rows (C18-01 / ENQUEUE-COVERAGE)
pgrx::extension_sql!(
    r#"
-- v0.129.0 L18-02: track drain retry attempts when the status UPDATE itself fails.
ALTER TABLE _pg_ripple.json_writeback_queue
    ADD COLUMN IF NOT EXISTS retry_count SMALLINT NOT NULL DEFAULT 0;

COMMENT ON COLUMN _pg_ripple.json_writeback_queue.retry_count IS
    'Incremented by drain_json_writeback_queue() when its own status-update fails, '
    'so the row stays pending instead of being silently dropped (v0.129.0 L18-02). '
    'Exposed via the pg_ripple_json_writeback_drain_errors_total metric.';

-- v0.129.0 H18-02 / TYPED-PARAMS: cache derived column-cast expressions.
ALTER TABLE _pg_ripple.json_mappings
    ADD COLUMN IF NOT EXISTS writeback_column_casts JSONB;

-- v0.129.0 C18-01: the enqueue trigger function now accepts an optional
-- second argument (predicate id) so it can be installed on tables shared by
-- multiple predicates (vp_rare), and treats an INSERT on a *_tombstones
-- table as a delete event for main-resident rows.
CREATE OR REPLACE FUNCTION _pg_ripple.json_writeback_enqueue_fn()
RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_mapping_name TEXT    := TG_ARGV[0];
    v_pred_filter  BIGINT  := NULLIF(TG_ARGV[1], '')::BIGINT;
    v_subject_id   BIGINT;
    v_operation    TEXT;
    v_row_pred     BIGINT;
    v_is_tombstone BOOLEAN := TG_TABLE_NAME LIKE '%\_tombstones' ESCAPE '\';
BEGIN
    IF TG_TABLE_NAME = 'vp_rare' THEN
        IF TG_OP = 'INSERT' THEN
            v_row_pred := NEW.p;
        ELSIF TG_OP = 'DELETE' THEN
            v_row_pred := OLD.p;
        END IF;
        IF v_pred_filter IS NOT NULL AND v_row_pred IS DISTINCT FROM v_pred_filter THEN
            RETURN NULL;
        END IF;
    END IF;

    IF v_is_tombstone THEN
        IF TG_OP <> 'INSERT' THEN
            RETURN NULL;
        END IF;
        v_subject_id := NEW.s;
        v_operation  := 'delete';
    ELSIF TG_OP = 'INSERT' THEN
        v_subject_id := NEW.s;
        v_operation  := 'upsert';
    ELSIF TG_OP = 'DELETE' THEN
        v_subject_id := OLD.s;
        v_operation  := 'delete';
    ELSE
        RETURN NULL;
    END IF;

    INSERT INTO _pg_ripple.json_writeback_queue
        (mapping_name, subject_id, operation)
    VALUES (v_mapping_name, v_subject_id, v_operation);
    RETURN NULL;
END;
$$;

COMMENT ON FUNCTION _pg_ripple.json_writeback_enqueue_fn() IS
    'Trigger function that enqueues VP delta/vp_rare/tombstone changes into '
    'json_writeback_queue (v0.128.0 JSON-WRITEBACK-01; v0.129.0 C18-01 adds '
    'vp_rare predicate filtering and tombstone-table support)';
"#,
    name = "v0129_json_writeback_fixes",
    requires = ["v0128_json_writeback_schema"]
);
