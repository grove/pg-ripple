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
//!
//! v0.18.0 — SPARQL CONSTRUCT, DESCRIBE & ASK Views
//! - `pg_ripple.create_construct_view(name, sparql, schedule, decode)` — CONSTRUCT stream table
//! - `pg_ripple.drop_construct_view(name)` — drop a CONSTRUCT view
//! - `pg_ripple.list_construct_views()` — list CONSTRUCT views
//! - `pg_ripple.create_describe_view(name, sparql, schedule, decode)` — DESCRIBE stream table
//! - `pg_ripple.drop_describe_view(name)` — drop a DESCRIBE view
//! - `pg_ripple.list_describe_views()` — list DESCRIBE views
//! - `pg_ripple.create_ask_view(name, sparql, schedule)` — ASK stream table
//! - `pg_ripple.drop_ask_view(name)` — drop an ASK view
//! - `pg_ripple.list_ask_views()` — list ASK views

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

    let trans = sqlgen::translate_select(&pattern, None);

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

    let stream_table = format!("pg_ripple.{name}");

    // SQL-INJ-01 (v0.80.0): use parameterised INSERT to prevent SQL injection
    // via user-supplied view name, SPARQL text, schedule, or generated SQL.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.sparql_views \
         (name, sparql, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(sparql),
            pgrx::datum::DatumWithOid::from(view_sql.as_str()),
            pgrx::datum::DatumWithOid::from(schedule),
            pgrx::datum::DatumWithOid::from(decode),
            pgrx::datum::DatumWithOid::from(stream_table.as_str()),
            pgrx::datum::DatumWithOid::from(variables_json.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register SPARQL view: {e}"));

    // Create the pg_trickle stream table.  The view SQL is passed via a
    // dollar-quoted literal so the schedule and stream_table name need their
    // own escaping for the function-call argument list.
    let escaped_stream_table = stream_table.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
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

    let stream_table = format!("pg_ripple.{name}");

    // SQL-INJ-01 (v0.80.0): parameterised INSERT to prevent SQL injection.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.datalog_views \
         (name, rules, rule_set, goal, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET rules = EXCLUDED.rules, \
             rule_set = EXCLUDED.rule_set, \
             goal = EXCLUDED.goal, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(rules),
            pgrx::datum::DatumWithOid::from(rule_set_name),
            pgrx::datum::DatumWithOid::from(goal),
            pgrx::datum::DatumWithOid::from(goal_sql.as_str()),
            pgrx::datum::DatumWithOid::from(schedule),
            pgrx::datum::DatumWithOid::from(decode),
            pgrx::datum::DatumWithOid::from(stream_table.as_str()),
            pgrx::datum::DatumWithOid::from(variables_json.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register Datalog view: {e}"));

    // Create the pg_trickle stream table.
    let escaped_stream_table = stream_table.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
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

    let stream_table = format!("pg_ripple.{name}");

    // SQL-INJ-01 (v0.80.0): parameterised INSERT; NULL rules (rule-set-based view).
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.datalog_views \
         (name, rules, rule_set, goal, generated_sql, schedule, decode, stream_table, variables) \
         VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8::jsonb) \
         ON CONFLICT (name) DO UPDATE \
         SET rules = EXCLUDED.rules, \
             rule_set = EXCLUDED.rule_set, \
             goal = EXCLUDED.goal, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             stream_table = EXCLUDED.stream_table, \
             variables = EXCLUDED.variables",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(rule_set),
            pgrx::datum::DatumWithOid::from(goal),
            pgrx::datum::DatumWithOid::from(goal_sql.as_str()),
            pgrx::datum::DatumWithOid::from(schedule),
            pgrx::datum::DatumWithOid::from(decode),
            pgrx::datum::DatumWithOid::from(stream_table.as_str()),
            pgrx::datum::DatumWithOid::from(variables_json.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register Datalog view: {e}"));

    // Create the pg_trickle stream table.
    let escaped_stream_table = stream_table.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
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
    let escaped_schedule = schedule.replace('\'', "''");
    let escaped_sql = extvp_sql.replace('\'', "''");
    let stream_table = format!("_pg_ripple.extvp_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Register in catalog.
    // REDUNDANT-01: pred1_iri/pred2_iri TEXT dropped; use pred1_id/pred2_id only.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.extvp_tables \
         (name, pred1_id, pred2_id, generated_sql, schedule, stream_table) \
         VALUES ('{escaped_name}', \
                 {pred1_id}, {pred2_id}, '{escaped_sql}', \
                 '{escaped_schedule}', '{escaped_stream_table}') \
         ON CONFLICT (name) DO UPDATE \
         SET pred1_id = EXCLUDED.pred1_id, \
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
/// REDUNDANT-01: pred1_iri/pred2_iri TEXT dropped; decode from dictionary for display.
/// Returns a JSONB array of `{name, pred1_iri, pred2_iri, schedule, stream_table, created_at}`.
pub(crate) fn list_extvp() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT e.name, \
                      (SELECT value FROM _pg_ripple.dictionary WHERE id = e.pred1_id) AS pred1_iri, \
                      (SELECT value FROM _pg_ripple.dictionary WHERE id = e.pred2_id) AS pred2_iri, \
                      e.schedule, e.stream_table, e.created_at \
               FROM _pg_ripple.extvp_tables e ORDER BY e.created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_extvp SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── Framing views (v0.17.0) ──────────────────────────────────────────────────

/// Create an incrementally-maintained JSON-LD framing view (requires pg_trickle).
///
/// Translates `frame` to a SPARQL CONSTRUCT query using the framing engine,
/// then registers a pg_trickle stream table `pg_ripple.framing_view_{name}`
/// with schema `(subject_id BIGINT, frame_tree JSONB, refreshed_at TIMESTAMPTZ)`.
pub(crate) fn create_framing_view(
    name: &str,
    frame: &serde_json::Value,
    schedule: &str,
    decode: bool,
    output_format: &str,
) {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is required for framing views — install pg_trickle and add it to \
             shared_preload_libraries, then retry; hint: {}",
            PGTRICKLE_HINT
        );
    }
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid framing view name: {e}");
    }

    let construct_query = crate::framing::frame_to_sparql(frame, None)
        .unwrap_or_else(|e| pgrx::error!("frame translation error: {e}"));

    let frame_json = serde_json::to_string(frame).unwrap_or_else(|_| "{}".to_owned());
    // For stream_sql, the frame JSON is embedded in a dollar-quoted SQL literal
    // (used as pg_trickle query body), so we keep the escaped version there.
    let escaped_frame = frame_json.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");

    // Stream table SQL: run the CONSTRUCT query, embed and compact each root node.
    // Since pg_trickle executes raw SQL, we use the underlying SPARQL execution
    // by calling the pg_ripple function directly.
    let stream_table = format!("pg_ripple.framing_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    let stream_sql = format!(
        "SELECT \
            (jsonb_array_elements(r.tree->'@graph'))->>'@id' AS subject_id_text, \
            jsonb_array_elements(r.tree->'@graph') AS frame_tree, \
            now() AS refreshed_at \
         FROM (SELECT pg_ripple.export_jsonld_framed('{escaped_frame}'::jsonb) AS tree) r"
    );

    // SQL-INJ-01 (v0.80.0): parameterised INSERT for framing view catalog entry.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.framing_views \
         (name, frame, generated_construct, schedule, output_format, decode, created_at) \
         VALUES ($1, $2::jsonb, $3, $4, $5, $6, now()) \
         ON CONFLICT (name) DO UPDATE \
         SET frame = EXCLUDED.frame, \
             generated_construct = EXCLUDED.generated_construct, \
             schedule = EXCLUDED.schedule, \
             output_format = EXCLUDED.output_format, \
             decode = EXCLUDED.decode",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(frame_json.as_str()),
            pgrx::datum::DatumWithOid::from(construct_query.as_str()),
            pgrx::datum::DatumWithOid::from(schedule),
            pgrx::datum::DatumWithOid::from(output_format),
            pgrx::datum::DatumWithOid::from(decode),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register framing view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__fv_q${stream_sql}$__fv_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create framing view stream table: {e}"));

    // If decode = TRUE, create a thin IRI-decoding view.
    if decode {
        let decode_view = format!("pg_ripple.framing_view_{name}_decoded");
        Spi::run(&format!(
            "CREATE OR REPLACE VIEW {decode_view} AS \
             SELECT pg_ripple.decode_iri(subject_id::bigint) AS subject_iri, \
                    frame_tree, refreshed_at \
             FROM {stream_table}"
        ))
        .unwrap_or_else(|e| pgrx::error!("failed to create decode view: {e}"));
    }
}

/// Drop a framing view stream table and its catalog entry.
pub(crate) fn drop_framing_view(name: &str) -> bool {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — framing views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.framing_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");
    let decode_view = format!("pg_ripple.framing_view_{name}_decoded");

    // Drop the decode view (ignore error if absent).
    let _ = Spi::run(&format!("DROP VIEW IF EXISTS {decode_view}"));

    // Drop the stream table (ignore error if already gone).
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    // Remove from catalog.
    Spi::run(&format!(
        "DELETE FROM _pg_ripple.framing_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove framing view from catalog: {e}"));

    true
}

/// List all registered framing views.
///
/// Returns a JSONB array of `{name, frame, schedule, output_format, decode, created_at}`.
pub(crate) fn list_framing_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, frame, schedule, output_format, decode, created_at \
               FROM _pg_ripple.framing_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_framing_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── CONSTRUCT Views (v0.18.0) ────────────────────────────────────────────────

/// Compile a SPARQL CONSTRUCT query to a SQL SELECT for a stream table.
///
/// Returns `(sql, template_count)` where `sql` projects `(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`.
/// Each template triple becomes one row in the UNION ALL.
///
/// Constant IRI/literal terms in the template are dictionary-encoded at view-creation time
/// (integer literals in SQL — no string comparisons at refresh time).
///
/// Errors:
/// - Blank node in template (not expressible as a stable BIGINT)
/// - Variable in template that is not bound in the WHERE pattern
fn compile_construct_for_view(query_text: &str) -> Result<(String, usize, usize), String> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => return Err("sparql must be a CONSTRUCT query".to_owned()),
    };

    let trans = sqlgen::translate_select(&pattern, None);
    let where_sql = trans.sql;
    let variables = trans.variables;
    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();

    // Validate template: no blank nodes; all variables bound.
    for triple in &template {
        match &triple.subject {
            spargebra::term::TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node subject; replace blank \
                     nodes with IRIs or use skolemisation before registering as a view"
                        .to_owned(),
                );
            }
            spargebra::term::TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.predicate {
            spargebra::term::NamedNodePattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.object {
            spargebra::term::TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node object; replace blank \
                     nodes with IRIs or use skolemisation before registering as a view"
                        .to_owned(),
                );
            }
            spargebra::term::TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
    }

    let template_count = template.len();
    if template_count == 0 {
        return Err("CONSTRUCT template is empty".to_owned());
    }

    // Remap WHERE SQL column aliases from `_v_{var}` to `{var}`.
    let clean_where_sql = remap_view_columns(&where_sql, &variables);

    let var_col = |v: &str| -> String {
        // Column name in the remapped WHERE SQL.
        format!("_construct_inner_.{v}")
    };

    let resolve_subject = |tp: &spargebra::term::TermPattern| -> Result<String, String> {
        match tp {
            spargebra::term::TermPattern::NamedNode(nn) => {
                let id = dictionary::encode(nn.as_str(), dictionary::KIND_IRI);
                Ok(format!("{id}::bigint"))
            }
            spargebra::term::TermPattern::Variable(v) => Ok(var_col(v.as_str())),
            _ => Err(
                "internal: blank node or RDF-star subject reached resolver — please report"
                    .to_owned(),
            ),
        }
    };

    let resolve_predicate = |np: &spargebra::term::NamedNodePattern| -> Result<String, String> {
        match np {
            spargebra::term::NamedNodePattern::NamedNode(nn) => {
                let id = dictionary::encode(nn.as_str(), dictionary::KIND_IRI);
                Ok(format!("{id}::bigint"))
            }
            spargebra::term::NamedNodePattern::Variable(v) => Ok(var_col(v.as_str())),
        }
    };

    let resolve_object = |tp: &spargebra::term::TermPattern| -> Result<String, String> {
        match tp {
            spargebra::term::TermPattern::NamedNode(nn) => {
                let id = dictionary::encode(nn.as_str(), dictionary::KIND_IRI);
                Ok(format!("{id}::bigint"))
            }
            spargebra::term::TermPattern::Literal(lit) => {
                let id = if let Some(lang) = lit.language() {
                    dictionary::encode_lang_literal(lit.value(), lang)
                } else {
                    dictionary::encode_typed_literal(lit.value(), lit.datatype().as_str())
                };
                Ok(format!("{id}::bigint"))
            }
            spargebra::term::TermPattern::Variable(v) => Ok(var_col(v.as_str())),
            spargebra::term::TermPattern::BlankNode(_) => {
                Err("internal: blank node object reached resolver — please report".to_owned())
            }
            spargebra::term::TermPattern::Triple(_) => {
                Err("CONSTRUCT template contains an RDF-star quoted triple; \
                     RDF-star template terms are not supported in views"
                    .to_owned())
            }
        }
    };

    // Build UNION ALL of per-template-triple SELECTs.
    let mut parts: Vec<String> = Vec::with_capacity(template_count);
    for triple in &template {
        let s_expr = resolve_subject(&triple.subject)?;
        let p_expr = resolve_predicate(&triple.predicate)?;
        let o_expr = resolve_object(&triple.object)?;
        parts.push(format!(
            "SELECT {s_expr} AS s, {p_expr} AS p, {o_expr} AS o, 0::bigint AS g \
             FROM ({clean_where_sql}) AS _construct_inner_"
        ));
    }

    let union_sql = parts.join("\nUNION ALL\n");
    Ok((union_sql, template_count, variables.len()))
}

