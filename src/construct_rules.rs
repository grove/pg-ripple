//! SPARQL CONSTRUCT writeback rules (v0.63.0).
//!
//! A *construct rule* is a named SPARQL CONSTRUCT query that is registered
//! once and executed incrementally on every write transaction that touches one
//! of the rule's source graphs.  Derived triples are written directly into a
//! target named graph inside the VP storage layer (with `source = 1`, the same
//! tag used for Datalog-inferred triples), so they are immediately queryable
//! from other SPARQL queries and rules.
//!
//! # Key design decisions
//!
//! - **Integer joins only** — all IRI/literal terms are dictionary-encoded at
//!   rule-registration time; no string comparisons in VP table queries.
//! - **Provenance table** — derived triples are tracked per-rule in
//!   `_pg_ripple.construct_rule_triples` so that multiple rules can safely
//!   share a target graph.  A derived triple is retracted only when its
//!   provenance row count drops to zero.
//! - **Cycle and stratification** — cycles are detected at registration time
//!   via DFS on the rule dependency graph; mutual recursion is rejected.
//! - **No pg_trickle dependency** — writeback rules write directly into VP
//!   tables via SPI; they do not require pg_trickle to be installed.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── Catalog bootstrap ────────────────────────────────────────────────────────

/// Ensure the `_pg_ripple.construct_rules` and `_pg_ripple.construct_rule_triples`
/// catalog tables exist.
///
/// Called lazily by every public function that touches the construct-rule
/// catalog.  The `CREATE TABLE IF NOT EXISTS` guards make it idempotent.
pub(crate) fn ensure_catalog() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rules (
            name            TEXT PRIMARY KEY,
            sparql          TEXT NOT NULL,
            generated_sql   TEXT,
            target_graph    TEXT NOT NULL,
            target_graph_id BIGINT NOT NULL,
            mode            TEXT NOT NULL DEFAULT 'incremental',
            source_graphs   TEXT[],
            rule_order      INT,
            created_at      TIMESTAMPTZ DEFAULT now(),
            last_refreshed  TIMESTAMPTZ
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("construct_rules catalog creation: {e}"));

    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rule_triples (
            rule_name TEXT   NOT NULL,
            pred_id   BIGINT NOT NULL,
            s         BIGINT NOT NULL,
            o         BIGINT NOT NULL,
            g         BIGINT NOT NULL,
            PRIMARY KEY (rule_name, pred_id, s, o, g)
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("construct_rule_triples catalog creation: {e}"));
}

// ─── SPARQL CONSTRUCT parsing helpers ────────────────────────────────────────

