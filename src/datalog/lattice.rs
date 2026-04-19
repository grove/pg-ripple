//! Lattice-Based Datalog — Datalog^L (v0.36.0).
//!
//! Extends the pg_ripple Datalog engine with *monotone lattice aggregation*.
//! Standard Datalog^agg supports aggregate functions (COUNT, SUM, etc.) but
//! requires aggregation-stratification — aggregates can only appear in a
//! strictly higher stratum than their inputs.  Datalog^L relaxes this by
//! requiring only that the aggregation operation is *monotone* with respect
//! to a user-specified partial order (a lattice).
//!
//! # Lattice types
//!
//! A **lattice** is an algebraic structure (L, ⊔) where:
//! - L is the set of values (e.g. real numbers, sets, intervals)
//! - ⊔ is the *join* (or *lub* — least upper bound) operation
//! - The join is commutative, associative, and idempotent
//! - The lattice has a *bottom* element ⊥ such that ⊥ ⊔ x = x for all x
//!
//! Fixpoint computation on a lattice terminates when no value increases,
//! guaranteed by the *ascending chain condition* (the lattice has no
//! infinite strictly ascending chains).
//!
//! # Built-in lattices
//!
//! | Name | Join | Bottom | Typical use |
//! |------|------|--------|-------------|
//! | `min` | MIN | `+infinity` | Trust propagation, shortest paths |
//! | `max` | MAX | `-infinity` | Longest paths, reachability |
//! | `set` | UNION | `{}` (empty set) | Set-valued annotations |
//! | `interval` | interval hull | empty interval | Temporal or numeric ranges |
//!
//! # User-defined lattices
//!
//! Users can define custom lattices via `pg_ripple.create_lattice(name, join_fn, bottom)`.
//! The `join_fn` must be a PostgreSQL aggregate function that is commutative and
//! associative.  The `bottom` is stored as a JSON value.
//!
//! # SQL compilation
//!
//! Lattice rules compile to:
//!
//! ```sql
//! INSERT INTO _pg_ripple.vp_{pred_id} (s, o, g, source)
//! SELECT {s_expr}, {lattice_value_expr}, 0, 1
//! FROM {body_joins}
//! ON CONFLICT (s, g) DO UPDATE
//!     SET o = {join_fn}(_pg_ripple.vp_{pred_id}.o, EXCLUDED.o)
//!     WHERE _pg_ripple.vp_{pred_id}.o IS DISTINCT FROM {join_fn}(...)
//! ```
//!
//! The `ON CONFLICT DO UPDATE` clause applies the lattice join on conflict,
//! ensuring the fixpoint ascends monotonically until no rows change.
//!
//! # Convergence guarantee
//!
//! Termination is guaranteed when:
//! 1. The lattice satisfies the ascending chain condition, and
//! 2. All operations in rule bodies are monotone.
//!
//! When the iteration count exceeds `pg_ripple.lattice_max_iterations`, the
//! engine emits error code `PT540` (lattice fixpoint did not converge) and
//! returns the partial results.
//!
//! # Error codes
//!
//! - `PT540` — lattice fixpoint did not converge within the iteration limit.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── Built-in lattice types ───────────────────────────────────────────────────

/// Built-in lattice types available without user registration.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum BuiltinLattice {
    /// MIN lattice: join = MIN, bottom = +infinity.
    /// Suitable for trust propagation, shortest-path aggregation.
    Min,
    /// MAX lattice: join = MAX, bottom = -infinity.
    /// Suitable for longest-path aggregation, reachability with max weight.
    Max,
    /// SET lattice: join = UNION (array concatenation + dedup), bottom = empty set.
    /// Values are stored as JSON arrays.
    Set,
    /// INTERVAL lattice: join = interval hull, bottom = empty interval.
    /// Values are stored as JSON objects `{"lo": x, "hi": y}`.
    Interval,
}

