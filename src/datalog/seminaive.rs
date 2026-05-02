//! Semi-naive evaluation engine for Datalog inference.
//!
//! # Algorithm
//!
//! For each stratum S of the rule set:
//! 1. **Seed**: run all rules once against the full VP tables to get the first
//!    round of derived triples.  Store in `_dl_delta_{pred_id}` temp tables.
//! 2. **Fixpoint loop**: on each iteration, generate one SQL variant per body
//!    atom that references a derived predicate, using the delta table for that
//!    atom and full VP tables for all others.  Terminate when no new triples.
//! 3. Materialise derived triples into `vp_rare`.

use pgrx::prelude::*;

use super::{
    BodyLiteral, Rule, Term, check_subsumption, compile_rule_delta_variants_to, compile_rule_set,
    compile_single_rule_to, has_variable_pred, parse_rules, vp_read_expr_pub,
};

// ─── Main semi-naive inference entry point ───────────────────────────────────

/// Execute on-demand materialization using semi-naive evaluation.
/// Returns `(total_triples_derived, iteration_count)`.
pub fn run_inference_seminaive(rule_set_name: &str) -> (i64, i32) {
    super::ensure_catalog();

    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get() as usize;
    let sequence_batch = crate::DATALOG_SEQUENCE_BATCH.get();
    if parallel_workers > 1 {
        Spi::connect(|client| {
            let _ =
                super::parallel::preallocate_sid_ranges(client, parallel_workers, sequence_batch);
        });
    }

    let rule_rows: Vec<(String, i32, bool)> = {
        let sql = "SELECT rule_text, stratum, is_recursive \
                   FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        Spi::connect(|client| {
            client
                .select(sql, None, &[pgrx::datum::DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("rule select SPI error: {e}"))
                .map(|row| {
                    let text: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let stratum: i32 = row.get::<i32>(2).ok().flatten().unwrap_or(0);
                    let recursive: bool = row.get::<bool>(3).ok().flatten().unwrap_or(false);
                    (text, stratum, recursive)
                })
                .collect::<Vec<_>>()
        })
    };

    if rule_rows.is_empty() {
        return (0, 0);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for (rule_text, _stratum, _recursive) in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("rule parse error during semi-naive inference: {e}"),
        }
    }

    if all_rules.is_empty() {
        return (0, 0);
    }

    let all_rules = if crate::SAMEAS_REASONING.get() {
        let sameas_map = super::rewrite::compute_sameas_map();
        super::rewrite::apply_sameas_to_rules(&all_rules, &sameas_map)
    } else {
        all_rules
    };

    let derived_pred_ids: std::collections::HashSet<i64> = all_rules
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
        .collect();

    let eliminated_rules = check_subsumption(&all_rules);
    let active_rules: Vec<Rule> = if eliminated_rules.is_empty() {
        all_rules.clone()
    } else {
        let eliminated_set: std::collections::HashSet<&str> =
            eliminated_rules.iter().map(|s| s.as_str()).collect();
        all_rules
            .iter()
            .filter(|r| !eliminated_set.contains(r.rule_text.as_str()))
            .cloned()
            .collect()
    };

    for &pred_id in &derived_pred_ids {
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(
            &format!(
                "CREATE TEMP TABLE _dl_delta_{pred_id} \
                 (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                  UNIQUE (s, o, g))"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("semi-naive: create delta temp table error: {e}"));
    }

    for rule in &active_rules {
        let Some(head_atom) = &rule.head else {
            continue;
        };
        let head_pred = match &head_atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        if !derived_pred_ids.contains(&head_pred) {
            continue;
        }
        let target = format!("_dl_delta_{head_pred}");
        match compile_single_rule_to(rule, &target) {
            Ok(sql) => {
                if let Err(e) = Spi::run_with_args(&sql, &[]) {
                    pgrx::warning!("semi-naive seed SQL error: {e}: SQL={sql}");
                }
            }
            Err(e) => pgrx::warning!("semi-naive rule compile error: {e}"),
        }
    }
    // v0.51.0 (S3-4): parallel::execute_with_savepoint() is available for
    // per-group SAVEPOINT isolation; wiring deferred to maintain test stability.

    let delta_index_threshold = crate::DELTA_INDEX_THRESHOLD.get() as i64;
    if delta_index_threshold > 0 {
        for &pred_id in &derived_pred_ids {
            let row_cnt = Spi::get_one::<i64>(&format!("SELECT count(*) FROM _dl_delta_{pred_id}"))
                .unwrap_or(None)
                .unwrap_or(0);
            if row_cnt >= delta_index_threshold {
                let idx_name = format!("_dl_delta_{pred_id}_so_idx");
                Spi::run_with_args(&format!("DROP INDEX IF EXISTS {idx_name}"), &[])
                    .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
                Spi::run_with_args(
                    &format!("CREATE INDEX {idx_name} ON _dl_delta_{pred_id} (s, o)"),
                    &[],
                )
                .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            }
        }
    }

    let mut iteration_count = 1i32;
    let max_iterations = 10_000i32;

    loop {
        if iteration_count >= max_iterations {
            pgrx::warning!(
                "semi-naive inference: reached max iteration limit ({max_iterations}); \
                 possible infinite derivation chain in rule set '{rule_set_name}'"
            );
            break;
        }
        iteration_count += 1;

        for &pred_id in &derived_pred_ids {
            Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE _dl_delta_new_{pred_id} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                      UNIQUE (s, o, g))"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("semi-naive: create delta_new error: {e}"));
        }

        let mut new_this_iter = 0i64;
        let delta_fn = |pred_id: i64| -> String { format!("_dl_delta_{pred_id}") };
        let new_delta_fn = |pred_id: i64| -> String { format!("_dl_delta_new_{pred_id}") };

        for rule in &active_rules {
            let Some(head_atom) = &rule.head else {
                continue;
            };
            let head_pred = match &head_atom.p {
                Term::Const(id) => *id,
                _ => continue,
            };
            if !derived_pred_ids.contains(&head_pred) {
                continue;
            }

            match compile_rule_delta_variants_to(
                rule,
                &derived_pred_ids,
                &delta_fn,
                Some(&new_delta_fn),
            ) {
                Ok(variant_sqls) => {
                    for sql in &variant_sqls {
                        if let Err(e) = Spi::run_with_args(sql, &[]) {
                            pgrx::warning!("semi-naive variant SQL error: {e}: SQL={sql}");
                        }
                    }
                }
                Err(e) => pgrx::warning!("semi-naive compile error: {e}"),
            }
        }

        for &pred_id in &derived_pred_ids {
            let cnt = Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM _dl_delta_new_{pred_id} n \
                 WHERE NOT EXISTS ( \
                     SELECT 1 FROM _dl_delta_{pred_id} d \
                     WHERE d.s = n.s AND d.o = n.o AND d.g = n.g \
                 )"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            new_this_iter += cnt;
        }

        for &pred_id in &derived_pred_ids {
            Spi::run_with_args(
                &format!(
                    "INSERT INTO _dl_delta_{pred_id} (s, o, g) \
                     SELECT s, o, g FROM _dl_delta_new_{pred_id} ON CONFLICT DO NOTHING"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }

        if new_this_iter == 0 {
            break;
        }
    }

    let mut total_derived: i64 = 0;
    for &pred_id in &derived_pred_ids {
        let cnt = Spi::get_one::<i64>(&format!(
            "WITH ins AS ( \
               INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
               SELECT {pred_id}::bigint, s, o, g FROM _dl_delta_{pred_id} \
               ON CONFLICT DO NOTHING RETURNING 1 \
             ) SELECT COUNT(*)::bigint FROM ins"
        ))
        .unwrap_or(None)
        .unwrap_or(0);
        total_derived += cnt;
        if cnt > 0 {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
                 VALUES ($1, NULL, $2) \
                 ON CONFLICT (id) DO UPDATE \
                     SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(cnt),
                ],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }
    }

    // v0.87.0 PROB-DATALOG-01: propagate confidence scores when probabilistic_datalog = on.
    if crate::PROBABILISTIC_DATALOG.get() {
        for rule in &active_rules {
            let rule_weight = rule.weight.unwrap_or(1.0);
            let Some(head_atom) = &rule.head else {
                continue;
            };
            let head_pred = match &head_atom.p {
                Term::Const(id) => *id,
                _ => continue,
            };
            if !derived_pred_ids.contains(&head_pred) {
                continue;
            }
            // Insert confidence rows for all newly-derived SIDs using noisy-OR merge.
            // Confidence = rule_weight * COALESCE(body atom confidence, 1.0) ... (conjunction).
            // ON CONFLICT implements noisy-OR: 1 - (1-existing) * (1-new).
            let conf_sql = format!(
                "INSERT INTO _pg_ripple.confidence (statement_id, confidence, model) \
                 SELECT vp.i, \
                   LEAST(1.0, {rule_weight}::float8), \
                   'datalog' \
                 FROM _pg_ripple.vp_rare vp \
                 WHERE vp.p = {head_pred}::bigint AND vp.source = 1 \
                 ON CONFLICT (statement_id, model) DO UPDATE \
                   SET confidence = 1.0 - \
                     (1.0 - EXCLUDED.confidence) * \
                     (1.0 - _pg_ripple.confidence.confidence)"
            );
            if let Err(e) = Spi::run_with_args(&conf_sql, &[]) {
                pgrx::warning!("probabilistic confidence insert error: {e}");
            }
        }
    }

    // JOURNAL-DATALOG-01: collect affected graph IDs from delta tables before
    // dropping them, then record writes for CONSTRUCT writeback rules (CF-D fix).
    if total_derived > 0 {
        let mut affected_graphs: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for &pred_id in &derived_pred_ids {
            let rows = Spi::connect(|client| {
                client
                    .select(
                        &format!("SELECT DISTINCT g FROM _dl_delta_{pred_id}"),
                        None,
                        &[],
                    )
                    .unwrap_or_else(|_| {
                        pgrx::error!("DISTINCT g query failed on _dl_delta_{pred_id}")
                    })
                    .map(|row| row.get::<i64>(1).ok().flatten())
                    .collect::<Vec<_>>()
            });
            for g in rows.into_iter().flatten() {
                affected_graphs.insert(g);
            }
        }
        for g in affected_graphs {
            crate::storage::mutation_journal::record_write(g);
        }
        crate::storage::mutation_journal::flush();
    }

    for &pred_id in &derived_pred_ids {
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(
            &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
            &[],
        )
        .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
    }

    (total_derived, iteration_count)
}