/// Collect the set of named-graph IRIs that appear inside `GRAPH <iri> { ... }`
/// clauses in a SPARQL algebra pattern.
fn collect_source_graphs(
    pattern: &spargebra::algebra::GraphPattern,
) -> std::collections::HashSet<String> {
    use spargebra::algebra::GraphPattern;
    use spargebra::term::NamedNodePattern;

    let mut graphs = std::collections::HashSet::new();

    fn walk(pat: &GraphPattern, graphs: &mut std::collections::HashSet<String>) {
        match pat {
            GraphPattern::Graph {
                name: NamedNodePattern::NamedNode(nn),
                inner,
            } => {
                graphs.insert(nn.as_str().to_owned());
                walk(inner, graphs);
            }
            GraphPattern::Join { left, right }
            | GraphPattern::LeftJoin { left, right, .. }
            | GraphPattern::Minus { left, right }
            | GraphPattern::Union { left, right } => {
                walk(left, graphs);
                walk(right, graphs);
            }
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Graph { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Group { inner, .. } => walk(inner, graphs),
            _ => {}
        }
    }

    walk(pattern, &mut graphs);
    graphs
}

// ─── Pipeline stratification ──────────────────────────────────────────────────

/// Compute the `rule_order` for a new rule.
///
/// Reads existing rules from the catalog and performs a topological sort.
/// Returns `Err` when a mutual-recursion cycle is detected.
fn compute_rule_order(
    new_name: &str,
    target_graph: &str,
    source_graphs: &[String],
) -> Result<i32, String> {
    // Load existing rules: (name, target_graph, source_graphs[])
    let existing: Vec<(String, String, Vec<String>)> = Spi::connect(|c| {
        c.select(
            "SELECT name, target_graph, COALESCE(source_graphs, '{}') \
             FROM _pg_ripple.construct_rules ORDER BY rule_order NULLS LAST",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let name = row.get::<String>(1).ok().flatten()?;
                let tg = row.get::<String>(2).ok().flatten()?;
                let sgs: Vec<String> = row.get::<Vec<String>>(3).ok().flatten().unwrap_or_default();
                Some((name, tg, sgs))
            })
            .collect()
        })
        .unwrap_or_default()
    });

    // Build adjacency: edge (A → B) means rule A writes to graph G and rule B
    // reads from graph G.  We represent nodes as rule names.

    // Collect all nodes.
    let mut all_names: Vec<String> = existing.iter().map(|(n, _, _)| n.clone()).collect();
    all_names.push(new_name.to_owned());

    // Build the write-graph-to-rule map: graph_iri → rule_name that writes it.
    let mut writer_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (name, tg, _) in &existing {
        writer_of.insert(tg.clone(), name.clone());
    }
    writer_of.insert(target_graph.to_owned(), new_name.to_owned());

    // Build adjacency list: for each rule, which rules must run before it?
    // Rule B must run before rule A if B writes to a graph that A reads from.
    let mut predecessors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for name in &all_names {
        predecessors.insert(name.clone(), Vec::new());
    }

    // Add existing rule edges.
    for (r_name, _, r_sources) in &existing {
        for sg in r_sources {
            if let Some(writer) = writer_of.get(sg.as_str())
                && writer != r_name
            {
                predecessors
                    .entry(r_name.clone())
                    .or_default()
                    .push(writer.clone());
            }
        }
    }
    // Add new rule edges.
    for sg in source_graphs {
        if let Some(writer) = writer_of.get(sg.as_str())
            && writer != new_name
        {
            predecessors
                .entry(new_name.to_owned())
                .or_default()
                .push(writer.clone());
        }
    }

    // Kahn's algorithm (topological sort with cycle detection).
    let mut in_degree: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in &all_names {
        in_degree.insert(name.clone(), 0);
    }
    for (rule, preds) in &predecessors {
        for pred in preds {
            *in_degree.entry(rule.clone()).or_insert(0) += 1;
            // Ensure pred is in the map.
            let _ = in_degree.entry(pred.clone()).or_insert(0);
        }
    }

    let mut queue: std::collections::VecDeque<String> = in_degree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    queue = queue.into_iter().collect(); // deterministic order not needed

    let mut order: Vec<String> = Vec::new();
    while let Some(n) = queue.pop_front() {
        order.push(n.clone());
        // Find all rules that have n as a predecessor.
        for (rule, preds) in &predecessors {
            if preds.contains(&n) {
                let deg = in_degree.get_mut(rule).unwrap_or_else(|| {
                    panic!("in_degree missing for {rule}");
                });
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(rule.clone());
                }
            }
        }
    }

    if order.len() < all_names.len() {
        // Cycle detected — find which rules are involved.
        let ordered_set: std::collections::HashSet<String> = order.into_iter().collect();
        let cycle_rules: Vec<String> = all_names
            .into_iter()
            .filter(|n| !ordered_set.contains(n))
            .collect();
        return Err(format!(
            "construct rules {} form a cycle — mutual writeback is not supported",
            cycle_rules.join(", ")
        ));
    }

    // The rule_order for new_name is its position in the topological order.
    let pos = order
        .iter()
        .position(|n| n == new_name)
        .unwrap_or(order.len()) as i32;
    Ok(pos)
}

// ─── CONSTRUCT SQL compilation ────────────────────────────────────────────────

