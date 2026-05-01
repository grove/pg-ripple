//! Bidirectional sync operations (MOD-BIDI-01, v0.83.0).
//!
//! Contains: BIDI-CONFLICT-01 (conflict resolution policies),
//! BIDI-DELETE-01 (symmetric delete), BIDI-LINKBACK-01 (target-assigned ID
//! rendezvous + subscription buffer flush).

use pgrx::prelude::*;
use serde_json::Value as JsonValue;

use super::protocol::{fetch_mapping_row, resolve_graph_iri, update_graph_metrics_triple_count};

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

    let config_json = config.map(|j| j.0.clone()).unwrap_or(serde_json::json!({}));

    if strategy == "latest_wins"
        && let Some(norm) = config_json.get("normalize").and_then(|v| v.as_str())
        && let Err(e) = super::protocol::validate_normalize_expression(norm)
    {
        pgrx::error!(
            "register_conflict_policy: invalid normalize expression: {}",
            e
        );
    }

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.conflict_policies \
         (predicate_iri, strategy, config) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (predicate_iri) DO UPDATE SET \
             strategy = EXCLUDED.strategy, \
             config   = EXCLUDED.config, \
             updated_at = now()",
        &[
            pgrx::datum::DatumWithOid::from(predicate),
            pgrx::datum::DatumWithOid::from(strategy),
            pgrx::datum::DatumWithOid::from(pgrx::JsonB(config_json)),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("register_conflict_policy: {e}"));
}

pub fn drop_conflict_policy_impl(predicate: &str) {
    Spi::run_with_args(
        "DELETE FROM _pg_ripple.conflict_policies WHERE predicate_iri = $1",
        &[pgrx::datum::DatumWithOid::from(predicate)],
    )
    .unwrap_or_else(|e| pgrx::warning!("drop_conflict_policy: {e}"));
}

pub fn recompute_conflict_winners_impl(predicate_iri: &str) {
    backfill_conflict_winners(predicate_iri);
}

/// Backfill or recompute conflict_winners for all existing subjects of a predicate.
fn backfill_conflict_winners(predicate_iri: &str) {
    let policy_opt: Option<(String, serde_json::Value)> = Spi::connect(|c| {
        let mut iter = c.select(
            "SELECT strategy, config FROM _pg_ripple.conflict_policies \
             WHERE predicate_iri = $1",
            None,
            &[pgrx::datum::DatumWithOid::from(predicate_iri)],
        )?;
        Ok::<_, pgrx::spi::Error>(iter.next().map(|row| {
            let strategy = row["strategy"]
                .value::<String>()
                .ok()
                .flatten()
                .unwrap_or_default();
            let config = row["config"]
                .value::<pgrx::JsonB>()
                .ok()
                .flatten()
                .map(|j| j.0)
                .unwrap_or(serde_json::json!({}));
            (strategy, config)
        }))
    })
    .unwrap_or(None);

    let (strategy, config) = match policy_opt {
        Some(p) => p,
        None => return, // no policy registered
    };

    let pred_id = crate::dictionary::encode(predicate_iri, crate::dictionary::KIND_IRI);

    // Collect all (subject, graph) pairs for this predicate.
    let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
        let mut out = Vec::new();
        let iter = c.select(
            "SELECT DISTINCT s, g FROM _pg_ripple.vp_rare WHERE p = $1 \
             UNION ALL \
             SELECT DISTINCT s, g FROM (SELECT s, g FROM _pg_ripple.vp_rare WHERE p = $1) sq",
            None,
            &[pgrx::datum::DatumWithOid::from(pred_id)],
        )?;
        for row in iter {
            let s = row["s"].value::<i64>()?.unwrap_or(0);
            let g = row["g"].value::<i64>()?.unwrap_or(0);
            out.push((s, g));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default();

    for (subject_id, graph_id) in pairs {
        let winner_id = match strategy.as_str() {
            "source_priority" => {
                let order: Vec<String> = config
                    .get("order")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut winner: Option<i64> = None;
                for source_iri in &order {
                    let source_id =
                        crate::dictionary::encode(source_iri, crate::dictionary::KIND_IRI);
                    let val: Option<i64> = Spi::get_one_with_args::<i64>(
                        "SELECT o FROM _pg_ripple.vp_rare \
                         WHERE p = $1 AND s = $2 AND g = $3 AND source = $4 \
                         ORDER BY i ASC LIMIT 1",
                        &[
                            pgrx::datum::DatumWithOid::from(pred_id),
                            pgrx::datum::DatumWithOid::from(subject_id),
                            pgrx::datum::DatumWithOid::from(graph_id),
                            pgrx::datum::DatumWithOid::from(source_id),
                        ],
                    )
                    .unwrap_or(None);
                    if let Some(v) = val {
                        winner = Some(v);
                        break;
                    }
                }
                winner
            }
            "latest_wins" => Spi::get_one_with_args::<i64>(
                "SELECT o FROM _pg_ripple.vp_rare \
                 WHERE p = $1 AND s = $2 AND g = $3 \
                 ORDER BY i DESC LIMIT 1",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(subject_id),
                    pgrx::datum::DatumWithOid::from(graph_id),
                ],
            )
            .unwrap_or(None),
            _ => None,
        };

        if let Some(winner_o) = winner_id {
            // Look up original statement ID.
            let stmt_id: Option<i64> = Spi::get_one_with_args::<i64>(
                "SELECT i FROM _pg_ripple.vp_rare \
                 WHERE p = $1 AND s = $2 AND o = $3 AND g = $4 \
                 ORDER BY i ASC LIMIT 1",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(subject_id),
                    pgrx::datum::DatumWithOid::from(winner_o),
                    pgrx::datum::DatumWithOid::from(graph_id),
                ],
            )
            .unwrap_or(None);

            Spi::run_with_args(
                "INSERT INTO _pg_ripple.conflict_winners \
                 (predicate_id, subject_id, graph_id, winner_object_id, statement_id) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (predicate_id, subject_id, graph_id) DO UPDATE SET \
                     winner_object_id = EXCLUDED.winner_object_id, \
                     statement_id     = EXCLUDED.statement_id, \
                     updated_at       = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(subject_id),
                    pgrx::datum::DatumWithOid::from(graph_id),
                    pgrx::datum::DatumWithOid::from(winner_o),
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