/// Like `run_inference_seminaive` but also returns subsumption and parallel statistics.
/// Returns `(total_derived, iterations, eliminated_rule_texts, parallel_groups, max_concurrent)`.
pub fn run_inference_seminaive_full(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
    super::ensure_catalog();

    let rule_rows: Vec<(String, i32, bool)> = {
        let sql = "SELECT rule_text, stratum, is_recursive \
                   FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        Spi::connect(|client| {
            client
                .select(sql, None, &[pgrx::datum::DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("rule select SPI error: {e}"))
                .map(|row| {
                    let text: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let stratum: i32 = row.get::<i32>(2).ok().flatten().unwrap_or(0);
                    let recursive: bool = row.get::<bool>(3).ok().flatten().unwrap_or(false);
                    (text, stratum, recursive)
                })
                .collect::<Vec<_>>()
        })
    };

    if rule_rows.is_empty() {
        return (0, 0, vec![], 0, 0);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for (rule_text, _stratum, _recursive) in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("rule parse error during full semi-naive inference: {e}"),
        }
    }

    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get();
    let analysis = super::parallel::partition_into_parallel_groups(&all_rules, parallel_workers);
    let parallel_groups = analysis.parallel_groups;
    let max_concurrent = analysis.max_concurrent;

    let eliminated = check_subsumption(&all_rules);
    let (derived, iters) = run_inference_seminaive(rule_set_name);
    (derived, iters, eliminated, parallel_groups, max_concurrent)
}

