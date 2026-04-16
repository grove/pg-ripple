//! Incremental SPARQL Views, Datalog Views, and Extended VP (ExtVP) — v0.11.0.
//!
//! All three features are soft-dependent on the pg_trickle extension.
//! Functions that require pg_trickle call [`crate::has_pg_trickle`] at call
//! time and raise a descriptive error when it is absent.
//!
//! # Public SQL functions
//!
//! - `pg_ripple.pg_trickle_available()` — check whether pg_trickle is installed
//! - `pg_ripple.create_sparql_view(name, sparql, schedule, decode)` — create an always-fresh SPARQL result table
//! - `pg_ripple.drop_sparql_view(name)` — drop a SPARQL view
//! - `pg_ripple.list_sparql_views()` — list all registered SPARQL views
//! - `pg_ripple.create_datalog_view(name, rules, goal, schedule, decode)` — create a Datalog-backed live view
//! - `pg_ripple.create_datalog_view(name, rule_set, goal, schedule, decode)` — same using a named rule set
//! - `pg_ripple.drop_datalog_view(name)` — drop a Datalog view
//! - `pg_ripple.list_datalog_views()` — list all registered Datalog views
//! - `pg_ripple.create_extvp(name, pred1_iri, pred2_iri, schedule)` — create an ExtVP semi-join stream table
//! - `pg_ripple.drop_extvp(name)` — drop an ExtVP table
//! - `pg_ripple.list_extvp()` — list all registered ExtVP tables

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use spargebra::SparqlParser;

use crate::dictionary;
use crate::sparql::sqlgen;

// ─── pg_trickle install hint ─────────────────────────────────────────────────

const PGTRICKLE_HINT: &str = "Install pg_trickle: https://github.com/grove/pg-trickle — \
     then run: CREATE EXTENSION pg_trickle";

// ─── SPARQL SQL generation for views ─────────────────────────────────────────

/// Compile a SPARQL SELECT query to a SQL SELECT suitable for a stream table.
///
/// Returns `(sql, variables)` where `sql` projects each SPARQL variable as
/// `{col} AS {varname}` (plain name, not `_v_{varname}`).  Column names are
/// safe SPARQL variable names and therefore valid SQL identifiers.
///
/// `decode = false` (Option B): the stream table stores raw `BIGINT` IDs;
/// a thin decode view is created on top.
/// `decode = true` (Option A): the stream table joins the dictionary to return
/// decoded TEXT values — wider CDC surface, easier reads.
fn compile_sparql_for_view(
    query_text: &str,
    decode: bool,
) -> Result<(String, Vec<String>), String> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let pattern = match query {
        spargebra::Query::Select { pattern, .. } => pattern,
        _ => return Err("only SELECT queries can be compiled to views".to_owned()),
    };

    let trans = sqlgen::translate_select(&pattern);

    // The standard translation uses `_v_{var}` column aliases.  Re-map them to
    // plain variable names so the stream table schema is readable.
    let clean_sql = remap_view_columns(&trans.sql, &trans.variables);

    if !decode {
        return Ok((clean_sql, trans.variables));
    }

    // Option A: wrap with dictionary decode joins for each variable.
    // Each variable col becomes: `(SELECT value FROM _pg_ripple.dictionary WHERE id = _inner.{var})`.
    // This is applied only when decode = true; the stream table stores TEXT values.
    let inner_alias = "_sv_inner";
    let decode_cols: Vec<String> = trans
        .variables
        .iter()
        .map(|v| {
            format!(
                "(SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = {inner_alias}.{v}) AS {v}"
            )
        })
        .collect();
    let decoded_sql = format!(
        "SELECT {} FROM ({}) AS {}",
        decode_cols.join(", "),
        clean_sql,
        inner_alias
    );
    Ok((decoded_sql, trans.variables))
}

/// Re-map `_v_{var}` column aliases in a translated SQL to plain `{var}`.
///
/// The standard SPARQL translator emits `... AS _v_{var}` to avoid name
/// collisions.  For views we want clean column names.
fn remap_view_columns(sql: &str, variables: &[String]) -> String {
    let mut result = sql.to_owned();
    for v in variables {
        let old = format!("AS _v_{v}");
        let new = format!("AS {v}");
        result = result.replace(&old, &new);
    }
    result
}

