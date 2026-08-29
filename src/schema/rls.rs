//! Late schema additions, row-level security, and BIDI relay tables (v0.74.0+).
//!
//! Split from `schema.rs` in v0.85.0 (Q13-02).

// partitioned tables, indexes, fillfactor, lz4 compression, UNLOGGED queues.

pgrx::extension_sql!(
    r#"
-- ── SCHEMA-NORM-04: drop target_graph TEXT from construct_rules (uses target_graph_id) ──
-- The v0.63.0 schema created this column; v0.74.0 removes it in favour of the
-- BIGINT foreign key target_graph_id. Must come before any code that INSERTs
-- into construct_rules without providing target_graph.
ALTER TABLE _pg_ripple.construct_rules
    DROP COLUMN IF EXISTS target_graph;

-- ── IDX-01 ────────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_statements_predicate
    ON _pg_ripple.statements (predicate_id);

-- ── FILL-01 ───────────────────────────────────────────────────────────────────
ALTER TABLE _pg_ripple.construct_rules      SET (fillfactor = 70);
ALTER TABLE _pg_ripple.predicates           SET (fillfactor = 70);
ALTER TABLE _pg_ripple.dictionary           SET (fillfactor = 80);
ALTER TABLE _pg_ripple.federation_endpoints SET (fillfactor = 90);

-- ── TOAST-01 ──────────────────────────────────────────────────────────────────
DO $$
BEGIN
    ALTER TABLE _pg_ripple.rules              ALTER COLUMN rule_text          SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.audit_log          ALTER COLUMN query              SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.replication_status ALTER COLUMN batch_data         SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.sparql_views       ALTER COLUMN generated_sql      SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.construct_views    ALTER COLUMN generated_sql      SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.datalog_views      ALTER COLUMN generated_sql      SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.describe_views     ALTER COLUMN generated_sql      SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.ask_views          ALTER COLUMN generated_sql      SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.framing_views      ALTER COLUMN generated_construct SET COMPRESSION lz4;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;

-- ── UNLOGGED-01/02 ────────────────────────────────────────────────────────────
ALTER TABLE _pg_ripple.validation_queue SET UNLOGGED;
ALTER TABLE _pg_ripple.embedding_queue  SET UNLOGGED;

-- ── SCHEMA-NORM-01: surrogate id on construct_rules ──────────────────────────
ALTER TABLE _pg_ripple.construct_rules
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

ALTER TABLE _pg_ripple.construct_rule_triples
    ADD COLUMN IF NOT EXISTS rule_id BIGINT;

CREATE INDEX IF NOT EXISTS idx_construct_rule_triples_rule_id
    ON _pg_ripple.construct_rule_triples (rule_id);

-- ── SCHEMA-NORM-02: surrogate id on rule_sets ────────────────────────────────
ALTER TABLE _pg_ripple.rule_sets
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

ALTER TABLE _pg_ripple.rules
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT;

CREATE INDEX IF NOT EXISTS idx_rules_rule_set_id
    ON _pg_ripple.rules (rule_set_id);

ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT;

-- ── SCHEMA-NORM-03: rule_firing_log integer ids ───────────────────────────────
ALTER TABLE _pg_ripple.rule_firing_log
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT,
    ADD COLUMN IF NOT EXISTS rule_id_int BIGINT;

-- ── SCHEMA-NORM-05: construct_rules.source_graph_ids BIGINT[] ────────────────
ALTER TABLE _pg_ripple.construct_rules
    ADD COLUMN IF NOT EXISTS source_graph_ids BIGINT[];

-- ── SCHEMA-NORM-06: tenants.graph_id BIGINT ──────────────────────────────────
ALTER TABLE _pg_ripple.tenants
    ADD COLUMN IF NOT EXISTS graph_id BIGINT;

-- ── SCHEMA-NORM-08: federation_endpoints.id BIGINT ───────────────────────────
ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

ALTER TABLE _pg_ripple.federation_cache
    ADD COLUMN IF NOT EXISTS endpoint_id BIGINT,
    ADD COLUMN IF NOT EXISTS query_hash_bytes BYTEA;

-- ── SCHEMA-NORM-07: federation_health.endpoint_id BIGINT ─────────────────────
ALTER TABLE _pg_ripple.federation_health
    ADD COLUMN IF NOT EXISTS endpoint_id BIGINT;

CREATE INDEX IF NOT EXISTS idx_federation_health_ep_time
    ON _pg_ripple.federation_health (endpoint_id, probed_at DESC);

-- ── SCHEMA-NORM-09: shape_hints.hint_type TEXT → SMALLINT ───────────────────
-- Migrate hint_type column from TEXT to SMALLINT (1=max_count_1, 2=min_count_1).
DO $$
BEGIN
    -- Only run if hint_type is still TEXT (idempotent).
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple' AND table_name = 'shape_hints'
          AND column_name = 'hint_type' AND data_type = 'text'
    ) THEN
        ALTER TABLE _pg_ripple.shape_hints ADD COLUMN IF NOT EXISTS hint_type_id SMALLINT;
        UPDATE _pg_ripple.shape_hints
            SET hint_type_id = CASE hint_type WHEN 'max_count_1' THEN 1 WHEN 'min_count_1' THEN 2 END;
        ALTER TABLE _pg_ripple.shape_hints DROP CONSTRAINT shape_hints_pkey;
        ALTER TABLE _pg_ripple.shape_hints RENAME COLUMN hint_type TO hint_type_text;
        ALTER TABLE _pg_ripple.shape_hints RENAME COLUMN hint_type_id TO hint_type;
        ALTER TABLE _pg_ripple.shape_hints ALTER COLUMN hint_type SET NOT NULL;
        ALTER TABLE _pg_ripple.shape_hints ADD PRIMARY KEY (predicate_id, hint_type);
        ALTER TABLE _pg_ripple.shape_hints DROP COLUMN hint_type_text;
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;

