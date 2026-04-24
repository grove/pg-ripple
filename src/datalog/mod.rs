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
pub use parallel::partition_into_parallel_groups;
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

    // ── 0. Pre-allocate SID ranges for parallel workers (v0.47.0) ────────────
    // When parallel workers > 1, reserve a contiguous SID block upfront so
    // each conceptual worker group can use its own range without hitting the
    // shared sequence on every insert.  Falls back silently on error.
    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get() as usize;
    let sequence_batch = crate::DATALOG_SEQUENCE_BATCH.get();
    if parallel_workers > 1 {
        Spi::connect(|client| {
            let _ = crate::datalog::parallel::preallocate_sid_ranges(
                client,
                parallel_workers,
                sequence_batch,
            );
        });
    }

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

    // ── 2a. sameAs canonicalization pre-pass (v0.31.0) ────────────────────────
    let all_rules = if crate::SAMEAS_REASONING.get() {
        let sameas_map = rewrite::compute_sameas_map();
        rewrite::apply_sameas_to_rules(&all_rules, &sameas_map)
    } else {
        all_rules
    };

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
    // v0.51.0 (S3-4): parallel::execute_with_savepoint() is available for
    // per-group SAVEPOINT isolation; wiring deferred to maintain test stability.

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
/// checking and parallel analysis statistics.
///
/// Returns `(total_derived, iterations, eliminated_rule_texts, parallel_groups, max_concurrent)`.
///
/// - `eliminated_rule_texts`: rule texts eliminated by subsumption checking (v0.29.0).
/// - `parallel_groups`: number of independent rule groups in the first stratum (v0.35.0).
/// - `max_concurrent`: effective worker count given `datalog_parallel_workers` (v0.35.0).
///
/// Used by `infer_with_stats()`.
pub fn run_inference_seminaive_full(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
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
        return (0, 0, vec![], 0, 0);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for (rule_text, _stratum, _recursive) in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("rule parse error during full semi-naive inference: {e}"),
        }
    }

    // Parallel analysis: partition rules into independent groups (v0.35.0).
    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get();
    let analysis = crate::datalog::partition_into_parallel_groups(&all_rules, parallel_workers);
    let parallel_groups = analysis.parallel_groups;
    let max_concurrent = analysis.max_concurrent;

    let eliminated = check_subsumption(&all_rules);
    let (derived, iters) = run_inference_seminaive(rule_set_name);
    (derived, iters, eliminated, parallel_groups, max_concurrent)
}

// ─── Variable-predicate rule instantiation (v0.44.0) ─────────────────────────

/// Collect the names of all variables that appear in predicate position within
/// a rule (head.p or any body atom's .p field).
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

