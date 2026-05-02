//! Datalog Reasoning Engine for pg_ripple (v0.10.0).
//!
//! # Architecture
//!
//! ```text
//! User rules (Datalog syntax or built-in rule set name)
//!     │
//!     ▼
//! Rule parser (parser.rs) → Rule IR (head ← body₁, body₂, …, ¬bodyₙ)
//!     │
//!     ▼
//! Dependency analysis → Stratification (stratify.rs)
//!     │
//!     ▼
//! Per-stratum SQL generator (compiler.rs):
//!   - Non-recursive rules → INSERT … SELECT … ON CONFLICT DO NOTHING
//!   - Recursive rules     → WITH RECURSIVE … CYCLE
//!   - Negation            → NOT EXISTS (higher strata only)
//!     │
//!     ▼
//! Execution modes:
//!   ├─ On-demand  (inline CTEs injected into SPARQL→SQL)
//!   └─ Materialized (pg_trickle stream tables — optional)
//! ```
//!
//! # Public SQL functions
//!
//! - `pg_ripple.load_rules(rules TEXT, rule_set TEXT)` — parse and store Datalog rules
//! - `pg_ripple.load_rules_builtin(name TEXT)` — load a built-in rule set ('rdfs', 'owl-rl')
//! - `pg_ripple.list_rules()` — list all stored rules
//! - `pg_ripple.drop_rules(rule_set TEXT)` — remove a rule set
//! - `pg_ripple.check_constraints()` — evaluate constraint rules and return violations
//! - `pg_ripple.enable_rule_set(name TEXT)` — activate a named rule set
//! - `pg_ripple.disable_rule_set(name TEXT)` — deactivate a named rule set
//! - `pg_ripple.infer(rule_set TEXT)` — materialize derived triples for a rule set

pub mod builtins;
pub mod cache;
pub mod compiler;
pub mod coordinator;
pub mod demand;
pub mod dred;
pub mod explain;
pub mod lattice;
pub mod magic;
pub mod parallel;
pub mod parser;
pub mod rewrite;
pub mod seminaive;
pub mod stratify;
pub mod tabling;
pub mod wfs;

pub use compiler::compile_aggregate_rule;
pub use compiler::compile_rule_delta_variants_to;
pub use compiler::compile_rule_set;
pub use compiler::compile_single_rule_to;
pub use compiler::has_variable_pred;
pub use compiler::vp_read_expr_pub;
pub use demand::parse_demands_json;
pub use demand::run_infer_demand;
pub use dred::{check_dred_safety, run_dred_on_delete};
pub use lattice::{ensure_lattice_catalog, register_lattice, run_infer_lattice};
pub use magic::parse_goal;
pub use magic::run_infer_goal;
pub use parser::parse_rules;
pub use stratify::check_aggregation_stratification;
pub use stratify::check_subsumption;
pub use stratify::stratify;
pub use tabling::{
    compute_goal_hash, ensure_tabling_catalog, tabling_invalidate_all, tabling_lookup,
    tabling_stats_impl, tabling_store,
};
pub use wfs::{build_wfs_jsonb, run_wfs};

use pgrx::prelude::*;

// ─── Rule IR ─────────────────────────────────────────────────────────────────

/// A Datalog term: variable, constant (dictionary-encoded), or wildcard.
#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    /// Variable: `?x` — unified across atoms in the same rule.
    Var(String),
    /// Constant: dictionary-encoded IRI or literal.
    Const(i64),
    /// Wildcard: `?_` — matches anything but is not bound.
    Wildcard,
    /// Default graph sentinel (unscoped atom, g = 0 or ANY depending on GUC).
    DefaultGraph,
}

/// A triple pattern in a Datalog rule body or head.
#[derive(Debug, Clone)]
pub struct Atom {
    pub s: Term,
    pub p: Term,
    pub o: Term,
    /// Graph dimension — `DefaultGraph` when no GRAPH clause is present.
    pub g: Term,
}