-- ── SCHEMA-NORM-10: embedding_models table + embeddings.model_id ─────────────
CREATE TABLE IF NOT EXISTS _pg_ripple.embedding_models (
    id    SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);

ALTER TABLE _pg_ripple.embeddings
    ADD COLUMN IF NOT EXISTS model_id SMALLINT;

-- ── SCHEMA-NORM-11: inferred_schema.class_id, property_id ────────────────────
ALTER TABLE _pg_ripple.inferred_schema
    ADD COLUMN IF NOT EXISTS class_id    BIGINT,
    ADD COLUMN IF NOT EXISTS property_id BIGINT;

-- (View pg_ripple.inferred_schema_decoded is created in the predicate_stats_view
--  finalize block because the pg_ripple schema is not yet available here.)

-- ── SCHEMA-NORM-12: federation_endpoints.graph_id BIGINT ─────────────────────
ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS graph_id BIGINT;

-- ── DICT-01: dictionary.hash_hi, hash_lo BIGINT ──────────────────────────────
-- Drop NOT NULL on the old hash BYTEA column so new inserts (hash_hi/hash_lo only) succeed.
ALTER TABLE _pg_ripple.dictionary
    ALTER COLUMN hash DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS hash_hi BIGINT,
    ADD COLUMN IF NOT EXISTS hash_lo BIGINT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hash_split
    ON _pg_ripple.dictionary (hash_hi, hash_lo);

ALTER TABLE _pg_ripple.dictionary_hot
    ALTER COLUMN hash DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS hash_hi BIGINT,
    ADD COLUMN IF NOT EXISTS hash_lo BIGINT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hot_hash_split
    ON _pg_ripple.dictionary_hot (hash_hi, hash_lo);

-- ── DICT-02: dictionary satellite tables ─────────────────────────────────────
CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary_literals (
    id       BIGINT NOT NULL PRIMARY KEY REFERENCES _pg_ripple.dictionary(id),
    datatype TEXT,
    lang     TEXT
);

CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary_quoted (
    id   BIGINT NOT NULL PRIMARY KEY REFERENCES _pg_ripple.dictionary(id),
    qt_s BIGINT NOT NULL,
    qt_p BIGINT NOT NULL,
    qt_o BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_s ON _pg_ripple.dictionary_quoted (qt_s);
CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_p ON _pg_ripple.dictionary_quoted (qt_p);
CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_o ON _pg_ripple.dictionary_quoted (qt_o);

-- ── DICT-03: dictionary_access_counts ────────────────────────────────────────
CREATE UNLOGGED TABLE IF NOT EXISTS _pg_ripple.dictionary_access_counts (
    id           BIGINT NOT NULL PRIMARY KEY,
    access_count BIGINT NOT NULL DEFAULT 0
);

-- ── ENUM-01: graph_access.permission_id SMALLINT ─────────────────────────────
ALTER TABLE _pg_ripple.graph_access
    ADD COLUMN IF NOT EXISTS permission_id SMALLINT;

-- (View pg_ripple.graph_access_decoded is created in the predicate_stats_view
--  finalize block because the pg_ripple schema is not yet available here.)

-- ── IRI-01: shacl_shapes.id BIGINT ───────────────────────────────────────────
ALTER TABLE _pg_ripple.shacl_shapes
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

-- ── IRI-02: shacl_dag_monitors.shape_id BIGINT ───────────────────────────────
ALTER TABLE _pg_ripple.shacl_dag_monitors
    ADD COLUMN IF NOT EXISTS shape_id BIGINT;

-- ── IRI-03: prov_catalog.activity_id BIGINT ──────────────────────────────────
ALTER TABLE _pg_ripple.prov_catalog
    ADD COLUMN IF NOT EXISTS activity_id BIGINT;

-- ── PART-03: statement_id_timeline_ranges ────────────────────────────────────
CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline_ranges (
    sid_min  BIGINT      NOT NULL,
    sid_max  BIGINT      NOT NULL,
    ts_min   TIMESTAMPTZ NOT NULL,
    ts_max   TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (sid_min)
);

CREATE INDEX IF NOT EXISTS idx_sitl_ranges_ts_min
    ON _pg_ripple.statement_id_timeline_ranges (ts_min);

-- ── HASH-01: rag_cache BYTEA companion columns ───────────────────────────────
ALTER TABLE _pg_ripple.rag_cache
    ADD COLUMN IF NOT EXISTS question_hash_bytes BYTEA,
    ADD COLUMN IF NOT EXISTS schema_digest_bytes  BYTEA;

-- ── ENUM-02: federation_endpoints.complexity TEXT → SMALLINT ─────────────────
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple' AND table_name = 'federation_endpoints'
          AND column_name = 'complexity' AND data_type = 'text'
    ) THEN
        ALTER TABLE _pg_ripple.federation_endpoints
            ADD COLUMN IF NOT EXISTS complexity_id SMALLINT;
        UPDATE _pg_ripple.federation_endpoints
            SET complexity_id = CASE complexity
                WHEN 'fast' THEN 1 WHEN 'normal' THEN 2 WHEN 'slow' THEN 3 ELSE 2 END;
        ALTER TABLE _pg_ripple.federation_endpoints DROP COLUMN IF EXISTS complexity;
        ALTER TABLE _pg_ripple.federation_endpoints RENAME COLUMN complexity_id TO complexity;
        ALTER TABLE _pg_ripple.federation_endpoints
            ALTER COLUMN complexity SET NOT NULL,
            ALTER COLUMN complexity SET DEFAULT 2;
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;

-- ── JSON-01: endpoint_stats.predicate_stats_json TEXT → predicate_stats JSONB ─
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple' AND table_name = 'endpoint_stats'
          AND column_name = 'predicate_stats_json'
    ) THEN
        ALTER TABLE _pg_ripple.endpoint_stats
            ADD COLUMN IF NOT EXISTS predicate_stats JSONB NOT NULL DEFAULT '{}';
        UPDATE _pg_ripple.endpoint_stats
            SET predicate_stats = predicate_stats_json::jsonb
            WHERE predicate_stats = '{}' AND predicate_stats_json <> '{}';
        ALTER TABLE _pg_ripple.endpoint_stats DROP COLUMN IF EXISTS predicate_stats_json;
        CREATE INDEX IF NOT EXISTS idx_endpoint_stats_predicate_stats
            ON _pg_ripple.endpoint_stats USING GIN (predicate_stats);
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;

-- ── REDUNDANT-01: drop extvp_tables.pred1_iri, pred2_iri TEXT ────────────────
ALTER TABLE _pg_ripple.extvp_tables
    DROP COLUMN IF EXISTS pred1_iri,
    DROP COLUMN IF EXISTS pred2_iri;