// ─── Stream-table name validation ────────────────────────────────────────────

/// Validate a user-supplied view/table name: ASCII alphanumeric + underscore, ≤ 63 chars.
/// Returns an error string if invalid.
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 63 {
        return Err("view name must be 1–63 characters".to_owned());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(
            "view name must contain only ASCII letters, digits, and underscores".to_owned(),
        );
    }
    Ok(())
}

// ─── Resolve a predicate IRI to its VP table name ────────────────────────────

/// Look up a predicate IRI and return the VP table name (`_pg_ripple.vp_{id}`)
/// or `_pg_ripple.vp_rare` with the predicate ID filter if rare.
///
/// Returns `Err` if the IRI is not in the dictionary or has no triples.
fn predicate_table_expr(pred_iri: &str) -> Result<(i64, String), String> {
    let pred_id = dictionary::lookup_iri(pred_iri)
        .ok_or_else(|| format!("predicate IRI not found in dictionary: {pred_iri}"))?;
    let table_expr = match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_)) => format!("_pg_ripple.vp_{pred_id}"),
        Ok(None) => format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {pred_id})"),
        Err(_) => {
            return Err(format!(
                "predicate not found in predicate catalog: {pred_iri}"
            ));
        }
    };
    Ok((pred_id, table_expr))
}

// ─── Public functions — exposed through lib.rs ────────────────────────────────

// These functions are re-exported in the `pg_ripple` schema module in lib.rs.
// They are `pub(crate)` so that lib.rs can call them from the schema module.

/// Return `true` when the pg_trickle extension is installed in the current database.
pub(crate) fn pg_trickle_available() -> bool {
    crate::has_pg_trickle()
}

// ─── SPARQL Views ─────────────────────────────────────────────────────────────

/// Create a named, incrementally-maintained SPARQL result table.
///
/// Requires pg_trickle. Raises an error with an install hint if absent.
///
/// Parameters:
/// - `name` — name for the view (also used as the pg_trickle stream table name under `pg_ripple`)
/// - `sparql` — a SPARQL SELECT query
/// - `schedule` — pg_trickle schedule string, e.g. `'1s'`, `'IMMEDIATE'`, `'30s'`
/// - `decode` — when `false` (recommended), the stream table stores `BIGINT` IDs with a decode view
///   on top; when `true`, the stream table stores decoded `TEXT` values
///
/// Returns the number of projected variables (columns) in the view.
pub(crate) fn create_sparql_view(name: &str, sparql: &str, schedule: &str, decode: bool) -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — SPARQL views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }

    let (view_sql, variables) = compile_sparql_for_view(sparql, decode)
        .unwrap_or_else(|e| pgrx::error!("SPARQL view compilation failed: {e}"));

    let var_count = variables.len() as i64;
    let variables_json = serde_json::to_string(&variables).unwrap_or_else(|_| "[]".to_owned());

    // Escape single quotes in the stored SQL for the catalog INSERT.
    let escaped_sparql = sparql.replace('\'', "''");
    let escaped_sql = view_sql.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let stream_table = format!("pg_ripple.{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Store the view in the catalog.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.sparql_views \
         (name, sparql, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ('{name}', '{escaped_sparql}', '{escaped_sql}', \
                 '{escaped_schedule}', {decode}, '{escaped_stream_table}', \
                 '{variables_json}'::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register SPARQL view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__pgrst_q${view_sql}$__pgrst_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create pg_trickle stream table: {e}"));

    var_count
}

/// Drop a SPARQL view and its underlying stream table.
pub(crate) fn drop_sparql_view(name: &str) -> bool {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — SPARQL views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Drop the stream table (ignore error if already gone).
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    // Remove from catalog.
    Spi::run(&format!(
        "DELETE FROM _pg_ripple.sparql_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove SPARQL view from catalog: {e}"));

    true
}

