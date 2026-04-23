//! CDC → pg-trickle Outbox Bridge (v0.52.0).
//!
//! Provides:
//! - `enable_cdc_bridge_trigger` — install a per-predicate VP-delta trigger that
//!   writes decoded JSON-LD events to an outbox table within the same transaction.
//! - `disable_cdc_bridge_trigger` — drop the trigger.
//! - `cdc_bridge_triggers` — catalog SRF listing all active triggers.
//! - Bridge schema initialisation (`_pg_ripple.cdc_bridge_triggers` catalog).
//!
//! The optional background worker (`_pg_ripple.cdc_bridge_worker`) is registered
//! from `worker.rs`; the worker body reads from the CDC NOTIFY channel, performs
//! a bulk dictionary-decode SPI call, and batch-inserts JSON-LD events into the
//! configured outbox table.
//!
//! # Graceful degradation
//!
//! All bridge SQL functions gate on `crate::TRICKLE_INTEGRATION.get()` and on
//! `crate::has_pg_trickle()`.  When pg-trickle is absent (or integration is
//! disabled), the functions return the `PT800` error code:
//!
//! ```text
//! PT800: pg_trickle extension is not installed; install pg_trickle to use bridge features
//! ```

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── Error code ───────────────────────────────────────────────────────────────

/// Raise a user-facing error when pg-trickle is not available or integration
/// is disabled via GUC.
///
/// Uses SQLSTATE PT800 (custom class PT) to allow callers to catch this
/// specific error condition.
pub(crate) fn require_trickle(fn_name: &str) {
    // Custom SQLSTATE PT800: MAKE_SQLSTATE('P','T','8','0','0') = 35104
    // class PT = custom extension error class defined by pg_ripple.
    let pt800 =
        unsafe { std::mem::transmute::<i32, pgrx::pg_sys::errcodes::PgSqlErrorCode>(35104_i32) };
    if !crate::TRICKLE_INTEGRATION.get() {
        let msg = format!(
            "{fn_name}(): pg_ripple.trickle_integration is off; \
             set it to on to use bridge features"
        );
        pgrx::pg_sys::panic::ErrorReport::new(pt800, msg, "require_trickle")
            .report(pgrx::PgLogLevel::ERROR);
        unreachable!();
    }
    if !crate::has_pg_trickle() {
        let msg = format!(
            "{fn_name}(): pg_trickle extension is not installed; \
             install pg_trickle to use bridge features"
        );
        pgrx::pg_sys::panic::ErrorReport::new(pt800, msg, "require_trickle")
            .report(pgrx::PgLogLevel::ERROR);
        unreachable!();
    }
}

// ─── Schema initialisation ────────────────────────────────────────────────────

/// Create the `_pg_ripple.cdc_bridge_triggers` catalog table.
///
/// Called once from `storage::initialize_schema`.
pub fn initialize_cdc_bridge_schema() {
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_bridge_triggers ( \
             name         TEXT NOT NULL PRIMARY KEY, \
             predicate_id BIGINT NOT NULL, \
             outbox_table TEXT NOT NULL, \
             created_at   TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc_bridge_triggers table creation error: {e}"));

    // PL/pgSQL trigger function used by per-predicate CDC bridge triggers.
    // Encodes the new row as a JSON-LD object and inserts into the outbox table.
    // TG_ARGV[0] = predicate_id (bigint text), TG_ARGV[1] = outbox table name.
    Spi::run_with_args(
        r#"
CREATE OR REPLACE FUNCTION _pg_ripple.cdc_bridge_trigger_fn()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    pred_id    BIGINT  := TG_ARGV[0]::bigint;
    outbox_tbl TEXT    := TG_ARGV[1];
    s_iri      TEXT;
    p_iri      TEXT;
    o_iri      TEXT;
    payload    JSONB;
    dedup_key  TEXT;
    sid        BIGINT;
BEGIN
    -- Look up human-readable IRIs from the dictionary
    SELECT value INTO s_iri FROM _pg_ripple.dictionary WHERE id = NEW.s;
    SELECT value INTO p_iri FROM _pg_ripple.dictionary WHERE id = pred_id;
    SELECT value INTO o_iri FROM _pg_ripple.dictionary WHERE id = NEW.o;

    -- Get the statement id (SID) for the dedup key
    sid := NEW.i;
    dedup_key := 'ripple:' || sid::text;

    payload := jsonb_build_object(
        '@context',   'https://schema.org/',
        '@id',        COALESCE(s_iri, '_:' || NEW.s::text),
        p_iri,        COALESCE(o_iri, NEW.o::text),
        '_dedup_key', dedup_key
    );

    EXECUTE format(
        'INSERT INTO %I (event_id, payload) VALUES ($1, $2) ON CONFLICT DO NOTHING',
        outbox_tbl
    ) USING dedup_key, payload;

    RETURN NEW;
END;
$$"#,
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("cdc_bridge_trigger_fn creation error: {e}"));
}

// ─── enable_cdc_bridge_trigger ────────────────────────────────────────────────

/// Install a CDC bridge trigger on the VP delta table for `predicate`.
///
/// When a triple is inserted into the VP delta table for the given predicate,
/// the trigger decodes the (s, p, o) dictionary IDs and writes a JSON-LD event
/// to `outbox` in the same transaction.
///
/// # Errors
/// Raises `PT800` when pg-trickle is absent or `trickle_integration = off`.
/// Raises an ERROR when the predicate IRI is not in the dictionary.
pub fn enable_cdc_bridge_trigger(name: &str, predicate: &str, outbox: &str) {
    require_trickle("enable_cdc_bridge_trigger");

    // Validate name
    if name.is_empty() || name.len() > 63 {
        pgrx::error!("enable_cdc_bridge_trigger: name must be 1–63 characters");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        pgrx::error!(
            "enable_cdc_bridge_trigger: name must contain only ASCII letters, digits, and underscores"
        );
    }

    // Resolve predicate IRI → dictionary ID
    let pred_iri = if predicate.starts_with('<') && predicate.ends_with('>') {
        &predicate[1..predicate.len() - 1]
    } else {
        predicate
    };
    let pred_id = crate::dictionary::lookup_iri(pred_iri).unwrap_or_else(|| {
        pgrx::error!(
            "enable_cdc_bridge_trigger: predicate IRI not in dictionary: {}",
            pred_iri
        )
    });

    // Determine VP delta table name
    let delta_table = match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_)) => format!("_pg_ripple.vp_{pred_id}_delta"),
        Ok(None) => "_pg_ripple.vp_rare".to_string(),
        Err(e) => pgrx::error!("enable_cdc_bridge_trigger: predicate catalog error: {e}"),
    };

    // Install trigger
    let trigger_name = format!("cdc_bridge_{name}");
    let sql = format!(
        "CREATE TRIGGER {trigger_name} \
         AFTER INSERT ON {delta_table} \
         FOR EACH ROW EXECUTE FUNCTION _pg_ripple.cdc_bridge_trigger_fn({pred_id}, '{outbox}')"
    );
    Spi::run_with_args(&sql, &[])
        .unwrap_or_else(|e| pgrx::error!("enable_cdc_bridge_trigger: trigger install error: {e}"));

    // Record in catalog
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.cdc_bridge_triggers (name, predicate_id, outbox_table) \
         VALUES ($1, $2, $3) ON CONFLICT (name) DO UPDATE \
         SET predicate_id = EXCLUDED.predicate_id, outbox_table = EXCLUDED.outbox_table, \
             created_at = now()",
        &[
            DatumWithOid::from(name),
            DatumWithOid::from(pred_id),
            DatumWithOid::from(outbox),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("enable_cdc_bridge_trigger: catalog insert error: {e}"));
}

