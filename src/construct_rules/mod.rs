//! SPARQL CONSTRUCT writeback rules — public API and write hooks.
//!
//! External callers use the `pub(crate)` functions below to register and
//! manage CONSTRUCT-writeback rules, and the `on_graph_write` /
//! `on_graph_delete` hooks integrate with the mutation journal.

pub(super) mod catalog;
pub(super) mod delta;
pub(super) mod retract;
pub(super) mod scheduler;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Register a SPARQL CONSTRUCT writeback rule.
///
/// Steps:
/// 1. Validate mode and name.
/// 2. Parse the query (must be CONSTRUCT).
/// 3. Validate the template (no blank nodes, no unbound variables).
/// 4. Identify source graphs; perform cycle check.
/// 5. Compute `rule_order` via topological sort; reject mutual recursion.
/// 6. Compile the WHERE pattern to SQL.
/// 7. Insert into `_pg_ripple.construct_rules` using parameterized SPI (CWB-FIX-05).
/// 8. Run an initial full recompute with exact provenance (CWB-FIX-04).
pub(crate) fn create_construct_rule(name: &str, sparql: &str, target_graph: &str, mode: &str) {
    catalog::ensure_catalog();

    if name.is_empty() || name.len() > 63 {
        pgrx::error!("construct rule name must be 1–63 characters");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        pgrx::error!(
            "construct rule name must contain only ASCII letters, digits, and underscores"
        );
    }

    // CWB-FIX-05: validate mode values.
    if mode != "incremental" && mode != "full" {
        pgrx::error!("construct rule mode must be 'incremental' or 'full'");
    }

    // Encode target_graph first so the dictionary is populated.
    let target_graph_id = crate::dictionary::encode(target_graph, crate::dictionary::KIND_IRI);

    // Compile the CONSTRUCT query to INSERT SQL statements.
    let (insert_sqls, source_graphs) = delta::compile_construct_to_inserts(sparql, target_graph_id)
        .unwrap_or_else(|e| pgrx::error!("{e}"));

    // Cycle check: target_graph must not be in source_graphs.
    if source_graphs.contains(&target_graph.to_owned()) {
        pgrx::error!(
            "construct rule '{}' reads from and writes to the same graph '{}' — cycle not allowed",
            name,
            target_graph
        );
    }

    // Compute rule_order (also detects mutual-recursion cycles).
    let rule_order = scheduler::compute_rule_order(name, target_graph, &source_graphs)
        .unwrap_or_else(|e| pgrx::error!("{e}"));

    let generated_sql = insert_sqls
        .iter()
        .map(|(_, plain, prov)| match plain {
            Some(ins) => format!("{ins};\n{prov}"),
            None => prov.clone(),
        })
        .collect::<Vec<_>>()
        .join(";\n");

    // CWB-FIX-05: use parameterized SPI for all scalar catalog writes.
    // source_graphs is a derived TEXT[] from the SPARQL parser — construct
    // the array literal with standard SQL quoting (single-quote escape).
    let source_graphs_literal: String = if source_graphs.is_empty() {
        "NULL".to_owned()
    } else {
        let quoted: Vec<String> = source_graphs
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();
        format!("ARRAY[{}]::text[]", quoted.join(", "))
    };

    // SCHEMA-NORM-04: target_graph TEXT column is dropped in v0.74.0.
    // Use target_graph_id (BIGINT) only; decode for display via dictionary::decode.
    Spi::run_with_args(
        &format!(
            "INSERT INTO _pg_ripple.construct_rules \
             (name, sparql, generated_sql, target_graph_id, mode, \
              source_graphs, rule_order) \
             VALUES ($1, $2, $3, $4, $5, {source_graphs_literal}, $6) \
             ON CONFLICT (name) DO UPDATE \
             SET sparql = EXCLUDED.sparql, \
                 generated_sql = EXCLUDED.generated_sql, \
                 target_graph_id = EXCLUDED.target_graph_id, \
                 mode = EXCLUDED.mode, \
                 source_graphs = EXCLUDED.source_graphs, \
                 rule_order = EXCLUDED.rule_order"
        ),
        &[
            DatumWithOid::from(name),
            DatumWithOid::from(sparql),
            DatumWithOid::from(generated_sql.as_str()),
            DatumWithOid::from(target_graph_id),
            DatumWithOid::from(mode),
            DatumWithOid::from(rule_order),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register construct rule: {e}"));

    // Run initial full recompute with exact provenance capture (CWB-FIX-04).
    delta::run_full_recompute(name, &insert_sqls, target_graph_id);
}

/// Drop a construct rule.
///
/// If `retract = true` (default), derived triples that are exclusively
/// owned by this rule are removed from the VP tables.
pub(crate) fn drop_construct_rule(name: &str, retract: bool) {
    catalog::ensure_catalog();

    if retract {
        retract::retract_exclusive_triples(name);
    }

    // Remove provenance rows for this rule.
    Spi::run_with_args(
        "DELETE FROM _pg_ripple.construct_rule_triples WHERE rule_name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::warning!("drop_construct_rule provenance cleanup: {e}"));

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.construct_rules WHERE name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::warning!("drop_construct_rule catalog cleanup: {e}"));
}

/// Full recompute: clear all triples in the target graph owned by this rule,
/// re-run the CONSTRUCT query, and rewrite provenance.
///
/// Returns the number of triples written.
pub(crate) fn refresh_construct_rule(name: &str) -> i64 {
    catalog::ensure_catalog();

    // Load the rule.
    let (sparql, target_graph_id): (String, i64) = Spi::connect(|c| {
        c.select(
            "SELECT sparql, target_graph_id FROM _pg_ripple.construct_rules WHERE name = $1",
            None,
            &[DatumWithOid::from(name)],
        )
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|row| {
            let s = row.get::<String>(1).ok().flatten()?;
            let gid = row.get::<i64>(2).ok().flatten()?;
            Some((s, gid))
        })
    })
    .unwrap_or_else(|| pgrx::error!("construct rule '{}' not found", name));

    let (insert_sqls, _) = delta::compile_construct_to_inserts(&sparql, target_graph_id)
        .unwrap_or_else(|e| pgrx::error!("refresh_construct_rule: {e}"));

    // Clear existing provenance for this rule (the VP rows will be cleaned up
    // by retract_exclusive_triples below).
    retract::retract_exclusive_triples(name);

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.construct_rule_triples WHERE rule_name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::warning!("refresh_construct_rule provenance clear: {e}"));

    let count = delta::run_full_recompute(name, &insert_sqls, target_graph_id);

    // Update last_refreshed.
    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules SET last_refreshed = now() WHERE name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::warning!("refresh_construct_rule: update last_refreshed: {e}"));

    count
}

