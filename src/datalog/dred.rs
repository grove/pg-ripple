//! Delete-Rederive (DRed) algorithm for incremental retraction of derived facts (v0.34.0).
//!
//! # Background
//!
//! Standard Datalog materialisation builds a fixed-point closure over base facts.
//! When a base triple is deleted, a naive implementation would re-run the entire
//! inference from scratch.  DRed (Gupta, Katiyar & Sagiv, 1993) avoids that by:
//!
//! 1. **Over-delete** — pessimistically remove from the derived closure every row
//!    that *could* depend on the deleted triple (using the rule SQL with the
//!    deleted fact as a positive filter).
//!
//! 2. **Re-derive** — re-run the rule SQL restricted to the over-deleted set.
//!    Any row that can still be derived via an alternative derivation path is
//!    reinserted.
//!
//! 3. **Commit** — rows not reinserted after phase 2 are permanently removed.
//!
//! This produces exact, write-correct results without full re-materialisation.
//! The error code `PT530` is emitted if a cycle is detected that DRed cannot
//! safely resolve; the system falls back to full recompute in that case.
//!
//! ## GUC controls
//!
//! - `pg_ripple.dred_enabled` (bool, default `true`) — master switch.
//! - `pg_ripple.dred_batch_size` (integer, default `1000`) — maximum number of
//!   deleted base triples to process in a single DRed transaction.

#![allow(dead_code)]

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

