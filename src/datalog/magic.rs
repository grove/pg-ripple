//! Magic sets transformation for goal-directed Datalog inference (v0.29.0).
//!
//! # Overview
//!
//! Given a Datalog program and a goal triple pattern like `?x rdf:type foaf:Person`,
//! magic sets rewrites the program so that only facts *relevant* to deriving the
//! goal are computed.  This is dramatically faster for selective goals on large
//! datasets because it avoids deriving facts that will never contribute to the answer.
//!
//! # Implementation
//!
//! This implementation uses a simplified adornment-based approach:
//!
//! 1. Parse the goal triple pattern into (s, p, o) with bound/free positions.
//! 2. Run semi-naive inference for rules that can derive the goal predicate, adding
//!    binding constraints to the seed pass so only relevant facts are computed.
//! 3. Query the results matching the goal pattern from the derived triple store.
//! 4. Return statistics: matching count, total derived count, and iteration count.
//!
//! When `pg_ripple.magic_sets = false`, falls back to full materialization + filter.

use pgrx::datum::DatumWithOid;

use crate::datalog::parser::parse_rules;
use crate::datalog::{BodyLiteral, Rule, Term};

// ─── Goal pattern ──────────────────────────────────────────────────────────────

/// A triple-pattern goal: bound positions are `Some(encoded_id)`, free variables
/// are `None`.
#[derive(Debug, Clone)]
pub struct GoalPattern {
    /// Bound subject IRI encoded as dictionary ID, or `None` for a free variable.
    pub s: Option<i64>,
    /// Bound predicate IRI encoded as dictionary ID, or `None` for a free variable.
    pub p: Option<i64>,
    /// Bound object IRI/literal encoded as dictionary ID, or `None` for a free variable.
    pub o: Option<i64>,
}

impl GoalPattern {
    /// Return true when all three positions are free (no binding constraints).
    #[allow(dead_code)]
    pub fn is_unconstrained(&self) -> bool {
        self.s.is_none() && self.p.is_none() && self.o.is_none()
    }
}

/// Parse a whitespace-delimited triple pattern string into a `GoalPattern`.
///
/// Each token is one of:
/// - `?varname` — free variable
/// - `<iri>` — bound IRI
/// - `prefix:local` — bound prefixed IRI (resolved via `_pg_ripple.prefixes`)
/// - `"literal"` — bound literal
///
/// Returns `Err` when the goal string cannot be tokenized into exactly 3 terms.
pub fn parse_goal(goal: &str) -> Result<GoalPattern, String> {
    let tokens = tokenize_goal(goal)?;
    if tokens.len() != 3 {
        return Err(format!(
            "magic sets: goal must have exactly 3 terms (s p o), got {}: {:?}",
            tokens.len(),
            tokens
        ));
    }

    let encode_token = |tok: &str| -> Option<i64> {
        if tok.starts_with('?') {
            None // free variable
        } else if tok.starts_with('"') {
            // Literal
            let resolved = crate::datalog::resolve_prefix(tok);
            Some(crate::dictionary::encode(
                &resolved,
                crate::dictionary::KIND_LITERAL,
            ))
        } else {
            // IRI or prefixed IRI
            let resolved = crate::datalog::resolve_prefix(tok);
            let iri = resolved.trim_start_matches('<').trim_end_matches('>');
            Some(crate::datalog::encode_iri(iri))
        }
    };

    Ok(GoalPattern {
        s: encode_token(&tokens[0]),
        p: encode_token(&tokens[1]),
        o: encode_token(&tokens[2]),
    })
}