// ─── Basic inference (non-semi-naive path) ───────────────────────────────────

/// Execute on-demand materialization using the simple (non-semi-naive) path.
pub fn run_inference(rule_set_name: &str) -> i64 {
    super::ensure_catalog();

    let rules_sql = "SELECT rule_text, stratum, is_recursive \
                     FROM _pg_ripple.rules \
                     WHERE rule_set = $1 AND active = true \
                     ORDER BY stratum, id";

    let rule_rows = Spi::connect(|client| {
        client
            .select(
                rules_sql,
                None,
                &[pgrx::datum::DatumWithOid::from(rule_set_name)],
            )
            .unwrap_or_else(|e| pgrx::error!("rule select SPI error: {e}"))
            .map(|row| {
                let text: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let stratum: i32 = row.get::<i32>(2).ok().flatten().unwrap_or(0);
                let recursive: bool = row.get::<bool>(3).ok().flatten().unwrap_or(false);
                (text, stratum, recursive)
            })
            .collect::<Vec<_>>()
    });

    if crate::SAMEAS_REASONING.get() {
        let _ = super::rewrite::compute_sameas_map();
    }

    let mut total_derived = 0i64;
    // P13-05 (v0.85.0): batch rule SQL statements in groups of 100 before sending to SPI,
    // reducing peak memory and SPI round-trip overhead for large rule sets.
    let mut sql_batch: Vec<String> = Vec::with_capacity(100);

    let flush_batch = |batch: &mut Vec<String>, derived: &mut i64| {
        if batch.is_empty() {
            return;
        }
        let combined = batch.join("; ");
        match Spi::run(&combined) {
            Ok(()) => *derived += batch.len() as i64,
            Err(e) => {
                // Fall back to individual execution on batch error.
                for sql in batch.iter() {
                    match Spi::run_with_args(sql, &[]) {
                        Ok(()) => *derived += 1,
                        Err(e2) => pgrx::warning!("inference SQL error: {e2}: SQL={sql}"),
                    }
                }
                let _ = e; // suppress unused-variable warning
            }
        }
        batch.clear();
    };

    for (rule_text, _stratum, _recursive) in rule_rows {
        let rules = match parse_rules(&rule_text, rule_set_name) {
            Ok(rs) => rs.rules,
            Err(e) => {
                pgrx::warning!("rule parse error during inference: {e}");
                continue;
            }
        };
        for rule in &rules {
            if has_variable_pred(rule) {
                flush_batch(&mut sql_batch, &mut total_derived);
                total_derived += run_var_pred_rule(rule);
            } else {
                match compile_rule_set(std::slice::from_ref(rule)) {
                    Ok(sqls) => {
                        for sql in sqls {
                            sql_batch.push(sql);
                            if sql_batch.len() >= 100 {
                                flush_batch(&mut sql_batch, &mut total_derived);
                            }
                        }
                    }
                    Err(e) => pgrx::warning!("rule compile error: {e}"),
                }
            }
        }
    }
    flush_batch(&mut sql_batch, &mut total_derived);
    // JOURNAL-DATALOG-01: flush mutation journal so CONSTRUCT writeback fires
    // after simple (non-seminaive) inference (CF-D fix).
    if total_derived > 0 {
        let graph_rows = Spi::connect(|client| {
            client
                .select("SELECT DISTINCT g FROM _pg_ripple.vp_rare", None, &[])
                .unwrap_or_else(|e| pgrx::error!("run_inference: graph query failed: {e}"))
                .map(|row| row.get::<i64>(1).ok().flatten())
                .collect::<Vec<_>>()
        });
        for g in graph_rows.into_iter().flatten() {
            crate::storage::mutation_journal::record_write(g);
        }
        crate::storage::mutation_journal::flush();
    }
    total_derived
}