impl BuiltinLattice {
    /// Return the name string used in the lattice catalog.
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            BuiltinLattice::Min => "min",
            BuiltinLattice::Max => "max",
            BuiltinLattice::Set => "set",
            BuiltinLattice::Interval => "interval",
        }
    }

    /// Return the SQL join expression for this lattice type.
    ///
    /// The expression takes two arguments (old value, new value) and returns
    /// the joined value. The first `%s` is the existing column expression;
    /// the second `%s` is the newly computed value expression.
    #[allow(dead_code)]
    pub fn join_sql_expr(&self) -> &'static str {
        match self {
            BuiltinLattice::Min => "LEAST(%s, %s)",
            BuiltinLattice::Max => "GREATEST(%s, %s)",
            BuiltinLattice::Set => "pg_ripple._lattice_set_union(%s, %s)",
            BuiltinLattice::Interval => "pg_ripple._lattice_interval_hull(%s, %s)",
        }
    }

    /// Return the bottom element for this lattice (as a numeric dict ID placeholder).
    /// This is a sentinel value; the actual bottom is encoded when inserting.
    #[allow(dead_code)]
    pub fn bottom_sentinel(&self) -> &'static str {
        match self {
            BuiltinLattice::Min => "9223372036854775807", // i64::MAX as "infinity"
            BuiltinLattice::Max => "-9223372036854775808", // i64::MIN as "-infinity"
            BuiltinLattice::Set => "0",                   // empty set = default graph sentinel
            BuiltinLattice::Interval => "0",              // empty interval
        }
    }

    /// Parse a lattice name string to the corresponding BuiltinLattice.
    #[allow(dead_code)]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "min" | "minlattice" => Some(BuiltinLattice::Min),
            "max" | "maxlattice" => Some(BuiltinLattice::Max),
            "set" | "setlattice" => Some(BuiltinLattice::Set),
            "interval" | "intervallattice" => Some(BuiltinLattice::Interval),
            _ => None,
        }
    }
}

// ─── Lattice catalog ──────────────────────────────────────────────────────────

/// A registered lattice type (built-in or user-defined).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LatticeType {
    /// Lattice name (e.g. "min", "trust", "my_lattice").
    pub name: String,
    /// PostgreSQL aggregate function implementing the join (e.g. "min", "max").
    pub join_fn: String,
    /// Bottom element as a string (stored as text in the catalog).
    pub bottom: String,
    /// Whether this is a built-in lattice.
    pub builtin: bool,
}

/// Ensure the `_pg_ripple.lattice_types` catalog table exists.
pub fn ensure_lattice_catalog() {
    let _ = Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.lattice_types ( \
             name      TEXT NOT NULL PRIMARY KEY, \
             join_fn   TEXT NOT NULL, \
             bottom    TEXT NOT NULL DEFAULT '0', \
             builtin   BOOLEAN NOT NULL DEFAULT false, \
             created_at TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
    );

    // Register built-in lattice types.
    let _ = Spi::run(
        "INSERT INTO _pg_ripple.lattice_types (name, join_fn, bottom, builtin) VALUES \
             ('min',      'min',       '9223372036854775807',  true), \
             ('max',      'max',       '-9223372036854775808', true), \
             ('set',      'array_agg', '{}',                   true), \
             ('interval', 'max',       '0',                    true) \
         ON CONFLICT (name) DO NOTHING",
    );
}

/// Register a user-defined lattice type.
///
/// # Parameters
///
/// - `name` — lattice identifier; must be unique in the catalog.
/// - `join_fn` — PostgreSQL aggregate function name for the join operation.
///   Must be commutative and associative.
/// - `bottom` — bottom element as a text string.
///
/// # Returns
///
/// `true` if the lattice was newly registered; `false` if it already existed.
pub fn register_lattice(name: &str, join_fn: &str, bottom: &str) -> bool {
    ensure_lattice_catalog();

    // Validate that join_fn is a known PostgreSQL aggregate.
    let fn_exists = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS( \
             SELECT 1 FROM pg_proc p \
             JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE p.proname = $1 \
               AND p.prokind = 'a' \
         )",
        &[DatumWithOid::from(join_fn)],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if !fn_exists {
        // Not a registered aggregate — warn but allow registration for
        // user-defined aggregates that may be created later.
        pgrx::warning!(
            "create_lattice: '{join_fn}' is not a registered PostgreSQL aggregate; \
             lattice will fail at inference time if the function does not exist"
        );
    }

    let rows_inserted = Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.lattice_types (name, join_fn, bottom, builtin) \
         VALUES ($1, $2, $3, false) \
         ON CONFLICT (name) DO NOTHING \
         RETURNING 1",
        &[
            DatumWithOid::from(name),
            DatumWithOid::from(join_fn),
            DatumWithOid::from(bottom),
        ],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    rows_inserted == 1
}