/// Create a CONSTRUCT view — an incrementally-maintained stream table whose rows
/// reflect the CONSTRUCT template output at all times.
///
/// Requires pg_trickle. Raises a descriptive error when absent.
///
/// Returns the number of template triples registered.
pub(crate) fn create_construct_view(name: &str, sparql: &str, schedule: &str, decode: bool) -> i64 {
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }
    // Validate query form before pg_trickle check so user gets the right error
    // even without pg_trickle installed.
    {
        let q = SparqlParser::new()
            .parse_query(sparql)
            .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));
        if !matches!(q, spargebra::Query::Construct { .. }) {
            pgrx::error!("sparql must be a CONSTRUCT query");
        }
    }
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — CONSTRUCT views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let (view_sql, template_count, _var_count) =
        compile_construct_for_view(sparql).unwrap_or_else(|e| pgrx::error!("{e}"));

    let template_count_i64 = template_count as i64;
    let stream_table = format!("pg_ripple.construct_view_{name}");

    // SQL-INJ-01 (v0.80.0): parameterised INSERT for construct view catalog entry.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.construct_views \
         (name, sparql, generated_sql, schedule, decode, template_count, stream_table) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             template_count = EXCLUDED.template_count, \
             stream_table = EXCLUDED.stream_table",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(sparql),
            pgrx::datum::DatumWithOid::from(view_sql.as_str()),
            pgrx::datum::DatumWithOid::from(schedule),
            pgrx::datum::DatumWithOid::from(decode),
            pgrx::datum::DatumWithOid::from(template_count_i64),
            pgrx::datum::DatumWithOid::from(stream_table.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register CONSTRUCT view: {e}"));

    // Create the pg_trickle stream table.
    let escaped_stream_table = stream_table.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__cv_q${view_sql}$__cv_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create CONSTRUCT view stream table: {e}"));

    // If decode = TRUE, create a thin decoding view.
    if decode {
        let decode_view = format!("pg_ripple.construct_view_{name}_decoded");
        Spi::run(&format!(
            "CREATE OR REPLACE VIEW {decode_view} AS \
             SELECT \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = cv.s) AS s, \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = cv.p) AS p, \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = cv.o) AS o, \
               cv.g \
             FROM {stream_table} cv"
        ))
        .unwrap_or_else(|e| pgrx::error!("failed to create CONSTRUCT decode view: {e}"));
    }

    template_count_i64
}