// ─── disable_cdc_bridge_trigger ───────────────────────────────────────────────

/// Drop a CDC bridge trigger previously installed by `enable_cdc_bridge_trigger`.
pub fn disable_cdc_bridge_trigger(name: &str) {
    // Look up predicate_id to find the right table
    let row = Spi::get_two_with_args::<i64, String>(
        "SELECT predicate_id, outbox_table FROM _pg_ripple.cdc_bridge_triggers WHERE name = $1",
        &[DatumWithOid::from(name)],
    );
    let pred_id = match row {
        Ok((Some(id), _)) => id,
        Ok((None, _)) => {
            pgrx::warning!("disable_cdc_bridge_trigger: trigger '{}' not found", name);
            return;
        }
        Err(e) => pgrx::error!("disable_cdc_bridge_trigger: catalog error: {e}"),
    };

    let delta_table = match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_)) => format!("_pg_ripple.vp_{pred_id}_delta"),
        Ok(None) => "_pg_ripple.vp_rare".to_owned(),
        Err(_) => format!("_pg_ripple.vp_{pred_id}_delta"),
    };

    let trigger_name = format!("cdc_bridge_{name}");
    let sql = format!("DROP TRIGGER IF EXISTS {trigger_name} ON {delta_table}");
    Spi::run_with_args(&sql, &[])
        .unwrap_or_else(|e| pgrx::error!("disable_cdc_bridge_trigger: {e}"));

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.cdc_bridge_triggers WHERE name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::error!("disable_cdc_bridge_trigger: catalog delete error: {e}"));
}

// ─── cdc_bridge_triggers SRF ─────────────────────────────────────────────────

/// Row type returned by the `cdc_bridge_triggers()` SRF.
pub struct CdcBridgeTriggerRow {
    /// User-supplied trigger name.
    pub name: String,
    /// Predicate IRI.
    pub predicate: String,
    /// Target outbox table.
    pub outbox: String,
    /// Whether the underlying PG trigger exists.
    pub active: bool,
}

/// List all registered CDC bridge triggers.
pub fn list_cdc_bridge_triggers() -> Vec<CdcBridgeTriggerRow> {
    let mut rows = Vec::new();
    let result = Spi::connect(|client| {
        let tup_table = client.select(
            "SELECT t.name, d.value AS predicate, t.outbox_table, \
             EXISTS( \
               SELECT 1 FROM pg_trigger pg \
               JOIN pg_class c ON c.oid = pg.tgrelid \
               WHERE pg.tgname = 'cdc_bridge_' || t.name \
             ) AS active \
             FROM _pg_ripple.cdc_bridge_triggers t \
             JOIN _pg_ripple.dictionary d ON d.id = t.predicate_id \
             ORDER BY t.name",
            None,
            &[],
        );
        match tup_table {
            Ok(table) => {
                for row in table {
                    let name: String = row["name"].value().unwrap_or(None).unwrap_or_default();
                    let predicate: String =
                        row["predicate"].value().unwrap_or(None).unwrap_or_default();
                    let outbox: String = row["outbox_table"]
                        .value()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let active: bool = row["active"].value().unwrap_or(None).unwrap_or(false);
                    rows.push(CdcBridgeTriggerRow {
                        name,
                        predicate,
                        outbox,
                        active,
                    });
                }
            }
            Err(e) => {
                pgrx::warning!("cdc_bridge_triggers: catalog query error: {e}");
            }
        }
        Ok::<(), pgrx::spi::Error>(())
    });
    if let Err(e) = result {
        pgrx::warning!("cdc_bridge_triggers: SPI connect error: {e}");
    }
    rows
}
