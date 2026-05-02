//! View catalog and supplementary tables (v0.11.0 -- v0.55.0).
//!
//! Split from `schema.rs` in v0.85.0 (Q13-02).

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
// v0.74.0: also adds inferred_schema_decoded and graph_access_decoded views
// which depend on columns added in v074_schema_additions. All finalize
// blocks run after all non-finalize extension_sql! blocks, so the
// v074_schema_additions columns are guaranteed to exist.
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

-- ── SCHEMA-NORM-11 view (v0.74.0): inferred_schema decoded ───────────────────
CREATE OR REPLACE VIEW pg_ripple.inferred_schema_decoded AS
    SELECT i.class_id, i.property_id,
           COALESCE(dc.value, i.class_iri)    AS class_iri,
           COALESCE(dp.value, i.property_iri) AS property_iri,
           i.cardinality
    FROM _pg_ripple.inferred_schema i
    LEFT JOIN _pg_ripple.dictionary dc ON dc.id = i.class_id
    LEFT JOIN _pg_ripple.dictionary dp ON dp.id = i.property_id;

-- ── ENUM-01 view (v0.74.0): graph_access decoded ───────────────────────────
CREATE OR REPLACE VIEW pg_ripple.graph_access_decoded AS
    SELECT role_name, graph_id, permission, permission_id,
           CASE permission_id WHEN 1 THEN 'read' WHEN 2 THEN 'write' WHEN 3 THEN 'admin' END
               AS permission_name
    FROM _pg_ripple.graph_access;
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

// ── v0.56.0 is in schema/triggers.rs ─────────────────────────────────────────