// ─── Variable-predicate rule instantiation (v0.44.0) ─────────────────────────

fn collect_pred_vars(rule: &Rule) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    if let Some(Term::Var(v)) = rule.head.as_ref().map(|h| &h.p)
        && !vars.contains(v)
    {
        vars.push(v.clone());
    }
    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) | BodyLiteral::Negated(a) => a,
            _ => continue,
        };
        if let Term::Var(v) = &atom.p
            && !vars.contains(v)
        {
            vars.push(v.clone());
        }
    }
    vars
}

fn substitute_pred_var(rule: &Rule, var_name: &str, pred_id: i64) -> Rule {
    let sub = |t: &Term| -> Term {
        match t {
            Term::Var(v) if v == var_name => Term::Const(pred_id),
            other => other.clone(),
        }
    };
    let sub_atom = |a: &super::Atom| -> super::Atom {
        super::Atom {
            s: sub(&a.s),
            p: sub(&a.p),
            o: sub(&a.o),
            g: sub(&a.g),
        }
    };
    let new_head = rule.head.as_ref().map(sub_atom);
    let new_body = rule
        .body
        .iter()
        .map(|lit| match lit {
            BodyLiteral::Positive(a) => BodyLiteral::Positive(sub_atom(a)),
            BodyLiteral::Negated(a) => BodyLiteral::Negated(sub_atom(a)),
            other => other.clone(),
        })
        .collect();
    Rule {
        head: new_head,
        body: new_body,
        rule_text: format!("/* {var_name}={pred_id} */ {}", rule.rule_text),
        weight: rule.weight,
    }
}

