-- Migration 0.76.0 → 0.77.0
--
-- v0.77.0: Bidirectional Integration Primitives
--
-- New SQL functions (compiled from Rust, no DDL required):
--
--   BIDI-CONFLICT-01:  pg_ripple.register_conflict_policy(predicate, strategy, config)
--                      pg_ripple.drop_conflict_policy(predicate)
--                      pg_ripple.recompute_conflict_winners(predicate_iri)
--   BIDI-DELETE-01:    pg_ripple.delete_by_subject(mapping, subject_iri, graph_iri)
--                      pg_ripple.delete_mapped_predicates(mapping, subject_iri, graph_iri)
--   BIDI-LINKBACK-01:  pg_ripple.record_linkback(event_id, target_id, target_iri)
--                      pg_ripple.abandon_linkback(event_id)
--   BIDI-INBOX-01:     pg_ripple.install_bidi_inbox(inbox_table)
--   BIDI-CAS-01:       pg_ripple.assert_cas(event, actual)
--   BIDI-OBS-01:       pg_ripple.graph_stats(graph_iri)
--   BIDI-WIRE-01:      pg_ripple.bidi_wire_version()
--   BIDI-ATTR-01:      pg_ripple.ingest_jsonld(document, graph_iri, mode, source_timestamp)
--
-- Schema changes (BIDI-MIG-01):

-- BIDI-ATTR-01: Extend json_mappings with BIDI attributes.
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

-- BIDI-REF-01: IRI rewrite miss tracking.
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

-- BIDI-LINKBACK-01: Subscription buffer for in-flight subjects.
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

-- Update schema version stamp.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.77.0', '0.76.0', clock_timestamp());