/// List all registered SPARQL views.
///
/// Returns a JSONB array of `{name, sparql, schedule, decode, stream_table, created_at}` objects.
pub(crate) fn list_sparql_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, schedule, decode, stream_table, variables, created_at \
               FROM _pg_ripple.sparql_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_sparql_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── Datalog Views ───────────────────────────────────────────────────────────

/// Create a Datalog view from inline rules and a SPARQL SELECT goal.
///
/// The rules are parsed and stored (as if by `load_rules`), then the goal SPARQL
/// query is compiled against the derived VP tables and registered as a pg_trickle
/// stream table.
///
/// `rule_set_name` is the logical name used to store the rules.  If a rule set
/// with the same name already exists its rules are replaced.
pub(crate) fn create_datalog_view_from_rules(
    name: &str,
    rules: &str,
    rule_set_name: &str,
    goal: &str,
    schedule: &str,
    decode: bool,
) -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — Datalog views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }

    // Load the rules (this handles parse, stratify, store).
    crate::datalog::load_and_store_rules(rules, rule_set_name);

    // Compile the goal SPARQL to SQL.
    let (goal_sql, variables) = compile_sparql_for_view(goal, decode)
        .unwrap_or_else(|e| pgrx::error!("Datalog view goal compilation failed: {e}"));

    let var_count = variables.len() as i64;
    let variables_json = serde_json::to_string(&variables).unwrap_or_else(|_| "[]".to_owned());

    let escaped_name = name.replace('\'', "''");
    let escaped_rules = rules.replace('\'', "''");
    let escaped_goal = goal.replace('\'', "''");
    let escaped_sql = goal_sql.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let escaped_rule_set = rule_set_name.replace('\'', "''");
    let stream_table = format!("pg_ripple.{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Store in catalog.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.datalog_views \
         (name, rules, rule_set, goal, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ('{escaped_name}', '{escaped_rules}', '{escaped_rule_set}', \
                 '{escaped_goal}', '{escaped_sql}', '{escaped_schedule}', \
                 {decode}, '{escaped_stream_table}', '{variables_json}'::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET rules = EXCLUDED.rules, \
             rule_set = EXCLUDED.rule_set, \
             goal = EXCLUDED.goal, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register Datalog view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__pgrdl_q${goal_sql}$__pgrdl_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create Datalog view stream table: {e}"));

    var_count
}

