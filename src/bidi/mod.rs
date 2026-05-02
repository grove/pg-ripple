//! Bidirectional Integration Primitives (v0.77.0 BIDI-*).
//!
//! This module implements all bidirectional integration features:
//! - BIDI-ATTR-01: Source attribution via named graphs (API consistency pass)
//! - BIDI-CONFLICT-01: Declarative conflict resolution policies
//! - BIDI-NORMALIZE-01: Echo-aware conflict resolution with normalize expressions
//! - BIDI-UPSERT-01: Upsert ingest mode driven by SHACL sh:maxCount 1
//! - BIDI-DIFF-01: Diff-mode ingest with RDF-star change timestamps
//! - BIDI-DELETE-01: Symmetric delete + tombstone CDC events
//! - BIDI-REF-01: Cross-source reference resolution via owl:sameAs
//! - BIDI-LOOP-01: Loop-safe subscriptions with exclude_graphs + propagation_depth
//! - BIDI-CAS-01: Object-level outbound events with optimistic-lock base
//! - BIDI-LINKBACK-01: Target-assigned ID rendezvous
//! - BIDI-OUTBOX-01: Outbound events via pg-trickle outbox
//! - BIDI-INBOX-01: Receiver feedback via pg-trickle inbox
//! - BIDI-WIRE-01: Frozen wire format and JSON Schema
//! - BIDI-OBS-01: Per-graph observability
//! - BIDI-PERF-01: Performance budget
//!
//! v0.83.0 (MOD-BIDI-01): module split into sub-modules:
//!   - `protocol`  — validation helpers, mapping helpers, CAS, inbox, graph metrics, shared utils
//!   - `sync`      — BIDI-CONFLICT-01, BIDI-DELETE-01, BIDI-LINKBACK-01
//!   - `relay`     — BIDI-OBS-01, BIDI-ATTR-01, BIDI-DIFF-01, BIDI-UPSERT-01
//!   - `subscribe` — BIDIOPS: queue, schema-evolution, auth, reconciliation, dash, audit

pub mod protocol;
pub mod relay;
pub mod subscribe;
pub mod sync;

pub use protocol::*;
pub use relay::*;
pub use subscribe::*;
pub use sync::*;