"#,
    name = "v074_schema_additions",
    // v028_embedding_queue is in an independent DAG branch; adding it here
    // ensures _pg_ripple.embedding_queue exists before SET UNLOGGED runs.
    // v038_shape_hints is also an independent leaf: add it here to guarantee
    // the hint_type TEXT→SMALLINT migration (SCHEMA-NORM-09) always runs
    // after the table is created (fixes non-deterministic pgrx sort ordering).
    requires = [
        "v073_schema_additions",
        "v028_embedding_queue",
        "v038_shape_hints"
    ]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.74.0', '0.73.0', clock_timestamp());",
    name = "v074_schema_version_stamp",
    requires = ["v074_schema_additions"]
);

// ─── v0.75.0: Assessment 11 medium finding remediation ───────────────────────
// No schema changes; only code-level fixes and CI additions.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.75.0', '0.74.0', clock_timestamp());",
    name = "v075_schema_version_stamp",
    requires = ["v074_schema_version_stamp"]
);

// ─── v0.76.0: Assessment 11 Low-Severity Findings and Production Polish ──────
// RLS policy names upgraded to XXH3-128 in migration script; no schema
// structure changes beyond the version stamp for a fresh install.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.76.0', '0.75.0', clock_timestamp());",
    name = "v076_schema_version_stamp",
    requires = ["v075_schema_version_stamp"]
);