/// List all registered construct rules as a JSONB array.
pub(crate) fn list_construct_rules() -> pgrx::JsonB {
    catalog::ensure_catalog();
    Spi::get_one::<pgrx::JsonB>(
        // SCHEMA-NORM-04: target_graph TEXT dropped; decode target_graph_id via dictionary.
        "SELECT COALESCE(json_agg(row_to_json(r))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, \
                      (SELECT value FROM _pg_ripple.dictionary WHERE id = target_graph_id) AS target_graph, \
                      mode, source_graphs, \
                      rule_order, last_refreshed, last_incremental_run, \
                      successful_run_count, failed_run_count, \
                      derived_triple_count, last_error \
               FROM _pg_ripple.construct_rules ORDER BY rule_order NULLS LAST, name) r",
    )
    .unwrap_or_else(|e| pgrx::error!("list_construct_rules SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

/// Return explain output for a construct rule.
///
/// Returns rows for `delta_insert_sql`, `source_graphs`, `rule_order`.
pub(crate) fn explain_construct_rule(name: &str) -> Vec<(String, String)> {
    catalog::ensure_catalog();

    #[allow(clippy::type_complexity)]
    let row: Option<(String, Option<String>, Option<Vec<String>>, Option<i32>)> =
        Spi::connect(|c| {
            c.select(
                "SELECT sparql, generated_sql, source_graphs, rule_order \
                 FROM _pg_ripple.construct_rules WHERE name = $1",
                None,
                &[DatumWithOid::from(name)],
            )
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .map(|row| {
                let sparql = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let generated = row.get::<String>(2).ok().flatten();
                let sources = row.get::<Vec<String>>(3).ok().flatten();
                let order = row.get::<i32>(4).ok().flatten();
                (sparql, generated, sources, order)
            })
        });

    if row.is_none() {
        pgrx::error!("construct rule '{}' not found", name);
    }
    let (_, generated, sources, order) = row.unwrap_or_else(|| {
        pgrx::error!(
            "internal: construct rule '{}' not found after is_none() check -- please report",
            name
        )
    });

    vec![
        (
            "delta_insert_sql".to_owned(),
            generated.unwrap_or_else(|| "(not compiled)".to_owned()),
        ),
        (
            "source_graphs".to_owned(),
            sources
                .map(|v| v.join(", "))
                .unwrap_or_else(|| "(none)".to_owned()),
        ),
        (
            "rule_order".to_owned(),
            order
                .map(|o| o.to_string())
                .unwrap_or_else(|| "0".to_owned()),
        ),
    ]
}

// ─── CWB-FIX-02: Delta maintenance kernel (source graph write hooks) ──────────

// Trigger incremental construct-rule maintenance after inserts into `graph_iri`.
//
// Called by `insert_triple` and `sparql_update` after modifying a named
// graph that may be a source graph for registered construct rules.
//
// For each affected rule (in `rule_order`):
// - Re-runs the INSERT SQL with `ON CONFLICT DO NOTHING RETURNING` to add new
//   derived triples.
// - Records exact provenance via CTE (CWB-FIX-04).
// - Updates health counters (CWB-FIX-07).

/// Quick check: returns `true` when there are no construct rules registered.
///
/// Allows the mutation journal to skip accumulation entirely (zero overhead).
/// (v0.67.0 MJOURNAL-01)
pub(crate) fn has_no_rules() -> bool {
    // Check if the catalog table even exists first.
    let table_exists = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
          WHERE table_schema = '_pg_ripple' AND table_name = 'construct_rules')",
        &[],
    )
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if !table_exists {
        return true;
    }

    let has_rules = Spi::get_one::<bool>("SELECT EXISTS(SELECT 1 FROM _pg_ripple.construct_rules)")
        .unwrap_or(Some(false))
        .unwrap_or(false);

    !has_rules
}

/// Trigger incremental construct-rule maintenance after inserts into `graph_iri`.
///
/// Called by `insert_triple` and `sparql_update` after modifying a named
/// graph that may be a source graph for registered construct rules.
///
/// For each affected rule (in `rule_order`):
///
/// - Re-runs the INSERT SQL with `ON CONFLICT DO NOTHING RETURNING` to add new
///   derived triples.
/// - Records exact provenance via CTE (CWB-FIX-04).
/// - Updates health counters (CWB-FIX-07).
pub(crate) fn on_graph_write(graph_iri: &str) {
    // Fast path: skip if no rules registered or catalog not yet initialized.
    let has_rules = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
          WHERE table_schema = '_pg_ripple' AND table_name = 'construct_rules')",
        &[],
    )
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if !has_rules {
        return;
    }

    let has_affected = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.construct_rules \
          WHERE source_graphs @> ARRAY[$1]::text[])",
        &[DatumWithOid::from(graph_iri)],
    )
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if !has_affected {
        return;
    }

    // Load affected rules in topological order.
    let rules: Vec<(String, String, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT name, sparql, target_graph_id \
             FROM _pg_ripple.construct_rules \
             WHERE source_graphs @> ARRAY[$1]::text[] \
             ORDER BY rule_order NULLS LAST, name",
            None,
            &[DatumWithOid::from(graph_iri)],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let name = row.get::<String>(1).ok().flatten()?;
                let sparql = row.get::<String>(2).ok().flatten()?;
                let tgid = row.get::<i64>(3).ok().flatten()?;
                Some((name, sparql, tgid))
            })
            .collect()
        })
        .unwrap_or_default()
    });

    for (rule_name, sparql, target_graph_id) in rules {
        let res = delta::compile_construct_to_inserts(&sparql, target_graph_id);
        let (insert_sqls, _) = match res {
            Ok(r) => r,
            Err(e) => {
                delta::record_run_failure(&rule_name, &e);
                continue;
            }
        };

        let mut ok = true;
        for (_pred_id, plain_insert, prov_sql) in &insert_sqls {
            if let Some(plain) = plain_insert
                && let Err(e) = Spi::run(plain)
            {
                delta::record_run_failure(&rule_name, &e.to_string());
                ok = false;
                break;
            }
            if let Err(e) = Spi::run_with_args(prov_sql, &[DatumWithOid::from(rule_name.as_str())])
            {
                delta::record_run_failure(&rule_name, &e.to_string());
                ok = false;
                break;
            }
        }

        if ok {
            let count = Spi::get_one_with_args::<i64>(
                "SELECT COUNT(*)::bigint FROM _pg_ripple.construct_rule_triples \
                 WHERE rule_name = $1",
                &[DatumWithOid::from(rule_name.as_str())],
            )
            .unwrap_or(Some(0))
            .unwrap_or(0);
            delta::record_run_success(&rule_name, count);
        }
    }
}

