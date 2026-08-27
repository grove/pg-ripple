/// Migrate an existing flat VP table (pre-v0.6.0) to the HTAP partition split.
///
/// Called automatically by the v0.5.1→v0.6.0 migration script, but can
/// also be called manually if needed.  The predicate is specified by its
/// dictionary integer ID.
#[pg_extern]
fn htap_migrate_predicate(pred_id: i64) {
    crate::storage::merge::migrate_flat_to_htap(pred_id);
}

/// Returns the estimated years remaining before `_pg_ripple.statement_id_seq`
/// wraps (i64::MAX ≈ 9.2 × 10^18).
///
/// Runway is computed as:
///   years_remaining = (max_value - current_value) / max(insert_rate_per_day, 1) / 365
///
/// `insert_rate_per_day` is estimated from the sequence's `last_value` divided by
/// the extension's installed age in days (read from `_pg_ripple.schema_version`).
/// Returns a single row; returns NULL for years_remaining if the rate cannot be determined.
#[pg_extern]
fn sid_runway() -> TableIterator<
    'static,
    (
        name!(current_value, i64),
        name!(max_value, i64),
        name!(insert_rate_per_day, i64),
        name!(years_remaining, Option<pgrx::AnyNumeric>),
    ),
> {
    let row = Spi::connect(|c| {
        // Get current sequence last_value.
        let current: i64 = c
            .select(
                "SELECT last_value FROM _pg_ripple.statement_id_seq",
                None,
                &[],
            )
            .ok()
            .and_then(|mut r| r.next())
            .and_then(|row| row.get::<i64>(1).ok().flatten())
            .unwrap_or(1);

        let max_val: i64 = i64::MAX;

        // Estimate daily insert rate from extension age.
        let days_installed: i64 = c
                .select(
                    "SELECT GREATEST(1, EXTRACT(EPOCH FROM (now() - MIN(installed_at))) / 86400)::bigint \
                     FROM _pg_ripple.schema_version",
                    None,
                    &[],
                )
                .ok()
                .and_then(|mut r| r.next())
                .and_then(|row| row.get::<i64>(1).ok().flatten())
                .unwrap_or(1);

        let rate_per_day: i64 = (current / days_installed).max(1);
        let remaining = max_val.saturating_sub(current);
        let years: Option<pgrx::AnyNumeric> = if rate_per_day > 0 {
            let years_f64 = (remaining as f64) / (rate_per_day as f64) / 365.0;
            let s = format!("{:.2}", years_f64);
            pgrx::AnyNumeric::try_from(s.as_str()).ok()
        } else {
            None
        };

        (current, max_val, rate_per_day, years)
    });

    TableIterator::new(vec![row])
}

/// Returns all rows from `_pg_ripple.audit_log` up to the configured limit.
///
/// Only meaningful when `pg_ripple.audit_log_enabled = on`.
#[pg_extern]
fn audit_log() -> TableIterator<
    'static,
    (
        name!(id, i64),
        name!(ts, pgrx::datum::TimestampWithTimeZone),
        name!(role, String),
        name!(txid, i64),
        name!(operation, String),
        name!(query, String),
    ),
> {
    let rows: Vec<(
        i64,
        pgrx::datum::TimestampWithTimeZone,
        String,
        i64,
        String,
        String,
    )> = Spi::connect(|c| {
        let results = c.select(
            "SELECT id, ts, role::text, txid, operation, query \
                     FROM _pg_ripple.audit_log ORDER BY id DESC LIMIT 10000",
            None,
            &[],
        );
        match results {
            Ok(tup) => {
                let mut out = Vec::new();
                for row in tup {
                    let id = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                    let ts = match row
                        .get::<pgrx::datum::TimestampWithTimeZone>(2)
                        .ok()
                        .flatten()
                    {
                        Some(t) => t,
                        None => continue,
                    };
                    let role = row.get::<&str>(3).ok().flatten().unwrap_or("").to_owned();
                    let txid = row.get::<i64>(4).ok().flatten().unwrap_or(0);
                    let op = row.get::<&str>(5).ok().flatten().unwrap_or("").to_owned();
                    let q = row.get::<&str>(6).ok().flatten().unwrap_or("").to_owned();
                    out.push((id, ts, role, txid, op, q));
                }
                out
            }
            Err(_) => vec![],
        }
    });
    TableIterator::new(rows)
}

/// Purge audit log entries older than `before`.
/// Returns the number of rows deleted.
#[pg_extern]
fn purge_audit_log(before: pgrx::datum::TimestampWithTimeZone) -> i64 {
    Spi::connect(|c| {
        c.select(
            "WITH del AS (DELETE FROM _pg_ripple.audit_log WHERE ts < $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM del",
            None,
            &[pgrx::datum::DatumWithOid::from(before)],
        )
        .ok()
        .and_then(|mut r| r.next())
        .and_then(|row| row.get::<i64>(1).ok().flatten())
        .unwrap_or(0)
    })
}

// ── R2RML Direct Mapping (v0.56.0 L-7.3) ─────────────────────────────────

/// Execute an R2RML mapping document that has already been loaded into the
/// triple store (e.g., via `pg_ripple.load_turtle()`).
///
/// Walks all `rr:TriplesMap` instances, queries the mapped PostgreSQL tables
/// via SPI, applies `rr:template`/`rr:column`/`rr:constant` rules, and
/// bulk-inserts the generated triples.
///
/// Returns the number of triples inserted.
///
/// ```sql
/// -- First load the mapping:
/// SELECT pg_ripple.load_turtle('<path_to_mapping.ttl>');
/// -- Then execute it:
/// SELECT pg_ripple.r2rml_load('http://example.org/mapping');
/// ```
#[pg_extern]
fn r2rml_load(mapping_iri: &str) -> i64 {
    crate::r2rml::r2rml_load(mapping_iri)
}

// ── VP Promotion Recovery (v0.81.0 PROMO-STUCK-01) ───────────────────────

/// Detect and recover VP table promotions that were abandoned mid-flight.
///
/// A promotion is considered "stuck" if `_pg_ripple.predicates` has a row
/// with `promotion_status = 'promoting'` and no backend currently holds the
/// corresponding per-predicate advisory lock (meaning the promoting backend
/// exited without completing the operation).
///
/// For each stuck promotion found, this function re-runs the promotion from
/// Phase 1 so the predicate ends up in its own HTAP VP table.
///
/// Returns the number of promotions recovered.
///
/// ```sql
/// SELECT pg_ripple.recover_stuck_promotions();
/// ```
#[pg_extern]
fn recover_stuck_promotions() -> i64 {
    // Find predicates whose promotion was started but never finished.
    // Use pg_try_advisory_lock to detect whether any session is actively
    // promoting: if we can acquire the lock, the original promoter is gone.
    let stuck_ids: Vec<i64> = pgrx::Spi::connect(|c| {
        c.select(
            "SELECT id \
                 FROM _pg_ripple.predicates \
                 WHERE promotion_status = 'promoting' \
                   AND pg_try_advisory_xact_lock(id) \
                 ORDER BY id",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("recover_stuck_promotions: query error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    let count = stuck_ids.len() as i64;
    for p_id in stuck_ids {
        pgrx::notice!(
            "pg_ripple.recover_stuck_promotions: recovering stuck promotion for predicate {p_id}"
        );
        crate::storage::promote::promote_predicate_pub(p_id);
    }

    if count > 0 {
        pgrx::log!(
            "pg_ripple.recover_stuck_promotions: recovered {} stuck promotion(s)",
            count
        );
    }
    count
}