/// A body literal: positive or negated atom, or an arithmetic guard.
#[derive(Debug, Clone)]
pub enum BodyLiteral {
    Positive(Atom),
    Negated(Atom),
    /// Arithmetic comparison: `?a OP ?b` or `?a OP <literal>`.
    Compare(Term, CompareOp, Term),
    /// String built-in: `STRLEN(?s) > ?n` or `REGEX(?s, ?pattern)`.
    StringBuiltin(StringBuiltin),
    /// Arithmetic assignment: `?z IS ?x + ?y`.
    Assign(String, Term, ArithOp, Term),
    /// Aggregate body literal (Datalog^agg, v0.30.0).
    /// Syntax: `COUNT(?aggVar WHERE subject pred object) = ?resultVar`
    Aggregate(AggregateLiteral),
}

/// Aggregate function kinds (v0.30.0).
#[derive(Debug, Clone, PartialEq)]
pub enum AggFunc {
    Count,
    Sum,
    Min,
    Max,
    Avg,
}

/// An aggregate literal in a rule body (Datalog^agg, v0.30.0).
///
/// Syntax: `COUNT(?aggVar WHERE subject pred object) = ?resultVar`
///
/// Compiles to a GROUP BY subquery with an aggregate function.
/// The predicate in the atom must come from a strictly lower stratum
/// than the head predicate (aggregation-stratification requirement).
#[derive(Debug, Clone)]
pub struct AggregateLiteral {
    /// The aggregate function (COUNT, SUM, MIN, MAX, AVG).
    pub func: AggFunc,
    /// The variable being aggregated (the inner variable inside the WHERE clause).
    pub agg_var: String,
    /// The triple pattern inside the WHERE clause.
    pub atom: Atom,
    /// The variable to bind the aggregate result to (from `= ?resultVar`).
    pub result_var: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub enum StringBuiltin {
    Strlen(Term, CompareOp, Term),
    Regex(Term, String),
}

/// A Datalog rule: head :- body .
///
/// Constraint rules (empty-head integrity constraints) have `head = None`.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Head atom; `None` for constraint rules (empty head: `:- body .`).
    pub head: Option<Atom>,
    /// Body literals.
    pub body: Vec<BodyLiteral>,
    /// Original text of this rule (for catalog storage).
    pub rule_text: String,
    /// Optional `@weight(FLOAT)` annotation for probabilistic Datalog (v0.87.0).
    /// Value must be in [0.0, 1.0]. `None` means confidence 1.0 (deterministic).
    pub weight: Option<f64>,
}

/// A named collection of rules.
#[derive(Debug, Clone)]
pub struct RuleSet {
    #[allow(dead_code)]
    pub name: String,
    pub rules: Vec<Rule>,
}

// ─── Catalog helpers ──────────────────────────────────────────────────────────

/// Ensure the Datalog catalog tables exist.
/// Called idempotently from `load_rules`.
pub fn ensure_catalog() {
    // _pg_ripple.rules
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.rules ( \
             id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
             rule_set      TEXT NOT NULL, \
             rule_text     TEXT NOT NULL, \
             head_pred     BIGINT, \
             stratum       INT NOT NULL DEFAULT 0, \
             is_recursive  BOOLEAN NOT NULL DEFAULT false, \
             active        BOOLEAN NOT NULL DEFAULT true, \
             created_at    TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("rules table creation error: {e}"));

    // _pg_ripple.rule_sets
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.rule_sets ( \
             name          TEXT NOT NULL PRIMARY KEY, \
             rule_hash     BYTEA, \
             active        BOOLEAN NOT NULL DEFAULT true, \
             created_at    TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("rule_sets table creation error: {e}"));

    // Extend predicates table with derived/rule_set columns if needed.
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.predicates \
             ADD COLUMN IF NOT EXISTS derived BOOLEAN NOT NULL DEFAULT FALSE, \
             ADD COLUMN IF NOT EXISTS rule_set TEXT",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("predicates extend error: {e}"));
}