/// Trigger DRed-style rederive-then-retract after deletes from `graph_iri`.
///
/// For each affected rule (in `rule_order`):
/// 1. Retract all triples exclusively owned by this rule (HTAP-aware).
/// 2. Clear provenance for this rule.
/// 3. Re-run the full CONSTRUCT SQL.
/// 4. Record exact new provenance.
/// 5. Update health counters.
pub(crate) fn on_graph_delete(graph_iri: &str) {
    let has_rules = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
          WHERE table_schema = '_pg_ripple' AND table_name = 'construct_rules')",
        &[],
    )
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if !has_rules {
        return;
    }

    let has_affected = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.construct_rules \
          WHERE source_graphs @> ARRAY[$1]::text[])",
        &[DatumWithOid::from(graph_iri)],
    )
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if !has_affected {
        return;
    }

    let rules: Vec<(String, String, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT name, sparql, target_graph_id \
             FROM _pg_ripple.construct_rules \
             WHERE source_graphs @> ARRAY[$1]::text[] \
             ORDER BY rule_order NULLS LAST, name",
            None,
            &[DatumWithOid::from(graph_iri)],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let name = row.get::<String>(1).ok().flatten()?;
                let sparql = row.get::<String>(2).ok().flatten()?;
                let tgid = row.get::<i64>(3).ok().flatten()?;
                Some((name, sparql, tgid))
            })
            .collect()
        })
        .unwrap_or_default()
    });

    for (rule_name, sparql, target_graph_id) in rules {
        // DRed: retract then rederive.
        retract::retract_exclusive_triples(&rule_name);
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.construct_rule_triples WHERE rule_name = $1",
            &[DatumWithOid::from(rule_name.as_str())],
        )
        .unwrap_or_else(|e| pgrx::warning!("on_graph_delete provenance clear: {e}"));

        let res = delta::compile_construct_to_inserts(&sparql, target_graph_id);
        let (insert_sqls, _) = match res {
            Ok(r) => r,
            Err(e) => {
                delta::record_run_failure(&rule_name, &e);
                continue;
            }
        };

        let count = delta::run_full_recompute(&rule_name, &insert_sqls, target_graph_id);
        delta::record_run_success(&rule_name, count);
    }
}

