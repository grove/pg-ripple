//! Delete-Rederive retraction — remove exclusively-owned triples when a rule is dropped.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

pub(super) fn retract_exclusive_triples(rule_name: &str) {
    // Collect (pred_id, s, o, g) tuples that only this rule owns.
    let exclusive: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT crt.pred_id, crt.s, crt.o, crt.g \
             FROM _pg_ripple.construct_rule_triples crt \
             WHERE crt.rule_name = $1 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM _pg_ripple.construct_rule_triples crt2 \
                   WHERE crt2.pred_id = crt.pred_id \
                     AND crt2.s = crt.s \
                     AND crt2.o = crt.o \
                     AND crt2.g = crt.g \
                     AND crt2.rule_name <> $1 \
               )",
            None,
            &[DatumWithOid::from(rule_name)],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let pred_id = row.get::<i64>(1).ok().flatten()?;
                let s = row.get::<i64>(2).ok().flatten()?;
                let o = row.get::<i64>(3).ok().flatten()?;
                let g = row.get::<i64>(4).ok().flatten()?;
                Some((pred_id, s, o, g))
            })
            .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    });

    for (pred_id, s, o, g) in exclusive {
        // Check if a promoted VP table exists.
        let has_table = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
              WHERE id = $1 AND table_oid IS NOT NULL)",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or(Some(false))
        .unwrap_or(false);

        if has_table {
            // CWB-FIX-03: HTAP-aware retraction.
            // Check if the predicate uses HTAP (delta + main + tombstones).
            let is_htap = crate::storage::merge::is_htap(pred_id);

            if is_htap {
                // Try delta first; tombstone main-resident rows.
                let delta = format!("_pg_ripple.vp_{pred_id}_delta");
                let tombs = format!("_pg_ripple.vp_{pred_id}_tombstones");

                let d = Spi::get_one_with_args::<i64>(
                    &format!(
                        "WITH d AS (DELETE FROM {delta} \
                         WHERE s=$1 AND o=$2 AND g=$3 AND source=1 \
                         RETURNING 1) \
                         SELECT count(*)::bigint FROM d"
                    ),
                    &[
                        DatumWithOid::from(s),
                        DatumWithOid::from(o),
                        DatumWithOid::from(g),
                    ],
                )
                .unwrap_or(Some(0))
                .unwrap_or(0);

                if d == 0 {
                    // Not in delta — tombstone from main.
                    Spi::run_with_args(
                        &format!(
                            "INSERT INTO {tombs} (s, o, g) \
                             SELECT s, o, g \
                             FROM _pg_ripple.vp_{pred_id}_main \
                             WHERE s=$1 AND o=$2 AND g=$3 AND source=1 \
                             ON CONFLICT DO NOTHING"
                        ),
                        &[
                            DatumWithOid::from(s),
                            DatumWithOid::from(o),
                            DatumWithOid::from(g),
                        ],
                    )
                    .unwrap_or_else(|e| pgrx::warning!("retract tombstone insert: {e}"));
                }
            } else {
                // Flat VP table — direct DELETE is correct (non-HTAP path).
                // RETRACT-PARAM-01 (v0.81.0): use parameterised query for WHERE
                // clause values; the table name is safe (integer pred_id).
                let table = format!("_pg_ripple.vp_{pred_id}");
                Spi::run_with_args(
                    &format!(
                        "DELETE FROM {table} \
                         WHERE s = $1 AND o = $2 AND g = $3 AND source = 1"
                    ),
                    &[
                        DatumWithOid::from(s),
                        DatumWithOid::from(o),
                        DatumWithOid::from(g),
                    ],
                )
                .unwrap_or_else(|e| pgrx::warning!("retract VP: {e}"));
            }
        } else {
            // vp_rare is always a flat table — direct DELETE is correct.
            Spi::run_with_args(
                "DELETE FROM _pg_ripple.vp_rare \
                 WHERE p = $1 AND s = $2 AND o = $3 AND g = $4 AND source = 1",
                &[
                    DatumWithOid::from(pred_id),
                    DatumWithOid::from(s),
                    DatumWithOid::from(o),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("retract vp_rare: {e}"));
        }
    }
    // CONF-GC-01b: purge confidence rows for inferred triples that were just retracted.
    // We do this as a deferred best-effort sweep rather than per-row because
    // we don't carry SIDs through this hot path.  The vacuum_confidence() API
    // function provides on-demand cleanup.
}
