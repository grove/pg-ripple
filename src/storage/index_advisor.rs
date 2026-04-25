//! Adaptive Index Selection for pg_ripple v0.57.0.
//!
//! The index advisor background worker monitors per-predicate query patterns
//! from `_pg_ripple.predicate_stats` and automatically creates additional
//! B-tree indices when a predicate sees more than 40% object-leading lookups.
//!
//! # GUC
//!
//! `pg_ripple.adaptive_indexing_enabled` (bool, default off) — enable this module.

#![allow(dead_code)]

use pgrx::prelude::*;

/// Minimum object-bound query fraction (0.0–1.0) before adding an (o,s) index.
const OBJ_LEADING_THRESHOLD: f64 = 0.40;

/// Minimum total query count before the advisor makes any decisions.
const MIN_QUERY_COUNT: i64 = 100;

// ─── Core analysis function ───────────────────────────────────────────────────

/// Analyze query access patterns and create/drop indices as needed.
///
/// Called by the background merge worker when `adaptive_indexing_enabled = on`.
/// Records index creation events in `_pg_ripple.catalog_events`.
pub fn run_index_advisor_cycle() {
    if !crate::ADAPTIVE_INDEXING_ENABLED.get() {
        return;
    }

    // Ensure catalog_events table exists.
    let _ = Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.catalog_events ( \
           id BIGSERIAL PRIMARY KEY, \
           event_type TEXT NOT NULL, \
           predicate_id BIGINT, \
           details TEXT, \
           created_at TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    );

    // Query predicate stats to find high-obj-access predicates.
    let candidates: Vec<(i64, i64, i64)> = Spi::connect(|client| {
        let rows = client.select(
            "SELECT predicate_id, \
                    coalesce(obj_bound_count, 0)::bigint, \
                    coalesce(subj_bound_count, 0)::bigint \
             FROM _pg_ripple.predicate_workload_stats \
             WHERE coalesce(obj_bound_count, 0) + coalesce(subj_bound_count, 0) >= $1 \
             LIMIT 100",
            None,
            &[pgrx::datum::DatumWithOid::from(MIN_QUERY_COUNT)],
        )?;
        let mut v = Vec::new();
        for row in rows {
            let pred_id = row.get::<i64>(1)?.unwrap_or(0);
            let obj_count = row.get::<i64>(2)?.unwrap_or(0);
            let subj_count = row.get::<i64>(3)?.unwrap_or(0);
            if pred_id != 0 {
                v.push((pred_id, obj_count, subj_count));
            }
        }
        Ok::<_, pgrx::spi::Error>(v)
    })
    .unwrap_or_default();

    for (pred_id, obj_count, subj_count) in candidates {
        let total = obj_count + subj_count;
        if total < MIN_QUERY_COUNT {
            continue;
        }

        let obj_fraction = obj_count as f64 / total as f64;

        if obj_fraction > OBJ_LEADING_THRESHOLD {
            // Look up the VP table OID for this predicate.
            let table_oid: Option<i64> = Spi::get_one_with_args::<i64>(
                "SELECT table_oid FROM _pg_ripple.predicates WHERE id = $1",
                &[pgrx::datum::DatumWithOid::from(pred_id)],
            )
            .unwrap_or(None);

            if let Some(oid) = table_oid {
                create_obj_index_for_predicate(pred_id, oid, obj_fraction);
            }
        }
    }
}

fn create_obj_index_for_predicate(pred_id: i64, _table_oid: i64, obj_fraction: f64) {
    // Get the table name from the predicates catalog.
    let table_name: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT format('_pg_ripple.vp_%s_main', id::text) \
         FROM _pg_ripple.predicates WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None);

    let Some(tname) = table_name else { return };

    let idx_name = format!("vp_{pred_id}_main_o_s_adaptive");

    // Check if index already exists.
    let exists: bool = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE indexname = $1)",
        &[pgrx::datum::DatumWithOid::from(idx_name.as_str())],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if exists {
        return;
    }

    // Create the (o, s) index.
    let create_idx_sql = format!("CREATE INDEX IF NOT EXISTS {idx_name} ON {tname} (o, s)");

    let created = Spi::run_with_args(&create_idx_sql, &[]).is_ok();

    if created {
        // Record the event.
        let details = format!(
            "adaptive index created for predicate {pred_id}; obj_fraction={obj_fraction:.2}"
        );
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.catalog_events (event_type, predicate_id, details) \
             VALUES ('adaptive_index_created', $1, $2)",
            &[
                pgrx::datum::DatumWithOid::from(pred_id),
                pgrx::datum::DatumWithOid::from(details.as_str()),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obj_leading_threshold_value() {
        assert!(OBJ_LEADING_THRESHOLD > 0.0);
        assert!(OBJ_LEADING_THRESHOLD < 1.0);
    }
}