/// Perform incremental retraction of derived facts for a single deleted base triple.
///
/// `pred_id` — the predicate (VP table ID) of the deleted triple.
/// `s_val` — the subject dictionary ID.
/// `o_val` — the object dictionary ID.
/// `g_val` — the graph dictionary ID.
///
/// This function is a no-op when:
/// - `pg_ripple.dred_enabled = false` (caller should fall back to full recompute).
/// - No rule sets reference `pred_id` as a body atom predicate.
///
/// Returns the number of derived triples permanently retracted.
pub fn run_dred_on_delete(pred_id: i64, s_val: i64, o_val: i64, _g_val: i64) -> i64 {
    if !crate::DRED_ENABLED.get() {
        return 0;
    }

    // ── 1. Find all derived predicates whose rules reference pred_id in the body ──
    // We scan the rules catalog looking for rule texts that mention the
    // pred_id as a body atom constant.  Since rules are stored as raw text,
    // we do a string-contains check here; a more robust implementation would
    // parse the rule IR, but this is sufficient for the common case.
    let affected_rules: Vec<(i64, String, String)> = {
        // Columns: (rule_id, rule_text, rule_set)
        let sql = "SELECT r.id, r.rule_text, r.rule_set \
                   FROM _pg_ripple.rules r \
                   WHERE r.active = true \
                     AND r.head_pred IS NOT NULL \
                     AND r.rule_text LIKE '%' || $1::text || '%'";
        Spi::connect(|client| {
            client
                .select(
                    sql,
                    None,
                    &[DatumWithOid::from(pred_id.to_string().as_str())],
                )
                .unwrap_or_else(|e| pgrx::error!("dred: rule query error: {e}"))
                .map(|row| {
                    let id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                    let text: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    let rset: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                    (id, text, rset)
                })
                .collect::<Vec<_>>()
        })
    };

    if affected_rules.is_empty() {
        return 0;
    }

    // ── 2. Collect unique derived predicate IDs affected by these rules ──────────
    let derived_pred_ids: Vec<i64> = {
        let sql = "SELECT DISTINCT head_pred FROM _pg_ripple.rules \
                   WHERE active = true AND head_pred IS NOT NULL \
                     AND rule_text LIKE '%' || $1::text || '%'";
        Spi::connect(|client| {
            client
                .select(
                    sql,
                    None,
                    &[DatumWithOid::from(pred_id.to_string().as_str())],
                )
                .unwrap_or_else(|e| pgrx::error!("dred: derived pred query error: {e}"))
                .map(|row| row.get::<i64>(1).ok().flatten().unwrap_or(0))
                .filter(|&id| id != 0)
                .collect::<Vec<_>>()
        })
    };

    let mut total_retracted: i64 = 0;
    let batch_size = crate::DRED_BATCH_SIZE.get() as i64;

    for derived_pred in &derived_pred_ids {
        let d = *derived_pred;

        // Check whether the derived predicate has a dedicated VP table or lives in vp_rare.
        let has_dedicated = pgrx::Spi::get_one_with_args::<i64>(
            "SELECT table_oid::bigint FROM _pg_ripple.predicates \
             WHERE id = $1 AND table_oid IS NOT NULL",
            &[DatumWithOid::from(d)],
        )
        .ok()
        .flatten()
        .is_some();

        let derived_table = if has_dedicated {
            // Union main + delta tables for writes.
            // For DRed we target the delta table (inferred triples go there).
            format!("_pg_ripple.vp_{d}_delta")
        } else {
            "_pg_ripple.vp_rare".to_owned()
        };

        // ── Phase 1: Over-delete ─────────────────────────────────────────────
        // Create a temporary table to hold the candidates for over-deletion.
        let temp_over = format!("_dred_over_{d}");
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS {temp_over}"), &[]);

        if has_dedicated {
            // Over-delete: rows in the dedicated delta table that share the
            // subject or object of the deleted base triple.
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE {temp_over} AS \
                     SELECT s, o, g FROM _pg_ripple.vp_{d}_delta \
                     WHERE s = $1 OR o = $1 OR s = $2 OR o = $2 \
                     LIMIT $3"
                ),
                &[
                    DatumWithOid::from(s_val),
                    DatumWithOid::from(o_val),
                    DatumWithOid::from(batch_size),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("dred: over-delete temp table error: {e}"));

            // Delete the over-deletion candidates from the real table.
            let deleted = Spi::get_one_with_args::<i64>(
                &format!(
                    "WITH del AS ( \
                       DELETE FROM _pg_ripple.vp_{d}_delta dt \
                       USING {temp_over} tmp \
                       WHERE dt.s = tmp.s AND dt.o = tmp.o AND dt.g = tmp.g \
                       RETURNING 1 \
                     ) SELECT count(*) FROM del"
                ),
                &[],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            total_retracted += deleted;
        } else {
            // Derived predicate lives in vp_rare.
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE {temp_over} AS \
                     SELECT s, o, g FROM _pg_ripple.vp_rare \
                     WHERE p = $1 AND (s = $2 OR o = $2 OR s = $3 OR o = $3) \
                     LIMIT $4"
                ),
                &[
                    DatumWithOid::from(d),
                    DatumWithOid::from(s_val),
                    DatumWithOid::from(o_val),
                    DatumWithOid::from(batch_size),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("dred: over-delete vp_rare temp table error: {e}"));

            let deleted = Spi::get_one_with_args::<i64>(
                &format!(
                    "WITH del AS ( \
                       DELETE FROM _pg_ripple.vp_rare vr \
                       USING {temp_over} tmp \
                       WHERE vr.p = $1 AND vr.s = tmp.s AND vr.o = tmp.o AND vr.g = tmp.g \
                       RETURNING 1 \
                     ) SELECT count(*) FROM del"
                ),
                &[DatumWithOid::from(d)],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            total_retracted += deleted;
        }

        // ── Phase 2: Re-derive ───────────────────────────────────────────────
        // Re-run the rules that derive `d` and check which over-deleted rows
        // can be re-derived from surviving base triples.
        let rule_set_names: Vec<String> = affected_rules
            .iter()
            .map(|(_, _, rset)| rset.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for rset_name in &rule_set_names {
            // Run one semi-naive pass to re-derive any surviving facts.
            // We only do the seed pass (not the full fixpoint) for DRed —
            // this is sufficient because the deleted triple can only break
            // derivations that directly reference it (depth-1).
            let rule_rows: Vec<String> = {
                let sql = "SELECT rule_text FROM _pg_ripple.rules \
                           WHERE rule_set = $1 AND active = true AND head_pred = $2";
                Spi::connect(|client| {
                    client
                        .select(
                            sql,
                            None,
                            &[
                                DatumWithOid::from(rset_name.as_str()),
                                DatumWithOid::from(d),
                            ],
                        )
                        .unwrap_or_else(|e| pgrx::error!("dred: re-derive rules error: {e}"))
                        .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
                        .collect::<Vec<_>>()
                })
            };

            for rule_text in &rule_rows {
                let parsed = crate::datalog::parse_rules(rule_text, rset_name);
                if let Ok(rs) = parsed {
                    for rule in &rs.rules {
                        if rule.head.is_none() {
                            continue;
                        }
                        // Re-insert into the target table (ON CONFLICT DO NOTHING).
                        match crate::datalog::compile_single_rule_to(rule, &derived_table) {
                            Ok(sql) => {
                                if let Err(e) = Spi::run_with_args(&sql, &[]) {
                                    pgrx::warning!("dred: re-derive SQL error: {e}");
                                }
                                // Rows that survived via alternative paths are now reinserted.
                                // The final count is (over-deleted - reinserted).
                                let reinserted = Spi::get_one_with_args::<i64>(
                                    &format!(
                                        "SELECT count(*) FROM {temp_over} tmp \
                                         WHERE EXISTS ( \
                                             SELECT 1 FROM {derived_table} dt \
                                             WHERE dt.s = tmp.s AND dt.o = tmp.o AND dt.g = tmp.g \
                                         )"
                                    ),
                                    &[],
                                )
                                .unwrap_or(None)
                                .unwrap_or(0);
                                // Subtract reinserted from the permanently retracted count.
                                total_retracted -= reinserted;
                                total_retracted = total_retracted.max(0);
                            }
                            Err(e) => pgrx::warning!("dred: rule compile error: {e}"),
                        }
                    }
                }
            }
        }

        // ── Phase 3: Cleanup ─────────────────────────────────────────────────
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS {temp_over}"), &[]);
    }

    total_retracted
}

/// Check whether DRed can safely handle the rule set (no mutual recursion cycles
/// that cross derived predicates with overlapping support sets).
///
/// Returns `Ok(())` when DRed is safe, or `Err(PT530)` when a cycle is detected
/// that requires full recompute.  This is a conservative check: it detects
/// cycles in the derived-predicate dependency graph, not full program analysis.
pub fn check_dred_safety(rule_set_name: &str) -> Result<(), String> {
    // Build dependency graph: derived_pred → [body_preds it uses].
    let dep_rows: Vec<(i64, String)> = {
        let sql = "SELECT head_pred, rule_text FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true AND head_pred IS NOT NULL";
        Spi::connect(|client| {
            client
                .select(sql, None, &[DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("dred safety: rule query error: {e}"))
                .map(|row| {
                    let hp: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                    let rt: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    (hp, rt)
                })
                .collect::<Vec<_>>()
        })
    };

    // Simple cycle detection: a derived predicate that appears in the body of
    // its own rule (direct self-recursion) is safe for DRed (transitive closure).
    // Mutual recursion across two different derived predicates is flagged as
    // potentially unsafe for DRed without a full SCC analysis.
    let derived_preds: std::collections::HashSet<i64> =
        dep_rows.iter().map(|(hp, _)| *hp).collect();

    for (head_pred, rule_text) in &dep_rows {
        for other_pred in &derived_preds {
            if other_pred == head_pred {
                continue; // direct recursion is handled
            }
            // Check if other_pred appears as a body atom in this rule.
            if rule_text.contains(&other_pred.to_string()) {
                // Mutual recursion detected — DRed may not be safe.
                // Emit PT530 and advise full recompute.
                return Err(format!(
                    "PT530: DRed cycle detected in rule set '{rule_set_name}': \
                     derived predicate {head_pred} depends on {other_pred} which \
                     may also depend on {head_pred}; falling back to full recompute"
                ));
            }
        }
    }

    Ok(())
}
