//! BIDIOPS subscription operations (MOD-BIDI-01, v0.83.0).
//!
//! Contains: BIDIOPS-QUEUE-01 (dead-letter management), BIDIOPS-EVOLVE-01
//! (schema-evolution), BIDIOPS-AUTH-01 (per-subscription tokens),
//! BIDIOPS-RECON-01 (reconciliation toolkit), BIDIOPS-DASH-01 (dashboard/health),
//! BIDIOPS-AUDIT-01 (audit recording).

use pgrx::prelude::*;
use sha2::{Digest, Sha256};

use super::protocol::{now_tstz, parse_uuid};

// ── BIDIOPS-QUEUE-01: Dead-letter management ──────────────────────────────────

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

pub fn requeue_dead_letter_impl(
    subscription_name: &str,
    outbox_table: &str,
    event_id: pgrx::datum::Uuid,
) {
    let eid = format!("{}", event_id);

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

            let old_val = Spi::get_one_with_args::<String>(
                &format!(
                    "SELECT {} FROM _pg_ripple.subscriptions WHERE name = $1",
                    field
                ),
                &[pgrx::datum::DatumWithOid::from(name)],
            )
            .unwrap_or(None);

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

pub fn register_subscription_token_impl(
    subscription_name: &str,
    scopes: &[String],
    label: Option<&str>,
) -> String {
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

    let rand_bytes = generate_random_bytes_32();
    let raw_token = format!("pgrt_{}", base64url_encode(&rand_bytes));

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
fn generate_random_bytes_32() -> [u8; 32] {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    match std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)) {
        Ok(()) => bytes,
        Err(e) => {
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

pub fn revoke_subscription_token_impl(token_hash: &[u8]) {
    Spi::run_with_args(
        "UPDATE _pg_ripple.subscription_tokens SET revoked_at = now() \
         WHERE token_hash = $1 AND revoked_at IS NULL",
        &[pgrx::datum::DatumWithOid::from(token_hash)],
    )
    .unwrap_or_else(|e| pgrx::warning!("revoke_subscription_token: {e}"));
}

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

pub fn reconciliation_enqueue_impl(
    event_id: pgrx::datum::Uuid,
    divergence_summary: &serde_json::Value,
) -> i64 {
    let eid = format!("{}", event_id);

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
                0i64, // outbox_depth
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

pub fn bidi_health_impl() -> Vec<(String, Vec<String>, pgrx::datum::TimestampWithTimeZone)> {
    let rows = bidi_status_impl();
    let mut reasons: Vec<String> = Vec::new();
    let mut is_paused = false;
    let is_failing = false;
    let mut is_degraded = false;

    for row in &rows {
        if row.1 == Some(true) {
            is_paused = true;
            reasons.push(format!("{} paused (pg-trickle)", row.0));
        }
        let dead_letters = row.5;
        if dead_letters > 0 {
            is_degraded = true;
            reasons.push(format!("{} dead_letter_count {}", row.0, dead_letters));
        }
        let pending = row.7;
        if pending > 0 {
            is_degraded = true;
        }
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