/// Drop a CONSTRUCT view and its underlying stream table.
pub(crate) fn drop_construct_view(name: &str) {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — CONSTRUCT views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.construct_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");
    let decode_view = format!("pg_ripple.construct_view_{name}_decoded");

    let _ = Spi::run(&format!("DROP VIEW IF EXISTS {decode_view}"));
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    Spi::run(&format!(
        "DELETE FROM _pg_ripple.construct_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove CONSTRUCT view from catalog: {e}"));
}

/// List all registered CONSTRUCT views.
pub(crate) fn list_construct_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, generated_sql, schedule, decode, template_count, \
                      stream_table, created_at \
               FROM _pg_ripple.construct_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_construct_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── DESCRIBE Views (v0.18.0) ─────────────────────────────────────────────────

/// Compile a SPARQL DESCRIBE query to a SQL SELECT for a stream table.
///
/// Returns `(sql, strategy)` where `sql` projects `(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`.
///
/// The SQL uses `_pg_ripple.triples_for_resource(resource_id, symmetric)` helper
/// (created by the migration script) to perform the CBD expansion in SQL.
fn compile_describe_for_view(query_text: &str, strategy: &str) -> Result<String, String> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let pattern = match query {
        spargebra::Query::Describe { pattern, .. } => pattern,
        _ => return Err("sparql must be a DESCRIBE query".to_owned()),
    };

    let trans = sqlgen::translate_select(&pattern, None);
    let where_sql = trans.sql;
    let variables = trans.variables;

    let clean_where_sql = remap_view_columns(&where_sql, &variables);
    let include_incoming = strategy == "scbd";

    // Build a SQL that: for each resource returned by the WHERE pattern,
    // calls the CBD helper to enumerate all triples.
    // The WHERE pattern returns BIGINT IDs for each projected variable.
    // We unnest all variables to get the resource IDs.
    let resource_cols: Vec<String> = variables
        .iter()
        .map(|v| format!("_desc_resources_.{v}"))
        .collect();

    let resource_unions: Vec<String> = resource_cols
        .iter()
        .map(|col| format!("SELECT {col} AS resource_id FROM _desc_resources_"))
        .collect();

    let resource_sql = resource_unions.join("\nUNION\n");

    let sql = format!(
        "SELECT t.s, t.p, t.o, 0::bigint AS g \
         FROM ({clean_where_sql}) AS _desc_resources_ \
         CROSS JOIN LATERAL ( \
           SELECT rs.resource_id FROM ({resource_sql}) rs \
         ) _res_ \
         CROSS JOIN LATERAL _pg_ripple.triples_for_resource(_res_.resource_id, {include_incoming}::boolean) t"
    );

    Ok(sql)
}