/// Return the pipeline status for all construct rules (CWB-FIX-10).
pub(crate) fn construct_pipeline_status() -> pgrx::JsonB {
    catalog::ensure_catalog();
    Spi::get_one::<pgrx::JsonB>(
        "SELECT jsonb_build_object(
            'rule_count', COUNT(*),
            'rules', COALESCE(jsonb_agg(jsonb_build_object(
                'name',                 name,
                'rule_order',           rule_order,
                'mode',                 mode,
                'source_graphs',        source_graphs,
                'target_graph',         (SELECT value FROM _pg_ripple.dictionary WHERE id = target_graph_id),
                'derived_triple_count', derived_triple_count,
                'successful_run_count', successful_run_count,
                'failed_run_count',     failed_run_count,
                'last_refreshed',       last_refreshed,
                'last_incremental_run', last_incremental_run,
                'last_error',           last_error,
                'stale',                (failed_run_count > 0 AND successful_run_count = 0)
            ) ORDER BY rule_order NULLS LAST, name), '[]'::jsonb)
         )
         FROM _pg_ripple.construct_rules",
    )
    .unwrap_or_else(|e| pgrx::error!("construct_pipeline_status SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::json!({"rule_count": 0, "rules": []})))
}

/// Public wrapper for manual incremental maintenance of all rules for a graph.
///
/// Called by `apply_construct_rules_for_graph` pg_extern and can also be used
/// by integration tests or the SPARQL update path.
///
/// Returns the total number of provenance rows after maintenance.
pub(crate) fn apply_for_graph(graph_iri: &str) -> i64 {
    on_graph_write(graph_iri);

    // Return current total provenance rows to give callers a count.
    Spi::get_one_with_args::<i64>(
        "SELECT COALESCE(SUM(derived_triple_count), 0)::bigint \
         FROM _pg_ripple.construct_rules \
         WHERE source_graphs @> ARRAY[$1]::text[]",
        &[DatumWithOid::from(graph_iri)],
    )
    .unwrap_or(Some(0))
    .unwrap_or(0)
}