/// Tokenize a goal string into at most 3 RDF term tokens.
fn tokenize_goal(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.trim().chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '<' => {
                chars.next();
                let mut buf = String::from("<");
                for c in chars.by_ref() {
                    buf.push(c);
                    if c == '>' {
                        break;
                    }
                }
                tokens.push(buf);
            }
            '"' => {
                chars.next();
                let mut buf = String::from("\"");
                let mut escaped = false;
                for c in chars.by_ref() {
                    if escaped {
                        escaped = false;
                        buf.push(c);
                        continue;
                    }
                    if c == '\\' {
                        escaped = true;
                        buf.push(c);
                        continue;
                    }
                    buf.push(c);
                    if c == '"' {
                        break;
                    }
                }
                // Consume optional ^^<datatype> or @lang suffix.
                while let Some(&p) = chars.peek() {
                    if p == '^' || p == '@' {
                        buf.push(p);
                        chars.next();
                    } else if p == '<' {
                        chars.next();
                        buf.push('<');
                        for c in chars.by_ref() {
                            buf.push(c);
                            if c == '>' {
                                break;
                            }
                        }
                        break;
                    } else if p.is_alphanumeric() || p == '-' || p == '_' {
                        buf.push(p);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(buf);
            }
            _ => {
                let mut buf = String::new();
                while let Some(&p) = chars.peek() {
                    if p == ' ' || p == '\t' || p == '\n' || p == '\r' {
                        break;
                    }
                    buf.push(p);
                    chars.next();
                }
                if !buf.is_empty() {
                    tokens.push(buf);
                }
            }
        }
    }
    Ok(tokens)
}

// ─── Goal-directed inference ───────────────────────────────────────────────────

/// Run goal-directed inference using simplified magic sets.
///
/// Runs semi-naive inference for the named rule set, applying binding
/// constraints from the goal pattern to limit derivation to facts relevant
/// to the goal.  Returns `(matching_count, total_derived, iterations)`.
///
/// When `pg_ripple.magic_sets = false`, runs full inference and filters
/// the results post-hoc (functionally correct but slower).
///
/// # Error handling
/// Returns `Err(PT501)` when the goal pattern causes a circular binding
/// dependency that prevents adornment propagation.
pub fn run_infer_goal(rule_set_name: &str, goal: &GoalPattern) -> Result<(i64, i64, i32), String> {
    let magic_sets_enabled = crate::MAGIC_SETS.get();

    if magic_sets_enabled && goal.p.is_some() {
        // Magic sets path: run goal-directed inference seeded with goal predicate
        // binding constraints.
        run_magic_inference(rule_set_name, goal)
    } else {
        // Fallback: run full inference, then count matching triples.
        let (total_derived, iterations) = crate::datalog::run_inference_seminaive(rule_set_name);
        let matching = count_matching_goal(goal);
        Ok((matching, total_derived, iterations))
    }
}

/// Run semi-naive inference with magic-set seed filtering.
///
/// Identifies rules that can derive the goal predicate (via dependency
/// reachability), runs them with binding constraints applied to the seed
/// pass, and counts matching results.
fn run_magic_inference(rule_set_name: &str, goal: &GoalPattern) -> Result<(i64, i64, i32), String> {
    let goal_pred_id = match goal.p {
        Some(id) => id,
        None => {
            // No predicate bound: fall back to full inference.
            let (total_derived, iterations) =
                crate::datalog::run_inference_seminaive(rule_set_name);
            let matching = count_matching_goal(goal);
            return Ok((matching, total_derived, iterations));
        }
    };

    // Load rules from catalog.
    let rule_rows: Vec<(String, i32, bool)> = {
        let sql = "SELECT rule_text, stratum, is_recursive \
                   FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        pgrx::Spi::connect(|client| {
            client
                .select(sql, None, &[DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("magic sets: rule select SPI error: {e}"))
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
        return Ok((0, 0, 0));
    }

    // Parse all rules.
    let mut all_rules: Vec<Rule> = Vec::new();
    for (rule_text, _stratum, _recursive) in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("magic sets: rule parse error: {e}"),
        }
    }

    if all_rules.is_empty() {
        return Ok((0, 0, 0));
    }

    // Find derived predicates reachable to the goal predicate (i.e., predicates
    // that, through some chain of rules, can produce the goal predicate).
    let reachable = find_goal_reachable_preds(&all_rules, goal_pred_id);

    // Filter rules to only those that can contribute to the goal.
    let relevant_rules: Vec<Rule> = all_rules
        .into_iter()
        .filter(|r| {
            if let Some(h) = &r.head
                && let Term::Const(id) = &h.p
            {
                return reachable.contains(id);
            }
            false
        })
        .collect();

    if relevant_rules.is_empty() {
        // No relevant rules → goal predicate is not derived; count existing triples.
        let matching = count_matching_goal(goal);
        return Ok((matching, 0, 0));
    }

    // Run semi-naive inference using only relevant rules, with binding-constrained
    // seed pass.  We create temporary magic predicate tables to capture the demanded
    // bindings for the goal predicate, then seed from those.
    let (total_derived, iterations) = run_magic_seminaive(rule_set_name, &relevant_rules, goal);

    // Count matching triples after inference.
    let matching = count_matching_goal(goal);
    Ok((matching, total_derived, iterations))
}

