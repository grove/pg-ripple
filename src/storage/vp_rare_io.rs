//! VP-table I/O helpers -- extracted from storage/mod.rs (MOD-01, v0.72.0).
//!
//! Low-level helpers for VP-rare consolidation table and VP-table scan.

use crate::dictionary;
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

/// Check if a dedicated VP table exists for `predicate_id`.
pub(crate) fn get_dedicated_vp_table(predicate_id: i64) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(predicate_id)],
    )
    .unwrap_or(None)
}

/// Insert a row into `_pg_ripple.vp_rare` and update the predicate count.
/// Returns the SID, or 0 if the quad already exists (duplicate no-op).
pub(crate) fn insert_into_vp_rare(p_id: i64, s_id: i64, o_id: i64, g: i64) -> i64 {
    // Use ON CONFLICT DO NOTHING for set semantics — vp_rare has a UNIQUE(p,s,o,g)
    // constraint (added in v0.44.0) so duplicate quads are silently skipped.
    // pgrx's get_one_with_args returns Err (not Ok(None)) when ON CONFLICT DO NOTHING
    // fires and no row is returned, so we match explicitly on Ok/Err.
    let sid = match Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (p, s, o, g) DO NOTHING RETURNING i",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    ) {
        Ok(Some(val)) => val,
        Ok(None) | Err(_) => 0, // ON CONFLICT fired — row already exists, skip
    };

    if sid == 0 {
        // Duplicate triple — UNIQUE constraint fired, no row inserted.
        return 0;
    }

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
         VALUES ($1, NULL, 1) \
         ON CONFLICT (id) DO UPDATE \
         SET triple_count = _pg_ripple.predicates.triple_count + 1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate count upsert SPI error: {e}"));

    sid
}

/// n-distinct and dependencies statistics on `(s, o)`.
pub(crate) fn create_extended_statistics(pred_id: i64) {
    let stats_name = format!("ext_stats_vp_{pred_id}");
    let delta_table = format!("_pg_ripple.vp_{pred_id}_delta");

    // Create extended statistics if not already present.
    let create_sql = format!(
        "CREATE STATISTICS IF NOT EXISTS _pg_ripple.{stats_name} \
         (ndistinct, dependencies) ON s, o FROM {delta_table}"
    );
    Spi::run_with_args(&create_sql, &[])
        .unwrap_or_else(|e| pgrx::warning!("extended stats creation for vp_{pred_id}: {e}"));
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Scan a single VP table and decode results to text tuples.
pub(crate) fn scan_vp_table(
    table: &str,
    p_id: i64,
    s_id: Option<i64>,
    o_id: Option<i64>,
    g: i64,
) -> Vec<(String, String, String, String)> {
    let mut conditions = vec!["g = $3".to_string()];
    if s_id.is_some() {
        conditions.push("s = $1".to_string());
    }
    if o_id.is_some() {
        conditions.push("o = $2".to_string());
    }
    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let sql = format!("SELECT s, o FROM {table} {where_clause}");

    let p_str = dictionary::format_ntriples(p_id);
    let g_str = if g == 0 {
        String::new()
    } else {
        dictionary::format_ntriples(g)
    };

    Spi::connect(|c| {
        c.select(
            &sql,
            None,
            &[
                DatumWithOid::from(s_id.unwrap_or(0)),
                DatumWithOid::from(o_id.unwrap_or(0)),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("VP table scan SPI error: {e}"))
        .filter_map(|row| {
            let s_val: Option<i64> = row.get(1).ok().flatten();
            let o_val: Option<i64> = row.get(2).ok().flatten();
            if let (Some(s_enc), Some(o_enc)) = (s_val, o_val) {
                let s_str = dictionary::format_ntriples(s_enc);
                let o_str = dictionary::format_ntriples(o_enc);
                Some((s_str, p_str.clone(), o_str, g_str.clone()))
            } else {
                None
            }
        })
        .collect()
    })
}

/// Scan vp_rare with optional predicate, subject, object, graph filters.
pub(crate) fn scan_vp_rare(
    p_id: Option<i64>,
    s_id: Option<i64>,
    o_id: Option<i64>,
    g: i64,
) -> Vec<(String, String, String, String)> {
    let mut conditions = vec!["g = $4".to_string()];
    if p_id.is_some() {
        conditions.push("p = $1".to_string());
    }
    if s_id.is_some() {
        conditions.push("s = $2".to_string());
    }
    if o_id.is_some() {
        conditions.push("o = $3".to_string());
    }
    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    let sql = format!("SELECT p, s, o FROM _pg_ripple.vp_rare {where_clause}");

    let g_str = if g == 0 {
        String::new()
    } else {
        dictionary::format_ntriples(g)
    };

    Spi::connect(|c| {
        c.select(
            &sql,
            None,
            &[
                DatumWithOid::from(p_id.unwrap_or(0)),
                DatumWithOid::from(s_id.unwrap_or(0)),
                DatumWithOid::from(o_id.unwrap_or(0)),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("vp_rare scan SPI error: {e}"))
        .filter_map(|row| {
            let p_val: Option<i64> = row.get(1).ok().flatten();
            let s_val: Option<i64> = row.get(2).ok().flatten();
            let o_val: Option<i64> = row.get(3).ok().flatten();
            if let (Some(pe), Some(se), Some(oe)) = (p_val, s_val, o_val) {
                Some((
                    dictionary::format_ntriples(se),
                    dictionary::format_ntriples(pe),
                    dictionary::format_ntriples(oe),
                    g_str.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
    })
}

// ─── Named Graph Management ───────────────────────────────────────────────────