fn enumerate_pred_var_values(rule: &Rule, var_name: &str) -> Vec<i64> {
    let mut values: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) => a,
            _ => continue,
        };
        let atom_pred_id = match &atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        let is_subj = matches!(&atom.s, Term::Var(v) if v == var_name);
        let is_obj = matches!(&atom.o, Term::Var(v) if v == var_name);
        if is_subj {
            let sql = match &atom.o {
                Term::Const(o_id) => format!(
                    "SELECT DISTINCT s FROM {} WHERE o = {o_id}",
                    vp_read_expr_pub(atom_pred_id)
                ),
                _ => format!("SELECT DISTINCT s FROM {}", vp_read_expr_pub(atom_pred_id)),
            };
            let ids: Vec<i64> = Spi::connect(|c| {
                c.select(&sql, None, &[])
                    .ok()
                    .map(|rows| {
                        rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            });
            values.extend(ids);
        } else if is_obj {
            let sql = match &atom.s {
                Term::Const(s_id) => format!(
                    "SELECT DISTINCT o FROM {} WHERE s = {s_id}",
                    vp_read_expr_pub(atom_pred_id)
                ),
                _ => format!("SELECT DISTINCT o FROM {}", vp_read_expr_pub(atom_pred_id)),
            };
            let ids: Vec<i64> = Spi::connect(|c| {
                c.select(&sql, None, &[])
                    .ok()
                    .map(|rows| {
                        rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            });
            values.extend(ids);
        }
    }
    values.into_iter().collect()
}

fn compute_pred_var_bindings(rule: &Rule, pred_vars: &[String]) -> Vec<Vec<(String, i64)>> {
    if pred_vars.is_empty() {
        return vec![vec![]];
    }

    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) => a,
            _ => continue,
        };
        let atom_pred_id = match &atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        let subj_var = match &atom.s {
            Term::Var(v) if pred_vars.contains(v) => Some(v.clone()),
            _ => None,
        };
        let obj_var = match &atom.o {
            Term::Var(v) if pred_vars.contains(v) => Some(v.clone()),
            _ => None,
        };
        if let (Some(sv), Some(ov)) = (subj_var, obj_var) {
            let sql = format!(
                "SELECT DISTINCT s, o FROM {}",
                vp_read_expr_pub(atom_pred_id)
            );
            let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
                c.select(&sql, None, &[])
                    .ok()
                    .map(|rows| {
                        rows.filter_map(|row| {
                            let s = row.get::<i64>(1).ok().flatten()?;
                            let o = row.get::<i64>(2).ok().flatten()?;
                            Some((s, o))
                        })
                        .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            });
            return pairs
                .into_iter()
                .map(|(s, o)| vec![(sv.clone(), s), (ov.clone(), o)])
                .collect();
        }
    }

    let mut per_var: Vec<(String, Vec<i64>)> = Vec::new();
    for var_name in pred_vars {
        let vals = enumerate_pred_var_values(rule, var_name);
        if vals.is_empty() {
            return vec![];
        }
        per_var.push((var_name.clone(), vals));
    }

    let mut result: Vec<Vec<(String, i64)>> = vec![vec![]];
    for (var_name, values) in &per_var {
        let mut new_result = Vec::new();
        for partial in &result {
            for &val in values {
                let mut extended = partial.clone();
                extended.push((var_name.clone(), val));
                new_result.push(extended);
            }
        }
        result = new_result;
    }
    result
}

/// Handle a rule with variable predicates by instantiating at runtime.
pub fn run_var_pred_rule(rule: &Rule) -> i64 {
    let pred_vars = collect_pred_vars(rule);
    if pred_vars.is_empty() {
        return 0;
    }
    let bindings = compute_pred_var_bindings(rule, &pred_vars);
    if bindings.is_empty() {
        return 0;
    }
    let mut total = 0i64;
    for binding in bindings {
        let mut specialized = rule.clone();
        for (var_name, pred_id) in &binding {
            specialized = substitute_pred_var(&specialized, var_name, *pred_id);
        }
        match compile_rule_set(std::slice::from_ref(&specialized)) {
            Ok(sqls) => {
                for sql in &sqls {
                    match Spi::run_with_args(sql, &[]) {
                        Ok(()) => total += 1,
                        Err(e) => pgrx::warning!("var_pred_rule SQL error: {e}"),
                    }
                }
            }
            Err(e) => pgrx::warning!("var_pred_rule compile error after instantiation: {e}"),
        }
    }
    total
}

// ─── Inner semi-naive helper (used by coordinator) ───────────────────────────

