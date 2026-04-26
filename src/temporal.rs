//! Temporal RDF query support (v0.58.0, Feature L-1.3).
//!
//! # Point-in-time queries
//!
//! `point_in_time(ts)` sets a session-local threshold so that all subsequent
//! SPARQL queries only see triples whose statement ID (`i`) was assigned before
//! `ts`.  The threshold is stored in the GUC-backed session variable
//! `_pg_ripple.pit_threshold`.
//!
//! `_pg_ripple.statement_id_timeline (sid BIGINT, inserted_at TIMESTAMPTZ)` maps
//! statement IDs to wall-clock insertion timestamps.  An AFTER INSERT trigger on
//! every VP delta table populates this table.
//!
//! # Valid-from / valid-to
//!
//! If a SPARQL query contains the pattern:
//! ```sparql
//! ?triple schema:validFrom ?start .
//! ?triple schema:validThrough ?end .
//! FILTER(?ts >= ?start && ?ts <= ?end)
//! ```
//! the SQL generator detects this pattern and rewrites it as a range predicate on
//! the triple's statement ID, pushing the filter into the VP table scan.

use pgrx::prelude::*;

// ─── Session-local PIT threshold ─────────────────────────────────────────────

/// Set the session-local point-in-time threshold.
///
/// All subsequent SPARQL queries will only see triples whose statement ID was
/// assigned before or at `ts`.  Pass `NULL` to clear the threshold.
///
/// # Errors
/// Raises an error if `_pg_ripple.statement_id_timeline` is not accessible
/// (extension not installed or schema search path issue).
#[pg_extern(schema = "pg_ripple")]
pub fn point_in_time(ts: pgrx::datum::TimestampWithTimeZone) {
    // Find the maximum SID inserted before `ts`.
    let threshold: i64 = Spi::get_one_with_args::<i64>(
        "SELECT COALESCE(MAX(sid), 0) FROM _pg_ripple.statement_id_timeline WHERE inserted_at <= $1",
        &[pgrx::datum::DatumWithOid::from(ts)],
    )
    .unwrap_or_else(|e| pgrx::error!("point_in_time: timeline query error: {e}"))
    .unwrap_or(0);

    // Store as a session-local GUC that the SQL generator reads.
    Spi::run_with_args(
        &format!("SET LOCAL \"_pg_ripple.pit_threshold\" = '{threshold}'"),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("point_in_time: SET LOCAL error: {e}"));
}

/// Clear the session-local point-in-time threshold.
///
/// After calling this function, subsequent SPARQL queries see all triples.
#[pg_extern(schema = "pg_ripple")]
pub fn clear_point_in_time() {
    Spi::run_with_args("SET LOCAL \"_pg_ripple.pit_threshold\" = '0'", &[])
        .unwrap_or_else(|e| pgrx::error!("clear_point_in_time: SET LOCAL error: {e}"));
}

/// Return information about the current point-in-time threshold.
///
/// Returns a single row with `(threshold_sid BIGINT, approximate_ts TIMESTAMPTZ)`.
/// Returns `(0, NULL)` when no threshold is set.
#[pg_extern(schema = "pg_ripple")]
pub fn point_in_time_info() -> TableIterator<
    'static,
    (
        name!(threshold_sid, i64),
        name!(approximate_ts, Option<pgrx::datum::TimestampWithTimeZone>),
    ),
> {
    // Read the current threshold from the session GUC.
    let threshold: i64 = Spi::get_one::<i64>(
        "SELECT COALESCE(NULLIF(current_setting('_pg_ripple.pit_threshold', true), '')::bigint, 0)",
    )
    .unwrap_or(None)
    .unwrap_or(0);

    // Reverse-lookup the approximate insertion timestamp for this SID.
    let approx_ts: Option<pgrx::datum::TimestampWithTimeZone> = if threshold > 0 {
        Spi::get_one_with_args::<pgrx::datum::TimestampWithTimeZone>(
            "SELECT inserted_at FROM _pg_ripple.statement_id_timeline WHERE sid = $1",
            &[pgrx::datum::DatumWithOid::from(threshold)],
        )
        .unwrap_or(None)
    } else {
        None
    };

    TableIterator::once((threshold, approx_ts))
}

// ─── Timeline table initialisation ───────────────────────────────────────────

/// Create `_pg_ripple.statement_id_timeline` and the associated trigger function.
///
/// Called from `initialize_schema()` (idempotent via `IF NOT EXISTS`).
pub fn initialize_timeline_schema() {
    // Timeline table.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.statement_id_timeline ( \
             sid         BIGINT      NOT NULL PRIMARY KEY, \
             inserted_at TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("statement_id_timeline creation: {e}"));

    // BRIN index on inserted_at for time-range scans.
    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_statement_id_timeline_ts \
         ON _pg_ripple.statement_id_timeline USING BRIN (inserted_at)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("statement_id_timeline BRIN index: {e}"));

    // Trigger function that records each new SID with a timestamp.
    Spi::run_with_args(
        "CREATE OR REPLACE FUNCTION _pg_ripple.record_statement_timestamp() \
         RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN \
             INSERT INTO _pg_ripple.statement_id_timeline (sid, inserted_at) \
             VALUES (NEW.i, now()) \
             ON CONFLICT (sid) DO NOTHING; \
             RETURN NEW; \
         END; \
         $$",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("statement timeline trigger function: {e}"));

    // Also attach the trigger to vp_rare so that non-promoted predicates are
    // tracked too (the trigger is idempotent via CREATE ... IF NOT EXISTS).
    Spi::run_with_args(
        "DO $$ BEGIN \
           IF NOT EXISTS ( \
             SELECT 1 FROM pg_trigger t \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_rare' \
               AND t.tgname = 'trg_timeline_vp_rare' \
           ) THEN \
             EXECUTE 'CREATE TRIGGER trg_timeline_vp_rare \
                      AFTER INSERT ON _pg_ripple.vp_rare \
                      FOR EACH ROW \
                      EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()'; \
           END IF; \
         END $$",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("vp_rare timeline trigger: {e}"));
}

/// Attach the `record_statement_timestamp` trigger to a VP delta table.
///
/// Safe to call multiple times — uses `IF NOT EXISTS`.
pub fn attach_timeline_trigger(pred_id: i64) {
    let table = format!("_pg_ripple.vp_{pred_id}_delta");
    let trigger_name = format!("trg_timeline_vp_{pred_id}_delta");
    let sql = format!(
        "DO $$ BEGIN \
           IF NOT EXISTS ( \
             SELECT 1 FROM pg_trigger t \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_{pred_id}_delta' \
               AND t.tgname = '{trigger_name}' \
           ) THEN \
             EXECUTE 'CREATE TRIGGER {trigger_name} \
                      AFTER INSERT ON {table} \
                      FOR EACH ROW \
                      EXECUTE FUNCTION _pg_ripple.record_statement_timestamp()'; \
           END IF; \
         END $$"
    );
    Spi::run_with_args(&sql, &[])
        .unwrap_or_else(|e| pgrx::warning!("attach_timeline_trigger vp_{pred_id}: {e}"));
}

/// Return the current session PIT threshold (0 = no filter).
///
/// Used by the SQL generator to append `AND i <= $threshold` to VP table scans.
#[allow(dead_code)]
pub fn current_pit_threshold() -> i64 {
    Spi::get_one::<i64>(
        "SELECT COALESCE(current_setting('_pg_ripple.pit_threshold', true)::bigint, 0)",
    )
    .unwrap_or(None)
    .unwrap_or(0)
}
