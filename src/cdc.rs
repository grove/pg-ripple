//! Change Data Capture (CDC) for pg_ripple v0.6.0.
//!
//! Provides `subscribe(pattern, channel)` / `unsubscribe(channel)` for
//! event-driven notification when triples matching a predicate pattern are
//! inserted or deleted.
//!
//! # Mechanism
//!
//! Subscriptions are stored in `_pg_ripple.cdc_subscriptions`.  An AFTER
//! INSERT OR DELETE trigger on every VP delta table calls
//! `_pg_ripple.notify_triple_change()`, which looks up matching subscriptions
//! and issues `NOTIFY channel, payload` for each.
//!
//! The payload JSON is:
//! ```json
//! {"op": "insert"|"delete", "s": <int>, "p": <int>, "o": <int>, "g": <int>}
//! ```
//! Values are integer dictionary IDs.  Use `pg_ripple.decode_id(id)` to get
//! the human-readable term.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde_json;

// ─── Schema initialisation ─────────────────────────────────────────────────────

/// Create `_pg_ripple.cdc_subscriptions` and the notify trigger function.
///
/// Called once from `storage::initialize_schema`.
#[allow(dead_code)]
pub fn initialize_cdc_schema() {
    // Subscription registry.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_subscriptions ( \
             id               BIGSERIAL PRIMARY KEY, \
             channel          TEXT    NOT NULL, \
             predicate_id     BIGINT, \
             predicate_pattern TEXT   NOT NULL DEFAULT '*' \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc_subscriptions table creation error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_cdc_subs_channel \
         ON _pg_ripple.cdc_subscriptions (channel)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc_subscriptions index error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_cdc_subs_predicate \
         ON _pg_ripple.cdc_subscriptions (predicate_id)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc_subscriptions predicate index error: {e}"));

    // Notify trigger function (created once; parameterised via TG_ARGV).
    Spi::run_with_args(
        r#"
CREATE OR REPLACE FUNCTION _pg_ripple.notify_triple_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    pred_id BIGINT := TG_ARGV[0]::bigint;
    payload TEXT;
    sub RECORD;
BEGIN
    IF TG_OP = 'INSERT' THEN
        payload := json_build_object(
            'op', 'insert',
            's', NEW.s, 'p', pred_id, 'o', NEW.o, 'g', NEW.g
        )::text;
    ELSE
        payload := json_build_object(
            'op', 'delete',
            's', OLD.s, 'p', pred_id, 'o', OLD.o, 'g', OLD.g
        )::text;
    END IF;

    FOR sub IN
        SELECT channel FROM _pg_ripple.cdc_subscriptions
        WHERE predicate_id = pred_id OR predicate_pattern = '*'
    LOOP
        PERFORM pg_notify(sub.channel, payload);
    END LOOP;

    RETURN NEW;
END;
$$
        "#,
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc notify function creation error: {e}"));
}

/// Install the CDC notify trigger on a VP delta table for `pred_id`.
///
/// Called when a new HTAP VP table is created (from `ensure_htap_tables`).
pub fn install_trigger(pred_id: i64) {
    let delta = format!("_pg_ripple.vp_{pred_id}_delta");
    let trigger_name = format!("cdc_notify_{pred_id}");

    Spi::run_with_args(
        &format!(
            "DROP TRIGGER IF EXISTS {trigger_name} ON {delta}; \
             CREATE TRIGGER {trigger_name} \
             AFTER INSERT OR DELETE ON {delta} \
             FOR EACH ROW EXECUTE FUNCTION _pg_ripple.notify_triple_change({pred_id})"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc trigger install error: {e}"));
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Register a CDC subscription.
///
/// `predicate_pattern` is either an IRI string (e.g. `<https://schema.org/name>`),
/// or `'*'` for all predicates.  Notifications are sent to `channel`.
pub fn subscribe(pattern: &str, channel: &str) -> i64 {
    // Resolve IRI pattern to predicate ID if it's not a wildcard.
    let (pred_id, pat_str): (Option<i64>, String) = if pattern == "*" {
        (None, "*".to_string())
    } else {
        let p = crate::storage::strip_angle_brackets_pub(pattern);
        let id = crate::dictionary::encode(p, crate::dictionary::KIND_IRI);
        (Some(id), pattern.to_string())
    };

    Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.cdc_subscriptions (channel, predicate_id, predicate_pattern) \
         VALUES ($1, $2, $3) RETURNING id",
        &[
            DatumWithOid::from(channel),
            DatumWithOid::from(pred_id),
            DatumWithOid::from(pat_str.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("subscribe SPI error: {e}"))
    .unwrap_or(0)
}

/// Remove all subscriptions for a given notification channel.
pub fn unsubscribe(channel: &str) -> i64 {
    Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.cdc_subscriptions WHERE channel = $1 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(channel)],
    )
    .unwrap_or_else(|e| pgrx::error!("unsubscribe SPI error: {e}"))
    .unwrap_or(0)
}

// ─── Named subscription API (v0.42.0) ────────────────────────────────────────

/// Create a named CDC subscription in `_pg_ripple.subscriptions`.
///
/// Returns `true` if the subscription was inserted, `false` if it already existed.
pub fn create_named_subscription(
    name: &str,
    filter_sparql: Option<&str>,
    filter_shape: Option<&str>,
) -> bool {
    // Validate subscription name (must be a valid PostgreSQL identifier for NOTIFY channel).
    if name.is_empty() || name.len() > 63 {
        pgrx::error!(
            "create_subscription: name must be 1–63 characters; got: {:?}",
            name
        );
    }
    if name
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    {
        pgrx::error!(
            "create_subscription: name must contain only alphanumeric characters, \
             underscores, or hyphens; got: {:?}",
            name
        );
    }

    let inserted: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS (
             INSERT INTO _pg_ripple.subscriptions (name, filter_sparql, filter_shape)
             VALUES ($1, $2, $3)
             ON CONFLICT (name) DO NOTHING
             RETURNING 1
         )
         SELECT count(*)::bigint FROM ins",
        &[
            DatumWithOid::from(name),
            DatumWithOid::from(filter_sparql),
            DatumWithOid::from(filter_shape),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("create_subscription SPI error: {e}"))
    .unwrap_or(0);

    inserted > 0
}

/// Drop a named CDC subscription from `_pg_ripple.subscriptions`.
///
/// Returns `true` if the subscription was found and removed, `false` otherwise.
pub fn drop_named_subscription(name: &str) -> bool {
    let deleted: i64 = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.subscriptions WHERE name = $1 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_subscription SPI error: {e}"))
    .unwrap_or(0);

    deleted > 0
}

/// List all named CDC subscriptions.
pub fn list_named_subscriptions() -> pgrx::iter::TableIterator<
    'static,
    (
        pgrx::name!(name, String),
        pgrx::name!(filter_sparql, Option<String>),
        pgrx::name!(filter_shape, Option<String>),
        pgrx::name!(created_at, pgrx::datum::TimestampWithTimeZone),
    ),
> {
    let mut rows: Vec<(
        String,
        Option<String>,
        Option<String>,
        pgrx::datum::TimestampWithTimeZone,
    )> = Vec::new();

    Spi::connect(|c| {
        let result = c.select(
            "SELECT name, filter_sparql, filter_shape, created_at \
             FROM _pg_ripple.subscriptions ORDER BY name",
            None,
            &[],
        );
        if let Ok(iter) = result {
            for row in iter {
                let name: String = row.get(1).ok().flatten().unwrap_or_default();
                let fs: Option<String> = row.get(2).ok().flatten();
                let fsh: Option<String> = row.get(3).ok().flatten();
                let ca: pgrx::datum::TimestampWithTimeZone =
                    row.get(4).ok().flatten().unwrap_or_else(|| {
                        // SAFETY: 0 is the PostgreSQL epoch (2000-01-01 00:00:00 UTC),
                        // a valid TimestampWithTimeZone value.
                        pgrx::datum::TimestampWithTimeZone::try_from(0i64 as pg_sys::TimestampTz)
                            .unwrap_or_else(|_| {
                                pgrx::datum::TimestampWithTimeZone::positive_infinity()
                            })
                    });
                rows.push((name, fs, fsh, ca));
            }
        }
    });

    pgrx::iter::TableIterator::new(rows)
}

/// Notify all active subscriptions matching a triple change.
///
/// Called from the trigger function (via `notify_named_subscription`) after INSERT/DELETE.
/// For each matching named subscription, emits `NOTIFY pg_ripple_cdc_{name}` with
/// a JSON payload: `{"op": "add"|"remove", "s": "...", "p": "...", "o": "...", "g": "..."}`.
#[allow(dead_code)]
pub fn notify_named_subscriptions(op: &str, s: i64, p: i64, o: i64, g: i64) {
    // Decode IDs to N-Triples format for the human-readable payload.
    let s_str = crate::dictionary::decode(s).unwrap_or_else(|| format!("_:{s}"));
    let p_str = crate::dictionary::decode(p).unwrap_or_else(|| format!("_:{p}"));
    let o_str = crate::dictionary::decode(o).unwrap_or_else(|| format!("_:{o}"));
    let g_str = if g == 0 {
        "".to_owned()
    } else {
        crate::dictionary::decode(g).unwrap_or_else(|| format!("_:{g}"))
    };

    let payload = format!(
        r#"{{"op":"{op}","s":{s_q},"p":{p_q},"o":{o_q},"g":{g_q}}}"#,
        s_q = serde_json::to_string(&s_str).unwrap_or_else(|_| format!("\"{}\"", s_str)),
        p_q = serde_json::to_string(&p_str).unwrap_or_else(|_| format!("\"{}\"", p_str)),
        o_q = serde_json::to_string(&o_str).unwrap_or_else(|_| format!("\"{}\"", o_str)),
        g_q = serde_json::to_string(&g_str).unwrap_or_else(|_| format!("\"{}\"", g_str)),
    );

    // Get all subscription names (filter_sparql / filter_shape processing is
    // deferred to the subscriber side in this v0.42.0 implementation).
    let names: Vec<String> = Spi::connect(|c| {
        c.select("SELECT name FROM _pg_ripple.subscriptions", None, &[])
            .unwrap_or_else(|e| pgrx::error!("notify_named_subscriptions SPI error: {e}"))
            .filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect()
    });

    for name in names {
        let channel = format!("pg_ripple_cdc_{name}");
        let _ = Spi::run_with_args(
            "SELECT pg_notify($1, $2)",
            &[
                DatumWithOid::from(channel.as_str()),
                DatumWithOid::from(payload.as_str()),
            ],
        );
    }
}
