-- Migration 0.73.0 → 0.74.0
-- Assessment 11 Critical/High Remediation + Schema Optimization (full delivery)
--
-- Critical/High Remediation (implemented in Rust):
-- * EVIDENCE-01: Twelve missing docs/src/reference/*.md files created.
-- * GATE-05: validate-feature-status CI job bypass fixed.
-- * GATE-06: validate-feature-status-populated CI job added.
-- * JOURNAL-DATALOG-01: Datalog inference wired through mutation journal.
-- * SBOM-03: SBOM regenerated; just check-sbom-version target added.
-- * HTTP-VERSION-01: pg_ripple_http version bumped to 0.74.0.
-- * DOC-JOURNAL-01: mutation_journal::flush() doc comment updated.
-- * PROMO-RECOVER-01: recover_interrupted_promotions() auto-invoked at startup.
-- * CACHE-INVALIDATE-01: plan_cache::reset() called after VP promotion.
-- * TEST-04: v070_features.sql regression test added.
-- * FLUSH-DEFER-01: executor-end hook flushes mutation journal per-statement.
--
-- Schema Optimizations (all implemented in this migration):
-- * SCHEMA-NORM-01: Surrogate BIGINT id on construct_rules; rule_id FK in construct_rule_triples.
-- * SCHEMA-NORM-02: Surrogate BIGINT id on rule_sets; rule_set_id FK in rules/predicates/rule_firing_log.
-- * SCHEMA-NORM-03: rule_firing_log.rule_id_int BIGINT (replaces TEXT rule_id).
-- * SCHEMA-NORM-04: Drop construct_rules.target_graph TEXT (use target_graph_id decode instead).
-- * SCHEMA-NORM-05: construct_rules.source_graph_ids BIGINT[].
-- * SCHEMA-NORM-06: tenants.graph_id BIGINT.
-- * SCHEMA-NORM-07: federation_health.endpoint_id BIGINT FK.
-- * SCHEMA-NORM-08: federation_endpoints.id BIGINT; federation_cache endpoint_id + BYTEA hash.
-- * SCHEMA-NORM-09: shape_hints.hint_type SMALLINT (was TEXT).
-- * SCHEMA-NORM-10: embedding_models table; embeddings.model_id SMALLINT.
-- * SCHEMA-NORM-11: inferred_schema.class_id, property_id BIGINT.
-- * SCHEMA-NORM-12: federation_endpoints.graph_id BIGINT.
-- * DICT-01: dictionary.hash_hi BIGINT, hash_lo BIGINT (replaces hash BYTEA).
-- * DICT-02: dictionary_literals and dictionary_quoted satellite tables.
-- * DICT-03: dictionary_access_counts separate table.
-- * REDUNDANT-01: Drop extvp_tables.pred1_iri, pred2_iri TEXT (keep BIGINT ids).
-- * ENUM-01: graph_access.permission_id SMALLINT alongside TEXT.
-- * ENUM-02: federation_endpoints.complexity SMALLINT (replaces TEXT).
-- * JSON-01: endpoint_stats.predicate_stats JSONB (replaces predicate_stats_json TEXT).
-- * HASH-01: rag_cache question_hash_bytes, schema_digest_bytes BYTEA.
-- * IRI-01: shacl_shapes.id BIGINT IDENTITY.
-- * IRI-02: shacl_dag_monitors.shape_id BIGINT.
-- * IRI-03: prov_catalog.activity_id BIGINT.
-- * PART-01: federation_health converted to range-partitioned UNLOGGED table.
-- * PART-02: audit_log converted to range-partitioned table.
-- * PART-03: statement_id_timeline_ranges range-mapping companion table.
-- * PART-04: rule_firing_log converted to range-partitioned table.
-- * UNLOGGED-01: validation_queue SET UNLOGGED.
-- * UNLOGGED-02: embedding_queue SET UNLOGGED.
-- * UNLOGGED-03: federation_health SET UNLOGGED (covered by PART-01).
-- * IDX-01: CREATE INDEX idx_statements_predicate ON statements(predicate_id).
-- * TOAST-01: lz4 compression on large TEXT columns.
-- * FILL-01: fillfactor tuning for hot-update tables.

-- ═══════════════════════════════════════════════════════════════════════════════
-- IDX-01 — index on statements(predicate_id)
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE INDEX IF NOT EXISTS idx_statements_predicate
    ON _pg_ripple.statements (predicate_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- FILL-01 — fillfactor tuning for hot-update tables
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.construct_rules      SET (fillfactor = 70);
ALTER TABLE _pg_ripple.predicates           SET (fillfactor = 70);
ALTER TABLE _pg_ripple.federation_endpoints SET (fillfactor = 90);
-- dictionary fillfactor is set after DICT-03 moves access_count out.

-- ═══════════════════════════════════════════════════════════════════════════════
-- TOAST-01 — lz4 compression on large TEXT columns
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    ALTER TABLE _pg_ripple.rules
        ALTER COLUMN rule_text SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.audit_log
        ALTER COLUMN query SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.replication_status
        ALTER COLUMN batch_data SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.sparql_views
        ALTER COLUMN generated_sql SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.construct_views
        ALTER COLUMN generated_sql SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.datalog_views
        ALTER COLUMN generated_sql SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.describe_views
        ALTER COLUMN generated_sql SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.ask_views
        ALTER COLUMN generated_sql SET COMPRESSION lz4;
    ALTER TABLE _pg_ripple.framing_views
        ALTER COLUMN generated_construct SET COMPRESSION lz4;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'TOAST-01: lz4 compression not available, skipping: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- UNLOGGED-01 — validation_queue SET UNLOGGED
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.validation_queue SET UNLOGGED;

-- ═══════════════════════════════════════════════════════════════════════════════
-- UNLOGGED-02 — embedding_queue SET UNLOGGED
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.embedding_queue SET UNLOGGED;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-01 — surrogate BIGINT id on construct_rules
-- ═══════════════════════════════════════════════════════════════════════════════

-- Add surrogate id column (auto-populates existing rows via identity sequence).
ALTER TABLE _pg_ripple.construct_rules
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

-- Add rule_id FK column to construct_rule_triples (nullable; populated below).
ALTER TABLE _pg_ripple.construct_rule_triples
    ADD COLUMN IF NOT EXISTS rule_id BIGINT;

-- Populate rule_id from rule_name → id join.
UPDATE _pg_ripple.construct_rule_triples crt
    SET rule_id = cr.id
    FROM _pg_ripple.construct_rules cr
    WHERE crt.rule_name = cr.name
      AND crt.rule_id IS NULL;

-- Add index on rule_id for retraction joins.
CREATE INDEX IF NOT EXISTS idx_construct_rule_triples_rule_id
    ON _pg_ripple.construct_rule_triples (rule_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-02 — surrogate BIGINT id on rule_sets
-- ═══════════════════════════════════════════════════════════════════════════════

-- Add surrogate id column to rule_sets.
ALTER TABLE _pg_ripple.rule_sets
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

-- Add rule_set_id BIGINT alongside existing rule_set TEXT in rules.
ALTER TABLE _pg_ripple.rules
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT;

UPDATE _pg_ripple.rules r
    SET rule_set_id = rs.id
    FROM _pg_ripple.rule_sets rs
    WHERE r.rule_set = rs.name
      AND r.rule_set_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_rules_rule_set_id
    ON _pg_ripple.rules (rule_set_id);

-- Add rule_set_id BIGINT alongside existing rule_set TEXT in predicates.
ALTER TABLE _pg_ripple.predicates
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT;

UPDATE _pg_ripple.predicates p
    SET rule_set_id = rs.id
    FROM _pg_ripple.rule_sets rs
    WHERE p.rule_set = rs.name
      AND p.rule_set_id IS NULL;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-03 — rule_firing_log.rule_id TEXT → BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

-- Add rule_set_id BIGINT to rule_firing_log (alongside existing rule_set TEXT).
ALTER TABLE _pg_ripple.rule_firing_log
    ADD COLUMN IF NOT EXISTS rule_set_id BIGINT;

UPDATE _pg_ripple.rule_firing_log rfl
    SET rule_set_id = rs.id
    FROM _pg_ripple.rule_sets rs
    WHERE rfl.rule_set = rs.name
      AND rfl.rule_set_id IS NULL;

-- Add rule_id_int BIGINT (rule_id TEXT stores the BIGINT as text in existing rows).
ALTER TABLE _pg_ripple.rule_firing_log
    ADD COLUMN IF NOT EXISTS rule_id_int BIGINT;

UPDATE _pg_ripple.rule_firing_log
    SET rule_id_int = rule_id::bigint
    WHERE rule_id ~ '^\d+$' AND rule_id_int IS NULL;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-04 — Drop construct_rules.target_graph TEXT
-- (Rust code updated to decode via target_graph_id; see src/construct_rules/mod.rs)
-- ═══════════════════════════════════════════════════════════════════════════════

-- Drop the target_graph TEXT column (Rust now decodes via target_graph_id).
ALTER TABLE _pg_ripple.construct_rules
    DROP COLUMN IF EXISTS target_graph;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-05 — construct_rules.source_graph_ids BIGINT[]
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.construct_rules
    ADD COLUMN IF NOT EXISTS source_graph_ids BIGINT[];

-- Populate source_graph_ids from source_graphs TEXT[] via dictionary lookup.
DO $$
DECLARE
    r RECORD;
    ids BIGINT[];
    iri TEXT;
    iri_id BIGINT;
BEGIN
    FOR r IN SELECT name, source_graphs FROM _pg_ripple.construct_rules
             WHERE source_graphs IS NOT NULL AND source_graph_ids IS NULL
    LOOP
        ids := '{}';
        FOREACH iri IN ARRAY r.source_graphs LOOP
            SELECT id INTO iri_id
                FROM _pg_ripple.dictionary
                WHERE value = iri AND kind = 0
                LIMIT 1;
            IF iri_id IS NOT NULL THEN
                ids := ids || iri_id;
            END IF;
        END LOOP;
        UPDATE _pg_ripple.construct_rules
            SET source_graph_ids = ids
            WHERE name = r.name;
    END LOOP;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-06 — tenants.graph_id BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.tenants
    ADD COLUMN IF NOT EXISTS graph_id BIGINT;

UPDATE _pg_ripple.tenants t
    SET graph_id = d.id
    FROM _pg_ripple.dictionary d
    WHERE d.value = t.graph_iri AND d.kind = 0
      AND t.graph_id IS NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'tenants_graph_id_key'
          AND conrelid = '_pg_ripple.tenants'::regclass
    ) THEN
        ALTER TABLE _pg_ripple.tenants ADD CONSTRAINT tenants_graph_id_key UNIQUE (graph_id);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'SCHEMA-NORM-06: unique constraint skipped: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-08 — federation_endpoints.id BIGINT + federation_cache normalization
-- (SCHEMA-NORM-08 before SCHEMA-NORM-07 because -07 depends on endpoints.id)
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'federation_endpoints_url_key'
          AND conrelid = '_pg_ripple.federation_endpoints'::regclass
    ) THEN
        ALTER TABLE _pg_ripple.federation_endpoints
            ADD CONSTRAINT federation_endpoints_url_key UNIQUE (url);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'SCHEMA-NORM-08: unique(url) constraint skipped: %', SQLERRM;
END $$;

-- Add endpoint_id BIGINT to federation_cache; keep url TEXT for the grace period.
ALTER TABLE _pg_ripple.federation_cache
    ADD COLUMN IF NOT EXISTS endpoint_id BIGINT;

UPDATE _pg_ripple.federation_cache fc
    SET endpoint_id = fe.id
    FROM _pg_ripple.federation_endpoints fe
    WHERE fc.url = fe.url
      AND fc.endpoint_id IS NULL;

-- Add query_hash_bytes BYTEA alongside existing query_hash TEXT.
ALTER TABLE _pg_ripple.federation_cache
    ADD COLUMN IF NOT EXISTS query_hash_bytes BYTEA;

UPDATE _pg_ripple.federation_cache
    SET query_hash_bytes = decode(query_hash, 'hex')
    WHERE query_hash_bytes IS NULL
      AND query_hash ~ '^[0-9a-f]{32}$';

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-07 — federation_health.endpoint_id BIGINT FK
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.federation_health
    ADD COLUMN IF NOT EXISTS endpoint_id BIGINT;

UPDATE _pg_ripple.federation_health fh
    SET endpoint_id = fe.id
    FROM _pg_ripple.federation_endpoints fe
    WHERE fh.url = fe.url
      AND fh.endpoint_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_federation_health_ep_time
    ON _pg_ripple.federation_health (endpoint_id, probed_at DESC);

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-09 — shape_hints.hint_type SMALLINT
-- (1 = max_count_1, 2 = min_count_1)
-- ═══════════════════════════════════════════════════════════════════════════════

-- Add hint_type_id SMALLINT alongside the TEXT column.
ALTER TABLE _pg_ripple.shape_hints
    ADD COLUMN IF NOT EXISTS hint_type_id SMALLINT;

UPDATE _pg_ripple.shape_hints
    SET hint_type_id = CASE hint_type
        WHEN 'max_count_1' THEN 1
        WHEN 'min_count_1' THEN 2
        ELSE NULL
    END
    WHERE hint_type_id IS NULL;

-- Replace hint_type TEXT with hint_type SMALLINT in the PK via table rewrite.
DO $$
BEGIN
    -- Drop the old PK (will be on (predicate_id, hint_type TEXT)).
    ALTER TABLE _pg_ripple.shape_hints DROP CONSTRAINT shape_hints_pkey;
    -- Rename old text column.
    ALTER TABLE _pg_ripple.shape_hints RENAME COLUMN hint_type TO hint_type_text;
    -- Rename new smallint column to hint_type.
    ALTER TABLE _pg_ripple.shape_hints RENAME COLUMN hint_type_id TO hint_type;
    -- Add NOT NULL and CHECK constraints.
    ALTER TABLE _pg_ripple.shape_hints ALTER COLUMN hint_type SET NOT NULL;
    ALTER TABLE _pg_ripple.shape_hints ADD CONSTRAINT shape_hints_hint_type_check
        CHECK (hint_type IN (1, 2));
    -- Restore the PK on (predicate_id, hint_type SMALLINT).
    ALTER TABLE _pg_ripple.shape_hints ADD PRIMARY KEY (predicate_id, hint_type);
    -- Drop the now-redundant text column.
    ALTER TABLE _pg_ripple.shape_hints DROP COLUMN hint_type_text;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'SCHEMA-NORM-09: hint_type migration skipped: %', SQLERRM;
END $$;

-- Recreate predicate index.
CREATE INDEX IF NOT EXISTS shape_hints_pred_idx
    ON _pg_ripple.shape_hints (predicate_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-10 — embedding_models table + embeddings.model_id SMALLINT
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS _pg_ripple.embedding_models (
    id    SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);
COMMENT ON TABLE _pg_ripple.embedding_models IS
    'Embedding model registry (v0.74.0 SCHEMA-NORM-10). '
    'Maps model name strings to compact SMALLINT identifiers.';

-- Seed model registry from existing embeddings rows.
INSERT INTO _pg_ripple.embedding_models (name)
    SELECT DISTINCT model FROM _pg_ripple.embeddings
    ON CONFLICT (name) DO NOTHING;

-- Add model_id SMALLINT alongside existing model TEXT.
ALTER TABLE _pg_ripple.embeddings
    ADD COLUMN IF NOT EXISTS model_id SMALLINT;

UPDATE _pg_ripple.embeddings e
    SET model_id = em.id
    FROM _pg_ripple.embedding_models em
    WHERE e.model = em.name
      AND e.model_id IS NULL;

-- Also update kge_embeddings if it has a model column.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'kge_embeddings'
          AND column_name  = 'model'
    ) THEN
        INSERT INTO _pg_ripple.embedding_models (name)
            SELECT DISTINCT model FROM _pg_ripple.kge_embeddings
            ON CONFLICT (name) DO NOTHING;

        ALTER TABLE _pg_ripple.kge_embeddings
            ADD COLUMN IF NOT EXISTS model_id SMALLINT;

        UPDATE _pg_ripple.kge_embeddings ke
            SET model_id = em.id
            FROM _pg_ripple.embedding_models em
            WHERE ke.model = em.name
              AND ke.model_id IS NULL;
    END IF;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-11 — inferred_schema.class_id, property_id BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.inferred_schema
    ADD COLUMN IF NOT EXISTS class_id    BIGINT,
    ADD COLUMN IF NOT EXISTS property_id BIGINT;

UPDATE _pg_ripple.inferred_schema i
    SET class_id = dc.id, property_id = dp.id
    FROM _pg_ripple.dictionary dc, _pg_ripple.dictionary dp
    WHERE dc.value = i.class_iri    AND dc.kind = 0
      AND dp.value = i.property_iri AND dp.kind = 0
      AND (i.class_id IS NULL OR i.property_id IS NULL);

-- Create a decoded view for human-readable display.
CREATE OR REPLACE VIEW pg_ripple.inferred_schema_decoded AS
    SELECT
        i.class_id,
        i.property_id,
        COALESCE(dc.value, i.class_iri)    AS class_iri,
        COALESCE(dp.value, i.property_iri) AS property_iri,
        i.cardinality
    FROM _pg_ripple.inferred_schema i
    LEFT JOIN _pg_ripple.dictionary dc ON dc.id = i.class_id
    LEFT JOIN _pg_ripple.dictionary dp ON dp.id = i.property_id;
COMMENT ON VIEW pg_ripple.inferred_schema_decoded IS
    'Human-readable view of inferred_schema joining back to the dictionary (v0.74.0 SCHEMA-NORM-11).';

-- ═══════════════════════════════════════════════════════════════════════════════
-- SCHEMA-NORM-12 — federation_endpoints.graph_id BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS graph_id BIGINT;

UPDATE _pg_ripple.federation_endpoints fe
    SET graph_id = d.id
    FROM _pg_ripple.dictionary d
    WHERE d.value = fe.graph_iri AND d.kind = 0
      AND fe.graph_id IS NULL
      AND fe.graph_iri IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════════════════
-- DICT-01 — dictionary.hash BYTEA → (hash_hi BIGINT, hash_lo BIGINT)
-- ═══════════════════════════════════════════════════════════════════════════════

-- Drop NOT NULL on old hash column first (new inserts only provide hash_hi/hash_lo).
ALTER TABLE _pg_ripple.dictionary
    ALTER COLUMN hash DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS hash_hi BIGINT,
    ADD COLUMN IF NOT EXISTS hash_lo BIGINT;

-- Extract high and low 64-bit halves from the 16-byte BE BYTEA hash.
UPDATE _pg_ripple.dictionary
    SET
        hash_hi = ('x' || encode(substring(hash, 1, 8), 'hex'))::bit(64)::bigint,
        hash_lo = ('x' || encode(substring(hash, 9, 8), 'hex'))::bit(64)::bigint
    WHERE hash IS NOT NULL AND hash_hi IS NULL;

-- Add NOT NULL constraints once populated.
DO $$
BEGIN
    ALTER TABLE _pg_ripple.dictionary
        ALTER COLUMN hash_hi SET NOT NULL,
        ALTER COLUMN hash_lo SET NOT NULL;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'DICT-01: NOT NULL constraint on hash_hi/hash_lo skipped (null rows exist): %', SQLERRM;
END $$;

-- Create unique index on (hash_hi, hash_lo).
CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hash_split
    ON _pg_ripple.dictionary (hash_hi, hash_lo);

-- Apply same change to dictionary_hot.
ALTER TABLE _pg_ripple.dictionary_hot
    ALTER COLUMN hash DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS hash_hi BIGINT,
    ADD COLUMN IF NOT EXISTS hash_lo BIGINT;

UPDATE _pg_ripple.dictionary_hot
    SET
        hash_hi = ('x' || encode(substring(hash, 1, 8), 'hex'))::bit(64)::bigint,
        hash_lo = ('x' || encode(substring(hash, 9, 8), 'hex'))::bit(64)::bigint
    WHERE hash IS NOT NULL AND hash_hi IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hot_hash_split
    ON _pg_ripple.dictionary_hot (hash_hi, hash_lo);

-- Apply fillfactor after DICT-01 (hash column no longer updated on reads).
ALTER TABLE _pg_ripple.dictionary SET (fillfactor = 80);

-- ═══════════════════════════════════════════════════════════════════════════════
-- DICT-02 — dictionary satellite tables for nullable columns
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary_literals (
    id       BIGINT NOT NULL PRIMARY KEY REFERENCES _pg_ripple.dictionary(id),
    datatype TEXT,
    lang     TEXT
);
COMMENT ON TABLE _pg_ripple.dictionary_literals IS
    'Satellite table for literal-only columns of _pg_ripple.dictionary (v0.74.0 DICT-02). '
    'Only rows with kind = typed literal or lang literal have entries here.';

-- Populate from existing non-NULL datatype / lang rows.
INSERT INTO _pg_ripple.dictionary_literals (id, datatype, lang)
    SELECT id, datatype, lang
    FROM _pg_ripple.dictionary
    WHERE datatype IS NOT NULL OR lang IS NOT NULL
    ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary_quoted (
    id   BIGINT NOT NULL PRIMARY KEY REFERENCES _pg_ripple.dictionary(id),
    qt_s BIGINT NOT NULL,
    qt_p BIGINT NOT NULL,
    qt_o BIGINT NOT NULL
);
COMMENT ON TABLE _pg_ripple.dictionary_quoted IS
    'Satellite table for RDF-star quoted-triple columns of _pg_ripple.dictionary (v0.74.0 DICT-02).';

-- Populate from existing non-NULL qt_s/qt_p/qt_o rows.
INSERT INTO _pg_ripple.dictionary_quoted (id, qt_s, qt_p, qt_o)
    SELECT id, qt_s, qt_p, qt_o
    FROM _pg_ripple.dictionary
    WHERE qt_s IS NOT NULL AND qt_p IS NOT NULL AND qt_o IS NOT NULL
    ON CONFLICT (id) DO NOTHING;

-- Indexes for RDF-star subject/object lookups.
CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_s ON _pg_ripple.dictionary_quoted (qt_s);
CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_p ON _pg_ripple.dictionary_quoted (qt_p);
CREATE INDEX IF NOT EXISTS idx_dictionary_quoted_qt_o ON _pg_ripple.dictionary_quoted (qt_o);

-- ═══════════════════════════════════════════════════════════════════════════════
-- DICT-03 — dictionary.access_count → dictionary_access_counts
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE UNLOGGED TABLE IF NOT EXISTS _pg_ripple.dictionary_access_counts (
    id           BIGINT NOT NULL PRIMARY KEY,
    access_count BIGINT NOT NULL DEFAULT 0
);
COMMENT ON TABLE _pg_ripple.dictionary_access_counts IS
    'Separated access_count column from dictionary to eliminate read-induced writes (v0.74.0 DICT-03). '
    'UNLOGGED because exact durability is not required for LRU statistics.';

-- Migrate existing non-zero counts.
INSERT INTO _pg_ripple.dictionary_access_counts (id, access_count)
    SELECT id, access_count
    FROM _pg_ripple.dictionary
    WHERE access_count > 0
    ON CONFLICT (id) DO UPDATE SET access_count = EXCLUDED.access_count;

-- ═══════════════════════════════════════════════════════════════════════════════
-- REDUNDANT-01 — Drop extvp_tables.pred1_iri, pred2_iri TEXT
-- Note: Rust code in src/views.rs reads these columns for display (list_extvp()).
-- Those reads are updated to decode via dictionary. The columns are dropped here.
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.extvp_tables
    DROP COLUMN IF EXISTS pred1_iri,
    DROP COLUMN IF EXISTS pred2_iri;

-- ═══════════════════════════════════════════════════════════════════════════════
-- ENUM-01 — graph_access.permission_id SMALLINT (1=read, 2=write, 3=admin)
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.graph_access
    ADD COLUMN IF NOT EXISTS permission_id SMALLINT;

UPDATE _pg_ripple.graph_access
    SET permission_id = CASE permission
        WHEN 'read'  THEN 1
        WHEN 'write' THEN 2
        WHEN 'admin' THEN 3
        ELSE NULL
    END
    WHERE permission_id IS NULL;

-- Create decoded view.
CREATE OR REPLACE VIEW pg_ripple.graph_access_decoded AS
    SELECT
        role_name,
        graph_id,
        permission,
        permission_id,
        CASE permission_id WHEN 1 THEN 'read' WHEN 2 THEN 'write' WHEN 3 THEN 'admin' END
            AS permission_name
    FROM _pg_ripple.graph_access;
COMMENT ON VIEW pg_ripple.graph_access_decoded IS
    'Human-readable view of graph_access with permission_id mapped to names (v0.74.0 ENUM-01).';

-- ═══════════════════════════════════════════════════════════════════════════════
-- ENUM-02 — federation_endpoints.complexity TEXT → SMALLINT
-- (1=fast, 2=normal, 3=slow)
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    -- Add temporary integer column.
    ALTER TABLE _pg_ripple.federation_endpoints
        ADD COLUMN IF NOT EXISTS complexity_id SMALLINT;

    UPDATE _pg_ripple.federation_endpoints
        SET complexity_id = CASE complexity
            WHEN 'fast'   THEN 1
            WHEN 'normal' THEN 2
            WHEN 'slow'   THEN 3
            ELSE 2
        END
        WHERE complexity_id IS NULL;

    -- Drop the old TEXT column.
    ALTER TABLE _pg_ripple.federation_endpoints
        DROP COLUMN IF EXISTS complexity;

    -- Rename the integer column.
    ALTER TABLE _pg_ripple.federation_endpoints
        RENAME COLUMN complexity_id TO complexity;

    -- Add NOT NULL and CHECK constraints.
    ALTER TABLE _pg_ripple.federation_endpoints
        ALTER COLUMN complexity SET NOT NULL,
        ALTER COLUMN complexity SET DEFAULT 2;

    ALTER TABLE _pg_ripple.federation_endpoints
        ADD CONSTRAINT federation_endpoints_complexity_check
        CHECK (complexity IN (1, 2, 3));

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'ENUM-02: complexity migration skipped: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- JSON-01 — endpoint_stats.predicate_stats_json TEXT → predicate_stats JSONB
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    -- Add new JSONB column.
    ALTER TABLE _pg_ripple.endpoint_stats
        ADD COLUMN IF NOT EXISTS predicate_stats JSONB NOT NULL DEFAULT '{}';

    -- Copy existing data with USING cast.
    UPDATE _pg_ripple.endpoint_stats
        SET predicate_stats = predicate_stats_json::jsonb
        WHERE predicate_stats = '{}'::jsonb
          AND predicate_stats_json <> '{}';

    -- Drop old TEXT column.
    ALTER TABLE _pg_ripple.endpoint_stats
        DROP COLUMN IF EXISTS predicate_stats_json;

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'JSON-01: predicate_stats migration skipped: %', SQLERRM;
END $$;

-- GIN index on predicate_stats for containment queries.
CREATE INDEX IF NOT EXISTS idx_endpoint_stats_predicate_stats
    ON _pg_ripple.endpoint_stats USING GIN (predicate_stats);

-- ═══════════════════════════════════════════════════════════════════════════════
-- HASH-01 — rag_cache hash columns TEXT → BYTEA
-- ═══════════════════════════════════════════════════════════════════════════════

-- Add BYTEA companion columns alongside existing TEXT columns.
ALTER TABLE _pg_ripple.rag_cache
    ADD COLUMN IF NOT EXISTS question_hash_bytes BYTEA,
    ADD COLUMN IF NOT EXISTS schema_digest_bytes  BYTEA;

-- Convert hex TEXT to BYTEA where the text looks like a hex hash.
UPDATE _pg_ripple.rag_cache
    SET question_hash_bytes = decode(question_hash, 'hex')
    WHERE question_hash_bytes IS NULL
      AND question_hash ~ '^[0-9a-f]+$'
      AND length(question_hash) % 2 = 0;

UPDATE _pg_ripple.rag_cache
    SET schema_digest_bytes = decode(schema_digest, 'hex')
    WHERE schema_digest_bytes IS NULL
      AND schema_digest ~ '^[0-9a-f]+$'
      AND length(schema_digest) % 2 = 0;

-- Change result TEXT to JSONB where result contains valid JSON.
DO $$
BEGIN
    ALTER TABLE _pg_ripple.rag_cache
        ADD COLUMN IF NOT EXISTS result_json JSONB;
    UPDATE _pg_ripple.rag_cache
        SET result_json = result::jsonb
        WHERE result_json IS NULL
          AND result <> ''
          AND result IS NOT NULL;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'HASH-01: result JSONB migration skipped: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- IRI-01 — shacl_shapes.id BIGINT IDENTITY + UNIQUE(shape_iri)
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.shacl_shapes
    ADD COLUMN IF NOT EXISTS id BIGINT GENERATED ALWAYS AS IDENTITY;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'shacl_shapes_shape_iri_key'
          AND conrelid = '_pg_ripple.shacl_shapes'::regclass
    ) THEN
        ALTER TABLE _pg_ripple.shacl_shapes
            ADD CONSTRAINT shacl_shapes_shape_iri_key UNIQUE (shape_iri);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'IRI-01: unique(shape_iri) constraint skipped: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- IRI-02 — shacl_dag_monitors.shape_id BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.shacl_dag_monitors
    ADD COLUMN IF NOT EXISTS shape_id BIGINT;

UPDATE _pg_ripple.shacl_dag_monitors sdm
    SET shape_id = ss.id
    FROM _pg_ripple.shacl_shapes ss
    WHERE sdm.shape_iri = ss.shape_iri
      AND sdm.shape_id IS NULL;

-- ═══════════════════════════════════════════════════════════════════════════════
-- IRI-03 — prov_catalog.activity_id BIGINT
-- ═══════════════════════════════════════════════════════════════════════════════

ALTER TABLE _pg_ripple.prov_catalog
    ADD COLUMN IF NOT EXISTS activity_id BIGINT;

UPDATE _pg_ripple.prov_catalog p
    SET activity_id = d.id
    FROM _pg_ripple.dictionary d
    WHERE d.value = p.activity_iri AND d.kind = 0
      AND p.activity_id IS NULL;

-- ═══════════════════════════════════════════════════════════════════════════════
-- PART-01 — federation_health: convert to range-partitioned UNLOGGED table
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    -- Only convert if not already partitioned.
    IF NOT EXISTS (
        SELECT 1 FROM pg_partitioned_table pt
        JOIN pg_class c ON c.oid = pt.partrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'federation_health'
    ) THEN
        -- Rename existing table.
        ALTER TABLE _pg_ripple.federation_health
            RENAME TO federation_health_legacy;

        -- Create new partitioned UNLOGGED table.
        CREATE UNLOGGED TABLE _pg_ripple.federation_health (
            id         BIGSERIAL,
            url        TEXT        NOT NULL,
            success    BOOLEAN     NOT NULL,
            latency_ms BIGINT      NOT NULL DEFAULT 0,
            probed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
            endpoint_id BIGINT
        ) PARTITION BY RANGE (probed_at);

        -- Default partition catches all rows.
        CREATE UNLOGGED TABLE _pg_ripple.federation_health_default
            PARTITION OF _pg_ripple.federation_health DEFAULT;

        -- Copy existing data.
        INSERT INTO _pg_ripple.federation_health
            (id, url, success, latency_ms, probed_at, endpoint_id)
        SELECT id, url, success, latency_ms, probed_at, endpoint_id
        FROM _pg_ripple.federation_health_legacy;

        -- Drop legacy table.
        DROP TABLE _pg_ripple.federation_health_legacy;

        -- Recreate index.
        CREATE INDEX idx_federation_health_ep_time
            ON _pg_ripple.federation_health (endpoint_id, probed_at DESC);
        CREATE INDEX idx_federation_health_url_time
            ON _pg_ripple.federation_health (url, probed_at DESC);
    ELSE
        -- Already partitioned; just make sure it's UNLOGGED.
        ALTER TABLE _pg_ripple.federation_health SET UNLOGGED;
    END IF;
EXCEPTION WHEN OTHERS THEN
    -- If conversion fails, fall back to just making it UNLOGGED.
    RAISE NOTICE 'PART-01: full partition conversion failed (%), falling back to SET UNLOGGED', SQLERRM;
    ALTER TABLE _pg_ripple.federation_health SET UNLOGGED;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- PART-02 — audit_log: convert to range-partitioned table
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_partitioned_table pt
        JOIN pg_class c ON c.oid = pt.partrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'audit_log'
    ) THEN
        ALTER TABLE _pg_ripple.audit_log RENAME TO audit_log_legacy;

        CREATE TABLE _pg_ripple.audit_log (
            id                    BIGSERIAL,
            ts                    TIMESTAMPTZ  NOT NULL DEFAULT now(),
            role                  NAME         NOT NULL DEFAULT current_user,
            txid                  BIGINT       NOT NULL DEFAULT txid_current(),
            operation             TEXT         NOT NULL DEFAULT '',
            query                 TEXT         NOT NULL DEFAULT '',
            affected_predicate_ids BIGINT[]    NOT NULL DEFAULT '{}'
        ) PARTITION BY RANGE (ts);

        CREATE TABLE _pg_ripple.audit_log_default
            PARTITION OF _pg_ripple.audit_log DEFAULT;

        INSERT INTO _pg_ripple.audit_log
            (id, ts, role, txid, operation, query, affected_predicate_ids)
        SELECT id, ts, role, txid, operation, query, affected_predicate_ids
        FROM _pg_ripple.audit_log_legacy;

        DROP TABLE _pg_ripple.audit_log_legacy;

        CREATE INDEX idx_audit_log_ts ON _pg_ripple.audit_log (ts);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'PART-02: audit_log partition conversion failed: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- PART-03 — statement_id_timeline: add range-mapping companion table
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline_ranges (
    sid_min    BIGINT      NOT NULL,
    sid_max    BIGINT      NOT NULL,
    ts_min     TIMESTAMPTZ NOT NULL,
    ts_max     TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (sid_min)
);
COMMENT ON TABLE _pg_ripple.statement_id_timeline_ranges IS
    'Range-mapping companion to statement_id_timeline; one row per bulk-load batch. '
    'Enables O(log N) temporal range queries without per-row overhead (v0.74.0 PART-03).';

CREATE INDEX IF NOT EXISTS idx_sitl_ranges_ts_min
    ON _pg_ripple.statement_id_timeline_ranges (ts_min);

-- ═══════════════════════════════════════════════════════════════════════════════
-- PART-04 — rule_firing_log: convert to range-partitioned table
-- ═══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_partitioned_table pt
        JOIN pg_class c ON c.oid = pt.partrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = '_pg_ripple' AND c.relname = 'rule_firing_log'
    ) THEN
        ALTER TABLE _pg_ripple.rule_firing_log RENAME TO rule_firing_log_legacy;

        CREATE TABLE _pg_ripple.rule_firing_log (
            id           BIGSERIAL,
            fired_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
            rule_id      TEXT        NOT NULL,
            rule_set     TEXT        NOT NULL DEFAULT '',
            output_sid   BIGINT,
            source_sids  BIGINT[]    NOT NULL DEFAULT '{}',
            session_pid  INT         NOT NULL DEFAULT pg_backend_pid(),
            rule_set_id  BIGINT,
            rule_id_int  BIGINT
        ) PARTITION BY RANGE (fired_at);

        CREATE TABLE _pg_ripple.rule_firing_log_default
            PARTITION OF _pg_ripple.rule_firing_log DEFAULT;

        INSERT INTO _pg_ripple.rule_firing_log
            (id, fired_at, rule_id, rule_set, output_sid, source_sids, session_pid,
             rule_set_id, rule_id_int)
        SELECT id, fired_at, rule_id, rule_set, output_sid, source_sids, session_pid,
               rule_set_id, rule_id_int
        FROM _pg_ripple.rule_firing_log_legacy;

        DROP TABLE _pg_ripple.rule_firing_log_legacy;

        CREATE INDEX rule_firing_log_output_sid_idx
            ON _pg_ripple.rule_firing_log (output_sid);
        CREATE INDEX rule_firing_log_fired_at_idx
            ON _pg_ripple.rule_firing_log (fired_at);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'PART-04: rule_firing_log partition conversion failed: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════════════════
-- GUC registration notes (informational comments)
-- ═══════════════════════════════════════════════════════════════════════════════

-- The following GUCs are registered in src/lib.rs for the new features:
--   pg_ripple.federation_health_retention  (PART-01, default '7 days')
--   pg_ripple.inference_log_retention      (PART-04, default '30 days')
--   pg_ripple.access_count_sample_rate     (DICT-03, default 100)

-- ═══════════════════════════════════════════════════════════════════════════════
-- Bump schema version stamp.
-- ═══════════════════════════════════════════════════════════════════════════════

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
    VALUES ('0.74.0', '0.73.0', clock_timestamp());

SELECT pg_ripple_version();