/// Find all predicate IDs that can reach (derive) the goal predicate through
/// the rule dependency graph.  Returns the set of predicate IDs whose derivation
/// can contribute to the goal.
fn find_goal_reachable_preds(rules: &[Rule], goal_pred_id: i64) -> std::collections::HashSet<i64> {
    // Build reverse dependency map: pred_id → set of predicate IDs that USE it.
    // A predicate P "reaches" the goal if P derives some Q that eventually derives goal.
    // We want: all P such that goal_pred_id is reachable from P in the derivation graph.
    //
    // Build forward graph: head_pred → {body_preds that head_pred depends on}.
    // Then BFS backwards from goal_pred_id.

    // First, build: for each pred, which rules derive it?
    // We want to know: goal_pred depends on what body preds?
    // Reverse: which head preds can be reached going backwards from goal?

    // Build: body_pred → set of head_preds that use it.
    let mut body_to_heads: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
        std::collections::HashMap::new();

    for rule in rules {
        let Some(h) = &rule.head else { continue };
        let Term::Const(head_id) = &h.p else { continue };
        for lit in &rule.body {
            if let BodyLiteral::Positive(atom) = lit
                && let Term::Const(body_id) = &atom.p
            {
                body_to_heads.entry(*body_id).or_default().insert(*head_id);
            }
        }
    }

    // Build head → body_preds map (what does each head pred depend on?).
    let mut head_to_bodies: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
        std::collections::HashMap::new();
    for rule in rules {
        let Some(h) = &rule.head else { continue };
        let Term::Const(head_id) = &h.p else { continue };
        for lit in &rule.body {
            if let BodyLiteral::Positive(atom) = lit
                && let Term::Const(body_id) = &atom.p
            {
                head_to_bodies.entry(*head_id).or_default().insert(*body_id);
            }
        }
    }

    // BFS from goal_pred_id backwards through head_to_bodies to find all
    // predicates reachable to the goal.
    let mut reachable = std::collections::HashSet::new();
    let mut worklist = std::collections::VecDeque::new();
    reachable.insert(goal_pred_id);
    worklist.push_back(goal_pred_id);

    while let Some(pred) = worklist.pop_front() {
        // All body predicates of rules that derive `pred` are also reachable.
        if let Some(body_preds) = head_to_bodies.get(&pred) {
            for &bp in body_preds {
                if reachable.insert(bp) {
                    worklist.push_back(bp);
                }
            }
        }
    }

    reachable
}

