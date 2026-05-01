//! Shared protocol utilities for the bidi module (MOD-BIDI-01, v0.83.0).
//!
//! Contains: validation helpers, JSON mapping helpers, BIDI-CAS-01,
//! BIDI-INBOX-01, statistics-cache (STATS-CACHE-01), shared helper functions,
//! and `update_graph_metrics_triple_count`.

use pgrx::prelude::*;

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
pub fn resolve_graph_iri<'a>(
    explicit: Option<&'a str>,
    mapping_default: Option<&'a str>,
) -> Option<&'a str> {
    explicit.or(mapping_default)
}

// ─── Graph metrics helper ─────────────────────────────────────────────────────

/// Update the graph_metrics table incrementally.
/// Used by both BIDI-OBS-01 (relay.rs) and BIDI-DELETE-01 (sync.rs).
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

// ── Helper functions ──────────────────────────────────────────────────────────

/// Apply frame-level `"@redact": true` markers to an event payload (BIDIOPS-REDACT-01).
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

pub fn parse_uuid(s: &str) -> pgrx::datum::Uuid {
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

pub fn now_tstz() -> pgrx::datum::TimestampWithTimeZone {
    Spi::get_one::<pgrx::datum::TimestampWithTimeZone>("SELECT now()::timestamptz")
        .unwrap_or(None)
        .unwrap_or_else(|| {
            // SAFETY: using positive_infinity as a safe non-panicking fallback
            pgrx::datum::TimestampWithTimeZone::positive_infinity()
        })
}

// ─── STATS-CACHE-01 (v0.82.0) ────────────────────────────────────────────────

/// Rebuild `_pg_ripple.predicate_stats_cache` from the current predicates table.
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