// ─── v0.77.0: Bidirectional Integration Primitives (BIDI-*) ──────────────────
pgrx::extension_sql!(
    r#"
-- BIDI-ATTR-01: Extend json_mappings with new columns for default graph,
-- timestamp path/predicate, IRI template, and IRI match pattern.
ALTER TABLE _pg_ripple.json_mappings
    ADD COLUMN IF NOT EXISTS default_graph_iri      TEXT,
    ADD COLUMN IF NOT EXISTS timestamp_path         TEXT,
    ADD COLUMN IF NOT EXISTS timestamp_predicate    TEXT
        DEFAULT 'http://www.w3.org/ns/prov#generatedAtTime',
    ADD COLUMN IF NOT EXISTS iri_template           TEXT,
    ADD COLUMN IF NOT EXISTS iri_match_pattern      TEXT;

-- BIDI-CONFLICT-01: Declarative conflict resolution catalog.
CREATE TABLE IF NOT EXISTS _pg_ripple.conflict_policies (
    predicate_iri TEXT PRIMARY KEY,
    strategy      TEXT NOT NULL CHECK (strategy IN
                  ('source_priority','latest_wins','reject_on_conflict','union')),
    config        JSONB,
    created_at    TIMESTAMPTZ DEFAULT now()
);

-- BIDI-CONFLICT-01: Non-authoritative resolved projection cache.
CREATE TABLE IF NOT EXISTS _pg_ripple.conflict_winners (
    predicate_id BIGINT NOT NULL,
    subject_id   BIGINT NOT NULL,
    object_id    BIGINT NOT NULL,
    graph_id     BIGINT NOT NULL,
    statement_id BIGINT NOT NULL,
    resolved_at  TIMESTAMPTZ DEFAULT now(),
    PRIMARY KEY (predicate_id, subject_id, object_id, graph_id)
);
CREATE INDEX IF NOT EXISTS idx_conflict_winners_pred_subj
    ON _pg_ripple.conflict_winners (predicate_id, subject_id);

-- BIDI-REF-01: Track IRI rewrite misses for operator visibility.
CREATE TABLE IF NOT EXISTS _pg_ripple.iri_rewrite_misses (
    target_graph_id BIGINT NOT NULL,
    original_iri    TEXT   NOT NULL,
    observed_at     TIMESTAMPTZ DEFAULT now(),
    miss_count      BIGINT DEFAULT 1,
    PRIMARY KEY (target_graph_id, original_iri)
);

-- BIDI-OBS-01: Per-graph observability metrics.
CREATE TABLE IF NOT EXISTS _pg_ripple.graph_metrics (
    graph_id        BIGINT PRIMARY KEY,
    triple_count    BIGINT DEFAULT 0,
    last_write_at   TIMESTAMPTZ,
    conflicts_total BIGINT DEFAULT 0
);

-- BIDI-LINKBACK-01: Pending linkback rendezvous.
CREATE TABLE IF NOT EXISTS _pg_ripple.pending_linkbacks (
    event_id          UUID PRIMARY KEY,
    subscription_name TEXT NOT NULL,
    target_graph_id   BIGINT NOT NULL,
    hub_subject_id    BIGINT NOT NULL,
    emitted_at        TIMESTAMPTZ DEFAULT now(),
    UNIQUE (subscription_name, target_graph_id, hub_subject_id)
);
CREATE INDEX IF NOT EXISTS idx_pending_linkbacks_sub_hub
    ON _pg_ripple.pending_linkbacks (subscription_name, hub_subject_id);

-- BIDI-LINKBACK-01: Buffered events for in-flight subjects.
CREATE TABLE IF NOT EXISTS _pg_ripple.subscription_buffer (
    subscription_name TEXT    NOT NULL,
    target_graph_id   BIGINT  NOT NULL,
    hub_subject_id    BIGINT  NOT NULL,
    sequence          BIGINT  NOT NULL,
    transaction_state JSONB   NOT NULL,
    buffered_at       TIMESTAMPTZ DEFAULT now(),
    PRIMARY KEY (subscription_name, target_graph_id, hub_subject_id, sequence)
);

-- BIDI-LOOP-01 / BIDI-OUTBOX-01: Subscription table extensions.
ALTER TABLE _pg_ripple.subscriptions
    ADD COLUMN IF NOT EXISTS target_graph               TEXT,
    ADD COLUMN IF NOT EXISTS frame                      JSONB,
    ADD COLUMN IF NOT EXISTS exclude_graphs             TEXT[],
    ADD COLUMN IF NOT EXISTS propagation_depth          SMALLINT DEFAULT 1,
    ADD COLUMN IF NOT EXISTS rewrite_target_graph       TEXT,
    ADD COLUMN IF NOT EXISTS on_missing_rewrite         TEXT DEFAULT 'emit_canonical',
    ADD COLUMN IF NOT EXISTS emit_base                  BOOLEAN DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS transaction_grouping       TEXT DEFAULT 'subject',
    ADD COLUMN IF NOT EXISTS outbox_table               TEXT,
    ADD COLUMN IF NOT EXISTS outbox_distribution_column TEXT,
    ADD COLUMN IF NOT EXISTS outbox_format              TEXT DEFAULT 'pg_trickle_v1',
    ADD COLUMN IF NOT EXISTS outbox_merge               BOOLEAN DEFAULT FALSE;
"#,
    name = "v077_schema_additions",
    requires = ["v076_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.77.0', '0.76.0', clock_timestamp());",
    name = "v077_schema_version_stamp",
    requires = ["v077_schema_additions"]
);

// ─────────────────────────────────────────────────────────────────────────────
// v0.78.0 — Bidirectional Integration Operations
// ─────────────────────────────────────────────────────────────────────────────

pgrx::extension_sql!(
    r#"
-- BIDIOPS-QUEUE-01: Outbox depth limits and dead-letter table.
ALTER TABLE _pg_ripple.subscriptions
    ADD COLUMN IF NOT EXISTS max_queue_depth   BIGINT   DEFAULT 1000000,
    ADD COLUMN IF NOT EXISTS dead_letter_after INTERVAL DEFAULT '7 days',
    ADD COLUMN IF NOT EXISTS overflow_policy   TEXT     DEFAULT 'pause';

CREATE TABLE IF NOT EXISTS _pg_ripple.event_dead_letters (
    event_id          UUID        NOT NULL,
    subscription_name TEXT        NOT NULL,
    outbox_table      TEXT        NOT NULL,
    outbox_variant    TEXT        DEFAULT 'default',
    s                 BIGINT,
    payload           JSONB       NOT NULL,
    emitted_at        TIMESTAMPTZ NOT NULL,
    dead_lettered_at  TIMESTAMPTZ DEFAULT now(),
    reason            TEXT        NOT NULL,
    extra             JSONB,
    last_attempt_at   TIMESTAMPTZ,
    PRIMARY KEY (subscription_name, outbox_table, event_id)
);
CREATE INDEX IF NOT EXISTS idx_event_dead_letters_sub_time
    ON _pg_ripple.event_dead_letters (subscription_name, dead_lettered_at);

-- BIDIOPS-EVOLVE-01: Schema-evolution policies.
ALTER TABLE _pg_ripple.subscriptions
    ADD COLUMN IF NOT EXISTS frame_change_policy   TEXT DEFAULT 'new_events_only',
    ADD COLUMN IF NOT EXISTS iri_change_policy     TEXT DEFAULT 'new_events_only',
    ADD COLUMN IF NOT EXISTS exclude_change_policy TEXT DEFAULT 'new_events_only';

CREATE TABLE IF NOT EXISTS _pg_ripple.subscription_schema_changes (
    subscription_name    TEXT        NOT NULL,
    changed_at           TIMESTAMPTZ DEFAULT now(),
    changed_by           TEXT,
    field                TEXT        NOT NULL,
    old_value            JSONB,
    new_value            JSONB,
    policy_applied       TEXT,
    affected_event_count BIGINT
);
CREATE INDEX IF NOT EXISTS idx_sub_schema_changes_sub_time
    ON _pg_ripple.subscription_schema_changes (subscription_name, changed_at);

-- BIDIOPS-AUTH-01: Per-subscription bearer tokens.
CREATE TABLE IF NOT EXISTS _pg_ripple.subscription_tokens (
    token_hash        BYTEA       PRIMARY KEY,
    subscription_name TEXT        NOT NULL,
    scopes            TEXT[]      NOT NULL,
    label             TEXT,
    created_at        TIMESTAMPTZ DEFAULT now(),
    last_used_at      TIMESTAMPTZ,
    revoked_at        TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_subscription_tokens_sub
    ON _pg_ripple.subscription_tokens (subscription_name)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS _pg_ripple.admin_tokens (
    token_hash   BYTEA       PRIMARY KEY,
    label        TEXT,
    created_at   TIMESTAMPTZ DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ
);

-- BIDIOPS-AUDIT-01: Side-band mutation audit log.
CREATE TABLE IF NOT EXISTS _pg_ripple.event_audit (
    audit_id          BIGSERIAL   PRIMARY KEY,
    event_id          UUID,
    subscription_name TEXT,
    resource_type     TEXT        NOT NULL,
    resource_id       TEXT,
    action            TEXT        NOT NULL,
    actor_token_hash  BYTEA,
    actor_session     TEXT,
    http_remote_addr  INET,
    observed_at       TIMESTAMPTZ DEFAULT now(),
    extra             JSONB
);
CREATE INDEX IF NOT EXISTS idx_event_audit_event_id
    ON _pg_ripple.event_audit (event_id) WHERE event_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_event_audit_sub_time
    ON _pg_ripple.event_audit (subscription_name, observed_at)
    WHERE subscription_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_event_audit_resource
    ON _pg_ripple.event_audit (resource_type, resource_id, observed_at);
CREATE INDEX IF NOT EXISTS idx_event_audit_time
    ON _pg_ripple.event_audit (observed_at);

-- BIDIOPS-RECON-01: Reconciliation queue.
CREATE TABLE IF NOT EXISTS _pg_ripple.reconciliation_queue (
    reconciliation_id  BIGSERIAL   PRIMARY KEY,
    event_id           UUID        NOT NULL,
    subscription_name  TEXT        NOT NULL,
    enqueued_at        TIMESTAMPTZ DEFAULT now(),
    leased_until       TIMESTAMPTZ,
    leased_by          TEXT,
    divergence_summary JSONB       NOT NULL,
    resolved_at        TIMESTAMPTZ,
    resolution         TEXT,
    resolved_by        TEXT,
    resolution_note    TEXT
);
CREATE INDEX IF NOT EXISTS idx_reconciliation_queue_open
    ON _pg_ripple.reconciliation_queue (subscription_name, leased_until, enqueued_at)
    WHERE resolved_at IS NULL;
"#,
    name = "v078_schema_additions",
    requires = ["v077_schema_version_stamp"]
);

pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.78.0', '0.77.0', clock_timestamp());",
    name = "v078_schema_version_stamp",
    requires = ["v078_schema_additions"]
);

// ─────────────────────────────────────────────────────────────────────────────
// v0.82.0 — Performance & Observability (fresh-install tables)
// ─────────────────────────────────────────────────────────────────────────────
// NOTE: migration-only tables (federation_stats, predicate_stats_cache) are
// added here so that fresh installs (CREATE EXTENSION) also get them.
// The migration script pg_ripple--0.81.0--0.82.0.sql handles upgrade paths.

pgrx::extension_sql!(
    r#"
-- STATS-CACHE-01a: Predicate stats cache table.
-- Materialised cache of per-predicate triple counts.
-- Refreshed by pg_ripple.refresh_stats_cache() and the merge background worker.
CREATE TABLE IF NOT EXISTS _pg_ripple.predicate_stats_cache (
    predicate_id    BIGINT       PRIMARY KEY,
    triple_count    BIGINT       NOT NULL DEFAULT 0,
    refreshed_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.predicate_stats_cache IS
    'Per-predicate triple-count cache. Refreshed by refresh_stats_cache(). (v0.82.0 STATS-CACHE-01)';

-- FED-COST-01b: Federation call statistics.
-- Accumulates call counts and latency for cost-based SERVICE clause planning.
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
    'Per-endpoint federation call statistics. (v0.82.0 FED-COST-01)';
"#,
    name = "v082_schema_additions",
    requires = ["v078_schema_version_stamp"]
);

// ─── v0.95.0 schema additions ─────────────────────────────────────────────────
// M15-03: sql_drop event trigger for DROP EXTENSION replication-slot cleanup.
// M15-07: autovacuum_scale_factor reloptions on _pg_ripple.dictionary.
// M15-10: schema_generation_seq sequence for plan cache invalidation.
pgrx::extension_sql!(
    r#"
-- M15-10 (v0.95.0): schema_generation sequence.
-- Incremented by ensure_vp_table() and promote_predicate() so that SPARQL plan
-- cache entries that depend on VP table layout are automatically invalidated
-- when new predicates are registered or promoted.
CREATE SEQUENCE IF NOT EXISTS _pg_ripple.schema_generation_seq
    START 1 INCREMENT 1 NO CYCLE;
COMMENT ON SEQUENCE _pg_ripple.schema_generation_seq IS
    'Monotonic counter bumped on every VP table schema change. \
     Included in plan cache keys to invalidate stale plans. (v0.95.0 M15-10)';

-- M15-07 (v0.95.0): Tune autovacuum for the high-churn dictionary table so
-- that it fires more aggressively after bulk encode operations.
-- autovacuum_vacuum_scale_factor = 0.01 → vacuum triggers after 1% of rows change.
-- autovacuum_analyze_scale_factor = 0.005 → analyze after 0.5% change.
ALTER TABLE _pg_ripple.dictionary
    SET (
        autovacuum_vacuum_scale_factor  = 0.01,
        autovacuum_analyze_scale_factor = 0.005
    );

-- M15-03 (v0.95.0): Event trigger to clean up CDC replication slots when the
-- extension is dropped (DROP EXTENSION pg_ripple).
-- Without this, orphaned slots continue to consume WAL and can eventually
-- exhaust disk space.
-- Elevated privilege is required to call pg_drop_replication_slot().
CREATE OR REPLACE FUNCTION _pg_ripple.cleanup_on_drop()
    RETURNS event_trigger
    LANGUAGE plpgsql
    -- SECURITY-JUSTIFY: Required to call pg_drop_replication_slot() which needs replication privilege; SET search_path pins execution context.
    SECURITY DEFINER
    SET search_path = pg_catalog, _pg_ripple, public
AS $$
DECLARE
    _rec record;
BEGIN
    -- Only fire when the pg_ripple extension itself is being dropped.
    IF NOT EXISTS (
        SELECT 1 FROM pg_event_trigger_dropped_objects()
        WHERE object_type = 'extension'
          AND object_name = 'pg_ripple'
    ) THEN
        RETURN;
    END IF;

    -- Drop all logical replication slots associated with pg_ripple.
    FOR _rec IN
        SELECT slot_name
        FROM pg_replication_slots
        WHERE plugin = 'pg_ripple'
           OR slot_name LIKE 'pg_ripple%'
    LOOP
        BEGIN
            PERFORM pg_drop_replication_slot(_rec.slot_name);
            RAISE NOTICE 'pg_ripple: dropped replication slot % on extension drop', _rec.slot_name;
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING 'pg_ripple: could not drop replication slot %: %',
                _rec.slot_name, SQLERRM;
        END;
    END LOOP;
END;
$$;

COMMENT ON FUNCTION _pg_ripple.cleanup_on_drop() IS
    'Event trigger function: drops pg_ripple CDC replication slots when the extension is uninstalled. \
     (M15-03 v0.95.0)';

-- Create the event trigger only if it does not already exist.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_event_trigger WHERE evtname = '_pg_ripple_cleanup_on_drop'
    ) THEN
        EXECUTE $et$
            CREATE EVENT TRIGGER _pg_ripple_cleanup_on_drop
                ON sql_drop
                EXECUTE FUNCTION _pg_ripple.cleanup_on_drop()
        $et$;
    END IF;
END;
$$;
"#,
    name = "v095_schema_additions",
    requires = ["v082_schema_additions"]
);

// ─── v0.97.0 schema additions ────────────────────────────────────────────────
// GEN-UUID-01 (v0.97.0): gen_random_uuid() availability check at CREATE EXTENSION time.
pgrx::extension_sql!(
    r#"
-- GEN-UUID-01 (v0.97.0): Verify gen_random_uuid() is available.
-- In PostgreSQL 14+, gen_random_uuid() is a built-in function that does not
-- require the pgcrypto extension.  On PostgreSQL 18 (the only supported target)
-- it is always present.  This check is a defensive guard for unusual build
-- configurations; if it fires, the user should run:
--   CREATE EXTENSION IF NOT EXISTS pgcrypto;
DO $$
DECLARE
    _uuid uuid;
BEGIN
    _uuid := gen_random_uuid();
    -- Function available; no action needed.
EXCEPTION WHEN undefined_function THEN
    RAISE WARNING
        'pg_ripple: gen_random_uuid() is not available. '
        'The bidi relay and SPARQL uuid() / struuid() functions will not work. '
        'HINT: run CREATE EXTENSION IF NOT EXISTS pgcrypto; to enable it.';
END;
$$;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.97.0', '0.96.0', clock_timestamp());
"#,
    name = "v097_schema_additions",
    requires = ["v095_schema_additions"]
);

// ─── v0.98.0 schema additions ────────────────────────────────────────────────
// SKOS support, named bundle API & graph intelligence (v0.98.0)
pgrx::extension_sql!(
    r#"
-- ── BUNDLE-01: Named Datalog bundle catalog ────────────────────────────────
-- Records which named Datalog rule bundles have been activated, in which
-- named graph (empty = all graphs), at what version, and when.
CREATE TABLE IF NOT EXISTS _pg_ripple.datalog_bundles (
    bundle_name    TEXT        NOT NULL,
    bundle_version INT         NOT NULL DEFAULT 1,
    loaded_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    named_graph    TEXT        NOT NULL DEFAULT '',
    PRIMARY KEY (bundle_name, named_graph)
);

-- Ensure the user-visible schema exists before creating objects in it.
CREATE SCHEMA IF NOT EXISTS pg_ripple;

-- ── FED-TRUST-01: User-facing federation endpoint trust registry ───────────
-- Holds per-endpoint trust scores and configuration for the federation trust
-- layer introduced in v0.98.0.  Remote SERVICE triples are tagged with
-- pg:sourceTrust = min_confidence via RDF-star when an endpoint is registered.
CREATE TABLE IF NOT EXISTS pg_ripple.federation_endpoints (
    name            TEXT PRIMARY KEY,
    endpoint_url    TEXT        NOT NULL,
    auth_token      TEXT,
    min_confidence  FLOAT4      NOT NULL DEFAULT 0.0
                    CHECK (min_confidence >= 0.0 AND min_confidence <= 1.0),
    timeout_ms      INT         NOT NULL DEFAULT 5000
                    CHECK (timeout_ms > 0),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── BUNDLE-02: active_datalog_bundles view ─────────────────────────────────
-- Exposes the bundle catalog in the user-visible pg_ripple schema.
CREATE OR REPLACE VIEW pg_ripple.active_datalog_bundles AS
SELECT bundle_name, bundle_version, loaded_at, named_graph
FROM _pg_ripple.datalog_bundles
ORDER BY bundle_name, named_graph;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.98.0', '0.97.0', clock_timestamp());
"#,
    name = "v098_schema_additions",
    requires = ["v097_schema_additions"]
);

// v0.133.0: keep the schema ledger current for fresh installs.  Releases
// after v0.98.0 were Rust-only, so this stamp also repairs health()'s
// migration status for newly created databases.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.133.0', '0.132.0', clock_timestamp());",
    name = "v133_schema_version_stamp",
    requires = ["v098_schema_additions"]
);

// v0.134.0: keep the schema ledger current for fresh installs.
// The release adds portal-backed streaming entry points and requires no
// additional table changes.
pgrx::extension_sql!(
    "INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at) \
     VALUES ('0.134.0', '0.133.0', clock_timestamp());",
    name = "v134_schema_version_stamp",
    requires = ["v133_schema_version_stamp"]
);