/// Create a Datalog view referencing an existing named rule set.
pub(crate) fn create_datalog_view_from_rule_set(
    name: &str,
    rule_set: &str,
    goal: &str,
    schedule: &str,
    decode: bool,
) -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — Datalog views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }

    // Verify the rule set exists.
    let exists = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.rule_sets WHERE name = $1 AND active = true)",
        &[DatumWithOid::from(rule_set)],
    )
    .unwrap_or_else(|e| pgrx::error!("rule set lookup error: {e}"))
    .unwrap_or(false);

    if !exists {
        pgrx::error!("rule set '{}' not found or is inactive", rule_set);
    }

    // Compile the goal SPARQL to SQL.
    let (goal_sql, variables) = compile_sparql_for_view(goal, decode)
        .unwrap_or_else(|e| pgrx::error!("Datalog view goal compilation failed: {e}"));

    let var_count = variables.len() as i64;
    let variables_json = serde_json::to_string(&variables).unwrap_or_else(|_| "[]".to_owned());

    let escaped_name = name.replace('\'', "''");
    let escaped_goal = goal.replace('\'', "''");
    let escaped_sql = goal_sql.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let escaped_rule_set = rule_set.replace('\'', "''");
    let stream_table = format!("pg_ripple.{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Store in catalog (no inline rules — reference the rule set by name).
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.datalog_views \
         (name, rules, rule_set, goal, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ('{escaped_name}', NULL, '{escaped_rule_set}', \
                 '{escaped_goal}', '{escaped_sql}', '{escaped_schedule}', \
                 {decode}, '{escaped_stream_table}', '{variables_json}'::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET rules = EXCLUDED.rules, \
             rule_set = EXCLUDED.rule_set, \
             goal = EXCLUDED.goal, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register Datalog view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__pgrdl_q${goal_sql}$__pgrdl_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create Datalog view stream table: {e}"));

    var_count
}

/// Drop a Datalog view and its underlying stream table.
pub(crate) fn drop_datalog_view(name: &str) -> bool {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — Datalog views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Drop the stream table (ignore error if already gone).
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    // Remove from catalog.
    Spi::run(&format!(
        "DELETE FROM _pg_ripple.datalog_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove Datalog view from catalog: {e}"));

    true
}

/// List all registered Datalog views.
///
/// Returns a JSONB array of objects.
pub(crate) fn list_datalog_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, rule_set, goal, schedule, decode, stream_table, variables, created_at \
               FROM _pg_ripple.datalog_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_datalog_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── ExtVP Semi-join Tables ───────────────────────────────────────────────────

/// Create an ExtVP semi-join stream table for two frequently co-joined predicates.
///
/// The stream table pre-computes: subjects that appear in BOTH `pred1_iri` triples
/// and `pred2_iri` triples.  The SPARQL→SQL translator automatically uses these
/// tables for star-pattern optimisation when both predicates appear in the same
/// query.
///
/// Returns the number of rows in the stream table after the first refresh.
pub(crate) fn create_extvp(name: &str, pred1_iri: &str, pred2_iri: &str, schedule: &str) -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — ExtVP requires pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid ExtVP name: {e}");
    }

    let (pred1_id, tbl1) = predicate_table_expr(pred1_iri)
        .unwrap_or_else(|e| pgrx::error!("create_extvp pred1 error: {e}"));
    let (pred2_id, tbl2) = predicate_table_expr(pred2_iri)
        .unwrap_or_else(|e| pgrx::error!("create_extvp pred2 error: {e}"));

    // Semi-join SQL: subjects that have triples for both predicates.
    let extvp_sql = format!(
        "SELECT p1.s, p1.o AS o1, p2.o AS o2 \
         FROM {tbl1} p1 \
         WHERE EXISTS (SELECT 1 FROM {tbl2} p2 WHERE p2.s = p1.s)"
    );

    let escaped_name = name.replace('\'', "''");
    let escaped_pred1 = pred1_iri.replace('\'', "''");
    let escaped_pred2 = pred2_iri.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let escaped_sql = extvp_sql.replace('\'', "''");
    let stream_table = format!("_pg_ripple.extvp_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Register in catalog.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.extvp_tables \
         (name, pred1_iri, pred2_iri, pred1_id, pred2_id, generated_sql, schedule, stream_table) \
         VALUES ('{escaped_name}', '{escaped_pred1}', '{escaped_pred2}', \
                 {pred1_id}, {pred2_id}, '{escaped_sql}', \
                 '{escaped_schedule}', '{escaped_stream_table}') \
         ON CONFLICT (name) DO UPDATE \
         SET pred1_iri = EXCLUDED.pred1_iri, \
             pred2_iri = EXCLUDED.pred2_iri, \
             pred1_id = EXCLUDED.pred1_id, \
             pred2_id = EXCLUDED.pred2_id, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             stream_table = EXCLUDED.stream_table"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register ExtVP: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__extvp_q${extvp_sql}$__extvp_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql).unwrap_or_else(|e| pgrx::error!("failed to create ExtVP stream table: {e}"));

    // Return the initial row count from the stream table.
    Spi::get_one::<i64>(&format!("SELECT COUNT(*)::bigint FROM {stream_table}"))
        .unwrap_or(Some(0))
        .unwrap_or(0)
}

/// Drop an ExtVP table and remove it from the catalog.
pub(crate) fn drop_extvp(name: &str) -> bool {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — ExtVP requires pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("_pg_ripple.extvp_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Drop the stream table (ignore error if already gone).
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    // Remove from catalog.
    Spi::run(&format!(
        "DELETE FROM _pg_ripple.extvp_tables WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove ExtVP from catalog: {e}"));

    true
}

/// List all registered ExtVP tables.
///
/// Returns a JSONB array of `{name, pred1_iri, pred2_iri, schedule, stream_table, created_at}`.
pub(crate) fn list_extvp() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, pred1_iri, pred2_iri, schedule, stream_table, created_at \
               FROM _pg_ripple.extvp_tables ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_extvp SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}
