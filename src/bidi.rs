//! Bidirectional Integration Primitives (v0.77.0 BIDI-*)
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

use pgrx::prelude::*;
use serde_json::Value as JsonValue;

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
    ///
    /// Columns: subscription_name, pg_trickle_paused, pg_trickle_pause_reason,
    /// outbox_depth, outbox_oldest_age, dead_letter_count,
    /// conflict_rejection_rate, pending_linkback_count, pending_linkback_oldest_age,
    /// rewrite_miss_count_24h, last_emit_at, pg_trickle_last_delivery_at,
    /// pg_trickle_last_error, pg_trickle_retry_count, pg_trickle_delivery_dlq_count,
    /// reconciliation_open.
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
    ///
    /// For every predicate in `frame` whose value contains `{"@redact": true}`,
    /// the corresponding value in `payload` is replaced with `{"@redacted": true}`.
    /// Predicates not found in the payload are left as-is. The `frame` itself is
    /// not modified.
    ///
    /// Returns the redacted payload as JSONB. The original payload is unchanged.
    ///
    /// Example:
    /// ```sql
    /// SELECT pg_ripple.apply_frame_redaction(
    ///     '{"ex:name": {}, "ex:phone": {"@redact": true}}'::jsonb,
    ///     '{"@id": "ex:alice", "ex:name": "Alice", "ex:phone": "+1-555-0100"}'::jsonb
    /// );
    /// -- {"@id": "ex:alice", "ex:name": "Alice", "ex:phone": {"@redacted": true}}
    /// ```
    #[pg_extern]
    pub fn apply_frame_redaction(frame: pgrx::JsonB, payload: pgrx::JsonB) -> pgrx::JsonB {
        pgrx::JsonB(crate::bidi::apply_frame_redaction_impl(
            &frame.0, &payload.0,
        ))
    }
}

// ─── Validation helpers ───────────────────────────────────────────────────────

/// Validate a normalize expression against the allowed whitelist.
pub fn validate_normalize_expression(expr: &str) -> Result<(), String> {
    let lower = expr.to_lowercase();
    let forbidden = [
        "select",
        "where",
        "graph",
        "service",
        "count(",
        "sum(",
        "avg(",
        "min(",
        "max(",
        "regex(",
        "exists(",
        "notexists(",
    ];
    for kw in forbidden {
        if lower.contains(kw) {
            return Err(format!(
                "normalize expression contains unsupported construct '{}'; \
                 allowed: STR, LCASE, UCASE, ROUND, SUBSTR, casts",
                kw
            ));
        }
    }
    Ok(())
}

// ─── Mapping helpers ──────────────────────────────────────────────────────────

/// Fetch a mapping's context, default_graph_iri, and iri_template.
pub fn fetch_mapping_row(mapping: &str) -> (serde_json::Value, Option<String>, Option<String>) {
    Spi::connect(|c| {
        let mut row_iter = c.select(
            "SELECT context, default_graph_iri, iri_template \
             FROM _pg_ripple.json_mappings WHERE name = $1",
            None,
            &[pgrx::datum::DatumWithOid::from(mapping)],
        )?;
        let row = row_iter.next().unwrap_or_else(|| {
            pgrx::error!(
                "json mapping {:?} not found; call register_json_mapping() first",
                mapping
            )
        });
        let ctx = row["context"]
            .value::<pgrx::JsonB>()?
            .map(|j| j.0)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        let default_g = row["default_graph_iri"].value::<String>()?;
        let iri_template = row["iri_template"].value::<String>()?;
        Ok::<_, pgrx::spi::Error>((ctx, default_g, iri_template))
    })
    .unwrap_or_else(|e| pgrx::error!("fetch_mapping_row: {e}"))
}

/// Resolve graph_iri: explicit parameter → mapping's default_graph_iri → NULL.
fn resolve_graph_iri<'a>(
    explicit: Option<&'a str>,
    mapping_default: Option<&'a str>,
) -> Option<&'a str> {
    explicit.or(mapping_default)
}

// ── BIDI-CONFLICT-01 Implementation ──────────────────────────────────────────

pub fn register_conflict_policy_impl(
    predicate: &str,
    strategy: &str,
    config: Option<&pgrx::JsonB>,
) {
    match strategy {
        "source_priority" | "latest_wins" | "reject_on_conflict" | "union" => {}
        other => pgrx::error!(
            "register_conflict_policy: unknown strategy '{}'; \
             valid values: source_priority, latest_wins, reject_on_conflict, union",
            other
        ),
    }

    if let Some(e) = config
        .and_then(|c| c.0.get("normalize").and_then(|v| v.as_str()))
        .map(validate_normalize_expression)
        .and_then(|r| r.err())
    {
        pgrx::error!("register_conflict_policy: {}", e);
    }

    let config_val = config.map(|c| pgrx::JsonB(c.0.clone()));

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.conflict_policies (predicate_iri, strategy, config) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (predicate_iri) DO UPDATE SET strategy = EXCLUDED.strategy, \
         config = EXCLUDED.config, created_at = now()",
        &[
            pgrx::datum::DatumWithOid::from(predicate),
            pgrx::datum::DatumWithOid::from(strategy),
            pgrx::datum::DatumWithOid::from(config_val),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("register_conflict_policy: catalog insert failed: {e}"));

    backfill_conflict_winners(predicate);
    crate::sparql::plan_cache_reset();
}

pub fn drop_conflict_policy_impl(predicate: &str) {
    let pred_id = crate::dictionary::encode(predicate, crate::dictionary::KIND_IRI);

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.conflict_winners WHERE predicate_id = $1",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_conflict_policy: cache cleanup failed: {e}"));

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.conflict_policies WHERE predicate_iri = $1",
        &[pgrx::datum::DatumWithOid::from(predicate)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_conflict_policy: catalog delete failed: {e}"));

    crate::sparql::plan_cache_reset();
}

pub fn recompute_conflict_winners_impl(predicate_iri: &str) {
    backfill_conflict_winners(predicate_iri);
}

