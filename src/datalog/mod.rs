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
pub mod magic;
pub mod parser;
pub mod stratify;

pub use compiler::compile_aggregate_rule;
pub use compiler::compile_rule_delta_variants_to;
pub use compiler::compile_rule_set;
pub use compiler::compile_single_rule_to;
pub use magic::parse_goal;
pub use magic::run_infer_goal;
pub use parser::parse_rules;
pub use stratify::check_aggregation_stratification;
pub use stratify::check_subsumption;
pub use stratify::stratify;

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

    // Stratify the rule set.
    let stratified = match stratify(rules) {
        Ok(s) => s,
        Err(e) => pgrx::error!("{e}"),
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

// ─── Inference execution ──────────────────────────────────────────────────────

/// Execute on-demand materialization for a rule set using **semi-naive evaluation**.
///
/// Returns `(total_triples_derived, iteration_count)`.
///
/// ## Semi-naive algorithm
///
/// For each stratum S of the rule set:
/// 1. **Seed**: run all rules once against the full VP tables to get the first
///    round of derived triples.  Store these new triples in both the VP delta
///    tables and temporary `_dl_delta_{pred_id}` tables.
/// 2. **Fixpoint loop**: on each subsequent iteration, generate one SQL variant
///    per body atom that references a derived predicate.  Each variant uses the
///    `_dl_delta_{pred_id}` table (triples derived in the *previous* iteration)
///    instead of the full VP table for that atom, and the full VP tables for all
///    other atoms.  This ensures only genuinely *new* derivations are attempted.
/// 3. Terminate when no iteration produces any new triples.
///
/// The number of iterations is bounded by the longest derivation chain in the
/// data, not by the total relation size — the key semi-naive property.
pub fn run_inference_seminaive(rule_set_name: &str) -> (i64, i32) {
    ensure_catalog();

    // ── 1. Load rules from catalog ────────────────────────────────────────────
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

    // ── 2. Parse all rules ────────────────────────────────────────────────────
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

    // ── 3. Collect derived predicate IDs (rule heads) ─────────────────────────
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

    // ── 3a. Subsumption checking (v0.29.0) ────────────────────────────────────
    // Check for subsumed rules and exclude them from the fixpoint evaluation.
    // Subsumed rules are those whose body atoms form a superset of another
    // rule's body atoms with the same head predicate.
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

    // ── 4. Create delta temp tables for each derived predicate ─────────────────
    // We use temp tables exclusively to avoid creating permanent HTAP tables for
    // predicates that may be below the promotion threshold.  Derived triples are
    // materialised into vp_rare at the end of inference.
    for &pred_id in &derived_pred_ids {
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
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

    // ── 5. Seeding pass: run all rules once, inserting into temp delta tables ──
    let seed_target_fn = |pred_id: i64| -> String { format!("_dl_delta_{pred_id}") };
    for rule in &active_rules {
        if rule.head.is_none() {
            continue;
        }
        let head_pred = match &rule.head.as_ref().unwrap().p {
            Term::Const(id) => *id,
            _ => continue,
        };
        if !derived_pred_ids.contains(&head_pred) {
            continue;
        }
        let target = seed_target_fn(head_pred);
        match compile_single_rule_to(rule, &target) {
            Ok(sql) => {
                if let Err(e) = Spi::run_with_args(&sql, &[]) {
                    pgrx::warning!("semi-naive seed SQL error: {e}: SQL={sql}");
                }
            }
            Err(e) => pgrx::warning!("semi-naive rule compile error: {e}"),
        }
    }

    // ── 5a. Delta table indexing (v0.29.0) ────────────────────────────────────
    // After the seeding pass, create B-tree indices on delta tables that have
    // enough rows to benefit from index access in subsequent fixpoint iterations.
    let delta_index_threshold = crate::DELTA_INDEX_THRESHOLD.get() as i64;
    if delta_index_threshold > 0 {
        for &pred_id in &derived_pred_ids {
            let row_cnt = Spi::get_one::<i64>(&format!("SELECT count(*) FROM _dl_delta_{pred_id}"))
                .unwrap_or(None)
                .unwrap_or(0);
            if row_cnt >= delta_index_threshold {
                // Create a B-tree index on the join columns used by the next iteration.
                let idx_name = format!("_dl_delta_{pred_id}_so_idx");
                let _ = Spi::run_with_args(&format!("DROP INDEX IF EXISTS {idx_name}"), &[]);
                let _ = Spi::run_with_args(
                    &format!("CREATE INDEX {idx_name} ON _dl_delta_{pred_id} (s, o)"),
                    &[],
                );
            }
        }
    }

    // ── 6. Fixpoint loop ───────────────────────────────────────────────────────
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

        // Create "new delta" temp tables.
        for &pred_id in &derived_pred_ids {
            let _ = Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
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
            if rule.head.is_none() {
                continue;
            }
            let head_pred = match &rule.head.as_ref().unwrap().p {
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

        // Count new rows across all "new delta" tables.
        for &pred_id in &derived_pred_ids {
            // Only count rows NOT already in the current delta.
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

        // Merge new delta into delta (union).
        for &pred_id in &derived_pred_ids {
            let _ = Spi::run_with_args(
                &format!(
                    "INSERT INTO _dl_delta_{pred_id} (s, o, g) \
                     SELECT s, o, g FROM _dl_delta_new_{pred_id} \
                     ON CONFLICT DO NOTHING"
                ),
                &[],
            );
            let _ = Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
        }

        if new_this_iter == 0 {
            break;
        }
    }

    // ── 7. Materialise derived triples into vp_rare ───────────────────────────
    // Insert derived triples permanently so they are visible to subsequent queries.
    // vp_rare accepts all predicates via its (p, s, o, g) schema.
    let mut total_derived: i64 = 0;
    for &pred_id in &derived_pred_ids {
        let cnt = Spi::get_one::<i64>(&format!(
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

        // Update predicate count in catalog.
        if cnt > 0 {
            let _ = Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
                 VALUES ($1, NULL, $2) \
                 ON CONFLICT (id) DO UPDATE \
                     SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(cnt),
                ],
            );
        }
    }

    // ── 8. Cleanup temp tables ─────────────────────────────────────────────────
    for &pred_id in &derived_pred_ids {
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
        let _ = Spi::run_with_args(
            &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
            &[],
        );
    }

    (total_derived, iteration_count)
}

/// Like `run_inference_seminaive` but also returns eliminated rules from subsumption
/// checking.  Returns `(total_derived, iterations, eliminated_rule_texts)`.
///
/// Used by `infer_with_stats()` to populate the `"eliminated_rules"` field (v0.29.0).
pub fn run_inference_seminaive_full(rule_set_name: &str) -> (i64, i32, Vec<String>) {
    ensure_catalog();

    // Load all rules to check subsumption before running inference.
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
        return (0, 0, vec![]);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for (rule_text, _stratum, _recursive) in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("rule parse error during full semi-naive inference: {e}"),
        }
    }

    let eliminated = check_subsumption(&all_rules);
    let (derived, iters) = run_inference_seminaive(rule_set_name);
    (derived, iters, eliminated)
}

/// Execute on-demand materialization for a rule set: run all rules in stratum
/// order and insert derived triples.  Returns the number of triples derived.
pub fn run_inference(rule_set_name: &str) -> i64 {
    ensure_catalog();

    // Fetch rules ordered by stratum.
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

    let mut total_derived = 0i64;

    for (rule_text, _stratum, _recursive) in rule_rows {
        let rules = match parse_rules(&rule_text, rule_set_name) {
            Ok(rs) => rs.rules,
            Err(e) => {
                pgrx::warning!("rule parse error during inference: {e}");
                continue;
            }
        };

        for rule in &rules {
            match compile_rule_set(std::slice::from_ref(rule)) {
                Ok(sqls) => {
                    for sql in sqls {
                        match Spi::run_with_args(&sql, &[]) {
                            Ok(()) => total_derived += 1,
                            Err(e) => pgrx::warning!("inference SQL error: {e}: SQL={sql}"),
                        }
                    }
                }
                Err(e) => pgrx::warning!("rule compile error: {e}"),
            }
        }
    }

    total_derived
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

// ─── v0.30.0: Aggregation inference ──────────────────────────────────────────

/// Run inference for a rule set that may contain aggregate body literals
/// (Datalog^agg, v0.30.0).
///
/// Returns `(total_derived, aggregate_derived, iteration_count)`.
///
/// - Non-aggregate rules are compiled and executed as in `run_inference_seminaive`.
/// - Aggregate rules (those with `BodyLiteral::Aggregate`) are compiled to GROUP BY
///   SQL and executed once (not in a fixpoint — aggregates are non-recursive by the
///   aggregation-stratification constraint).
/// - PT510 is emitted as a WARNING if a cycle through an aggregate is detected.
pub fn run_inference_agg(rule_set_name: &str) -> (i64, i64, i32) {
    ensure_catalog();

    // ── 1. Load rule texts from catalog ──────────────────────────────────────
    let rule_rows: Vec<String> = {
        let sql = "SELECT rule_text \
                   FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        Spi::connect(|client| {
            client
                .select(sql, None, &[pgrx::datum::DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("rule select SPI error: {e}"))
                .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
                .collect::<Vec<_>>()
        })
    };

    if rule_rows.is_empty() {
        return (0, 0, 0);
    }

    // ── 2. Parse all rules ────────────────────────────────────────────────────
    let mut all_rules: Vec<Rule> = Vec::new();
    for rule_text in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("infer_agg: rule parse error: {e}"),
        }
    }

    if all_rules.is_empty() {
        return (0, 0, 0);
    }

    // ── 3. Check aggregation stratification (PT510) ───────────────────────────
    if let Err(e) = check_aggregation_stratification(&all_rules) {
        pgrx::warning!(
            "infer_agg: aggregation stratification violation (PT510): {}; \
             aggregate rules will be skipped",
            e
        );
        // Fall back to running only non-aggregate rules.
        let non_agg_rules: Vec<Rule> = all_rules
            .iter()
            .filter(|r| {
                !r.body
                    .iter()
                    .any(|lit| matches!(lit, BodyLiteral::Aggregate(_)))
            })
            .cloned()
            .collect();
        let (derived, iters) = run_seminaive_inner(&non_agg_rules, rule_set_name);
        return (derived, 0, iters);
    }

    // ── 4. Separate aggregate rules from non-aggregate rules ──────────────────
    let (agg_rules, non_agg_rules): (Vec<Rule>, Vec<Rule>) = all_rules.into_iter().partition(
        |r| r.body.iter().any(|lit| matches!(lit, BodyLiteral::Aggregate(_))),
    );

    // ── 5. Run non-aggregate rules via semi-naive evaluation ──────────────────
    let (normal_derived, iterations) = if !non_agg_rules.is_empty() {
        run_seminaive_inner(&non_agg_rules, rule_set_name)
    } else {
        (0, 0)
    };

    // ── 6. Run aggregate rules (single pass, GROUP BY SQL) ────────────────────
    let mut agg_derived: i64 = 0;

    // Try the plan cache first (v0.30.0).
    let cached_sqls = cache::lookup_agg(rule_set_name);
    let agg_sqls: Vec<String> = if let Some(sqls) = cached_sqls {
        sqls
    } else {
        let mut compiled = Vec::new();
        for rule in &agg_rules {
            if rule.head.is_none() {
                continue;
            }
            let head_pred = match &rule.head.as_ref().unwrap().p {
                Term::Const(id) => *id,
                _ => continue,
            };
            // Ensure HTAP tables exist for the head predicate.
            crate::storage::merge::ensure_htap_tables(head_pred);
            let target = format!("_pg_ripple.vp_{head_pred}_delta");

            match compile_aggregate_rule(rule, &target) {
                Ok(sql) => compiled.push(sql),
                Err(e) => pgrx::warning!("infer_agg: aggregate rule compile error: {e}"),
            }
        }
        cache::store_agg(rule_set_name, &compiled);
        compiled
    };

    for sql in &agg_sqls {
        match Spi::get_one::<i64>(&format!(
            "WITH ins AS ({sql} RETURNING 1) SELECT COUNT(*)::bigint FROM ins"
        )) {
            Ok(Some(n)) => agg_derived += n,
            Ok(None) => {}
            Err(e) => pgrx::warning!("infer_agg: aggregate SQL execution error: {e}"),
        }
    }

    (normal_derived + agg_derived, agg_derived, iterations)
}