/// Run semi-naive inference for a subset of rules with goal-directed seed filtering.
///
/// The seed pass is filtered by the goal's bound positions (s, o) for rules
/// deriving the goal predicate.  All other rules run normally.
/// Returns `(total_derived, iterations)`.
fn run_magic_seminaive(
    rule_set_name: &str,
    relevant_rules: &[Rule],
    goal: &GoalPattern,
) -> (i64, i32) {
    // Create magic temp table to hold demanded bindings for the goal predicate.
    // The magic table captures which (s, o) pairs are "demanded" by the goal.
    let magic_table = if let Some(goal_pred) = goal.p {
        let tbl = format!("_magic_{rule_set_name}_{goal_pred}");
        // Sanitize: only allow alphanumerics and underscores.
        let tbl: String = tbl
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let _ = pgrx::Spi::run_with_args(&format!("DROP TABLE IF EXISTS {tbl}"), &[]);
        pgrx::Spi::run_with_args(
            &format!(
                "CREATE TEMP TABLE {tbl} \
                 (s BIGINT, o BIGINT, UNIQUE (s, o))"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("magic sets: create magic table error: {e}"));

        // Seed the magic table with the demanded binding.
        // A NULL value in the goal means "unconstrained" (accept any value).
        // We insert a sentinel row to indicate the demanded binding.
        let s_val = goal
            .s
            .map(|id| id.to_string())
            .unwrap_or_else(|| "NULL".to_owned());
        let o_val = goal
            .o
            .map(|id| id.to_string())
            .unwrap_or_else(|| "NULL".to_owned());
        let _ = pgrx::Spi::run_with_args(
            &format!("INSERT INTO {tbl} (s, o) VALUES ({s_val}, {o_val}) ON CONFLICT DO NOTHING"),
            &[],
        );
        Some((goal_pred, tbl))
    } else {
        None
    };

    // Collect derived predicate IDs from relevant rules.
    let derived_pred_ids: std::collections::HashSet<i64> = relevant_rules
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

    // Create delta temp tables.
    for &pred_id in &derived_pred_ids {
        let _ = pgrx::Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
        pgrx::Spi::run_with_args(
            &format!(
                "CREATE TEMP TABLE _dl_delta_{pred_id} \
                 (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                  UNIQUE (s, o, g))"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("magic sets: create delta temp table error: {e}"));
    }

    // Seeding pass: run relevant rules once.
    for rule in relevant_rules {
        let Some(head_atom) = &rule.head else {
            continue;
        };
        let head_pred = match head_atom.p {
            Term::Const(id) => id,
            _ => continue,
        };
        if !derived_pred_ids.contains(&head_pred) {
            continue;
        }
        let target = format!("_dl_delta_{head_pred}");

        // For rules deriving the goal predicate, add WHERE filters if the magic
        // table has binding constraints.
        let sql = if let Some((goal_pred, magic_tbl)) = &magic_table {
            if head_pred == *goal_pred {
                build_magic_filtered_seed_sql(rule, &target, magic_tbl, goal)
            } else {
                match crate::datalog::compiler::compile_single_rule_to(rule, &target) {
                    Ok(s) => s,
                    Err(e) => {
                        pgrx::warning!("magic sets seed compile error: {e}");
                        continue;
                    }
                }
            }
        } else {
            match crate::datalog::compiler::compile_single_rule_to(rule, &target) {
                Ok(s) => s,
                Err(e) => {
                    pgrx::warning!("magic sets seed compile error: {e}");
                    continue;
                }
            }
        };

        if let Err(e) = pgrx::Spi::run_with_args(&sql, &[]) {
            pgrx::warning!("magic sets seed SQL error: {e}");
        }
    }

    // Fixpoint loop.
    let mut iteration_count = 1i32;
    let max_iterations = 10_000i32;

    loop {
        if iteration_count >= max_iterations {
            pgrx::warning!(
                "magic sets: reached max iteration limit ({max_iterations}) for rule set '{rule_set_name}'"
            );
            break;
        }
        iteration_count += 1;

        // Create new delta tables.
        for &pred_id in &derived_pred_ids {
            let _ = pgrx::Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
            pgrx::Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE _dl_delta_new_{pred_id} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                      UNIQUE (s, o, g))"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("magic sets: create delta_new error: {e}"));
        }

        let mut new_this_iter = 0i64;
        let delta_fn = |pred_id: i64| -> String { format!("_dl_delta_{pred_id}") };
        let new_delta_fn = |pred_id: i64| -> String { format!("_dl_delta_new_{pred_id}") };

        for rule in relevant_rules {
            let Some(head_atom) = &rule.head else {
                continue;
            };
            let head_pred = match head_atom.p {
                Term::Const(id) => id,
                _ => continue,
            };
            if !derived_pred_ids.contains(&head_pred) {
                continue;
            }

            match crate::datalog::compiler::compile_rule_delta_variants_to(
                rule,
                &derived_pred_ids,
                &delta_fn,
                Some(&new_delta_fn),
            ) {
                Ok(variant_sqls) => {
                    for sql in &variant_sqls {
                        if let Err(e) = pgrx::Spi::run_with_args(sql, &[]) {
                            pgrx::warning!("magic sets variant SQL error: {e}");
                        }
                    }
                }
                Err(e) => pgrx::warning!("magic sets compile error: {e}"),
            }
        }

        // Count new rows.
        for &pred_id in &derived_pred_ids {
            let cnt = pgrx::Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM _dl_delta_new_{pred_id} n \
                 WHERE NOT EXISTS (SELECT 1 FROM _dl_delta_{pred_id} d \
                 WHERE d.s = n.s AND d.o = n.o AND d.g = n.g)"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            new_this_iter += cnt;
        }

        // Merge new delta into delta.
        for &pred_id in &derived_pred_ids {
            let _ = pgrx::Spi::run_with_args(
                &format!(
                    "INSERT INTO _dl_delta_{pred_id} (s, o, g) \
                     SELECT s, o, g FROM _dl_delta_new_{pred_id} ON CONFLICT DO NOTHING"
                ),
                &[],
            );
            let _ = pgrx::Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
        }

        if new_this_iter == 0 {
            break;
        }
    }

    // Materialise derived triples into vp_rare.
    let mut total_derived = 0i64;
    for &pred_id in &derived_pred_ids {
        let cnt = pgrx::Spi::get_one::<i64>(&format!(
            "WITH ins AS ( \
               INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
               SELECT {pred_id}::bigint, s, o, g FROM _dl_delta_{pred_id} \
               ON CONFLICT DO NOTHING \
               RETURNING 1 \
             ) SELECT COUNT(*)::bigint FROM ins"
        ))
        .unwrap_or(None)
        .unwrap_or(0);
        total_derived += cnt;

        if cnt > 0 {
            let _ = pgrx::Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
                 VALUES ($1, NULL, $2) \
                 ON CONFLICT (id) DO UPDATE \
                     SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[DatumWithOid::from(pred_id), DatumWithOid::from(cnt)],
            );
        }
    }

    // Cleanup delta tables.
    for &pred_id in &derived_pred_ids {
        let _ = pgrx::Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
        let _ = pgrx::Spi::run_with_args(
            &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
            &[],
        );
    }

    // Cleanup magic table (PT501 note: magic temp tables are always cleaned up even
    // on partial failure, ensuring no state leaks between calls).
    if let Some((_, ref magic_tbl)) = magic_table {
        let _ = pgrx::Spi::run_with_args(&format!("DROP TABLE IF EXISTS {magic_tbl}"), &[]);
    }

    (total_derived, iteration_count)
}