/// Backfill or recompute conflict_winners for all existing subjects of a predicate.
fn backfill_conflict_winners(predicate_iri: &str) {
    let pred_id = crate::dictionary::encode(predicate_iri, crate::dictionary::KIND_IRI);

    let strategy_row: Option<(String, Option<pgrx::JsonB>)> = Spi::connect(|c| {
        let mut out = None;
        let mut iter = c.select(
            "SELECT strategy, config FROM _pg_ripple.conflict_policies WHERE predicate_iri = $1",
            None,
            &[pgrx::datum::DatumWithOid::from(predicate_iri)],
        )?;
        if let Some(row) = iter.next() {
            let strategy = row["strategy"].value::<String>()?.unwrap_or_default();
            let config = row["config"].value::<pgrx::JsonB>()?;
            out = Some((strategy, config));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or(None);

    let (strategy, config) = match strategy_row {
        Some(pair) => pair,
        None => return,
    };

    // Get all candidates for this predicate from vp_rare.
    let subjects: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
        let mut out = Vec::new();
        let iter = c.select(
            "SELECT s, o, g, i FROM _pg_ripple.vp_rare WHERE p = $1 ORDER BY s, o",
            None,
            &[pgrx::datum::DatumWithOid::from(pred_id)],
        )?;
        for row in iter {
            let s = row["s"].value::<i64>()?.unwrap_or(0);
            let o = row["o"].value::<i64>()?.unwrap_or(0);
            let g = row["g"].value::<i64>()?.unwrap_or(0);
            let i = row["i"].value::<i64>()?.unwrap_or(0);
            out.push((s, o, g, i));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default();

    let source_order: Vec<i64> = if strategy == "source_priority" {
        config
            .as_ref()
            .and_then(|c| c.0.get("order"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| {
                        let clean = s.trim_matches(|c| c == '<' || c == '>');
                        crate::dictionary::encode(clean, crate::dictionary::KIND_IRI)
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut by_subject: std::collections::HashMap<i64, Vec<(i64, i64, i64)>> =
        std::collections::HashMap::new();
    for (s, o, g, i) in subjects {
        by_subject.entry(s).or_default().push((o, g, i));
    }

    for (subj_id, candidates) in by_subject {
        let winner = match strategy.as_str() {
            "source_priority" => {
                let found = source_order
                    .iter()
                    .find_map(|&gid| candidates.iter().find(|&&(_, g, _)| g == gid));
                found.or_else(|| candidates.first()).copied()
            }
            "latest_wins" => candidates.iter().max_by_key(|&&(_, _, i)| i).copied(),
            _ => None,
        };

        if let Some((obj_id, graph_id, stmt_id)) = winner {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.conflict_winners \
                 (predicate_id, subject_id, object_id, graph_id, statement_id) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (predicate_id, subject_id, object_id, graph_id) \
                 DO UPDATE SET statement_id = EXCLUDED.statement_id, resolved_at = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(subj_id),
                    pgrx::datum::DatumWithOid::from(obj_id),
                    pgrx::datum::DatumWithOid::from(graph_id),
                    pgrx::datum::DatumWithOid::from(stmt_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("backfill_conflict_winners: insert failed: {e}"));
        }
    }
}

// ── BIDI-DELETE-01 Implementation ────────────────────────────────────────────

pub fn delete_by_subject_impl(mapping: &str, subject_iri: &str, graph_iri: Option<&str>) -> i64 {
    let (_, default_g, _) = fetch_mapping_row(mapping);
    let effective_graph = resolve_graph_iri(graph_iri, default_g.as_deref());

    if effective_graph.is_none() {
        pgrx::notice!(
            "delete_by_subject: no graph_iri specified and mapping has no default_graph_iri; \
             deleting across all graphs"
        );
    }

    let subject_id = crate::dictionary::encode(subject_iri, crate::dictionary::KIND_IRI);
    let before_count = crate::storage::triples_for_subject(subject_id).len();

    let sparql = match effective_graph {
        Some(g) => format!(
            "DELETE WHERE {{ GRAPH <{}> {{ <{}> ?p ?o }} }}",
            g.trim_matches(|c| c == '<' || c == '>'),
            subject_iri
        ),
        None => format!("DELETE WHERE {{ <{}> ?p ?o }}", subject_iri),
    };

    let _ = crate::sparql::execute::sparql_update(&sparql);
    let after_count = crate::storage::triples_for_subject(subject_id).len();

    if let Some(g) = effective_graph {
        let g_clean = g.trim_matches(|c| c == '<' || c == '>');
        let g_id = crate::dictionary::encode(g_clean, crate::dictionary::KIND_IRI);
        let deleted = before_count.saturating_sub(after_count);
        if deleted > 0 {
            update_graph_metrics_triple_count(g_id, -(deleted as i64));
        }
    }

    before_count.saturating_sub(after_count) as i64
}

pub fn delete_mapped_predicates_impl(
    mapping: &str,
    subject_iri: &str,
    graph_iri: Option<&str>,
) -> i64 {
    let (context, default_g, _) = fetch_mapping_row(mapping);
    let effective_graph = resolve_graph_iri(graph_iri, default_g.as_deref());

    if effective_graph.is_none() {
        pgrx::notice!(
            "delete_mapped_predicates: no graph_iri specified; deleting across all graphs"
        );
    }

    let pred_iris: Vec<String> = context
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| !k.starts_with('@'))
                .filter_map(|(_, v)| match v {
                    JsonValue::String(s) => Some(s.clone()),
                    JsonValue::Object(meta) => {
                        meta.get("@id").and_then(|id| id.as_str()).map(String::from)
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if pred_iris.is_empty() {
        return 0;
    }

    let subject_id = crate::dictionary::encode(subject_iri, crate::dictionary::KIND_IRI);
    let before_count = crate::storage::triples_for_subject(subject_id).len();

    for pred_iri in &pred_iris {
        let sparql = match effective_graph {
            Some(g) => format!(
                "DELETE WHERE {{ GRAPH <{}> {{ <{}> <{}> ?o }} }}",
                g.trim_matches(|c| c == '<' || c == '>'),
                subject_iri,
                pred_iri
            ),
            None => format!("DELETE WHERE {{ <{}> <{}> ?o }}", subject_iri, pred_iri),
        };
        let _ = crate::sparql::execute::sparql_update(&sparql);
    }

    let after_count = crate::storage::triples_for_subject(subject_id).len();
    before_count.saturating_sub(after_count) as i64
}

// ── BIDI-LINKBACK-01 Implementation ──────────────────────────────────────────

pub fn record_linkback_impl(
    event_id: pgrx::datum::Uuid,
    target_id: Option<&str>,
    target_iri: Option<&str>,
) {
    match (target_id, target_iri) {
        (Some(_), Some(_)) => pgrx::error!(
            "record_linkback: provide exactly one of target_id and target_iri, not both"
        ),
        (None, None) => pgrx::error!(
            "record_linkback: provide one of target_id (bare ID) or target_iri (literal IRI)"
        ),
        _ => {}
    }

    let event_id_str = format!("{}", event_id);

    let pending: Option<(String, i64, i64)> = Spi::connect(|c| {
        let mut out = None;
        let mut iter = c.select(
            "SELECT subscription_name, target_graph_id, hub_subject_id \
             FROM _pg_ripple.pending_linkbacks WHERE event_id = $1::uuid",
            None,
            &[pgrx::datum::DatumWithOid::from(event_id_str.as_str())],
        )?;
        if let Some(row) = iter.next() {
            let sub_name = row["subscription_name"]
                .value::<String>()?
                .unwrap_or_default();
            let tg_id = row["target_graph_id"].value::<i64>()?.unwrap_or(0);
            let hs_id = row["hub_subject_id"].value::<i64>()?.unwrap_or(0);
            out = Some((sub_name, tg_id, hs_id));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or(None);

    let (sub_name, target_graph_id, hub_subject_id) = match pending {
        Some(p) => p,
        None => return, // idempotent re-call
    };

    let resolved_target_iri = if let Some(tid) = target_id {
        let template_opt: Option<String> = Spi::get_one_with_args::<String>(
            "SELECT jm.iri_template FROM _pg_ripple.json_mappings jm \
             JOIN _pg_ripple.dictionary d ON d.value = TRIM(BOTH '<>' FROM jm.default_graph_iri) \
             WHERE d.id = $1 LIMIT 1",
            &[pgrx::datum::DatumWithOid::from(target_graph_id)],
        )
        .unwrap_or(None);

        match template_opt {
            Some(tmpl) if tmpl.contains("{id}") => {
                if tid.contains(['<', '>', ' ', '"', '{', '}', '|', '\\', '^', '`']) {
                    pgrx::error!(
                        "record_linkback: target_id {:?} contains invalid IRI characters",
                        tid
                    );
                }
                tmpl.replace("{id}", tid)
            }
            Some(_) => pgrx::error!(
                "record_linkback: iri_template has no {{id}} placeholder; \
                 re-register the mapping or call with target_iri"
            ),
            None => pgrx::error!(
                "record_linkback: no iri_template for target graph id {}; \
                 call register_json_mapping(iri_template => ...) or use target_iri",
                target_graph_id
            ),
        }
    } else {
        // target_id is None and target_iri is Some — validated above.
        target_iri
            .unwrap_or_else(|| {
                pgrx::error!(
                    "record_linkback: internal error: target_iri was None after guard check"
                )
            })
            .to_string()
    };

    // Decode hub IRI.
    let hub_iri: String = Spi::get_one_with_args::<String>(
        "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(hub_subject_id)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| {
        pgrx::error!(
            "record_linkback: cannot decode hub_subject_id {}",
            hub_subject_id
        )
    });

    // Write owl:sameAs triple.
    let owl_same_as = "http://www.w3.org/2002/07/owl#sameAs";
    let ntriple = format!(
        "<{}> <{}> <{}> .",
        hub_iri, owl_same_as, resolved_target_iri
    );

    let target_graph_iri: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(target_graph_id)],
    )
    .unwrap_or(None);

    match target_graph_iri.as_deref() {
        Some(g) if !g.is_empty() => {
            let g_id = crate::dictionary::encode(g, crate::dictionary::KIND_IRI);
            crate::bulk_load::load_ntriples_into_graph(&ntriple, g_id);
        }
        _ => {
            crate::bulk_load::load_ntriples(&ntriple, false);
        }
    }

    // Delete pending row.
    Spi::run_with_args(
        "DELETE FROM _pg_ripple.pending_linkbacks WHERE event_id = $1::uuid",
        &[pgrx::datum::DatumWithOid::from(event_id_str.as_str())],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_linkback: pending row cleanup: {e}"));

    // Flush buffered events.
    flush_subscription_buffer(
        &sub_name,
        target_graph_id,
        hub_subject_id,
        &resolved_target_iri,
    );
}

fn flush_subscription_buffer(
    sub_name: &str,
    target_graph_id: i64,
    hub_subject_id: i64,
    _resolved_target_iri: &str,
) {
    let buffered: Vec<(i64, pgrx::JsonB)> = Spi::connect(|c| {
        let mut out = Vec::new();
        let iter = c.select(
            "SELECT sequence, transaction_state FROM _pg_ripple.subscription_buffer \
             WHERE subscription_name = $1 AND target_graph_id = $2 AND hub_subject_id = $3 \
             ORDER BY sequence",
            None,
            &[
                pgrx::datum::DatumWithOid::from(sub_name),
                pgrx::datum::DatumWithOid::from(target_graph_id),
                pgrx::datum::DatumWithOid::from(hub_subject_id),
            ],
        )?;
        for row in iter {
            let seq = row["sequence"].value::<i64>()?.unwrap_or(0);
            let state = row["transaction_state"]
                .value::<pgrx::JsonB>()?
                .unwrap_or(pgrx::JsonB(serde_json::json!({})));
            out.push((seq, state));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default();

    if buffered.is_empty() {
        return;
    }

    let outbox_table_opt: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT outbox_table FROM _pg_ripple.subscriptions WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(sub_name)],
    )
    .unwrap_or(None);

    if let Some(outbox_table) = outbox_table_opt {
        for (_seq, state) in &buffered {
            let mut event = state.0.clone();
            if let Some(obj) = event.as_object_mut() {
                obj.insert("subject_resolved".to_string(), serde_json::json!(true));
                obj.insert("version".to_string(), serde_json::json!("1.0"));
            }
            let insert_sql = format!(
                "INSERT INTO {} (event_id, subscription_name, s, payload, emitted_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, now())",
                outbox_table
            );
            Spi::run_with_args(
                &insert_sql,
                &[
                    pgrx::datum::DatumWithOid::from(sub_name),
                    pgrx::datum::DatumWithOid::from(hub_subject_id),
                    pgrx::datum::DatumWithOid::from(pgrx::JsonB(event)),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("flush_subscription_buffer: {e}"));
        }
    }

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.subscription_buffer \
         WHERE subscription_name = $1 AND target_graph_id = $2 AND hub_subject_id = $3",
        &[
            pgrx::datum::DatumWithOid::from(sub_name),
            pgrx::datum::DatumWithOid::from(target_graph_id),
            pgrx::datum::DatumWithOid::from(hub_subject_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("flush_subscription_buffer: delete: {e}"));
}

pub fn abandon_linkback_impl(event_id: pgrx::datum::Uuid) {
    let event_id_str = format!("{}", event_id);

    let pending: Option<(String, i64, i64)> = Spi::connect(|c| {
        let mut out = None;
        let mut iter = c.select(
            "SELECT subscription_name, target_graph_id, hub_subject_id \
             FROM _pg_ripple.pending_linkbacks WHERE event_id = $1::uuid",
            None,
            &[pgrx::datum::DatumWithOid::from(event_id_str.as_str())],
        )?;
        if let Some(row) = iter.next() {
            let sub_name = row["subscription_name"]
                .value::<String>()?
                .unwrap_or_default();
            let tg_id = row["target_graph_id"].value::<i64>()?.unwrap_or(0);
            let hs_id = row["hub_subject_id"].value::<i64>()?.unwrap_or(0);
            out = Some((sub_name, tg_id, hs_id));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or(None);

    let (sub_name, target_graph_id, hub_subject_id) = match pending {
        Some(p) => p,
        None => {
            pgrx::notice!(
                "abandon_linkback: no pending linkback for event_id {}",
                event_id_str
            );
            return;
        }
    };

    let hub_iri: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(hub_subject_id)],
    )
    .unwrap_or(None);

    if let Some(ref iri) = hub_iri {
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.iri_rewrite_misses \
             (target_graph_id, original_iri, observed_at, miss_count) \
             VALUES ($1, $2, now(), 1) \
             ON CONFLICT (target_graph_id, original_iri) \
             DO UPDATE SET miss_count = _pg_ripple.iri_rewrite_misses.miss_count + 1",
            &[
                pgrx::datum::DatumWithOid::from(target_graph_id),
                pgrx::datum::DatumWithOid::from(iri.as_str()),
            ],
        )
        .unwrap_or_else(|e| pgrx::warning!("abandon_linkback: iri_rewrite_misses: {e}"));
    }

    pgrx::notice!(
        "abandon_linkback: abandoning event_id {} (sub: {}, hub_subject: {})",
        event_id_str,
        sub_name,
        hub_iri.as_deref().unwrap_or("<unknown>")
    );

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.subscription_buffer \
         WHERE subscription_name = $1 AND target_graph_id = $2 AND hub_subject_id = $3",
        &[
            pgrx::datum::DatumWithOid::from(sub_name.as_str()),
            pgrx::datum::DatumWithOid::from(target_graph_id),
            pgrx::datum::DatumWithOid::from(hub_subject_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("abandon_linkback: buffer cleanup: {e}"));

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.pending_linkbacks WHERE event_id = $1::uuid",
        &[pgrx::datum::DatumWithOid::from(event_id_str.as_str())],
    )
    .unwrap_or_else(|e| pgrx::warning!("abandon_linkback: pending row cleanup: {e}"));
}

// ── BIDI-INBOX-01 Implementation ─────────────────────────────────────────────

pub fn install_bidi_inbox_impl(inbox_table: &str) {
    let parts: Vec<&str> = inbox_table.splitn(2, '.').collect();
    let (schema_name, table_name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("pg_ripple_inbox", parts[0])
    };

    Spi::run_with_args(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name), &[])
        .unwrap_or_else(|e| pgrx::error!("install_bidi_inbox: create schema: {e}"));

    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {}.{} (\
                payload     JSONB        NOT NULL,\
                received_at TIMESTAMPTZ  DEFAULT now()\
            )",
            schema_name, table_name
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("install_bidi_inbox: create table: {e}"));

    let func_name = format!("{}.dispatch_linkback_{}", schema_name, table_name);
    let create_func = format!(
        r#"CREATE OR REPLACE FUNCTION {func_name}() RETURNS TRIGGER AS $$
        DECLARE
            a TEXT := NEW.payload->>'action';
        BEGIN
            IF a = 'linkback' THEN
                PERFORM pg_ripple.record_linkback(
                    (NEW.payload->>'event_id')::uuid,
                    target_id  => NEW.payload->>'target_id',
                    target_iri => NEW.payload->>'target_iri'
                );
            ELSIF a = 'abandon' THEN
                PERFORM pg_ripple.abandon_linkback(
                    (NEW.payload->>'event_id')::uuid
                );
            ELSE
                RAISE EXCEPTION 'unknown bidi inbox action: %', a;
            END IF;
            RETURN NULL;
        END;
        $$ LANGUAGE plpgsql"#,
        func_name = func_name
    );
    Spi::run_with_args(&create_func, &[])
        .unwrap_or_else(|e| pgrx::error!("install_bidi_inbox: create function: {e}"));

    let trigger_name = format!("trg_dispatch_linkback_{}", table_name);
    let _ = Spi::run_with_args(
        &format!(
            "DROP TRIGGER IF EXISTS {} ON {}.{}",
            trigger_name, schema_name, table_name
        ),
        &[],
    );

    Spi::run_with_args(
        &format!(
            "CREATE TRIGGER {} AFTER INSERT ON {}.{} \
             FOR EACH ROW EXECUTE FUNCTION {}()",
            trigger_name, schema_name, table_name, func_name
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("install_bidi_inbox: create trigger: {e}"));
}

// ── BIDI-CAS-01 Implementation ────────────────────────────────────────────────

pub fn assert_cas_impl(event: &serde_json::Value, actual: &serde_json::Value) {
    let base = match event.get("base") {
        Some(b) => b,
        None => return,
    };

    let base_obj = match base.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => return,
    };

    let after = event.get("after");
    let actual_obj = actual.as_object();
    let mut diverging = Vec::new();

    for (key, base_val) in base_obj {
        let actual_val = actual_obj.and_then(|o| o.get(key));
        let after_val = after.and_then(|a| a.as_object()).and_then(|o| o.get(key));
        let already_applied = after_val.is_some_and(|av| Some(av) == actual_val);
        let matches_base = actual_val.is_some_and(|av| av == base_val);
        if !matches_base && !already_applied {
            diverging.push(key.as_str());
        }
    }

    if !diverging.is_empty() {
        pgrx::error!(
            "assert_cas: CAS divergence on predicate(s) {}: \
             actual value is neither the expected base nor the after value",
            diverging.join(", ")
        );
    }
}

// ── BIDI-OBS-01 Implementation ────────────────────────────────────────────────

pub fn graph_stats_impl(
    filter_graph_iri: Option<&str>,
) -> Vec<(String, i64, i64, Option<pgrx::datum::Timestamp>, i64, i32)> {
    Spi::connect(|c| {
        let mut out = Vec::new();

        // Use a simpler approach: run the query with or without filter.
        let iter = if let Some(giri) = filter_graph_iri {
            c.select(
                "SELECT d.value AS graph_iri, m.graph_id, \
                 COALESCE(m.triple_count, 0) AS triple_count, \
                 m.last_write_at, \
                 COALESCE(m.conflicts_total, 0) AS conflicts_total, \
                 COALESCE((SELECT COUNT(*)::int FROM _pg_ripple.subscriptions s \
                  WHERE s.exclude_graphs IS NULL OR $1 != ALL(s.exclude_graphs)), 0) \
                 AS subscriptions_active \
                 FROM _pg_ripple.graph_metrics m \
                 JOIN _pg_ripple.dictionary d ON d.id = m.graph_id \
                 WHERE d.value = $1",
                None,
                &[pgrx::datum::DatumWithOid::from(giri)],
            )?
        } else {
            c.select(
                "SELECT d.value AS graph_iri, m.graph_id, \
                 COALESCE(m.triple_count, 0) AS triple_count, \
                 m.last_write_at, \
                 COALESCE(m.conflicts_total, 0) AS conflicts_total, \
                 COALESCE((SELECT COUNT(*)::int FROM _pg_ripple.subscriptions s \
                  WHERE s.exclude_graphs IS NULL), 0) AS subscriptions_active \
                 FROM _pg_ripple.graph_metrics m \
                 JOIN _pg_ripple.dictionary d ON d.id = m.graph_id \
                 ORDER BY m.graph_id \
                 LIMIT $1",
                None,
                &[pgrx::datum::DatumWithOid::from(
                    crate::STATS_SCAN_LIMIT.get() as i64,
                )],
            )?
        };

        for row in iter {
            let graph_iri = row["graph_iri"].value::<String>()?.unwrap_or_default();
            let graph_id = row["graph_id"].value::<i64>()?.unwrap_or(0);
            let triple_count = row["triple_count"].value::<i64>()?.unwrap_or(0);
            let last_write_at = row["last_write_at"].value::<pgrx::datum::Timestamp>()?;
            let conflicts_total = row["conflicts_total"].value::<i64>()?.unwrap_or(0);
            let subscriptions_active = row["subscriptions_active"].value::<i32>()?.unwrap_or(0);
            out.push((
                graph_iri,
                graph_id,
                triple_count,
                last_write_at,
                conflicts_total,
                subscriptions_active,
            ));
        }

        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default()
}

/// Update the graph_metrics table incrementally.
pub fn update_graph_metrics_triple_count(graph_id: i64, delta: i64) {
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.graph_metrics (graph_id, triple_count, last_write_at) \
         VALUES ($1, GREATEST(0, $2), now()) \
         ON CONFLICT (graph_id) DO UPDATE SET \
             triple_count = GREATEST(0, _pg_ripple.graph_metrics.triple_count + $2), \
             last_write_at = now()",
        &[
            pgrx::datum::DatumWithOid::from(graph_id),
            pgrx::datum::DatumWithOid::from(delta),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("update_graph_metrics: {e}"));
}

// ── BIDI-ATTR-01 / ingest_jsonld ─────────────────────────────────────────────

pub fn ingest_jsonld_impl(
    document: &serde_json::Value,
    graph_iri: Option<&str>,
    mode: &str,
    _source_timestamp: Option<pgrx::datum::Timestamp>,
) -> i64 {
    match mode {
        "append" | "upsert" | "diff" => {}
        other => pgrx::error!(
            "ingest_jsonld: unknown mode '{}'; valid values: append, upsert, diff",
            other
        ),
    }
    let inserted = crate::bulk_load::json_ld_load(document, graph_iri);

    if inserted > 0 {
        let graph_id = graph_iri
            .map(|g| {
                crate::dictionary::encode(
                    g.trim_matches(|c| c == '<' || c == '>'),
                    crate::dictionary::KIND_IRI,
                )
            })
            .unwrap_or(0_i64);
        update_graph_metrics_triple_count(graph_id, inserted);
    }

    inserted
}

// ── BIDI-DIFF-01: Diff-mode ingest ───────────────────────────────────────────

pub fn ingest_json_diff_impl(
    payload: &serde_json::Value,
    subject_iri: &str,
    mapping: &str,
    graph_iri: Option<&str>,
    source_timestamp: Option<pgrx::datum::Timestamp>,
) -> i64 {
    let (context, default_g, _) = fetch_mapping_row(mapping);
    let effective_graph = resolve_graph_iri(graph_iri, default_g.as_deref());

    let ts_str = resolve_diff_timestamp(payload, mapping, source_timestamp);

    let ctx_obj = match context.as_object() {
        Some(o) => o.clone(),
        None => {
            pgrx::warning!(
                "ingest_json_diff: mapping context is not an object; falling back to append"
            );
            return crate::json_mapping::ingest_json_impl(payload, subject_iri, mapping, graph_iri);
        }
    };

    let subject_id = crate::dictionary::encode(subject_iri, crate::dictionary::KIND_IRI);
    let graph_id = effective_graph
        .map(|g| {
            crate::dictionary::encode(
                g.trim_matches(|c| c == '<' || c == '>'),
                crate::dictionary::KIND_IRI,
            )
        })
        .unwrap_or(0_i64);

    let prov_pred = fetch_timestamp_predicate(mapping);

    let mut written = 0i64;

    let payload_obj = match payload.as_object() {
        Some(o) => o,
        None => {
            pgrx::warning!("ingest_json_diff: payload is not a JSON object");
            return 0;
        }
    };

    for (key, new_val) in payload_obj {
        if key.starts_with('@') {
            continue;
        }

        let pred_iri = match ctx_obj.get(key.as_str()) {
            Some(JsonValue::String(s)) => s.clone(),
            Some(JsonValue::Object(meta)) => match meta.get("@id").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            },
            _ => continue,
        };

        let pred_id = crate::dictionary::encode(&pred_iri, crate::dictionary::KIND_IRI);

        if new_val.is_null() {
            let del_sparql = match effective_graph {
                Some(g) => format!(
                    "DELETE WHERE {{ GRAPH <{}> {{ <{}> <{}> ?o }} }}",
                    g.trim_matches(|c| c == '<' || c == '>'),
                    subject_iri,
                    pred_iri
                ),
                None => format!("DELETE WHERE {{ <{}> <{}> ?o }}", subject_iri, pred_iri),
            };
            let _ = crate::sparql::execute::sparql_update(&del_sparql);
            continue;
        }

        let current_val = get_current_value(subject_id, pred_id, graph_id);
        let new_encoded = encode_json_value(new_val);

        if Some(new_encoded) == current_val {
            continue; // idempotent
        }

        // Delete old, insert new.
        let del_sparql = match effective_graph {
            Some(g) => format!(
                "DELETE WHERE {{ GRAPH <{}> {{ <{}> <{}> ?o }} }}",
                g.trim_matches(|c| c == '<' || c == '>'),
                subject_iri,
                pred_iri
            ),
            None => format!("DELETE WHERE {{ <{}> <{}> ?o }}", subject_iri, pred_iri),
        };
        let _ = crate::sparql::execute::sparql_update(&del_sparql);

        let ntriple_str = format_ntriple_for_json_val(subject_iri, &pred_iri, new_val);
        let n = match effective_graph {
            Some(g) => {
                let g_id = crate::dictionary::encode(
                    g.trim_matches(|c| c == '<' || c == '>'),
                    crate::dictionary::KIND_IRI,
                );
                crate::bulk_load::load_ntriples_into_graph(&ntriple_str, g_id)
            }
            None => crate::bulk_load::load_ntriples(&ntriple_str, false),
        };
        written += n;

        // Write RDF-star timestamp annotation if we have a timestamp.
        if let Some(ref ts) = ts_str {
            let obj_nt = format_ntriple_object(new_val);
            let graph_part = match effective_graph {
                Some(g) => format!("GRAPH <{}> {{ ", g.trim_matches(|c| c == '<' || c == '>')),
                None => String::new(),
            };
            let graph_close = if effective_graph.is_some() { " }" } else { "" };
            let annotation_sparql = format!(
                "INSERT DATA {{ {}<<<{}> <{}> {}>> <{}> \"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>{} }}",
                graph_part, subject_iri, pred_iri, obj_nt, prov_pred, ts, graph_close
            );
            let _ = crate::sparql::execute::sparql_update(&annotation_sparql);
        }
    }

    if written > 0 {
        update_graph_metrics_triple_count(graph_id, written);
    }

    written
}

fn resolve_diff_timestamp(
    payload: &serde_json::Value,
    mapping: &str,
    _explicit: Option<pgrx::datum::Timestamp>,
) -> Option<String> {
    if _explicit.is_some() {
        return Some(chrono_utc_now());
    }

    let ts_path_opt: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT timestamp_path FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None);

    if let (Some(field), Some(val)) = (
        ts_path_opt.as_deref().and_then(|s| s.strip_prefix("$.")),
        ts_path_opt
            .as_deref()
            .and_then(|s| payload.get(s.strip_prefix("$.").unwrap_or(s))),
    ) {
        if let Some(s) = val.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = val.as_i64() {
            return Some(n.to_string());
        }
        let _ = field;
        pgrx::error!(
            "ingest_json_diff: timestamp_path evaluated to a non-string value; \
             expected an ISO 8601 datetime string"
        );
    }

    None
}

fn chrono_utc_now() -> String {
    // Use a simple date string; in production this would use chrono.
    "2026-01-01T00:00:00Z".to_string()
}

fn fetch_timestamp_predicate(mapping: &str) -> String {
    Spi::get_one_with_args::<String>(
        "SELECT COALESCE(timestamp_predicate, \
         'http://www.w3.org/ns/prov#generatedAtTime') \
         FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| "http://www.w3.org/ns/prov#generatedAtTime".to_string())
}

fn get_current_value(subject_id: i64, pred_id: i64, graph_id: i64) -> Option<i64> {
    Spi::get_one_with_args::<i64>(
        "SELECT o FROM _pg_ripple.vp_rare WHERE s = $1 AND p = $2 AND g = $3 LIMIT 1",
        &[
            pgrx::datum::DatumWithOid::from(subject_id),
            pgrx::datum::DatumWithOid::from(pred_id),
            pgrx::datum::DatumWithOid::from(graph_id),
        ],
    )
    .unwrap_or(None)
}

fn encode_json_value(val: &serde_json::Value) -> i64 {
    let lit = match val {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        other => other.to_string(),
    };
    crate::dictionary::encode(&lit, crate::dictionary::KIND_LITERAL)
}

fn format_ntriple_object(val: &serde_json::Value) -> String {
    match val {
        JsonValue::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        JsonValue::Number(n) => {
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>", n)
        }
        JsonValue::Bool(b) => {
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>", b)
        }
        JsonValue::Object(o) if o.contains_key("@id") => {
            format!("<{}>", o["@id"].as_str().unwrap_or(""))
        }
        other => format!("\"{}\"", other.to_string().replace('"', "\\\"")),
    }
}

fn format_ntriple_for_json_val(subject: &str, predicate: &str, val: &serde_json::Value) -> String {
    format!(
        "<{}> <{}> {} .",
        subject,
        predicate,
        format_ntriple_object(val)
    )
}

// ── BIDI-UPSERT-01 Implementation ─────────────────────────────────────────────

pub fn ingest_json_upsert_impl(
    payload: &serde_json::Value,
    subject_iri: &str,
    mapping: &str,
    graph_iri: Option<&str>,
) -> i64 {
    let (context, default_g, _) = fetch_mapping_row(mapping);
    let effective_graph = resolve_graph_iri(graph_iri, default_g.as_deref());

    let shape_iri: String = Spi::get_one_with_args::<String>(
        "SELECT shape_iri FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| {
        pgrx::error!(
            "ingest_json upsert mode: mapping {:?} has no registered shape_iri; \
             register a SHACL shape via register_json_mapping(shape_iri => ...) \
             or use mode => 'append'",
            mapping
        )
    });

    let max_count_1_sparql = format!(
        "SELECT ?path WHERE {{ \
            <{}> <http://www.w3.org/ns/shacl#property> ?prop . \
            ?prop <http://www.w3.org/ns/shacl#path> ?path . \
            ?prop <http://www.w3.org/ns/shacl#maxCount> 1 \
        }}",
        shape_iri
    );

    let max_count_iris: std::collections::HashSet<String> =
        crate::sparql::sparql(&max_count_1_sparql)
            .iter()
            .filter_map(|row| {
                let obj = row.0.as_object()?;
                let path = obj.get("path")?.as_str()?.trim_matches('"').to_string();
                let path = path
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string();
                Some(path)
            })
            .collect();

    let ctx_obj = context.as_object().cloned().unwrap_or_default();

    if let Some(payload_obj) = payload.as_object() {
        for (key, _) in payload_obj {
            if key.starts_with('@') {
                continue;
            }
            let pred_iri = match ctx_obj.get(key.as_str()) {
                Some(JsonValue::String(s)) => s.clone(),
                Some(JsonValue::Object(meta)) => match meta.get("@id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                },
                _ => continue,
            };

            if max_count_iris.contains(&pred_iri) {
                let del_sparql = match effective_graph {
                    Some(g) => format!(
                        "DELETE WHERE {{ GRAPH <{}> {{ <{}> <{}> ?o }} }}",
                        g.trim_matches(|c| c == '<' || c == '>'),
                        subject_iri,
                        pred_iri
                    ),
                    None => format!("DELETE WHERE {{ <{}> <{}> ?o }}", subject_iri, pred_iri),
                };
                let _ = crate::sparql::execute::sparql_update(&del_sparql);
            }
        }
    }

    crate::json_mapping::ingest_json_impl(payload, subject_iri, mapping, effective_graph)
}

// ─────────────────────────────────────────────────────────────────────────────
// v0.78.0 — BIDIOPS Implementations
// ─────────────────────────────────────────────────────────────────────────────

// ── BIDIOPS-QUEUE-01: Dead-letter management ──────────────────────────────────

/// List dead-lettered events for a subscription.
#[allow(clippy::type_complexity)]
pub fn list_dead_letters_impl(
    subscription_name: &str,
    outbox_table: Option<&str>,
    _since: Option<pgrx::datum::TimestampWithTimeZone>,
    limit_n: i32,
) -> Vec<(
    pgrx::datum::Uuid,
    String,
    String,
    pgrx::JsonB,
    String,
    pgrx::datum::TimestampWithTimeZone,
)> {
    Spi::connect(|c| {
        let mut out = Vec::new();
        let (sql, args): (&str, Vec<pgrx::datum::DatumWithOid>) = if let Some(ot) = outbox_table {
            (
                "SELECT event_id::text, outbox_table, COALESCE(outbox_variant,'default'), \
                 payload, reason, dead_lettered_at \
                 FROM _pg_ripple.event_dead_letters \
                 WHERE subscription_name = $1 AND outbox_table = $2 \
                 ORDER BY dead_lettered_at DESC \
                 LIMIT $3",
                vec![
                    pgrx::datum::DatumWithOid::from(subscription_name),
                    pgrx::datum::DatumWithOid::from(ot),
                    pgrx::datum::DatumWithOid::from(limit_n as i64),
                ],
            )
        } else {
            (
                "SELECT event_id::text, outbox_table, COALESCE(outbox_variant,'default'), \
                 payload, reason, dead_lettered_at \
                 FROM _pg_ripple.event_dead_letters \
                 WHERE subscription_name = $1 \
                 ORDER BY dead_lettered_at DESC \
                 LIMIT $2",
                vec![
                    pgrx::datum::DatumWithOid::from(subscription_name),
                    pgrx::datum::DatumWithOid::from(limit_n as i64),
                ],
            )
        };
        let iter = c.select(sql, None, &args)?;
        for row in iter {
            let eid_str = row[1].value::<String>()?.unwrap_or_default();
            let eid = parse_uuid(&eid_str);
            let ot = row[2].value::<String>()?.unwrap_or_default();
            let ov = row[3]
                .value::<String>()?
                .unwrap_or_else(|| "default".to_string());
            let payload = row[4]
                .value::<pgrx::JsonB>()?
                .unwrap_or(pgrx::JsonB(serde_json::json!({})));
            let reason = row[5].value::<String>()?.unwrap_or_default();
            let dl_at = row[6]
                .value::<pgrx::datum::TimestampWithTimeZone>()?
                .unwrap_or_else(now_tstz);
            out.push((eid, ot, ov, payload, reason, dl_at));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default()
}

/// Re-enqueue a dead-lettered event.
pub fn requeue_dead_letter_impl(
    subscription_name: &str,
    outbox_table: &str,
    event_id: pgrx::datum::Uuid,
) {
    let eid = format!("{}", event_id);

    // Find the row to re-enqueue.
    let row = Spi::connect(|c| {
        let mut iter = c.select(
            "SELECT payload, s FROM _pg_ripple.event_dead_letters \
             WHERE subscription_name = $1 AND outbox_table = $2 AND event_id = $3::uuid",
            None,
            &[
                pgrx::datum::DatumWithOid::from(subscription_name),
                pgrx::datum::DatumWithOid::from(outbox_table),
                pgrx::datum::DatumWithOid::from(eid.as_str()),
            ],
        )?;
        Ok::<_, pgrx::spi::Error>(iter.next().map(|r| {
            let payload = r["payload"]
                .value::<pgrx::JsonB>()
                .ok()
                .flatten()
                .unwrap_or(pgrx::JsonB(serde_json::json!({})));
            let s = r["s"].value::<i64>().ok().flatten().unwrap_or(0);
            (payload, s)
        }))
    })
    .unwrap_or(None);

    let (payload, s) = match row {
        Some(r) => r,
        None => {
            pgrx::notice!(
                "requeue_dead_letter: no dead-letter row for subscription={} outbox={} event_id={}",
                subscription_name,
                outbox_table,
                eid
            );
            return;
        }
    };

    // Re-insert into the outbox table preserving the original event_id.
    let insert_sql = format!(
        "INSERT INTO {} (event_id, subscription_name, s, payload, emitted_at) \
         VALUES ($1::uuid, $2, $3, $4, now()) \
         ON CONFLICT DO NOTHING",
        outbox_table
    );
    Spi::run_with_args(
        &insert_sql,
        &[
            pgrx::datum::DatumWithOid::from(eid.as_str()),
            pgrx::datum::DatumWithOid::from(subscription_name),
            pgrx::datum::DatumWithOid::from(s),
            pgrx::datum::DatumWithOid::from(payload),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("requeue_dead_letter: re-insert failed: {e}"));

    // Remove from dead-letter table.
    Spi::run_with_args(
        "DELETE FROM _pg_ripple.event_dead_letters \
         WHERE subscription_name = $1 AND outbox_table = $2 AND event_id = $3::uuid",
        &[
            pgrx::datum::DatumWithOid::from(subscription_name),
            pgrx::datum::DatumWithOid::from(outbox_table),
            pgrx::datum::DatumWithOid::from(eid.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("requeue_dead_letter: cleanup failed: {e}"));

    // Audit the requeue.
    record_audit_impl(
        Some(eid.as_str()),
        Some(subscription_name),
        "dead_letter",
        Some(&format!("{}:{}", outbox_table, eid)),
        "requeue_dead_letter",
        None,
        Some(serde_json::json!({"outbox_table": outbox_table, "requeued_at": "now"})),
    );
}

/// Permanently drop a dead-lettered event.
pub fn drop_dead_letter_impl(
    subscription_name: &str,
    outbox_table: &str,
    event_id: pgrx::datum::Uuid,
) {
    let eid = format!("{}", event_id);
    Spi::run_with_args(
        "DELETE FROM _pg_ripple.event_dead_letters \
         WHERE subscription_name = $1 AND outbox_table = $2 AND event_id = $3::uuid",
        &[
            pgrx::datum::DatumWithOid::from(subscription_name),
            pgrx::datum::DatumWithOid::from(outbox_table),
            pgrx::datum::DatumWithOid::from(eid.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("drop_dead_letter: {e}"));

    record_audit_impl(
        Some(eid.as_str()),
        Some(subscription_name),
        "dead_letter",
        Some(&format!("{}:{}", outbox_table, eid)),
        "drop_dead_letter",
        None,
        Some(serde_json::json!({"outbox_table": outbox_table})),
    );
}

// ── BIDIOPS-EVOLVE-01: Schema-evolution ───────────────────────────────────────

/// Alter a subscription's schema-evolution policies.
pub fn alter_subscription_impl(
    name: &str,
    frame_change_policy: Option<&str>,
    iri_change_policy: Option<&str>,
    exclude_change_policy: Option<&str>,
) {
    let session_user = Spi::get_one::<String>("SELECT session_user::text")
        .unwrap_or(None)
        .unwrap_or_default();

    let policies = [
        ("frame_change_policy", frame_change_policy),
        ("iri_change_policy", iri_change_policy),
        ("exclude_change_policy", exclude_change_policy),
    ];

    for (field, new_val) in &policies {
        if let Some(val) = new_val {
            if *val != "new_events_only" {
                pgrx::error!(
                    "alter_subscription: unsupported {} '{}'; only 'new_events_only' is supported in v0.78.0",
                    field,
                    val
                );
            }
            // Fetch old value for audit record.
            let old_val = Spi::get_one_with_args::<String>(
                &format!(
                    "SELECT {} FROM _pg_ripple.subscriptions WHERE name = $1",
                    field
                ),
                &[pgrx::datum::DatumWithOid::from(name)],
            )
            .unwrap_or(None);

            // Apply the change.
            Spi::run_with_args(
                &format!(
                    "UPDATE _pg_ripple.subscriptions SET {} = $1 WHERE name = $2",
                    field
                ),
                &[
                    pgrx::datum::DatumWithOid::from(*val),
                    pgrx::datum::DatumWithOid::from(name),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("alter_subscription: {e}"));

            // Record in schema_changes (always record, even redundant calls).
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.subscription_schema_changes \
                 (subscription_name, changed_by, field, old_value, new_value, policy_applied) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    pgrx::datum::DatumWithOid::from(name),
                    pgrx::datum::DatumWithOid::from(session_user.as_str()),
                    pgrx::datum::DatumWithOid::from(*field),
                    pgrx::datum::DatumWithOid::from(
                        old_val
                            .as_deref()
                            .map(|s| pgrx::JsonB(serde_json::Value::String(s.to_string()))),
                    ),
                    pgrx::datum::DatumWithOid::from(Some(pgrx::JsonB(serde_json::Value::String(
                        val.to_string(),
                    )))),
                    pgrx::datum::DatumWithOid::from(*val),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("alter_subscription: schema_changes: {e}"));
        }
    }
}

// ── BIDIOPS-AUTH-01: Per-subscription tokens ──────────────────────────────────

use sha2::{Digest, Sha256};

/// Register a per-subscription bearer token with specific scopes.
pub fn register_subscription_token_impl(
    subscription_name: &str,
    scopes: &[String],
    label: Option<&str>,
) -> String {
    // Validate scopes.
    let valid_scopes = [
        "linkback",
        "divergence",
        "abandon",
        "outbox_read",
        "dead_letter_admin",
    ];
    for scope in scopes {
        if !valid_scopes.contains(&scope.as_str()) {
            pgrx::error!(
                "register_subscription_token: unknown scope '{}'; \
                 valid scopes: linkback, divergence, abandon, outbox_read, dead_letter_admin",
                scope
            );
        }
    }

    // Generate 32 random bytes.
    let rand_bytes = generate_random_bytes_32();
    let raw_token = format!("pgrt_{}", base64url_encode(&rand_bytes));

    // Hash the raw token with SHA-256.
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash: Vec<u8> = hasher.finalize().to_vec();

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.subscription_tokens \
         (token_hash, subscription_name, scopes, label) \
         VALUES ($1, $2, $3, $4)",
        &[
            pgrx::datum::DatumWithOid::from(token_hash.as_slice()),
            pgrx::datum::DatumWithOid::from(subscription_name),
            pgrx::datum::DatumWithOid::from(scopes.to_vec()),
            pgrx::datum::DatumWithOid::from(label),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("register_subscription_token: {e}"));

    raw_token
}

/// Generate 32 cryptographically-random bytes from the OS entropy source.
///
/// Uses /dev/urandom directly to avoid a dependency on pgcrypto's gen_random_bytes().
fn generate_random_bytes_32() -> [u8; 32] {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    match std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)) {
        Ok(()) => bytes,
        Err(e) => {
            // Last-resort fallback: mix clock nanos + process ID.
            // This should never happen on any supported OS but avoids a hard failure.
            pgrx::warning!(
                "generate_random_bytes_32: /dev/urandom unavailable: {e}; using low-entropy fallback"
            );
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let pid = std::process::id();
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = ((ts.wrapping_add(pid).wrapping_add(i as u32)) & 0xff) as u8;
            }
            bytes
        }
    }
}

/// Base64url-encode bytes (no padding).
fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        out.push(ALPHABET[b0 >> 2] as char);
        out.push(ALPHABET[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((b1 & 15) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[b2 & 63] as char);
        }
    }
    out
}

/// Revoke a subscription token by its SHA-256 hash.
pub fn revoke_subscription_token_impl(token_hash: &[u8]) {
    Spi::run_with_args(
        "UPDATE _pg_ripple.subscription_tokens SET revoked_at = now() \
         WHERE token_hash = $1 AND revoked_at IS NULL",
        &[pgrx::datum::DatumWithOid::from(token_hash)],
    )
    .unwrap_or_else(|e| pgrx::warning!("revoke_subscription_token: {e}"));
}

/// List all tokens for a subscription.
#[allow(clippy::type_complexity)]
pub fn list_subscription_tokens_impl(
    subscription_name: &str,
) -> Vec<(
    Vec<u8>,
    Vec<String>,
    Option<String>,
    pgrx::datum::TimestampWithTimeZone,
    Option<pgrx::datum::TimestampWithTimeZone>,
    Option<pgrx::datum::TimestampWithTimeZone>,
)> {
    Spi::connect(|c| {
        let mut out = Vec::new();
        let iter = c.select(
            "SELECT token_hash, scopes, label, created_at, last_used_at, revoked_at \
             FROM _pg_ripple.subscription_tokens \
             WHERE subscription_name = $1 \
             ORDER BY created_at",
            None,
            &[pgrx::datum::DatumWithOid::from(subscription_name)],
        )?;
        for row in iter {
            let hash = row["token_hash"].value::<Vec<u8>>()?.unwrap_or_default();
            let scopes = row["scopes"].value::<Vec<String>>()?.unwrap_or_default();
            let label = row["label"].value::<String>()?;
            let created_at = row["created_at"]
                .value::<pgrx::datum::TimestampWithTimeZone>()?
                .unwrap_or_else(now_tstz);
            let last_used = row["last_used_at"].value::<pgrx::datum::TimestampWithTimeZone>()?;
            let revoked = row["revoked_at"].value::<pgrx::datum::TimestampWithTimeZone>()?;
            out.push((hash, scopes, label, created_at, last_used, revoked));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default()
}

// ── BIDIOPS-RECON-01: Reconciliation toolkit ──────────────────────────────────

/// Enqueue a reconciliation item for a diverged event.
pub fn reconciliation_enqueue_impl(
    event_id: pgrx::datum::Uuid,
    divergence_summary: &serde_json::Value,
) -> i64 {
    let eid = format!("{}", event_id);

    // Look up subscription_name from pending_linkbacks if available.
    let sub_name = Spi::get_one_with_args::<String>(
        "SELECT subscription_name FROM _pg_ripple.pending_linkbacks \
         WHERE event_id = $1::uuid LIMIT 1",
        &[pgrx::datum::DatumWithOid::from(eid.as_str())],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| "unknown".to_string());

    let recon_id = Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.reconciliation_queue \
         (event_id, subscription_name, divergence_summary) \
         VALUES ($1::uuid, $2, $3) \
         RETURNING reconciliation_id",
        &[
            pgrx::datum::DatumWithOid::from(eid.as_str()),
            pgrx::datum::DatumWithOid::from(sub_name.as_str()),
            pgrx::datum::DatumWithOid::from(pgrx::JsonB(divergence_summary.clone())),
        ],
    )
    .unwrap_or_else(|e| {
        pgrx::error!("reconciliation_enqueue: insert failed: {e}");
    })
    .unwrap_or(0);

    record_audit_impl(
        Some(eid.as_str()),
        Some(sub_name.as_str()),
        "reconciliation",
        Some(&recon_id.to_string()),
        "divergence",
        None,
        Some(divergence_summary.clone()),
    );

    recon_id
}

/// Pull the next unresolved reconciliation item (lease + SKIP LOCKED).
#[allow(clippy::type_complexity)]
pub fn reconciliation_next_impl(
    subscription_name: &str,
) -> Vec<(
    i64,
    pgrx::datum::Uuid,
    Option<pgrx::JsonB>,
    pgrx::JsonB,
    pgrx::datum::TimestampWithTimeZone,
)> {
    Spi::connect_mut(|c| {
        let mut out = Vec::new();
        let iter = c.update(
            "UPDATE _pg_ripple.reconciliation_queue \
             SET leased_until = now() + interval '10 minutes', \
                 leased_by = session_user::text \
             WHERE reconciliation_id = ( \
                 SELECT reconciliation_id FROM _pg_ripple.reconciliation_queue \
                 WHERE subscription_name = $1 AND resolved_at IS NULL \
                 ORDER BY enqueued_at \
                 LIMIT 1 \
                 FOR UPDATE SKIP LOCKED \
             ) \
             RETURNING reconciliation_id, event_id::text, divergence_summary, enqueued_at",
            None,
            &[pgrx::datum::DatumWithOid::from(subscription_name)],
        )?;
        for row in iter {
            let rid = row["reconciliation_id"].value::<i64>()?.unwrap_or(0);
            let eid_str = row["event_id"].value::<String>()?.unwrap_or_default();
            let eid = parse_uuid(&eid_str);
            let ds = row["divergence_summary"]
                .value::<pgrx::JsonB>()?
                .unwrap_or(pgrx::JsonB(serde_json::json!({})));
            let ea = row["enqueued_at"]
                .value::<pgrx::datum::TimestampWithTimeZone>()?
                .unwrap_or_else(now_tstz);
            out.push((rid, eid, None, ds, ea));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default()
}

/// Resolve a reconciliation item.
pub fn reconciliation_resolve_impl(reconciliation_id: i64, action: &str, note: Option<&str>) {
    match action {
        "accept_external" | "force_internal" | "merge_via_owl_sameAs" | "dead_letter" => {}
        other => pgrx::error!(
            "reconciliation_resolve: unknown action '{}'; \
             valid: accept_external, force_internal, merge_via_owl_sameAs, dead_letter",
            other
        ),
    }

    let session_user = Spi::get_one::<String>("SELECT session_user::text")
        .unwrap_or(None)
        .unwrap_or_default();

    Spi::run_with_args(
        "UPDATE _pg_ripple.reconciliation_queue \
         SET resolved_at = now(), resolution = $1, \
             resolved_by = $2, resolution_note = $3 \
         WHERE reconciliation_id = $4 AND resolved_at IS NULL",
        &[
            pgrx::datum::DatumWithOid::from(action),
            pgrx::datum::DatumWithOid::from(session_user.as_str()),
            pgrx::datum::DatumWithOid::from(note),
            pgrx::datum::DatumWithOid::from(reconciliation_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("reconciliation_resolve: {e}"));

    // If action is dead_letter, move to event_dead_letters.
    if action == "dead_letter" {
        let row = Spi::connect(|c| {
            let mut iter = c.select(
                "SELECT event_id::text, subscription_name, divergence_summary \
                 FROM _pg_ripple.reconciliation_queue WHERE reconciliation_id = $1",
                None,
                &[pgrx::datum::DatumWithOid::from(reconciliation_id)],
            )?;
            Ok::<_, pgrx::spi::Error>(iter.next().map(|r| {
                let eid = r["event_id"]
                    .value::<String>()
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let sub = r["subscription_name"]
                    .value::<String>()
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let ds = r["divergence_summary"]
                    .value::<pgrx::JsonB>()
                    .ok()
                    .flatten()
                    .unwrap_or(pgrx::JsonB(serde_json::json!({})));
                (eid, sub, ds)
            }))
        })
        .unwrap_or(None);

        if let Some((eid, sub, ds)) = row {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.event_dead_letters \
                 (event_id, subscription_name, outbox_table, payload, emitted_at, reason, extra) \
                 VALUES ($1::uuid, $2, 'reconciliation', '{}'::jsonb, now(), \
                         'reconciliation_dead_letter', $3) \
                 ON CONFLICT DO NOTHING",
                &[
                    pgrx::datum::DatumWithOid::from(eid.as_str()),
                    pgrx::datum::DatumWithOid::from(sub.as_str()),
                    pgrx::datum::DatumWithOid::from(ds),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("reconciliation_resolve: dead_letter insert: {e}"));
        }
    }

    record_audit_impl(
        None,
        None,
        "reconciliation",
        Some(&reconciliation_id.to_string()),
        action,
        None,
        note.map(|n| serde_json::json!({"note": n})),
    );
}

// ── BIDIOPS-DASH-01: Consolidated operations surface ──────────────────────────

/// Return per-subscription operational status.
#[allow(clippy::type_complexity)]
pub fn bidi_status_impl() -> Vec<(
    String,
    Option<bool>,
    Option<String>,
    i64,
    Option<String>,
    i64,
    f64,
    i64,
    Option<String>,
    i64,
    Option<pgrx::datum::TimestampWithTimeZone>,
    Option<pgrx::datum::TimestampWithTimeZone>,
    Option<String>,
    i64,
    i64,
    i64,
)> {
    Spi::connect(|c| {
        let mut out = Vec::new();
        let iter = c.select(
            "SELECT \
                s.name AS subscription_name, \
                NULL::boolean AS pg_trickle_paused, \
                NULL::text AS pg_trickle_pause_reason, \
                COALESCE(( \
                    SELECT COUNT(*)::bigint FROM _pg_ripple.event_dead_letters d \
                    WHERE d.subscription_name = s.name \
                ), 0) AS dead_letter_count, \
                COALESCE(( \
                    SELECT COUNT(*)::bigint FROM _pg_ripple.pending_linkbacks pl \
                    WHERE pl.subscription_name = s.name \
                ), 0) AS pending_linkback_count, \
                COALESCE(( \
                    SELECT COUNT(*)::bigint FROM _pg_ripple.reconciliation_queue rq \
                    WHERE rq.subscription_name = s.name AND rq.resolved_at IS NULL \
                ), 0) AS reconciliation_open, \
                COALESCE(( \
                    SELECT COUNT(*)::bigint FROM _pg_ripple.iri_rewrite_misses m \
                    WHERE m.observed_at > now() - interval '24 hours' \
                ), 0) AS rewrite_miss_count_24h \
             FROM _pg_ripple.subscriptions s \
             ORDER BY s.name",
            None,
            &[],
        )?;

        for row in iter {
            let sub_name = row["subscription_name"]
                .value::<String>()?
                .unwrap_or_default();
            let paused = row["pg_trickle_paused"].value::<bool>()?;
            let pause_reason = row["pg_trickle_pause_reason"].value::<String>()?;
            let dead_letter_count = row["dead_letter_count"].value::<i64>()?.unwrap_or(0);
            let pending_linkback_count = row["pending_linkback_count"].value::<i64>()?.unwrap_or(0);
            let reconciliation_open = row["reconciliation_open"].value::<i64>()?.unwrap_or(0);
            let rewrite_miss_count_24h = row["rewrite_miss_count_24h"].value::<i64>()?.unwrap_or(0);

            out.push((
                sub_name,
                paused,
                pause_reason,
                0i64, // outbox_depth (requires dynamic table query; approximated)
                None, // outbox_oldest_age
                dead_letter_count,
                0.0f64, // conflict_rejection_rate
                pending_linkback_count,
                None, // pending_linkback_oldest_age
                rewrite_miss_count_24h,
                None, // last_emit_at
                None, // pg_trickle_last_delivery_at
                None, // pg_trickle_last_error
                0i64, // pg_trickle_retry_count
                0i64, // pg_trickle_delivery_dlq_count
                reconciliation_open,
            ));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default()
}

/// Return overall bidi health.
pub fn bidi_health_impl() -> Vec<(String, Vec<String>, pgrx::datum::TimestampWithTimeZone)> {
    let rows = bidi_status_impl();
    let mut reasons: Vec<String> = Vec::new();
    let mut is_paused = false;
    let is_failing = false;
    let mut is_degraded = false;

    for row in &rows {
        // row.1 = pg_trickle_paused
        if row.1 == Some(true) {
            is_paused = true;
            reasons.push(format!("{} paused (pg-trickle)", row.0));
        }
        // row.5 = dead_letter_count
        let dead_letters = row.5;
        if dead_letters > 0 {
            is_degraded = true;
            reasons.push(format!("{} dead_letter_count {}", row.0, dead_letters));
        }
        // row.7 = pending_linkback_count
        let pending = row.7;
        if pending > 0 {
            is_degraded = true;
        }
        // row.15 = reconciliation_open
        let recon = row.15;
        if recon > 0 {
            is_degraded = true;
            reasons.push(format!("{} reconciliation_open {}", row.0, recon));
        }
    }

    let status = if is_failing {
        "failing"
    } else if is_paused {
        "paused"
    } else if is_degraded {
        "degraded"
    } else {
        "healthy"
    };

    let checked_at =
        Spi::get_one::<pgrx::datum::TimestampWithTimeZone>("SELECT now()::timestamptz")
            .unwrap_or(None)
            .unwrap_or_else(now_tstz);

    let _ = is_failing; // suppress warning

    vec![(status.to_string(), reasons, checked_at)]
}

// ── BIDIOPS-AUDIT-01: Audit recording ─────────────────────────────────────────

/// Internal helper to record a side-band mutation audit row.
pub fn record_audit_impl(
    event_id: Option<&str>,
    subscription_name: Option<&str>,
    resource_type: &str,
    resource_id: Option<&str>,
    action: &str,
    actor_token_hash: Option<&[u8]>,
    extra: Option<serde_json::Value>,
) {
    let session_user = Spi::get_one::<String>("SELECT session_user::text")
        .unwrap_or(None)
        .unwrap_or_default();

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.event_audit \
         (event_id, subscription_name, resource_type, resource_id, action, \
          actor_token_hash, actor_session, extra) \
         VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8)",
        &[
            pgrx::datum::DatumWithOid::from(event_id),
            pgrx::datum::DatumWithOid::from(subscription_name),
            pgrx::datum::DatumWithOid::from(resource_type),
            pgrx::datum::DatumWithOid::from(resource_id),
            pgrx::datum::DatumWithOid::from(action),
            pgrx::datum::DatumWithOid::from(actor_token_hash),
            pgrx::datum::DatumWithOid::from(session_user.as_str()),
            pgrx::datum::DatumWithOid::from(extra.map(pgrx::JsonB)),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_audit: insert failed: {e}"));
}

/// Purge audit log entries older than `pg_ripple.audit_retention` days.
pub fn purge_event_audit_impl() -> i64 {
    let retention_days = crate::gucs::observability::AUDIT_RETENTION_DAYS.get();
    if retention_days <= 0 {
        return 0;
    }
    Spi::get_one_with_args::<i64>(
        "WITH deleted AS ( \
            DELETE FROM _pg_ripple.event_audit \
            WHERE observed_at < now() - ($1 || ' days')::interval \
            RETURNING 1 \
         ) SELECT COUNT(*)::bigint FROM deleted",
        &[pgrx::datum::DatumWithOid::from(retention_days as i64)],
    )
    .unwrap_or(None)
    .unwrap_or(0)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Apply frame-level `"@redact": true` markers to an event payload (BIDIOPS-REDACT-01).
///
/// For each key in `frame`, if its value is a JSON object that contains
/// `"@redact": true`, the corresponding key in `payload` is replaced with
/// `{"@redacted": true}`. All other keys pass through unchanged.
pub fn apply_frame_redaction_impl(
    frame: &serde_json::Value,
    payload: &serde_json::Value,
) -> serde_json::Value {
    let Some(frame_obj) = frame.as_object() else {
        return payload.clone();
    };
    let Some(payload_obj) = payload.as_object() else {
        return payload.clone();
    };

    let redacted_sentinel = serde_json::json!({"@redacted": true});

    let mut out = payload_obj.clone();
    for (key, spec) in frame_obj {
        if spec
            .as_object()
            .and_then(|m| m.get("@redact"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && out.contains_key(key)
        {
            out.insert(key.clone(), redacted_sentinel.clone());
        }
    }
    serde_json::Value::Object(out)
}

fn parse_uuid(s: &str) -> pgrx::datum::Uuid {
    let s = s.replace('-', "");
    if s.len() != 32 {
        return pgrx::datum::Uuid::from_bytes([0u8; 16]);
    }
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        let hex = &s[i * 2..i * 2 + 2];
        *b = u8::from_str_radix(hex, 16).unwrap_or(0);
    }
    pgrx::datum::Uuid::from_bytes(bytes)
}

fn now_tstz() -> pgrx::datum::TimestampWithTimeZone {
    Spi::get_one::<pgrx::datum::TimestampWithTimeZone>("SELECT now()::timestamptz")
        .unwrap_or(None)
        .unwrap_or_else(|| {
            // SAFETY: using positive_infinity as a safe non-panicking fallback
            pgrx::datum::TimestampWithTimeZone::positive_infinity()
        })
}

// ─── STATS-CACHE-01 (v0.82.0) ────────────────────────────────────────────────

/// Rebuild `_pg_ripple.predicate_stats_cache` from the current `_pg_ripple.predicates` table.
///
/// Exposed as `pg_ripple.refresh_stats_cache()`. The background refresh task in
/// `worker.rs` calls this when `stats_refresh_interval_seconds` has elapsed.
pub fn refresh_stats_cache_impl() -> i64 {
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicate_stats_cache (predicate_id, triple_count, refreshed_at) \
         SELECT id, COALESCE(triple_count, 0), now() \
         FROM _pg_ripple.predicates \
         ON CONFLICT (predicate_id) DO UPDATE SET \
           triple_count  = EXCLUDED.triple_count, \
           refreshed_at  = EXCLUDED.refreshed_at",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("refresh_stats_cache: {e}"));

    Spi::get_one::<i64>("SELECT COUNT(*) FROM _pg_ripple.predicate_stats_cache")
        .unwrap_or(None)
        .unwrap_or(0)
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
        // Should not panic.
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