/// Parse a SPARQL CONSTRUCT query and generate INSERT SQL statements.
///
/// Returns `(insert_sqls, source_graphs)` where:
/// - `insert_sqls` — `Vec<(pred_id, insert_sql)>`; pred_id is 0 for variable predicates
/// - `source_graphs` — set of named-graph IRIs referenced in the WHERE pattern
#[allow(clippy::type_complexity)]
fn compile_construct_to_inserts(
    query_text: &str,
    target_graph_id: i64,
) -> Result<(Vec<(i64, String)>, Vec<String>), String> {
    use spargebra::SparqlParser;
    use spargebra::term::{NamedNodePattern, TermPattern};

    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => return Err("sparql must be a CONSTRUCT query".to_owned()),
    };

    if template.is_empty() {
        return Err("CONSTRUCT template is empty".to_owned());
    }

    // Collect source graphs.
    let source_graphs: Vec<String> = collect_source_graphs(&pattern).into_iter().collect();

    let trans = crate::sparql::sqlgen::translate_select(&pattern, None);
    let variables = trans.variables;
    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();

    // Validate template: no blank nodes; all variables bound.
    for triple in &template {
        match &triple.subject {
            TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node subject; replace blank \
                     nodes with IRIs or use skolemisation before registering as a rule"
                        .to_owned(),
                );
            }
            TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.predicate {
            NamedNodePattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.object {
            TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node object; replace blank \
                     nodes with IRIs or use skolemisation before registering as a rule"
                        .to_owned(),
                );
            }
            TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
    }

    // Remap column aliases and build INSERT SQL for each template triple.
    let clean_sql = remap_cols(&trans.sql, &variables);
    let inner_alias = "_cr_inner_";
    let var_col = |v: &str| -> String { format!("{inner_alias}.{v}") };

    let mut results: Vec<(i64, String)> = Vec::new();

    for triple in &template {
        // Resolve subject expression.
        let s_expr = match &triple.subject {
            TermPattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                format!("{id}::bigint")
            }
            TermPattern::Variable(v) => var_col(v.as_str()),
            _ => unreachable!("validated above"),
        };

        // Resolve predicate expression and extract pred_id.
        let (p_expr, pred_id) = match &triple.predicate {
            NamedNodePattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                (format!("{id}::bigint"), id)
            }
            NamedNodePattern::Variable(v) => (var_col(v.as_str()), 0_i64),
        };

        // Resolve object expression.
        let o_expr = match &triple.object {
            TermPattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                format!("{id}::bigint")
            }
            TermPattern::Literal(lit) => {
                let id = if let Some(lang) = lit.language() {
                    crate::dictionary::encode_lang_literal(lit.value(), lang)
                } else {
                    crate::dictionary::encode_typed_literal(lit.value(), lit.datatype().as_str())
                };
                format!("{id}::bigint")
            }
            TermPattern::Variable(v) => var_col(v.as_str()),
            TermPattern::BlankNode(_) => unreachable!("validated above"),
            TermPattern::Triple(_) => {
                return Err("CONSTRUCT template contains an RDF-star quoted triple; \
                     RDF-star template terms are not supported in writeback rules"
                    .to_owned());
            }
        };

        // Choose the target table.
        let has_vp_table = pred_id != 0 && {
            Spi::get_one_with_args::<bool>(
                "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
                  WHERE id = $1 AND table_oid IS NOT NULL)",
                &[DatumWithOid::from(pred_id)],
            )
            .unwrap_or(Some(false))
            .unwrap_or(false)
        };

        let sql = if has_vp_table {
            format!(
                "INSERT INTO _pg_ripple.vp_{pred_id} (s, o, g, source) \
                 SELECT DISTINCT {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                 ON CONFLICT DO NOTHING"
            )
        } else {
            let p_col = if pred_id != 0 {
                format!("{pred_id}::bigint")
            } else {
                p_expr
            };
            format!(
                "INSERT INTO _pg_ripple.vp_rare (p, s, o, g, source) \
                 SELECT DISTINCT {p_col}, {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                 ON CONFLICT DO NOTHING"
            )
        };

        results.push((pred_id, sql));
    }

    Ok((results, source_graphs))
}

// ─── SQL helpers ─────────────────────────────────────────────────────────────