/// Inner helper: run semi-naive inference over a specific set of (non-aggregate)
/// rules and materialise results into vp_rare.  Returns (total_derived, iterations).
fn run_seminaive_inner(rules: &[Rule], rule_set_name: &str) -> (i64, i32) {
    // Collect derived predicate IDs.
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

    // Create delta temp tables.
    for &pred_id in &derived_pred_ids {
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
        Spi::run_with_args(
            &format!(
                "CREATE TEMP TABLE _dl_delta_{pred_id} \
                 (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                  UNIQUE (s, o, g))"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("run_seminaive_inner: delta table error: {e}"));
    }

    // Seeding pass.
    for rule in rules {
        if rule.head.is_none() {
            continue;
        }
        let head_pred = match &rule.head.as_ref().unwrap().p {
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

    // Fixpoint loop.
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
            let _ = Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
            Spi::run_with_args(
                &format!(
                    "CREATE TEMP TABLE _dl_delta_new_{pred_id} \
                     (s BIGINT NOT NULL, o BIGINT NOT NULL, g BIGINT NOT NULL DEFAULT 0, \
                      UNIQUE (s, o, g))"
                ),
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("run_seminaive_inner: delta_new error: {e}"));
        }

        let mut new_this_iter = 0i64;
        let delta_fn = |pred_id: i64| -> String { format!("_dl_delta_{pred_id}") };
        let new_delta_fn = |pred_id: i64| -> String { format!("_dl_delta_new_{pred_id}") };

        for rule in rules {
            if rule.head.is_none() {
                continue;
            }
            let head_pred = match &rule.head.as_ref().unwrap().p {
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
                 WHERE NOT EXISTS (SELECT 1 FROM _dl_delta_{pred_id} d \
                 WHERE d.s=n.s AND d.o=n.o AND d.g=n.g)"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            new_this_iter += cnt;
        }

        for &pred_id in &derived_pred_ids {
            let _ = Spi::run_with_args(
                &format!(
                    "INSERT INTO _dl_delta_{pred_id} (s,o,g) \
                     SELECT s,o,g FROM _dl_delta_new_{pred_id} ON CONFLICT DO NOTHING"
                ),
                &[],
            );
            let _ = Spi::run_with_args(
                &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
                &[],
            );
        }

        if new_this_iter == 0 {
            break;
        }
    }

    // Materialise into vp_rare.
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
            let _ = Spi::run_with_args(
                "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
                 VALUES ($1, NULL, $2) ON CONFLICT (id) DO UPDATE \
                 SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(cnt),
                ],
            );
        }
    }

    // Cleanup.
    for &pred_id in &derived_pred_ids {
        let _ = Spi::run_with_args(&format!("DROP TABLE IF EXISTS _dl_delta_{pred_id}"), &[]);
        let _ = Spi::run_with_args(
            &format!("DROP TABLE IF EXISTS _dl_delta_new_{pred_id}"),
            &[],
        );
    }

    (total, iteration_count)
}
