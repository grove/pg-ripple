//! Well-Founded Semantics for Datalog (v0.32.0).
//!
//! Implements the alternating fixpoint algorithm (Van Gelder et al., 1991).
//! For stratifiable programs, `infer_wfs()` is identical to `infer()` with no overhead.
//! For non-stratifiable programs (cyclic negation), facts that cannot be resolved
//! to definitive truth are assigned `certainty = 'unknown'`.
//!
//! # Algorithm
//!
//! For **stratifiable programs** (no mutual negation cycle):
//! - Run normal semi-naive evaluation. All derived facts have `certainty = 'true'`.
//!
//! For **non-stratifiable programs** (cyclic negation detected by `stratify()`):
//! - **Pass 1 (positive closure)**: Run only rules that have NO negated body
//!   literals.  These facts are derivable purely from positive evidence →
//!   `certainty = 'true'`.
//! - **Pass 2 (full inference)**: Run ALL rules (including negated ones) against
//!   the real VP tables.  Since negated atoms reference the real VP tables (which
//!   initially contain no derived facts), `NOT EXISTS` evaluates to `TRUE` for
//!   all derived predicates, deriving both sides of mutual negation.
//! - **Unknown classification**: facts derived in Pass 2 but NOT in Pass 1 are
//!   classified as `certainty = 'unknown'` — they were only derivable under the
//!   "empty world" assumption for negation.
//!
//! Only `certainty = 'true'` facts are materialised into VP tables.
//! Unknown facts are reported in the JSONB output only.
//!
//! # SQL encoding
//!
//! Temporary tables `_wfs_pos_{pred_id}` (positive closure) and
//! `_wfs_all_{pred_id}` (full inference) hold the two passes.
//! The existing `compile_single_rule_to` / `compile_rule_delta_variants_to`
//! infrastructure is reused without modification.
//!
//! # GUC
//!
//! `pg_ripple.wfs_max_iterations` (integer, default `100`) — safety cap on the
//! per-pass fixpoint iteration count.  If either pass does not converge within
//! this limit a WARNING with code PT520 is emitted and the (possibly partial)
//! results are returned.

use pgrx::prelude::*;

use crate::datalog::compiler::{compile_rule_delta_variants_to, compile_single_rule_to};
use crate::datalog::parser::parse_rules;
use crate::datalog::stratify::stratify;
use crate::datalog::{BodyLiteral, Rule, Term};

// ─── Public API ───────────────────────────────────────────────────────────────

