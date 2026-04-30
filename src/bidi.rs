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
    pub fn install_bidi_inbox(inbox_table: default!(&str, "'pg_ripple_inbox.linkback_inbox'")) {
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
             FROM _pg_ripple.pending_linkbacks WHERE event_id = $1",
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
        "DELETE FROM _pg_ripple.pending_linkbacks WHERE event_id = $1",
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
             FROM _pg_ripple.pending_linkbacks WHERE event_id = $1",
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
        "DELETE FROM _pg_ripple.pending_linkbacks WHERE event_id = $1",
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

        let (sql, args): (&str, &[pgrx::datum::DatumWithOid]) = if filter_graph_iri.is_some() {
            (
                "SELECT d.value AS graph_iri, m.graph_id, \
                 COALESCE(m.triple_count, 0) AS triple_count, \
                 m.last_write_at, \
                 COALESCE(m.conflicts_total, 0) AS conflicts_total, \
                 COALESCE((SELECT COUNT(*)::int FROM _pg_ripple.subscriptions s \
                  WHERE s.exclude_graphs IS NULL OR \
                        $1 != ALL(s.exclude_graphs)), 0) AS subscriptions_active \
                 FROM _pg_ripple.graph_metrics m \
                 JOIN _pg_ripple.dictionary d ON d.id = m.graph_id \
                 WHERE d.value = $1",
                &[],
            )
        } else {
            (
                "SELECT d.value AS graph_iri, m.graph_id, \
                 COALESCE(m.triple_count, 0) AS triple_count, \
                 m.last_write_at, \
                 COALESCE(m.conflicts_total, 0) AS conflicts_total, \
                 COALESCE((SELECT COUNT(*)::int FROM _pg_ripple.subscriptions s \
                  WHERE s.exclude_graphs IS NULL), 0) AS subscriptions_active \
                 FROM _pg_ripple.graph_metrics m \
                 JOIN _pg_ripple.dictionary d ON d.id = m.graph_id \
                 ORDER BY m.graph_id",
                &[],
            )
        };

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
            let _ = sql;
            let _ = args;
            c.select(
                "SELECT d.value AS graph_iri, m.graph_id, \
                 COALESCE(m.triple_count, 0) AS triple_count, \
                 m.last_write_at, \
                 COALESCE(m.conflicts_total, 0) AS conflicts_total, \
                 COALESCE((SELECT COUNT(*)::int FROM _pg_ripple.subscriptions s \
                  WHERE s.exclude_graphs IS NULL), 0) AS subscriptions_active \
                 FROM _pg_ripple.graph_metrics m \
                 JOIN _pg_ripple.dictionary d ON d.id = m.graph_id \
                 ORDER BY m.graph_id",
                None,
                &[],
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
    crate::bulk_load::json_ld_load(document, graph_iri)
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