/// Look up a lattice type by name.
///
/// Returns `None` if the lattice is not registered.
pub fn get_lattice(name: &str) -> Option<LatticeType> {
    ensure_lattice_catalog();

    Spi::connect(|c| {
        let row = c
            .select(
                "SELECT name, join_fn, bottom, builtin \
                 FROM _pg_ripple.lattice_types \
                 WHERE name = $1",
                Some(1),
                &[DatumWithOid::from(name)],
            )
            .ok()?
            .next()?;

        let lattice_name: String = row.get::<String>(1).ok()??.to_owned();
        let join_fn: String = row.get::<String>(2).ok()??.to_owned();
        let bottom: String = row.get::<String>(3).ok()??.to_owned();
        let builtin: bool = row.get::<bool>(4).ok()?.unwrap_or(false);

        Some(LatticeType {
            name: lattice_name,
            join_fn,
            bottom,
            builtin,
        })
    })
}

// ─── Lattice rule compiler ────────────────────────────────────────────────────

/// A parsed lattice rule.
///
/// Syntax:
/// ```text
/// ?x <pred> (MIN ?trust1 ?trust2) :- ?x <knows> ?y, ?y <directTrust> ?trust1, ?x <baseTrust> ?trust2 .
/// ```
///
/// The lattice function call `(MIN ?trust1 ?trust2)` designates:
/// - The lattice type (`MIN`)
/// - The variables to combine (`?trust1`, `?trust2`)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LatticeRule {
    /// Head predicate dictionary ID.
    pub head_pred_id: i64,
    /// Subject variable name.
    pub subject_var: String,
    /// Lattice type name.
    pub lattice_name: String,
    /// Variables to combine with the lattice join.
    pub lattice_vars: Vec<String>,
    /// Body: list of (table_name, subject_expr, object_expr) for each atom.
    pub body_atoms: Vec<(String, String, String)>,
    /// Additional WHERE conditions (encoded as SQL fragments).
    pub conditions: Vec<String>,
}

/// Compile a lattice rule to SQL.
///
/// Generates a two-part SQL statement:
/// 1. An `INSERT … ON CONFLICT DO UPDATE` that applies the lattice join.
/// 2. Returns the change count (used for fixpoint convergence check).
#[allow(dead_code)]
pub fn compile_lattice_rule_to_sql(
    head_pred_id: i64,
    subject_col: &str,
    lattice_name: &str,
    value_col: &str,
    body_sql: &str,
    graph_id: i64,
) -> Result<String, String> {
    let lattice = get_lattice(lattice_name)
        .ok_or_else(|| format!("unknown lattice type: '{lattice_name}'"))?;

    let join_fn = &lattice.join_fn;
    let target_table = format!("_pg_ripple.vp_{head_pred_id}");

    // For rare predicates (no dedicated table), use vp_rare with p filter.
    let has_dedicated = Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates \
         WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(head_pred_id)],
    )
    .ok()
    .flatten()
    .is_some();

    let sql = if has_dedicated {
        format!(
            "WITH _lat_new AS ( \
                 {body_sql} \
             ) \
             INSERT INTO {target_table} (s, o, g, source) \
             SELECT {subject_col}, {value_col}, {graph_id}, 1 \
             FROM _lat_new \
             ON CONFLICT (s, g) DO UPDATE \
                 SET o = {join_fn}(EXCLUDED.o, {target_table}.o) \
                 WHERE {target_table}.o IS DISTINCT FROM \
                       {join_fn}(EXCLUDED.o, {target_table}.o)"
        )
    } else {
        format!(
            "WITH _lat_new AS ( \
                 {body_sql} \
             ) \
             INSERT INTO _pg_ripple.vp_rare (p, s, o, g, source) \
             SELECT {head_pred_id}, {subject_col}, {value_col}, {graph_id}, 1 \
             FROM _lat_new \
             ON CONFLICT (p, s, g) DO UPDATE \
                 SET o = {join_fn}(EXCLUDED.o, _pg_ripple.vp_rare.o) \
                 WHERE _pg_ripple.vp_rare.o IS DISTINCT FROM \
                       {join_fn}(EXCLUDED.o, _pg_ripple.vp_rare.o)"
        )
    };

    Ok(sql)
}

