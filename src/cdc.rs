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