/// Remap `_v_{var}` column aliases in a SQL string to plain `{var}`.
fn remap_cols(sql: &str, variables: &[String]) -> String {
    let mut result = sql.to_owned();
    for v in variables {
        let old = format!("AS _v_{v}");
        let new = format!("AS {v}");
        result = result.replace(&old, &new);
    }
    result
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Register a SPARQL CONSTRUCT writeback rule.
///
/// Steps:
/// 1. Parse the query (must be CONSTRUCT).
/// 2. Validate the template (no blank nodes, no unbound variables).
/// 3. Identify source graphs; perform cycle check.
/// 4. Compute `rule_order` via topological sort; reject mutual recursion.
/// 5. Compile the WHERE pattern to SQL.
/// 6. Insert into `_pg_ripple.construct_rules`.
/// 7. Run an initial full recompute.
pub(crate) fn create_construct_rule(name: &str, sparql: &str, target_graph: &str, mode: &str) {
    ensure_catalog();

    if name.is_empty() || name.len() > 63 {
        pgrx::error!("construct rule name must be 1–63 characters");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        pgrx::error!(
            "construct rule name must contain only ASCII letters, digits, and underscores"
        );
    }

    // Encode target_graph first so the dictionary is populated.
    let target_graph_id = crate::dictionary::encode(target_graph, crate::dictionary::KIND_IRI);

    // Compile the CONSTRUCT query to INSERT SQL statements.
    let (insert_sqls, source_graphs) = compile_construct_to_inserts(sparql, target_graph_id)
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
    let rule_order = compute_rule_order(name, target_graph, &source_graphs)
        .unwrap_or_else(|e| pgrx::error!("{e}"));

    let generated_sql = insert_sqls
        .iter()
        .map(|(_, sql)| sql.as_str())
        .collect::<Vec<_>>()
        .join(";\n");

    // Serialize source_graphs as a Postgres TEXT array literal.
    let source_graphs_sql: String = {
        let quoted: Vec<String> = source_graphs
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();
        if quoted.is_empty() {
            "NULL".to_owned()
        } else {
            format!("ARRAY[{}]::text[]", quoted.join(", "))
        }
    };

    let escaped_name = name.replace('\'', "''");
    let escaped_sparql = sparql.replace('\'', "''");
    let escaped_target = target_graph.replace('\'', "''");
    let escaped_mode = mode.replace('\'', "''");
    let escaped_sql = generated_sql.replace('\'', "''");

    Spi::run(&format!(
        "INSERT INTO _pg_ripple.construct_rules \
         (name, sparql, generated_sql, target_graph, target_graph_id, mode, \
          source_graphs, rule_order) \
         VALUES ('{escaped_name}', '{escaped_sparql}', '{escaped_sql}', \
                 '{escaped_target}', {target_graph_id}, '{escaped_mode}', \
                 {source_graphs_sql}, {rule_order}) \
         ON CONFLICT (name) DO UPDATE \
         SET sparql = EXCLUDED.sparql, \
             generated_sql = EXCLUDED.generated_sql, \
             target_graph = EXCLUDED.target_graph, \
             target_graph_id = EXCLUDED.target_graph_id, \
             mode = EXCLUDED.mode, \
             source_graphs = EXCLUDED.source_graphs, \
             rule_order = EXCLUDED.rule_order",
    ))
    .unwrap_or_else(|e| pgrx::error!("failed to register construct rule: {e}"));

    // Run initial full recompute.
    run_full_recompute(name, &insert_sqls, target_graph_id);
}

/// Drop a construct rule.
///
/// If `retract = true` (default), derived triples that are exclusively
/// owned by this rule are removed from the VP tables.
pub(crate) fn drop_construct_rule(name: &str, retract: bool) {
    ensure_catalog();

    if retract {
        retract_exclusive_triples(name);
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
    ensure_catalog();

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

    let (insert_sqls, _) = compile_construct_to_inserts(&sparql, target_graph_id)
        .unwrap_or_else(|e| pgrx::error!("refresh_construct_rule: {e}"));

    // Clear existing provenance for this rule (the VP rows will be cleaned up
    // by retract_exclusive_triples below).
    retract_exclusive_triples(name);

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.construct_rule_triples WHERE rule_name = $1",
        &[DatumWithOid::from(name)],
    )
    .unwrap_or_else(|e| pgrx::warning!("refresh_construct_rule provenance clear: {e}"));

    let count = run_full_recompute(name, &insert_sqls, target_graph_id);

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
    ensure_catalog();
    Spi::get_one::<pgrx::JsonB>(
        "SELECT COALESCE(json_agg(row_to_json(r))::jsonb, '[]'::jsonb) \
         FROM (SELECT name, sparql, target_graph, mode, source_graphs, \
                      rule_order, last_refreshed \
               FROM _pg_ripple.construct_rules ORDER BY rule_order NULLS LAST, name) r",
    )
    .unwrap_or_else(|e| pgrx::error!("list_construct_rules SPI error: {e}"))
    .unwrap_or_else(|| pgrx::JsonB(serde_json::Value::Array(vec![])))
}

/// Return explain output for a construct rule.
///
/// Returns rows for `delta_insert_sql`, `source_graphs`, `rule_order`.
pub(crate) fn explain_construct_rule(name: &str) -> Vec<(String, String)> {
    ensure_catalog();

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
    let (_, generated, sources, order) = row.unwrap_or_else(|| unreachable!());

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

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Execute the INSERT SQLs and record provenance in `construct_rule_triples`.
///
/// Returns the total number of derived triples written.
fn run_full_recompute(rule_name: &str, insert_sqls: &[(i64, String)], target_graph_id: i64) -> i64 {
    let mut total: i64 = 0;

    for (pred_id, sql) in insert_sqls {
        // Execute the INSERT.
        Spi::run(sql.as_str())
            .unwrap_or_else(|e| pgrx::warning!("construct rule insert error: {e}"));

        // Record provenance for every derived triple we just inserted.
        // We query the VP table immediately after insert to capture the
        // (s, o) pairs that were actually written.
        let prov_sql = if *pred_id != 0 {
            let has_table = Spi::get_one_with_args::<bool>(
                "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
                  WHERE id = $1 AND table_oid IS NOT NULL)",
                &[DatumWithOid::from(*pred_id)],
            )
            .unwrap_or(Some(false))
            .unwrap_or(false);

            if has_table {
                format!(
                    "INSERT INTO _pg_ripple.construct_rule_triples \
                     (rule_name, pred_id, s, o, g) \
                     SELECT $1, {pred_id}, s, o, g \
                     FROM _pg_ripple.vp_{pred_id} \
                     WHERE g = {target_graph_id} AND source = 1 \
                     ON CONFLICT DO NOTHING"
                )
            } else {
                format!(
                    "INSERT INTO _pg_ripple.construct_rule_triples \
                     (rule_name, pred_id, s, o, g) \
                     SELECT $1, {pred_id}, s, o, g \
                     FROM _pg_ripple.vp_rare \
                     WHERE p = {pred_id} AND g = {target_graph_id} AND source = 1 \
                     ON CONFLICT DO NOTHING"
                )
            }
        } else {
            format!(
                "INSERT INTO _pg_ripple.construct_rule_triples \
                 (rule_name, pred_id, s, o, g) \
                 SELECT $1, p, s, o, g \
                 FROM _pg_ripple.vp_rare \
                 WHERE g = {target_graph_id} AND source = 1 \
                 ON CONFLICT DO NOTHING"
            )
        };

        Spi::run_with_args(&prov_sql, &[DatumWithOid::from(rule_name)])
            .unwrap_or_else(|e| pgrx::warning!("construct rule provenance insert: {e}"));

        // Count newly inserted triples from provenance.
        let inserted: i64 = Spi::get_one_with_args::<i64>(
            "SELECT COUNT(*)::bigint FROM _pg_ripple.construct_rule_triples \
             WHERE rule_name = $1",
            &[DatumWithOid::from(rule_name)],
        )
        .unwrap_or(Some(0))
        .unwrap_or(0);

        total = total.max(inserted);
    }

    total
}

/// Delete derived triples from VP tables that are exclusively owned by this
/// rule (no other rule's provenance row covers the same `(pred_id, s, o, g)`).
fn retract_exclusive_triples(rule_name: &str) {
    // Collect (pred_id, s, o, g) tuples that only this rule owns.
    let exclusive: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT crt.pred_id, crt.s, crt.o, crt.g \
             FROM _pg_ripple.construct_rule_triples crt \
             WHERE crt.rule_name = $1 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM _pg_ripple.construct_rule_triples crt2 \
                   WHERE crt2.pred_id = crt.pred_id \
                     AND crt2.s = crt.s \
                     AND crt2.o = crt.o \
                     AND crt2.g = crt.g \
                     AND crt2.rule_name <> $1 \
               )",
            None,
            &[DatumWithOid::from(rule_name)],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let pred_id = row.get::<i64>(1).ok().flatten()?;
                let s = row.get::<i64>(2).ok().flatten()?;
                let o = row.get::<i64>(3).ok().flatten()?;
                let g = row.get::<i64>(4).ok().flatten()?;
                Some((pred_id, s, o, g))
            })
            .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    });

    for (pred_id, s, o, g) in exclusive {
        // Check if a promoted table exists.
        let has_table = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
              WHERE id = $1 AND table_oid IS NOT NULL)",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or(Some(false))
        .unwrap_or(false);

        if has_table {
            let sql = format!(
                "DELETE FROM _pg_ripple.vp_{pred_id} \
                 WHERE s = {s} AND o = {o} AND g = {g} AND source = 1"
            );
            Spi::run(&sql).unwrap_or_else(|e| pgrx::warning!("retract VP: {e}"));
        } else {
            Spi::run_with_args(
                "DELETE FROM _pg_ripple.vp_rare \
                 WHERE p = $1 AND s = $2 AND o = $3 AND g = $4 AND source = 1",
                &[
                    DatumWithOid::from(pred_id),
                    DatumWithOid::from(s),
                    DatumWithOid::from(o),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("retract vp_rare: {e}"));
        }
    }
}