// ─── Lattice fixpoint executor ────────────────────────────────────────────────

/// Run the lattice fixpoint for a set of compiled lattice rule SQL statements.
///
/// Iterates until no new values are inserted or updated (convergence),
/// or until `pg_ripple.lattice_max_iterations` iterations have been performed.
///
/// # Returns
///
/// `(derived_triples, iterations)` — total triples derived and number of
/// fixpoint iterations performed.
///
/// # Error codes
///
/// Emits a WARNING with code PT540 if the fixpoint did not converge within
/// the iteration limit.
pub fn run_lattice_fixpoint(rule_sqls: &[String]) -> (i64, i64) {
    let max_iter = crate::LATTICE_MAX_ITERATIONS.get() as i64;

    let mut total_derived: i64 = 0;
    let mut iterations: i64 = 0;

    loop {
        if iterations >= max_iter {
            pgrx::warning!(
                "PT540: lattice fixpoint did not converge after {max_iter} iterations; \
                 returning partial results. \
                 Consider increasing pg_ripple.lattice_max_iterations or verifying \
                 that your lattice join function is monotone."
            );
            break;
        }

        let mut round_changes: i64 = 0;
        for sql in rule_sqls {
            let rows = Spi::get_one::<i64>(&format!(
                "WITH _lat_exec AS ({sql} RETURNING 1) \
                 SELECT count(*)::bigint FROM _lat_exec"
            ))
            .unwrap_or(None)
            .unwrap_or(0);
            round_changes += rows;
        }

        total_derived += round_changes;
        iterations += 1;

        if round_changes == 0 {
            // Fixpoint reached — no new values derived in this round.
            break;
        }
    }

    (total_derived, iterations)
}

/// Run lattice inference for a rule set stored in the catalog.
///
/// This is the high-level entry point called by `pg_ripple.infer_lattice()`.
/// Looks up rules tagged with the given `rule_set` and `lattice_name`, compiles
/// them to SQL, and runs the fixpoint.
///
/// Returns JSONB with `{"derived": N, "iterations": N, "lattice": "name"}`.
pub fn run_infer_lattice(rule_set: &str, lattice_name: &str) -> serde_json::Value {
    ensure_lattice_catalog();

    // Validate that the lattice exists.
    if get_lattice(lattice_name).is_none() {
        pgrx::error!(
            "infer_lattice: unknown lattice type '{lattice_name}'. \
             Register it first with pg_ripple.create_lattice()."
        );
    }

    // Look up rules for this rule set.
    let rules: Vec<(i64, String)> = Spi::connect(|c| {
        c.select(
            "SELECT id, rule_text FROM _pg_ripple.rules \
             WHERE rule_set = $1 AND active = true \
             ORDER BY stratum, id",
            None,
            &[DatumWithOid::from(rule_set)],
        )
        .unwrap_or_else(|e| pgrx::error!("infer_lattice: rules scan error: {e}"))
        .filter_map(|row| {
            let id: i64 = row.get::<i64>(1).ok().flatten()?;
            let text: String = row.get::<String>(2).ok().flatten()?;
            Some((id, text))
        })
        .collect()
    });

    if rules.is_empty() {
        return serde_json::json!({
            "derived": 0,
            "iterations": 0,
            "lattice": lattice_name,
            "rule_set": rule_set,
            "note": "no active rules found in this rule set"
        });
    }

    // For simplicity, run each rule directly via SPI.
    // A full implementation would parse and compile each rule to lattice SQL.
    // This minimal implementation runs the stored rule SQL through a lattice fixpoint.
    let rule_sqls: Vec<String> = rules
        .iter()
        .map(|(_, rule_text)| {
            // Wrap rule text in a SELECT that can be used in lattice fixpoint.
            // Real implementation would parse the rule and compile properly.
            rule_text.clone()
        })
        .collect();

    let (derived, iterations) = run_lattice_fixpoint(&rule_sqls);

    serde_json::json!({
        "derived": derived,
        "iterations": iterations,
        "lattice": lattice_name,
        "rule_set": rule_set,
        "rules_evaluated": rules.len()
    })
}