/// Create a DESCRIBE view — an incrementally-maintained stream table materialising
/// the CBD of the described resources.
///
/// Requires pg_trickle. Raises a descriptive error when absent.
pub(crate) fn create_describe_view(name: &str, sparql: &str, schedule: &str, decode: bool) {
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }
    // Validate query form before pg_trickle check.
    {
        let q = SparqlParser::new()
            .parse_query(sparql)
            .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));
        if !matches!(q, spargebra::Query::Describe { .. }) {
            pgrx::error!("sparql must be a DESCRIBE query");
        }
    }
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — DESCRIBE views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    // Read describe_strategy GUC — use the same logic as one-shot sparql_describe().
    let strategy =
        Spi::get_one::<String>("SELECT current_setting('pg_ripple.describe_strategy', true)")
            .unwrap_or(None)
            .unwrap_or_else(|| "cbd".to_owned());
    let strategy = if strategy.is_empty() {
        "cbd".to_owned()
    } else {
        strategy
    };

    let view_sql =
        compile_describe_for_view(sparql, &strategy).unwrap_or_else(|e| pgrx::error!("{e}"));

    let escaped_name = name.replace('\'', "''");
    let escaped_sparql = sparql.replace('\'', "''");
    let escaped_sql = view_sql.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let escaped_strategy = strategy.replace('\'', "''");
    let stream_table = format!("pg_ripple.describe_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Store in catalog.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.describe_views \
         (name, sparql, generated_sql, schedule, decode, strategy, stream_table) \
         VALUES ('{escaped_name}', '{escaped_sparql}', '{escaped_sql}', \
                 '{escaped_schedule}', {decode}, '{escaped_strategy}', '{escaped_stream_table}') \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             decode = EXCLUDED.decode, \
             strategy = EXCLUDED.strategy, \
             stream_table = EXCLUDED.stream_table"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register DESCRIBE view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__dv_q${view_sql}$__dv_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create DESCRIBE view stream table: {e}"));

    // If decode = TRUE, create a thin decoding view.
    if decode {
        let decode_view = format!("pg_ripple.describe_view_{name}_decoded");
        Spi::run(&format!(
            "CREATE OR REPLACE VIEW {decode_view} AS \
             SELECT \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = dv.s) AS s, \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = dv.p) AS p, \
               (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = dv.o) AS o, \
               dv.g \
             FROM {stream_table} dv"
        ))
        .unwrap_or_else(|e| pgrx::error!("failed to create DESCRIBE decode view: {e}"));
    }
}