/// Run semi-naive inference over a specific set of rules and materialise results.
pub(crate) fn run_seminaive_inner(rules: &[Rule], rule_set_name: &str) -> (i64, i32) {
    let derived_pred_ids: std::collections::HashSet<i64> = rules
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
        .collect();

    if derived_pred_ids.is_empty() {
        return (0, 0);
    }

    for &pred_id in &derived_pred_ids {
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(
            &format!("CREATE TEMP TABLE _dl_delta_{pred_id} \
                 (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, UNIQUE (s, o, g))"),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("run_seminaive_inner: delta table error: {e}"));
    }

    for rule in rules {
        let Some(head_atom) = &rule.head else {
            continue;
        };
        let head_pred = match &head_atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        if !derived_pred_ids.contains(&head_pred) {
            continue;
        }
        let target = format!("_dl_delta_{head_pred}");
        match compile_single_rule_to(rule, &target) {
            Ok(sql) => {
                if let Err(e) = Spi::run_with_args(&sql, &[]) {
                    pgrx::warning!("run_seminaive_inner: seed SQL error: {e}");
                }
            }
            Err(e) => pgrx::warning!("run_seminaive_inner: seed compile error: {e}"),
        }
    }

    let mut iteration_count = 1i32;
    loop {
        if iteration_count >= 10_000 {
            pgrx::warning!(
                "run_seminaive_inner: max iterations reached for rule_set '{rule_set_name}'"
            );
            break;
        }
        iteration_count += 1;

        for &pred_id in &derived_pred_ids {
            Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!("CREATE TEMP TABLE _dl_delta_new_{pred_id} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, UNIQUE (s, o, g))"),
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("run_seminaive_inner: delta_new error: {e}"));
        }

        let mut new_this_iter = 0i64;
        let delta_fn = |pred_id: i64| -> String { format!("_dl_delta_{pred_id}") };
        let new_delta_fn = |pred_id: i64| -> String { format!("_dl_delta_new_{pred_id}") };

        for rule in rules {
            let Some(head_atom) = &rule.head else {
                continue;
            };
            let head_pred = match &head_atom.p {
                Term::Const(id) => *id,
                _ => continue,
            };
            if !derived_pred_ids.contains(&head_pred) {
                continue;
            }
            match compile_rule_delta_variants_to(
                rule,
                &derived_pred_ids,
                &delta_fn,
                Some(&new_delta_fn),
            ) {
                Ok(sqls) => {
                    for sql in &sqls {
                        if let Err(e) = Spi::run_with_args(sql, &[]) {
                            pgrx::warning!("run_seminaive_inner: variant SQL error: {e}");
                        }
                    }
                }
                Err(e) => pgrx::warning!("run_seminaive_inner: compile error: {e}"),
            }
        }

        for &pred_id in &derived_pred_ids {
            let cnt = Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM _dl_delta_new_{pred_id} n \
                 WHERE NOT EXISTS (SELECT 1 FROM _dl_delta_{pred_id} d WHERE d.s=n.s AND d.o=n.o AND d.g=n.g)"
            )).unwrap_or(None).unwrap_or(0);
            new_this_iter += cnt;
        }

        for &pred_id in &derived_pred_ids {
            Spi::run_with_args(
                &format!(
                    "INSERT INTO _dl_delta_{pred_id} (s,o,g) SELECT s,o,g FROM _dl_delta_new_{pred_id} ON CONFLICT DO NOTHING"
                ),
                &[],
            ).unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
            Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            )
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }

        if new_this_iter == 0 {
            break;
        }
    }

    let mut total: i64 = 0;
    for &pred_id in &derived_pred_ids {
        let cnt = Spi::get_one::<i64>(&format!(
            "WITH ins AS (INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
             SELECT {pred_id}::bigint, s, o, g FROM _dl_delta_{pred_id} \
             ON CONFLICT DO NOTHING RETURNING 1) SELECT COUNT(*)::bigint FROM ins"
        ))
        .unwrap_or(None)
        .unwrap_or(0);
        total += cnt;
        if cnt > 0 {
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) VALUES ($1, NULL, $2) \
                 ON CONFLICT (id) DO UPDATE SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(cnt),
                ],
            ).unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        }
    }

    for &pred_id in &derived_pred_ids {
        Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[])
            .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
        Spi::run_with_args(
            &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
            &[],
        )
        .unwrap_or_else(|e| pgrx::log!("datalog cleanup: {e}"));
    }

    (total, iteration_count)
}