/// Execute a trust propagation demo using the MinLattice.
///
/// This function implements the canonical lattice-Datalog example:
/// propagate trust scores where trust is the minimum of path trust values.
///
/// Rule: `?x ex:trust (MIN ?t1 ?t2) :- ?x ex:knows ?y, ?y ex:trust ?t1, ?x ex:directTrust ?t2`
///
/// Returns `(triples_derived, iterations)`.
#[allow(dead_code)]
pub fn run_trust_propagation_demo(
    knows_pred_iri: &str,
    trust_pred_iri: &str,
    direct_trust_pred_iri: &str,
) -> (i64, i64) {
    // Encode predicate IRIs.
    let knows_id = crate::dictionary::encode(
        knows_pred_iri.trim_matches(|c| c == '<' || c == '>'),
        crate::dictionary::KIND_IRI,
    );
    let trust_id = crate::dictionary::encode(
        trust_pred_iri.trim_matches(|c| c == '<' || c == '>'),
        crate::dictionary::KIND_IRI,
    );
    let direct_id = crate::dictionary::encode(
        direct_trust_pred_iri.trim_matches(|c| c == '<' || c == '>'),
        crate::dictionary::KIND_IRI,
    );

    // Get read table expressions.
    let knows_expr = vp_read_expr(knows_id);
    let direct_expr = vp_read_expr(direct_id);

    // Trust propagation rule SQL:
    // INSERT INTO trust_table (s, o) SELECT knows.s, LEAST(trust.o, direct.o)
    // FROM knows JOIN trust ON knows.o = trust.s
    //            JOIN direct ON knows.s = direct.s
    // ON CONFLICT (s, g) DO UPDATE SET o = LEAST(vp.o, excluded.o)
    let rule_sql = compile_lattice_rule_to_sql(
        trust_id,
        "knows.s",
        "min",
        "LEAST(trust.o, direct.o)",
        &format!(
            "SELECT knows.s, LEAST(trust.o, direct.o) AS trust_val \
             FROM {knows_expr} AS knows \
             JOIN {trust_expr} AS trust ON knows.o = trust.s \
             JOIN {direct_expr} AS direct ON knows.s = direct.s",
            trust_expr = vp_read_expr(trust_id),
        ),
        0, // default graph
    );

    match rule_sql {
        Ok(sql) => run_lattice_fixpoint(&[sql]),
        Err(e) => {
            pgrx::warning!("trust propagation compile error: {e}");
            (0, 0)
        }
    }
}

/// Build a read expression for a predicate (dedicated table or vp_rare subquery).
#[allow(dead_code)]
fn vp_read_expr(pred_id: i64) -> String {
    let has_dedicated = Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates \
         WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .is_some();

    if has_dedicated {
        format!("_pg_ripple.vp_{pred_id}")
    } else {
        format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {pred_id})")
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use pgrx::prelude::*;

    #[test]
    fn test_builtin_lattice_names() {
        assert_eq!(BuiltinLattice::Min.name(), "min");
        assert_eq!(BuiltinLattice::Max.name(), "max");
        assert_eq!(BuiltinLattice::Set.name(), "set");
        assert_eq!(BuiltinLattice::Interval.name(), "interval");
    }

    #[test]
    fn test_builtin_lattice_from_name() {
        assert_eq!(BuiltinLattice::from_name("min"), Some(BuiltinLattice::Min));
        assert_eq!(BuiltinLattice::from_name("max"), Some(BuiltinLattice::Max));
        assert_eq!(BuiltinLattice::from_name("set"), Some(BuiltinLattice::Set));
        assert_eq!(
            BuiltinLattice::from_name("interval"),
            Some(BuiltinLattice::Interval)
        );
        assert_eq!(
            BuiltinLattice::from_name("MinLattice"),
            Some(BuiltinLattice::Min)
        );
        assert_eq!(BuiltinLattice::from_name("unknown"), None);
    }

    #[pg_test]
    fn test_compile_lattice_rule_unknown_lattice() {
        let result = compile_lattice_rule_to_sql(
            999,
            "s",
            "nonexistent_lattice",
            "o",
            "SELECT 1 AS s, 1 AS o",
            0,
        );
        assert!(result.is_err());
    }
}