/// Resolve a prefixed IRI using the `_pg_ripple.prefixes` table.
/// Returns the expanded IRI string (without angle brackets).
pub fn resolve_prefix(prefixed: &str) -> String {
    // Handle <full-iri>
    if let Some(inner) = prefixed.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return inner.to_owned();
    }
    // Handle prefix:local
    if let Some(colon) = prefixed.find(':') {
        let prefix = &prefixed[..colon];
        let local = &prefixed[colon + 1..];
        let expansion = Spi::get_one_with_args::<String>(
            "SELECT expansion FROM _pg_ripple.prefixes WHERE prefix = $1",
            &[pgrx::datum::DatumWithOid::from(prefix)],
        )
        .ok()
        .flatten();
        if let Some(exp) = expansion {
            return format!("{exp}{local}");
        }
    }
    prefixed.to_owned()
}

/// Encode a resolved IRI string to a dictionary ID.
pub fn encode_iri(iri: &str) -> i64 {
    crate::dictionary::encode(iri, crate::dictionary::KIND_IRI)
}

/// Parse rules text and store them under the given rule set name.
///
/// Convenience wrapper used by the views module so it can load rules inline
/// without going through the full `pg_extern` path.
/// Returns the number of rules stored.
pub fn load_and_store_rules(rules_text: &str, rule_set_name: &str) -> i64 {
    let rule_set = match parse_rules(rules_text, rule_set_name) {
        Ok(rs) => rs,
        Err(e) => pgrx::error!("Datalog rule parse error: {e}"),
    };
    store_rules(rule_set_name, &rule_set.rules)
}

/// Store rules into the catalog, computing strata.
/// Returns the number of rules stored.
pub fn store_rules(rule_set: &str, rules: &[Rule]) -> i64 {
    ensure_catalog();

    // Stratify the rule set.  For non-stratifiable programs (cyclic negation),
    // fall back to a single stratum containing all rules at stratum 0 so that
    // the rules are stored and can be processed by `infer_wfs()` later.
    let stratified = match stratify(rules) {
        Ok(s) => s,
        Err(_) => {
            // Non-stratifiable: store all rules in stratum 0, recursive = true.
            // WFS inference re-stratifies at query time.
            crate::datalog::stratify::StratifiedProgram {
                strata: vec![crate::datalog::stratify::Stratum {
                    rules: rules.to_vec(),
                    is_recursive: true,
                    derived_predicates: vec![],
                }],
            }
        }
    };

    // Upsert the rule set record.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.rule_sets (name, active) \
         VALUES ($1, true) \
         ON CONFLICT (name) DO UPDATE SET active = true",
        &[pgrx::datum::DatumWithOid::from(rule_set)],
    )
    .unwrap_or_else(|e| pgrx::error!("rule_sets upsert error: {e}"));

    let mut count = 0i64;
    for (stratum_idx, stratum) in stratified.strata.iter().enumerate() {
        for rule in &stratum.rules {
            let head_pred: Option<i64> = rule.head.as_ref().and_then(|h| {
                if let Term::Const(id) = &h.p {
                    Some(*id)
                } else {
                    None
                }
            });

            Spi::run_with_args(
                "INSERT INTO _pg_ripple.rules \
                     (rule_set, rule_text, head_pred, stratum, is_recursive) \
                     VALUES ($1, $2, $3, $4, $5)",
                &[
                    pgrx::datum::DatumWithOid::from(rule_set),
                    pgrx::datum::DatumWithOid::from(rule.rule_text.as_str()),
                    pgrx::datum::DatumWithOid::from(head_pred),
                    pgrx::datum::DatumWithOid::from(stratum_idx as i32),
                    pgrx::datum::DatumWithOid::from(stratum.is_recursive),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("rule insert error: {e}"));
            count += 1;
        }
    }

    count
}

// ─── Inference re-exports (moved to seminaive.rs / coordinator.rs) ───────────
pub use coordinator::run_inference_agg;
pub(crate) use seminaive::run_seminaive_inner;
pub use seminaive::{run_inference, run_inference_seminaive, run_inference_seminaive_full};

// ─── L-3.3 (v0.56.0): Incremental RDFS closure ──────────────────────────────