// ─── SQL API Layer ────────────────────────────────────────────────────────────

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── BIDI-CONFLICT-01: Conflict policy registration ────────────────────────

    /// Register (or replace) a conflict resolution policy for a predicate.
    ///
    /// Strategies: `source_priority`, `latest_wins`, `reject_on_conflict`, `union`.
    ///
    /// Config examples:
    /// - `source_priority`: `{"order": ["<urn:source:crm>", "<urn:source:erp>"]}`
    /// - `latest_wins`: `{"timestamp_predicate": "..."}`
    /// - `latest_wins` with normalize: `{"normalize": "ROUND(?o, 2)"}`
    #[pg_extern]
    pub fn register_conflict_policy(
        predicate: &str,
        strategy: &str,
        config: default!(Option<pgrx::JsonB>, "NULL"),
    ) {
        crate::bidi::register_conflict_policy_impl(predicate, strategy, config.as_ref());
    }

    /// Drop a conflict resolution policy for a predicate.
    #[pg_extern]
    pub fn drop_conflict_policy(predicate: &str) {
        crate::bidi::drop_conflict_policy_impl(predicate);
    }

    /// Recompute conflict_winners cache for a predicate.
    ///
    /// Run after manual data fixes or batch reconciliation.
    #[pg_extern]
    pub fn recompute_conflict_winners(predicate_iri: &str) {
        crate::bidi::recompute_conflict_winners_impl(predicate_iri);
    }

    // ── BIDI-DELETE-01: Symmetric delete ─────────────────────────────────────

    /// Delete all triples for a subject using a named mapping.
    ///
    /// When `graph_iri` is NULL, uses the mapping's `default_graph_iri`;
    /// if that is also NULL, deletes across all graphs with a NOTICE warning.
    ///
    /// Returns the number of triples deleted.
    #[pg_extern]
    pub fn delete_by_subject(
        mapping: &str,
        subject_iri: &str,
        graph_iri: default!(Option<&str>, "NULL"),
    ) -> i64 {
        crate::bidi::delete_by_subject_impl(mapping, subject_iri, graph_iri)
    }

    /// Delete only the predicates declared in the mapping's context for a subject.
    ///
    /// Leaves other predicates on the same subject intact (e.g. derived facts).
    ///
    /// Returns the number of triples deleted.
    #[pg_extern]
    pub fn delete_mapped_predicates(
        mapping: &str,
        subject_iri: &str,
        graph_iri: default!(Option<&str>, "NULL"),
    ) -> i64 {
        crate::bidi::delete_mapped_predicates_impl(mapping, subject_iri, graph_iri)
    }

    // ── BIDI-LINKBACK-01: Target-assigned ID rendezvous ───────────────────────

    /// Record a target-assigned ID for a pending linkback.
    ///
    /// Atomic: writes `owl:sameAs` into the target graph, deletes the
    /// pending row, and flushes any buffered subscription events.
    ///
    /// Exactly one of `target_id` and `target_iri` must be provided.
    ///
    /// Idempotent: calling twice with the same event_id is a no-op.
    #[pg_extern]
    pub fn record_linkback(
        event_id: pgrx::datum::Uuid,
        target_id: default!(Option<&str>, "NULL"),
        target_iri: default!(Option<&str>, "NULL"),
    ) {
        crate::bidi::record_linkback_impl(event_id, target_id, target_iri);
    }

    /// Declare a pending linkback abandoned.
    ///
    /// Drops the buffered events with a NOTICE and inserts one row into
    /// `_pg_ripple.iri_rewrite_misses` for operator visibility.
    #[pg_extern]
    pub fn abandon_linkback(event_id: pgrx::datum::Uuid) {
        crate::bidi::abandon_linkback_impl(event_id);
    }

    // ── BIDI-INBOX-01: pg-trickle inbox setup ────────────────────────────────

    /// Install the standard bidi inbox table, dispatch function, and trigger.
    #[pg_extern]
    pub fn install_bidi_inbox(inbox_table: default!(&str, "'ripple_inbox.linkback_inbox'")) {
        crate::bidi::install_bidi_inbox_impl(inbox_table);
    }

    // ── BIDI-CAS-01: CAS assertion helper ────────────────────────────────────

    /// Verify that all keys in `event->'base'` match the corresponding keys in
    /// `actual`. Raises a descriptive exception on divergence.
    ///
    /// Use in relay handlers to implement compare-and-swap safety.
    #[pg_extern]
    pub fn assert_cas(event: pgrx::JsonB, actual: pgrx::JsonB) {
        crate::bidi::assert_cas_impl(&event.0, &actual.0);
    }

    // ── BIDI-OBS-01: Per-graph observability ─────────────────────────────────

    /// Return per-graph statistics.
    ///
    /// Columns: `graph_iri`, `graph_id`, `triple_count`, `last_write_at`,
    /// `conflicts_total`, `subscriptions_active`.
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn graph_stats(
        graph_iri: default!(Option<&str>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(graph_iri, String),
            name!(graph_id, i64),
            name!(triple_count, i64),
            name!(last_write_at, Option<pgrx::datum::Timestamp>),
            name!(conflicts_total, i64),
            name!(subscriptions_active, i32),
        ),
    > {
        TableIterator::new(crate::bidi::graph_stats_impl(graph_iri))
    }

    // ── BIDI-WIRE-01: Wire format version ────────────────────────────────────

    /// Return the current bidi wire format version string.
    #[pg_extern]
    pub fn bidi_wire_version() -> &'static str {
        "1.0"
    }

    // ── STATS-CACHE-01 (v0.82.0): refresh_stats_cache ────────────────────────

    /// Immediately rebuild `_pg_ripple.predicate_stats_cache` from the current
    /// `_pg_ripple.predicates` table. Returns the number of cache rows written.
    ///
    /// The cache is also refreshed automatically by the merge worker every
    /// `pg_ripple.stats_refresh_interval_seconds` seconds (default: 300).
    #[pg_extern]
    pub fn refresh_stats_cache() -> i64 {
        crate::bidi::refresh_stats_cache_impl()
    }

    // ── BIDI-ATTR-01: Extended ingest_jsonld ──────────────────────────────────

    /// Ingest a full JSON-LD document with optional graph_iri and mode parameters.
    ///
    /// - `document` — JSONB value representing the JSON-LD document.
    /// - `graph_iri` — named graph IRI for triples without explicit @graph.
    /// - `mode` — `'append'` (default), `'upsert'`, or `'diff'`.
    /// - `source_timestamp` — explicit source timestamp override for diff mode.
    ///
    /// Returns the total number of triples loaded.
    #[pg_extern]
    pub fn ingest_jsonld(
        document: pgrx::JsonB,
        graph_iri: default!(Option<&str>, "NULL"),
        mode: default!(&str, "'append'"),
        source_timestamp: default!(Option<pgrx::datum::Timestamp>, "NULL"),
    ) -> i64 {
        crate::bidi::ingest_jsonld_impl(&document.0, graph_iri, mode, source_timestamp)
    }

    // ── BIDIOPS-QUEUE-01: Dead-letter management ──────────────────────────────

    /// List dead-lettered events for a subscription (paginated).
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn list_dead_letters(
        subscription_name: &str,
        outbox_table: default!(Option<&str>, "NULL"),
        since: default!(Option<pgrx::datum::TimestampWithTimeZone>, "NULL"),
        limit_n: default!(i32, "100"),
    ) -> TableIterator<
        'static,
        (
            name!(event_id, pgrx::datum::Uuid),
            name!(outbox_table, String),
            name!(outbox_variant, String),
            name!(payload, pgrx::JsonB),
            name!(reason, String),
            name!(dead_lettered_at, pgrx::datum::TimestampWithTimeZone),
        ),
    > {
        TableIterator::new(crate::bidi::list_dead_letters_impl(
            subscription_name,
            outbox_table,
            since,
            limit_n,
        ))
    }

    /// Re-enqueue a dead-lettered event (e.g. after fixing the relay).
    ///
    /// Inserts back into the subscription's outbox table with a fresh emitted_at.
    /// The original event_id is preserved for traceability.
    #[pg_extern]
    pub fn requeue_dead_letter(
        subscription_name: &str,
        outbox_table: &str,
        event_id: pgrx::datum::Uuid,
    ) {
        crate::bidi::requeue_dead_letter_impl(subscription_name, outbox_table, event_id);
    }

    /// Permanently drop a dead-lettered event after operator review.
    #[pg_extern]
    pub fn drop_dead_letter(
        subscription_name: &str,
        outbox_table: &str,
        event_id: pgrx::datum::Uuid,
    ) {
        crate::bidi::drop_dead_letter_impl(subscription_name, outbox_table, event_id);
    }

    // ── BIDIOPS-EVOLVE-01: Schema-evolution API ───────────────────────────────

    /// Alter a subscription's schema-evolution policies or other mutable fields.
    ///
    /// Changes are applied with `new_events_only` semantics: queued outbox rows
    /// drain under the old policy; events emitted after this call use the new
    /// policy. Each changed field is recorded in `subscription_schema_changes`.
    #[pg_extern]
    pub fn alter_subscription(
        name: &str,
        frame_change_policy: default!(Option<&str>, "NULL"),
        iri_change_policy: default!(Option<&str>, "NULL"),
        exclude_change_policy: default!(Option<&str>, "NULL"),
    ) {
        crate::bidi::alter_subscription_impl(
            name,
            frame_change_policy,
            iri_change_policy,
            exclude_change_policy,
        );
    }

    // ── BIDIOPS-AUTH-01: Per-subscription bearer tokens ──────────────────────

    /// Register a per-subscription bearer token with specific scopes.
    ///
    /// Returns the raw token string (shown ONCE; only metadata returned thereafter).
    /// Token format: `'pgrt_' || base64url(32 random bytes)`.
    #[pg_extern]
    pub fn register_subscription_token(
        subscription_name: &str,
        scopes: default!(Vec<String>, "ARRAY['linkback','divergence','abandon']"),
        label: default!(Option<&str>, "NULL"),
    ) -> String {
        crate::bidi::register_subscription_token_impl(subscription_name, &scopes, label)
    }

    /// Revoke a subscription token by its SHA-256 hash.
    #[pg_extern]
    pub fn revoke_subscription_token(token_hash: &[u8]) {
        crate::bidi::revoke_subscription_token_impl(token_hash);
    }

    /// List all tokens for a subscription (metadata only; raw tokens not stored).
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn list_subscription_tokens(
        subscription_name: &str,
    ) -> TableIterator<
        'static,
        (
            name!(token_hash, Vec<u8>),
            name!(scopes, Vec<String>),
            name!(label, Option<String>),
            name!(created_at, pgrx::datum::TimestampWithTimeZone),
            name!(last_used_at, Option<pgrx::datum::TimestampWithTimeZone>),
            name!(revoked_at, Option<pgrx::datum::TimestampWithTimeZone>),
        ),
    > {
        TableIterator::new(crate::bidi::list_subscription_tokens_impl(
            subscription_name,
        ))
    }

    // ── BIDIOPS-RECON-01: Reconciliation toolkit ──────────────────────────────

    /// Enqueue a reconciliation item for a diverged event.
    ///
    /// Returns the `reconciliation_id` of the new queue entry.
    #[pg_extern]
    pub fn reconciliation_enqueue(
        event_id: pgrx::datum::Uuid,
        divergence_summary: pgrx::JsonB,
    ) -> i64 {
        crate::bidi::reconciliation_enqueue_impl(event_id, &divergence_summary.0)
    }

    /// Pull the next unresolved reconciliation item (lease + SKIP LOCKED).
    ///
    /// Marked VOLATILE because it issues an UPDATE to set the lease timestamp.
    #[pg_extern(volatile)]
    #[allow(clippy::type_complexity)]
    pub fn reconciliation_next(
        subscription_name: &str,
    ) -> TableIterator<
        'static,
        (
            name!(reconciliation_id, i64),
            name!(event_id, pgrx::datum::Uuid),
            name!(payload, Option<pgrx::JsonB>),
            name!(divergence_summary, pgrx::JsonB),
            name!(enqueued_at, pgrx::datum::TimestampWithTimeZone),
        ),
    > {
        TableIterator::new(crate::bidi::reconciliation_next_impl(subscription_name))
    }

    /// Resolve a reconciliation item with one of four actions.
    ///
    /// `action` must be one of: `accept_external`, `force_internal`,
    /// `merge_via_owl_sameAs`, `dead_letter`.
    #[pg_extern]
    pub fn reconciliation_resolve(
        reconciliation_id: i64,
        action: &str,
        note: default!(Option<&str>, "NULL"),
    ) {
        crate::bidi::reconciliation_resolve_impl(reconciliation_id, action, note);
    }

    // ── BIDIOPS-DASH-01: Consolidated operations surface ─────────────────────

    /// Return per-subscription operational status.
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn bidi_status() -> TableIterator<
        'static,
        (
            name!(subscription_name, String),
            name!(pg_trickle_paused, Option<bool>),
            name!(pg_trickle_pause_reason, Option<String>),
            name!(outbox_depth, i64),
            name!(outbox_oldest_age, Option<String>),
            name!(dead_letter_count, i64),
            name!(conflict_rejection_rate, f64),
            name!(pending_linkback_count, i64),
            name!(pending_linkback_oldest_age, Option<String>),
            name!(rewrite_miss_count_24h, i64),
            name!(last_emit_at, Option<pgrx::datum::TimestampWithTimeZone>),
            name!(
                pg_trickle_last_delivery_at,
                Option<pgrx::datum::TimestampWithTimeZone>
            ),
            name!(pg_trickle_last_error, Option<String>),
            name!(pg_trickle_retry_count, i64),
            name!(pg_trickle_delivery_dlq_count, i64),
            name!(reconciliation_open, i64),
        ),
    > {
        TableIterator::new(crate::bidi::bidi_status_impl())
    }

    /// Return overall bidi health: `healthy`, `degraded`, `paused`, or `failing`.
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn bidi_health() -> TableIterator<
        'static,
        (
            name!(status, String),
            name!(reasons, Vec<String>),
            name!(checked_at, pgrx::datum::TimestampWithTimeZone),
        ),
    > {
        TableIterator::new(crate::bidi::bidi_health_impl())
    }

    // ── BIDIOPS-AUDIT-01: Audit recording (SQL-direct) ───────────────────────

    /// Purge audit log entries older than `pg_ripple.audit_retention` days.
    ///
    /// Called automatically by the background worker once per hour.
    /// Returns the number of rows deleted.
    #[pg_extern]
    pub fn purge_event_audit() -> i64 {
        crate::bidi::purge_event_audit_impl()
    }

    /// Apply frame-level redaction to a JSON-LD event payload (BIDIOPS-REDACT-01).
    #[pg_extern]
    pub fn apply_frame_redaction(frame: pgrx::JsonB, payload: pgrx::JsonB) -> pgrx::JsonB {
        pgrx::JsonB(crate::bidi::apply_frame_redaction_impl(
            &frame.0, &payload.0,
        ))
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_validate_normalize_allowed() {
        crate::bidi::validate_normalize_expression("ROUND(?o, 2)").unwrap();
        crate::bidi::validate_normalize_expression("LCASE(STR(?o))").unwrap();
        crate::bidi::validate_normalize_expression("UCASE(?o)").unwrap();
    }

    #[pg_test]
    fn test_validate_normalize_forbidden() {
        assert!(crate::bidi::validate_normalize_expression("SELECT ?x WHERE { }").is_err());
        assert!(crate::bidi::validate_normalize_expression("count(?o)").is_err());
    }

    #[pg_test]
    fn test_assert_cas_empty_base_noop() {
        let event = serde_json::json!({"base": {}, "after": {"ex:name": "Alice"}});
        let actual = serde_json::json!({"ex:name": "Bob"});
        crate::bidi::assert_cas_impl(&event, &actual);
    }

    #[pg_test]
    fn test_bidi_wire_version() {
        assert_eq!(super::pg_ripple::bidi_wire_version(), "1.0");
    }

    #[pg_test]
    fn test_graph_stats_no_panic() {
        let rows = crate::bidi::graph_stats_impl(None);
        let _ = rows.len();
    }
}