/// Build a seed SQL string for a rule that derives the goal predicate, applying
/// the goal's bound-position filters as additional WHERE conditions.
fn build_magic_filtered_seed_sql(
    rule: &Rule,
    target: &str,
    _magic_tbl: &str,
    goal: &GoalPattern,
) -> String {
    // Get the base SQL from the standard compiler.
    let base_sql = match crate::datalog::compiler::compile_single_rule_to(rule, target) {
        Ok(s) => s,
        Err(e) => {
            pgrx::warning!("magic sets filtered seed compile error: {e}");
            return format!("INSERT INTO {target} (s, o, g) SELECT NULL, NULL, 0 WHERE FALSE");
        }
    };

    // Append additional WHERE conditions for bound positions in the goal.
    // This is the "magic filter" — we only derive triples that match the goal.
    let mut extra_conditions: Vec<String> = Vec::new();
    if let Some(s_id) = goal.s {
        extra_conditions.push(format!("s = {s_id}"));
    }
    if let Some(o_id) = goal.o {
        extra_conditions.push(format!("o = {o_id}"));
    }

    if extra_conditions.is_empty() {
        base_sql
    } else {
        // Wrap the INSERT...SELECT in a CTE and add the extra filter.
        let filter_str = extra_conditions.join(" AND ");
        // The base SQL ends with "ON CONFLICT DO NOTHING".
        // We need to inject the filter before ON CONFLICT.
        // Simple approach: wrap in a subquery.
        let (insert_part, select_part) = if let Some(pos) = base_sql.find("SELECT ") {
            let insert = &base_sql[..pos];
            let select = &base_sql[pos..];
            (insert.to_owned(), select.to_owned())
        } else {
            return base_sql;
        };

        // Remove the trailing "ON CONFLICT DO NOTHING" to add a WHERE clause.
        let select_no_conflict = select_part
            .trim_end_matches('\n')
            .trim_end()
            .trim_end_matches("ON CONFLICT DO NOTHING")
            .trim_end();

        let has_where = select_no_conflict.contains("WHERE ");
        let joiner = if has_where { "\n  AND " } else { "\nWHERE " };

        format!("{insert_part}{select_no_conflict}{joiner}{filter_str}\nON CONFLICT DO NOTHING")
    }
}