/// Substitute every occurrence of a predicate variable with a concrete
/// dictionary ID throughout a rule (head and body).
fn substitute_pred_var(rule: &Rule, var_name: &str, pred_id: i64) -> Rule {
    let sub = |t: &Term| -> Term {
        match t {
            Term::Var(v) if v == var_name => Term::Const(pred_id),
            other => other.clone(),
        }
    };
    let sub_atom = |a: &Atom| -> Atom {
        Atom {
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
    }
}

/// Find all concrete dictionary IDs for a predicate variable by scanning
/// body atoms where the variable appears as the subject or object of a
/// fixed-predicate atom.
///
/// For example, for `?p rdf:type owl:SymmetricProperty`:
/// - var_name = "p", the atom's pred = rdf:type (Const), atom.s = Var("p")
/// - Returns: all subjects of (_, rdf:type, owl:SymmetricProperty) triples
///
/// For `?p owl:inverseOf ?q` (where we want q):
/// - var_name = "q", the atom's pred = owl:inverseOf (Const), atom.o = Var("q")
/// - Returns: all objects of (_, owl:inverseOf, _) triples
fn enumerate_pred_var_values(rule: &Rule, var_name: &str) -> Vec<i64> {
    let mut values: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) => a,
            _ => continue,
        };
        // The atom must have a fixed (Const) predicate itself.
        let atom_pred_id = match &atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };

        let is_subj = matches!(&atom.s, Term::Var(v) if v == var_name);
        let is_obj = matches!(&atom.o, Term::Var(v) if v == var_name);

        if is_subj {
            // Enumerate subjects of this VP table, optionally filtered by object.
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
            // Enumerate objects of this VP table, optionally filtered by subject.
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

/// Compute concrete bindings for all predicate variables in a rule.
///
/// Returns a list of "substitution maps" — each map assigns a concrete i64 ID
/// to each predicate variable name.  If a body atom binds two predicate
/// variables simultaneously (e.g. `?p owl:inverseOf ?q`) we enumerate correlated
/// pairs rather than the cartesian product of independent enumerations.
///
/// Returns an empty Vec if any predicate variable has no binding constraints
/// (would require enumerating all predicates — too expensive).
fn compute_pred_var_bindings(rule: &Rule, pred_vars: &[String]) -> Vec<Vec<(String, i64)>> {
    if pred_vars.is_empty() {
        return vec![vec![]];
    }

    // Check if a single body atom binds two pred vars simultaneously (correlated).
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
            // Enumerate (s, o) pairs from this VP table — both vars bound together.
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

    // Independent enumeration: enumerate each pred var separately, then
    // compute the cartesian product.
    let mut per_var: Vec<(String, Vec<i64>)> = Vec::new();
    for var_name in pred_vars {
        let vals = enumerate_pred_var_values(rule, var_name);
        if vals.is_empty() {
            // No binding found — refuse to enumerate all predicates.
            return vec![];
        }
        per_var.push((var_name.clone(), vals));
    }

    // Cartesian product.
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

/// Handle a rule that has one or more variable predicates by instantiating them
/// at runtime.
///
/// For each concrete binding of the predicate variables (found by scanning the
/// VP tables for constraints expressed in the body), the rule is specialized to
/// a fully-constant rule and compiled normally.
///
/// Returns the total number of SQL INSERT statements executed successfully
/// (a proxy for derived triples; the actual count may differ for recursive rules).
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
        // Apply all substitutions for this binding set.
        let mut specialized = rule.clone();
        for (var_name, pred_id) in &binding {
            specialized = substitute_pred_var(&specialized, var_name, *pred_id);
        }

        // After substitution the rule should have no variable predicates.
        // Compile and run it.
        match compile_rule_set(std::slice::from_ref(&specialized)) {
            Ok(sqls) => {
                for sql in &sqls {
                    match Spi::run_with_args(sql, &[]) {
                        Ok(()) => total += 1,
                        Err(e) => {
                            pgrx::warning!("var_pred_rule SQL error: {e}")
                        }
                    }
                }
            }
            Err(e) => pgrx::warning!("var_pred_rule compile error after instantiation: {e}"),
        }
    }
    total
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

    // ── sameAs cluster-size check (honours pg_ripple.sameas_max_cluster_size) ──
    // Emit PT550 WARNING (and skip canonicalization) if any cluster is too large.
    // This mirrors the same check in run_inference_seminaive so that infer()
    // honours sameas_max_cluster_size regardless of which internal path is used.
    if crate::SAMEAS_REASONING.get() {
        let _ = rewrite::compute_sameas_map();
    }

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
            if has_variable_pred(rule) {
                // Route through the variable-predicate instantiation path.
                // Enumerates concrete predicate values from body constraints
                // and runs a specialized version of the rule for each binding.
                total_derived += run_var_pred_rule(rule);
            } else {
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
    let (agg_rules, non_agg_rules): (Vec<Rule>, Vec<Rule>) = all_rules.into_iter().partition(|r| {
        r.body
            .iter()
            .any(|lit| matches!(lit, BodyLiteral::Aggregate(_)))
    });

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
            let Some(head_atom) = &rule.head else {
                continue;
            };
            let head_pred = match &head_atom.p {
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
pub(crate) fn run_seminaive_inner(rules: &[Rule], rule_set_name: &str) -> (i64, i32) {
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