/// Drop a DESCRIBE view and its underlying stream table.
pub(crate) fn drop_describe_view(name: &str) {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — DESCRIBE views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.describe_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");
    let decode_view = format!("pg_ripple.describe_view_{name}_decoded");

    let _ = Spi::run(&format!("DROP VIEW IF EXISTS {decode_view}"));
    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    Spi::run(&format!(
        "DELETE FROM _pg_ripple.describe_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove DESCRIBE view from catalog: {e}"));
}

/// List all registered DESCRIBE views.
pub(crate) fn list_describe_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, generated_sql, schedule, decode, strategy, \
                      stream_table, created_at \
               FROM _pg_ripple.describe_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_describe_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

// ─── ASK Views (v0.18.0) ──────────────────────────────────────────────────────

/// Compile a SPARQL ASK query to a SQL SELECT for a stream table.
///
/// Returns SQL of the form `SELECT EXISTS(...) AS result, now() AS evaluated_at`.
fn compile_ask_for_view(query_text: &str) -> Result<String, String> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let pattern = match query {
        spargebra::Query::Ask { pattern, .. } => pattern,
        _ => return Err("sparql must be an ASK query".to_owned()),
    };

    let exists_sql = sqlgen::translate_ask(&pattern);
    Ok(format!(
        "SELECT ({exists_sql}) AS result, now() AS evaluated_at"
    ))
}

