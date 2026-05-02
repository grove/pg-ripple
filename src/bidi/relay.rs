//! Relay operations for bidirectional integration (MOD-BIDI-01, v0.83.0).
//!
//! Contains: BIDI-OBS-01 (per-graph observability), BIDI-ATTR-01 (ingest_jsonld),
//! BIDI-DIFF-01 (diff-mode ingest), BIDI-UPSERT-01 (upsert ingest mode).

use pgrx::prelude::*;
use serde_json::Value as JsonValue;

use super::protocol::{fetch_mapping_row, resolve_graph_iri, update_graph_metrics_triple_count};

// ── BIDI-OBS-01 Implementation ────────────────────────────────────────────────

pub fn graph_stats_impl(
    filter_graph_iri: Option<&str>,
) -> Vec<(String, i64, i64, Option<pgrx::datum::Timestamp>, i64, i32)> {
    Spi::connect(|c| {
        let mut out = Vec::new();

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

    if let Some(field) = ts_path_opt.as_deref().and_then(|s| s.strip_prefix("$."))
        && let Some(val) = payload.get(field)
    {
        if let Some(s) = val.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = val.as_i64() {
            return Some(n.to_string());
        }
        pgrx::error!(
            "ingest_json_diff: timestamp_path evaluated to a non-string value; \
                 expected an ISO 8601 datetime string"
        );
    }

    None
}

fn chrono_utc_now() -> String {
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