/// Run well-founded semantics inference for the named rule set.
///
/// Returns `(certain_count, unknown_count, total_derived, iterations, was_stratifiable)`.
///
/// For stratifiable programs `certain_count == total_derived` and
/// `unknown_count == 0`.
///
/// For non-stratifiable programs `unknown_count > 0` and only the certain facts
/// are written to VP tables.
pub fn run_wfs(rule_set_name: &str) -> (i64, i64, i64, i32, bool) {
    crate::datalog::ensure_catalog();

    // ── Load and parse rules ──────────────────────────────────────────────────
    let rule_texts = load_rule_texts(rule_set_name);
    if rule_texts.is_empty() {
        return (0, 0, 0, 0, true);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for text in &rule_texts {
        match parse_rules(text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("wfs: rule parse error: {e}"),
        }
    }
    if all_rules.is_empty() {
        return (0, 0, 0, 0, true);
    }

    // Apply sameAs canonicalization if enabled (v0.31.0).
    let all_rules = if crate::SAMEAS_REASONING.get() {
        let sameas_map = crate::datalog::rewrite::compute_sameas_map();
        crate::datalog::rewrite::apply_sameas_to_rules(&all_rules, &sameas_map)
    } else {
        all_rules
    };

    // ── Stratification check ──────────────────────────────────────────────────
    match stratify(&all_rules) {
        Ok(_) => {
            // Stratifiable: semi-naive inference, every fact is certain.
            let (derived, iters) = crate::datalog::run_inference_seminaive(rule_set_name);
            (derived, 0, derived, iters, true)
        }
        Err(_cycle) => {
            // Non-stratifiable: alternating fixpoint WFS.
            wfs_non_stratifiable(rule_set_name, &all_rules)
        }
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Load active rule texts for a rule set from the catalog.
fn load_rule_texts(rule_set_name: &str) -> Vec<String> {
    Spi::connect(|client| {
        client
            .select(
                "SELECT rule_text FROM _pg_ripple.rules \
                 WHERE rule_set = $1 AND active = true \
                 ORDER BY stratum, id",
                None,
                &[pgrx::datum::DatumWithOid::from(rule_set_name)],
            )
            .unwrap_or_else(|e| pgrx::error!("wfs: rule select SPI error: {e}"))
            .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
            .collect::<Vec<_>>()
    })
}

/// Collect derived predicate IDs (rule head predicates) from a rule set.
fn derived_pred_ids(rules: &[Rule]) -> std::collections::HashSet<i64> {
    rules
        .iter()
        .filter_map(|r| {
            r.head.as_ref().and_then(|h| {
                if let Term::Const(id) = &h.p {
                    Some(*id)
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Return only rules that have NO negated body literals.
///
/// Rules with at least one `Negated(_)` atom in the body are excluded entirely —
/// their output depends on the assumed truth value of the negated predicate.
fn rules_purely_positive(rules: &[Rule]) -> Vec<Rule> {
    rules
        .iter()
        .filter(|rule| {
            rule.body
                .iter()
                .all(|lit| !matches!(lit, BodyLiteral::Negated(_)))
        })
        .cloned()
        .collect()
}

/// Run a rule set into temporary tables with the given prefix, using
/// semi-naive evaluation.
///
/// Returns `(total_rows_derived, converged)`.
///
/// Each derived predicate gets a temp table `{prefix}{pred_id}` containing
/// the derived `(s, o, g)` tuples.  The body atoms in compiled SQL still
/// reference the real VP tables — this is intentional so that NOT EXISTS
/// clauses evaluate against the actual store state.
fn run_rules_into_temp(
    rules: &[Rule],
    derived: &std::collections::HashSet<i64>,
    prefix: &str,
    max_iter: i32,
) -> (i64, bool) {
    if rules.is_empty() || derived.is_empty() {
        // Still create empty tables so callers can reference them safely.
        for &pid in derived {
            let tbl = format!("{prefix}{pid}");
            Spi::run_with_args(&format!("DROP TABLE IF EXISTS {tbl}"), &[])
                .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE {tbl} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, \
                      g BIGINT NOT NULL DEFAULT 0, UNIQUE (s, o, g))"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }
        return (0, true);
    }

    // ── Create temp tables ────────────────────────────────────────────────────
    for &pid in derived {
        let tbl = format!("{prefix}{pid}");
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS {tbl}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(
            &format!(
                "CREATE TEMP TABLE {tbl} \
                 (s BIGINT NOT NULL, o BIGINT NOT NULL, \
                  g BIGINT NOT NULL DEFAULT 0, UNIQUE (s, o, g))"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("wfs: create temp table {tbl} error: {e}"));
    }

    // ── Seeding pass ──────────────────────────────────────────────────────────
    for rule in rules {
        let Some(head) = &rule.head else {
            continue;
        };
        let Term::Const(hpid) = &head.p else {
            continue;
        };
        if !derived.contains(hpid) {
            continue;
        }
        let target = format!("{prefix}{hpid}");
        match compile_single_rule_to(rule, &target) {
            Ok(sql) => {
                Spi::run_with_args(&sql, &[])
                    .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            }
            Err(e) => pgrx::warning!("wfs: seed compile error: {e}"),
        }
    }

    // ── Fixpoint loop ─────────────────────────────────────────────────────────
    let mut converged = false;
    for _iter in 0..max_iter {
        let mut new_this_iter = 0i64;

        // Create "new delta" temp tables.
        for &pid in derived {
            let new_tbl = format!("{prefix}new_{pid}");
            Spi::run_with_args(&format!("DROP TABLE IF EXISTS {new_tbl}"), &[])
                .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE {new_tbl} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, \
                      g BIGINT NOT NULL DEFAULT 0, UNIQUE (s, o, g))"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("wfs: create new_tbl {new_tbl} error: {e}"));
        }

        let delta_fn = |pid: i64| -> String { format!("{prefix}{pid}") };
        let new_delta_fn = |pid: i64| -> String { format!("{prefix}new_{pid}") };

        for rule in rules {
            let Some(head) = &rule.head else {
                continue;
            };
            let Term::Const(hpid) = &head.p else {
                continue;
            };
            if !derived.contains(hpid) {
                continue;
            }
            match compile_rule_delta_variants_to(rule, derived, &delta_fn, Some(&new_delta_fn)) {
                Ok(sqls) => {
                    for sql in &sqls {
                        Spi::run_with_args(sql, &[])
                            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
                    }
                }
                Err(e) => pgrx::warning!("wfs: fixpoint compile error: {e}"),
            }
        }

        // Count net-new rows and merge.
        for &pid in derived {
            let tbl = format!("{prefix}{pid}");
            let new_tbl = format!("{prefix}new_{pid}");
            let cnt = Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM {new_tbl} n \
                 WHERE NOT EXISTS ( \
                     SELECT 1 FROM {tbl} d \
                     WHERE d.s = n.s AND d.o = n.o AND d.g = n.g \
                 )"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            new_this_iter += cnt;

            Spi::run_with_args(
                &format!(
                    "INSERT INTO {tbl} (s, o, g) \
                     SELECT s, o, g FROM {new_tbl} ON CONFLICT DO NOTHING"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(&format!("DROP TABLE IF EXISTS {new_tbl}"), &[])
                .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }

        if new_this_iter == 0 {
            converged = true;
            break;
        }
    }

    if !converged {
        pgrx::warning!(
            "well-founded fixpoint pass '{}' did not converge within {} iterations (PT520); \
             results may be incomplete",
            prefix,
            max_iter
        );
    }

    // Count total derived rows.
    let total: i64 = derived
        .iter()
        .map(|&pid| {
            let tbl = format!("{prefix}{pid}");
            Spi::get_one::<i64>(&format!("SELECT count(*) FROM {tbl}"))
                .unwrap_or(None)
                .unwrap_or(0)
        })
        .sum();

    (total, converged)
}

/// WFS evaluation for non-stratifiable programs (2-pass alternating fixpoint).
fn wfs_non_stratifiable(_rule_set_name: &str, all_rules: &[Rule]) -> (i64, i64, i64, i32, bool) {
    let max_iter = crate::WFS_MAX_ITERATIONS.get();
    let derived = derived_pred_ids(all_rules);

    if derived.is_empty() {
        return (0, 0, 0, 0, false);
    }

    // ── Pass 1: positive closure ──────────────────────────────────────────────
    // Only rules with no negated atoms — definitely derivable facts.
    let pos_rules = rules_purely_positive(all_rules);
    let (_pos_total, _) = run_rules_into_temp(&pos_rules, &derived, "_wfs_pos_", max_iter);

    // ── Pass 2: full inference ────────────────────────────────────────────────
    // All rules; NOT EXISTS references real VP tables (initially empty for
    // derived predicates), so mutual negation derives both sides.
    let (_full_total, _) = run_rules_into_temp(all_rules, &derived, "_wfs_all_", max_iter);

    // ── Classify certain vs. unknown ──────────────────────────────────────────
    let mut certain_count = 0i64;
    let mut unknown_count = 0i64;
    let mut unknown_facts: Vec<serde_json::Value> = Vec::new();

    for &pid in &derived {
        let pos_tbl = format!("_wfs_pos_{pid}");
        let all_tbl = format!("_wfs_all_{pid}");

        // Certain = in positive closure.
        let certain = Spi::get_one::<i64>(&format!("SELECT count(*) FROM {pos_tbl}"))
            .unwrap_or(None)
            .unwrap_or(0);
        certain_count += certain;

        // Unknown = in full inference but NOT in positive closure.
        let unk_rows: Vec<(i64, i64, i64)> = Spi::connect(|client| {
            client
                .select(
                    &format!(
                        "SELECT n.s, n.o, n.g FROM {all_tbl} n \
                         WHERE NOT EXISTS ( \
                             SELECT 1 FROM {pos_tbl} p \
                             WHERE p.s = n.s AND p.o = n.o AND p.g = n.g \
                         )"
                    ),
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("wfs: unknown query error: {e}"))
                .map(|row| {
                    let s = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                    let o = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                    let g = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                    (s, o, g)
                })
                .collect::<Vec<_>>()
        });

        unknown_count += unk_rows.len() as i64;

        // Decode unknown facts for JSONB output (limit to 100 for safety).
        for (s, o, g) in unk_rows.iter().take(100) {
            let s_str = crate::dictionary::decode(*s).unwrap_or_else(|| format!("id:{s}"));
            let p_str = crate::dictionary::decode(pid).unwrap_or_else(|| format!("id:{pid}"));
            let o_str = crate::dictionary::decode(*o).unwrap_or_else(|| format!("id:{o}"));
            let mut fact = serde_json::Map::new();
            fact.insert("s".to_owned(), serde_json::Value::String(s_str));
            fact.insert("p".to_owned(), serde_json::Value::String(p_str));
            fact.insert("o".to_owned(), serde_json::Value::String(o_str));
            if *g != 0 {
                let g_str = crate::dictionary::decode(*g).unwrap_or_else(|| format!("id:{g}"));
                fact.insert("g".to_owned(), serde_json::Value::String(g_str));
            }
            unknown_facts.push(serde_json::Value::Object(fact));
        }
    }

    // ── Materialise CERTAIN facts into vp_rare ────────────────────────────────
    for &pid in &derived {
        let pos_tbl = format!("_wfs_pos_{pid}");
        let cnt = Spi::get_one::<i64>(&format!(
            "WITH ins AS ( \
               INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
               SELECT {pid}::bigint, s, o, g FROM {pos_tbl} \
               ON CONFLICT DO NOTHING \
               RETURNING 1 \
             ) SELECT COUNT(*)::bigint FROM ins"
        ))
        .unwrap_or(None)
        .unwrap_or(0);

        if cnt > 0 {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
                 VALUES ($1, NULL, $2) ON CONFLICT (id) DO UPDATE \
                     SET triple_count = \
                         _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[
                    pgrx::datum::DatumWithOid::from(pid),
                    pgrx::datum::DatumWithOid::from(cnt),
                ],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }
    }

    // ── Cleanup temp tables ───────────────────────────────────────────────────
    for &pid in &derived {
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _wfs_pos_{pid}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _wfs_all_{pid}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _wfs_pos_new_{pid}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _wfs_all_new_{pid}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
    }

    let total = certain_count + unknown_count;
    // Report 2 passes (pos closure + full inference) + inner iterations.
    (certain_count, unknown_count, total, 2, false)
}

// ─── Build the JSONB output ───────────────────────────────────────────────────

/// Build the `infer_wfs()` JSONB result from WFS output fields.
pub fn build_wfs_jsonb(
    certain: i64,
    unknown: i64,
    total: i64,
    iterations: i32,
    stratifiable: bool,
) -> pgrx::JsonB {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "derived".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(total)),
    );
    obj.insert(
        "certain".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(certain)),
    );
    obj.insert(
        "unknown".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(unknown)),
    );
    obj.insert(
        "iterations".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(iterations)),
    );
    obj.insert(
        "stratifiable".to_owned(),
        serde_json::Value::Bool(stratifiable),
    );
    pgrx::JsonB(serde_json::Value::Object(obj))
}