/// Run incremental RDFS closure rules for a specific predicate (by dictionary ID).
///
/// Only re-runs the four targeted rules when the predicate is `rdfs:subClassOf` or
/// `rdfs:subPropertyOf`:
/// - rdfs2 (domain inference), rdfs3 (range inference),
/// - rdfs7 (subPropertyOf propagation), rdfs9 (subClassOf type propagation)
///
/// Called by the merge worker when `inference_mode = 'incremental_rdfs'`.
/// No-op for predicates that are not RDFS schema predicates.
pub fn run_incremental_rdfs_for_predicate(pred_id: i64) {
    // Look up the predicate IRI from the dictionary.
    let pred_iri: Option<String> = Spi::connect(|c| {
        c.select(
            "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
            None,
            &[pgrx::datum::DatumWithOid::from(pred_id)],
        )
        .ok()
        .and_then(|mut r| r.next())
        .and_then(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
    });

    let Some(iri) = pred_iri else {
        return;
    };

    // Only trigger re-inference for RDFS schema predicates.
    let is_rdfs_schema = matches!(
        iri.as_str(),
        "http://www.w3.org/2000/01/rdf-schema#subClassOf"
            | "http://www.w3.org/2000/01/rdf-schema#subPropertyOf"
            | "http://www.w3.org/2000/01/rdf-schema#domain"
            | "http://www.w3.org/2000/01/rdf-schema#range"
    );

    if !is_rdfs_schema {
        return;
    }

    // Run only the RDFS entailment rules from the built-in 'rdfs' rule set.
    // This re-uses the full semi-naive evaluator but restricted to the rdfs
    // rule set — efficient because only RDFS rules apply here.
    let derived = run_inference("rdfs");
    pgrx::debug1!(
        "incremental_rdfs: triggered by predicate {} ({iri}): {} triples derived",
        pred_id,
        derived
    );
}

// ─── Constraint checking ──────────────────────────────────────────────────────

/// Check all active constraint rules (empty-head rules) for the given rule set
/// (or all rule sets if `rule_set` is NULL).  Returns violations as JSONB rows.
pub fn check_all_constraints(rule_set_filter: Option<&str>) -> Vec<pgrx::JsonB> {
    ensure_catalog();

    let sql = if rule_set_filter.is_some() {
        "SELECT rule_text FROM _pg_ripple.rules \
         WHERE head_pred IS NULL AND active = true AND rule_set = $1 \
         ORDER BY id"
    } else {
        "SELECT rule_text FROM _pg_ripple.rules \
         WHERE head_pred IS NULL AND active = true \
         ORDER BY id"
    };

    let args: Vec<pgrx::datum::DatumWithOid> = if let Some(rs) = rule_set_filter {
        vec![pgrx::datum::DatumWithOid::from(rs)]
    } else {
        vec![]
    };

    let rule_texts = Spi::connect(|client| {
        client
            .select(sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("constraint query SPI error: {e}"))
            .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
            .collect::<Vec<_>>()
    });

    let mut violations: Vec<pgrx::JsonB> = Vec::new();

    for rule_text in rule_texts {
        let rules = match parse_rules(&rule_text, "check") {
            Ok(rs) => rs.rules,
            Err(e) => {
                pgrx::warning!("constraint parse error: {e}");
                continue;
            }
        };
        for rule in &rules {
            if rule.head.is_some() {
                continue;
            }
            match compiler::compile_constraint_check(rule) {
                Ok(check_sql) => {
                    let violated = Spi::get_one_with_args::<bool>(&check_sql, &[])
                        .ok()
                        .flatten()
                        .unwrap_or(false);
                    if violated {
                        let mut obj = serde_json::Map::new();
                        obj.insert(
                            "rule".to_owned(),
                            serde_json::Value::String(rule.rule_text.clone()),
                        );
                        obj.insert("violated".to_owned(), serde_json::Value::Bool(true));
                        violations.push(pgrx::JsonB(serde_json::Value::Object(obj)));
                    }
                }
                Err(e) => pgrx::warning!("constraint compile error: {e}"),
            }
        }
    }

    violations
}

/// Build an on-demand CTE string for a derived predicate, to be prepended to
/// SPARQL→SQL output.  Returns `None` if the predicate is not derived.
#[allow(dead_code)]
pub fn get_on_demand_cte(pred_id: i64) -> Option<String> {
    let rule_text: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT r.rule_text FROM _pg_ripple.rules r \
         WHERE r.head_pred = $1 AND r.active = true \
         LIMIT 1",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten();

    let rule_text = rule_text?;

    let rules = match parse_rules(&rule_text, "on_demand") {
        Ok(rs) => rs.rules,
        Err(_) => return None,
    };

    let cte = compiler::compile_on_demand_cte(&rules, pred_id).ok()?;
    Some(cte)
}

// ─── Incremental rule updates (v0.34.0) ──────────────────────────────────────

/// Add a single rule to an existing rule set without triggering a full recompute.
///
/// The rule is parsed, stratified with the existing rules, and stored in the
/// catalog.  Only the new rule's derived predicate gets one fresh seed pass
/// using the current VP-table data.  Other derived predicates are not affected.
///
/// Returns the new rule's catalog ID on success, or an error string.
pub fn add_rule_to_set(rule_set_name: &str, rule_text: &str) -> Result<i64, String> {
    ensure_catalog();

    // Parse the new rule.
    let rs = parse_rules(rule_text, rule_set_name).map_err(|e| e.to_string())?;
    if rs.rules.is_empty() {
        return Err("no rules parsed from rule_text".to_owned());
    }

    // Ensure the rule set exists.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.rule_sets (name, active) \
         VALUES ($1, true) ON CONFLICT (name) DO UPDATE SET active = true",
        &[pgrx::datum::DatumWithOid::from(rule_set_name)],
    )
    .map_err(|e| e.to_string())?;

    let new_rule = &rs.rules[0];
    let head_pred: Option<i64> = new_rule.head.as_ref().and_then(|h| {
        if let Term::Const(id) = &h.p {
            Some(*id)
        } else {
            None
        }
    });

    // Determine stratum for the new rule.
    let max_stratum: i32 = Spi::get_one_with_args::<i32>(
        "SELECT COALESCE(MAX(stratum), 0) FROM _pg_ripple.rules WHERE rule_set = $1",
        &[pgrx::datum::DatumWithOid::from(rule_set_name)],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    let is_recursive = new_rule.head.as_ref().is_some_and(|h| {
        if let Term::Const(head_p) = &h.p {
            new_rule.body.iter().any(|lit| {
                if let BodyLiteral::Positive(atom) = lit {
                    if let Term::Const(body_p) = &atom.p {
                        body_p == head_p
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
        } else {
            false
        }
    });

    let new_rule_id: i64 = Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.rules \
             (rule_set, rule_text, head_pred, stratum, is_recursive) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        &[
            pgrx::datum::DatumWithOid::from(rule_set_name),
            pgrx::datum::DatumWithOid::from(rule_text),
            pgrx::datum::DatumWithOid::from(head_pred),
            pgrx::datum::DatumWithOid::from(max_stratum),
            pgrx::datum::DatumWithOid::from(is_recursive),
        ],
    )
    .map_err(|e| e.to_string())?
    .unwrap_or(0);

    // One fresh seed pass for the new rule's head predicate only.
    if let Some(pred_id) = head_pred {
        // Ensure HTAP tables exist.
        crate::storage::merge::ensure_htap_tables(pred_id);

        // Compile and execute the seed pass.
        let target = format!("_pg_ripple.vp_{pred_id}_delta");
        match compile_single_rule_to(new_rule, &target) {
            Ok(sql) => {
                if let Err(e) = Spi::run_with_args(&sql, &[]) {
                    pgrx::warning!("add_rule: seed pass error: {e}");
                }
            }
            Err(e) => pgrx::warning!("add_rule: rule compile error: {e}"),
        }
    }

    Ok(new_rule_id)
}

/// Remove a rule from a rule set and retract any derived facts solely supported
/// by it, using DRed internally when `pg_ripple.dred_enabled = true`.
///
/// Returns the number of derived triples permanently retracted.
pub fn remove_rule_by_id(rule_id: i64) -> Result<i64, String> {
    ensure_catalog();

    // Fetch the rule before deletion.
    let rule_info: Option<(String, Option<i64>)> = Spi::connect(|client| {
        client
            .select(
                "SELECT rule_set, head_pred FROM _pg_ripple.rules WHERE id = $1",
                None,
                &[pgrx::datum::DatumWithOid::from(rule_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("remove_rule: query error: {e}"))
            .next()
            .map(|row| {
                let rs: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let hp: Option<i64> = row.get::<i64>(2).ok().flatten();
                (rs, hp)
            })
    });

    let (rule_set_name, head_pred) = match rule_info {
        Some(info) => info,
        None => return Err(format!("no rule with id {rule_id}")),
    };

    // Mark rule as inactive (soft-delete so the ID can still be referenced).
    Spi::run_with_args(
        "UPDATE _pg_ripple.rules SET active = false WHERE id = $1",
        &[pgrx::datum::DatumWithOid::from(rule_id)],
    )
    .map_err(|e| e.to_string())?;

    let mut retracted: i64 = 0;

    // If there is a head predicate, retract derived facts for it.
    if let Some(pred_id) = head_pred {
        if crate::DRED_ENABLED.get() {
            // Check DRed safety — if unsafe, fall back to full recompute.
            match check_dred_safety(&rule_set_name) {
                Ok(()) => {
                    // DRed is safe: retract using a conservative approach.
                    // Over-delete all rows derived by the removed rule and
                    // re-derive survivors from remaining active rules.
                    let has_dedicated = pgrx::Spi::get_one_with_args::<i64>(
                        "SELECT table_oid::bigint FROM _pg_ripple.predicates \
                         WHERE id = $1 AND table_oid IS NOT NULL",
                        &[pgrx::datum::DatumWithOid::from(pred_id)],
                    )
                    .ok()
                    .flatten()
                    .is_some();

                    if has_dedicated {
                        // Clear the delta table and re-run all remaining active rules.
                        Spi::run_with_args(
                            &format!("DELETE FROM _pg_ripple.vp_{pred_id}_delta WHERE source = 1"),
                            &[],
                        )
                        .unwrap_or_else(|e| pgrx::warning!("remove_rule: delta clear error: {e}"));
                    } else {
                        let deleted = Spi::get_one_with_args::<i64>(
                            "WITH del AS (DELETE FROM _pg_ripple.vp_rare WHERE p = $1 AND source = 1 RETURNING 1) \
                             SELECT count(*) FROM del",
                            &[pgrx::datum::DatumWithOid::from(pred_id)],
                        )
                        .unwrap_or(None)
                        .unwrap_or(0);
                        retracted += deleted;
                    }

                    // Re-run remaining active rules for this head_pred.
                    let remaining_rules: Vec<String> = {
                        let sql = "SELECT rule_text FROM _pg_ripple.rules \
                                   WHERE rule_set = $1 AND active = true AND head_pred = $2";
                        Spi::connect(|client| {
                            client
                                .select(
                                    sql,
                                    None,
                                    &[
                                        pgrx::datum::DatumWithOid::from(rule_set_name.as_str()),
                                        pgrx::datum::DatumWithOid::from(pred_id),
                                    ],
                                )
                                .unwrap_or_else(|e| {
                                    pgrx::error!("remove_rule: re-derive query error: {e}")
                                })
                                .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
                                .collect::<Vec<_>>()
                        })
                    };

                    for rt in &remaining_rules {
                        if let Ok(rs) = parse_rules(rt, &rule_set_name) {
                            for rule in &rs.rules {
                                if rule.head.is_none() {
                                    continue;
                                }
                                let target = if has_dedicated {
                                    format!("_pg_ripple.vp_{pred_id}_delta")
                                } else {
                                    "_pg_ripple.vp_rare".to_owned()
                                };
                                if let Ok(sql) = compile_single_rule_to(rule, &target) {
                                    let _ = Spi::run_with_args(&sql, &[]);
                                }
                            }
                        }
                    }
                }
                Err(warning) => {
                    // Unsafe for DRed — fall back to full recompute.
                    pgrx::warning!("{warning}");
                    let (derived, _) = run_inference_seminaive(&rule_set_name);
                    retracted = derived;
                }
            }
        } else {
            // DRed disabled — full recompute.
            let (derived, _) = run_inference_seminaive(&rule_set_name);
            retracted = derived;
        }
    }

    Ok(retracted)
}