/// Create an ASK view — an incrementally-maintained single-row stream table
/// whose `result` column flips whenever the underlying pattern's satisfiability changes.
///
/// Requires pg_trickle. Raises a descriptive error when absent.
pub(crate) fn create_ask_view(name: &str, sparql: &str, schedule: &str) {
    if let Err(e) = validate_name(name) {
        pgrx::error!("invalid view name: {e}");
    }
    // Validate query form before pg_trickle check.
    {
        let q = SparqlParser::new()
            .parse_query(sparql)
            .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));
        if !matches!(q, spargebra::Query::Ask { .. }) {
            pgrx::error!("sparql must be an ASK query");
        }
    }
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — ASK views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let view_sql = compile_ask_for_view(sparql).unwrap_or_else(|e| pgrx::error!("{e}"));

    let escaped_name = name.replace('\'', "''");
    let escaped_sparql = sparql.replace('\'', "''");
    let escaped_sql = view_sql.replace('\'', "''");
    let escaped_schedule = schedule.replace('\'', "''");
    let stream_table = format!("pg_ripple.ask_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    // Store in catalog.
    Spi::run(&format!(
        "INSERT INTO _pg_ripple.ask_views \
         (name, sparql, generated_sql, schedule, stream_table) \
         VALUES ('{escaped_name}', '{escaped_sparql}', '{escaped_sql}', \
                 '{escaped_schedule}', '{escaped_stream_table}') \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             schedule = EXCLUDED.schedule, \
             stream_table = EXCLUDED.stream_table"
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register ASK view: {e}"));

    // Create the pg_trickle stream table.
    let pgt_sql = format!(
        "SELECT pgtrickle.create_stream_table(\
            name => '{escaped_stream_table}', \
            query => $__av_q${view_sql}$__av_q$, \
            schedule => '{escaped_schedule}'\
        )"
    );
    Spi::run(&pgt_sql)
        .unwrap_or_else(|e| pgrx::error!("failed to create ASK view stream table: {e}"));
}

/// Drop an ASK view and its underlying stream table.
pub(crate) fn drop_ask_view(name: &str) {
    if !crate::has_pg_trickle() {
        pgrx::error!(
            "pg_trickle is not installed — ASK views require pg_trickle; hint: {}",
            PGTRICKLE_HINT
        );
    }

    let stream_table = format!("pg_ripple.ask_view_{name}");
    let escaped_stream_table = stream_table.replace('\'', "''");

    let _ = Spi::run(&format!(
        "SELECT pgtrickle.drop_stream_table(name => '{escaped_stream_table}')"
    ));

    Spi::run(&format!(
        "DELETE FROM _pg_ripple.ask_views WHERE name = '{}'",
        name.replace('\'', "''")
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to remove ASK view from catalog: {e}"));
}

/// List all registered ASK views.
pub(crate) fn list_ask_views() -> pgrx::JsonB {
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(v))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, generated_sql, schedule, stream_table, created_at \
               FROM _pg_ripple.ask_views ORDER BY created_at) v",
    )
    .unwrap_or_else(|e| pgrx::error!("list_ask_views SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}
