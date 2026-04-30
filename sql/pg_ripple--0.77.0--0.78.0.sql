-- Migration 0.77.0 → 0.78.0
--
-- v0.78.0: Bidirectional Integration Operations
--
-- New SQL functions (compiled from Rust):
--
--   BIDIOPS-QUEUE-01:  pg_ripple.list_dead_letters(subscription_name, outbox_table, since, limit_n)
--                      pg_ripple.requeue_dead_letter(subscription_name, outbox_table, event_id)
--                      pg_ripple.drop_dead_letter(subscription_name, outbox_table, event_id)
--   BIDIOPS-EVOLVE-01: pg_ripple.alter_subscription(name, frame_change_policy, iri_change_policy, exclude_change_policy)
--   BIDIOPS-AUTH-01:   pg_ripple.register_subscription_token(subscription_name, scopes, label)
--                      pg_ripple.revoke_subscription_token(token_hash)
--                      pg_ripple.list_subscription_tokens(subscription_name)
--   BIDIOPS-RECON-01:  pg_ripple.reconciliation_enqueue(event_id, divergence_summary)
--                      pg_ripple.reconciliation_next(subscription_name)
--                      pg_ripple.reconciliation_resolve(reconciliation_id, action, note)
--   BIDIOPS-DASH-01:   pg_ripple.bidi_status()
--                      pg_ripple.bidi_health()
--   BIDIOPS-AUDIT-01:  pg_ripple.purge_event_audit()
--
-- Schema changes:

-- BIDIOPS-QUEUE-01: Outbox depth limits.
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

-- BIDIOPS-PAUSE-01: pg-trickle handles pause/resume; no pg_ripple SQL helpers added.
-- bidi_status() joins pg_trickle.subscriptions to surface pause state.

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

-- Function registrations are emitted by pgrx schema generation.