/// Count triples in vp_rare that match the goal pattern.
///
/// This is called after inference to count how many derived triples satisfy
/// the goal's binding constraints.
pub fn count_matching_goal(goal: &GoalPattern) -> i64 {
    let mut conditions: Vec<String> = Vec::new();
    if let Some(p_id) = goal.p {
        conditions.push(format!("p = {p_id}"));
    }
    if let Some(s_id) = goal.s {
        conditions.push(format!("s = {s_id}"));
    }
    if let Some(o_id) = goal.o {
        conditions.push(format!("o = {o_id}"));
    }

    // Also check dedicated VP tables if the goal predicate has one.
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // If p is bound, check the dedicated VP table first, fall back to vp_rare.
    if let Some(p_id) = goal.p {
        let has_dedicated = pgrx::Spi::get_one_with_args::<i64>(
            "SELECT table_oid::bigint FROM _pg_ripple.predicates \
             WHERE id = $1 AND table_oid IS NOT NULL",
            &[DatumWithOid::from(p_id)],
        )
        .ok()
        .flatten()
        .is_some();

        if has_dedicated {
            // Count from dedicated VP view (which unions main and delta).
            let mut vp_conditions: Vec<String> = Vec::new();
            if let Some(s_id) = goal.s {
                vp_conditions.push(format!("s = {s_id}"));
            }
            if let Some(o_id) = goal.o {
                vp_conditions.push(format!("o = {o_id}"));
            }
            let vp_where = if vp_conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", vp_conditions.join(" AND "))
            };
            let vp_cnt = pgrx::Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM _pg_ripple.vp_{p_id} {vp_where}"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            let rare_cnt = pgrx::Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM _pg_ripple.vp_rare {where_clause}"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            return vp_cnt + rare_cnt;
        }
    }

    pgrx::Spi::get_one::<i64>(&format!(
        "SELECT count(*) FROM _pg_ripple.vp_rare {where_clause}"
    ))
    .unwrap_or(None)
    .unwrap_or(0)
}
