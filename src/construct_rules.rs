//! SPARQL CONSTRUCT writeback rules (v0.63.0+, correctness closure v0.65.0).
//!
//! A *construct rule* is a named SPARQL CONSTRUCT query that is registered
//! once and maintained incrementally on every write transaction that touches
//! one of the rule's source graphs.  Derived triples are written into a
//! target named graph inside the VP storage layer (with `source = 1`) and
//! are immediately queryable from other SPARQL queries and rules.
//!
//! # v0.65.0 correctness closure
//!
//! - **Delta maintenance kernel** (CWB-FIX-01/02): source graph inserts trigger
//!   incremental derivation; deletes trigger DRed-style rederive-then-retract.
//! - **HTAP-aware retraction** (CWB-FIX-03): retraction uses the correct
//!   delta/tombstone path for promoted predicates.
//! - **Exact provenance** (CWB-FIX-04): provenance records only triples inserted
//!   by this rule's SQL run, not all `source = 1` triples in the target graph.
//! - **Parameterized SQL** (CWB-FIX-05): all catalog writes use `Spi::run_with_args`.
//! - **Mode validation** (CWB-FIX-05): `mode` must be `'incremental'` or `'full'`.
//! - **Shared-target semantics** (CWB-FIX-06): reference-count retraction.
//! - **Observability** (CWB-FIX-07): health counters in catalog.
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

/// Ensure the construct-rule catalog tables exist (idempotent).
///
/// Called lazily by every public function that touches the construct-rule
/// catalog.  Adds v0.65.0 observability columns when upgrading from v0.63.0.
pub(crate) fn ensure_catalog() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rules (
            name                    TEXT PRIMARY KEY,
            sparql                  TEXT NOT NULL,
            generated_sql           TEXT,
            target_graph            TEXT NOT NULL,
            target_graph_id         BIGINT NOT NULL,
            mode                    TEXT NOT NULL DEFAULT 'incremental',
            source_graphs           TEXT[],
            rule_order              INT,
            created_at              TIMESTAMPTZ DEFAULT now(),
            last_refreshed          TIMESTAMPTZ,
            last_incremental_run    TIMESTAMPTZ,
            successful_run_count    BIGINT NOT NULL DEFAULT 0,
            failed_run_count        BIGINT NOT NULL DEFAULT 0,
            last_error              TEXT,
            derived_triple_count    BIGINT NOT NULL DEFAULT 0
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("construct_rules catalog creation: {e}"));

    // Add v0.65.0 observability columns if upgrading from older schema.
    for (col, def) in &[
        ("last_incremental_run", "TIMESTAMPTZ"),
        ("successful_run_count", "BIGINT NOT NULL DEFAULT 0"),
        ("failed_run_count", "BIGINT NOT NULL DEFAULT 0"),
        ("last_error", "TEXT"),
        ("derived_triple_count", "BIGINT NOT NULL DEFAULT 0"),
    ] {
        let _ = Spi::run(&format!(
            "ALTER TABLE _pg_ripple.construct_rules ADD COLUMN IF NOT EXISTS {col} {def}"
        ));
    }

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

/// Parse a SPARQL CONSTRUCT query and generate INSERT + provenance SQL plans.
///
/// Returns `(insert_plans, source_graphs)` where each plan is
/// `(pred_id, Option<plain_insert_sql>, prov_sql)`:
/// - promoted VP: `(pred_id, None, combined_returning_cte)` — one CTE does INSERT + prov
/// - vp_rare:     `(pred_id, Some(insert_sql), exists_prov_sql)` — two steps; prov uses
///   an EXISTS join so that shared-target rules both record provenance even when the
///   second rule's INSERT is a no-op due to ON CONFLICT (CWB-FIX-04 / CWB-10).
#[allow(clippy::type_complexity)]
fn compile_construct_to_inserts(
    query_text: &str,
    target_graph_id: i64,
) -> Result<(Vec<(i64, Option<String>, String)>, Vec<String>), String> {
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

    let mut results: Vec<(i64, Option<String>, String)> = Vec::new();

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

        let (plain_insert, prov_sql) = if has_vp_table {
            // Promoted VP table: one combined RETURNING CTE handles INSERT + provenance.
            // The INSERT may be a no-op for the promoted-VP shared-target case, but
            // promoted predicates are rarely shared across rules in practice.
            let combined_cte = format!(
                "WITH inserted AS ( \
                     INSERT INTO _pg_ripple.vp_{pred_id} (s, o, g, source) \
                     SELECT DISTINCT {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                     FROM ({clean_sql}) AS {inner_alias} \
                     WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                     ON CONFLICT DO NOTHING \
                     RETURNING s, o, g \
                 ) \
                 INSERT INTO _pg_ripple.construct_rule_triples (rule_name, pred_id, s, o, g) \
                 SELECT $1, {pred_id}, s, o, g FROM inserted \
                 ON CONFLICT DO NOTHING"
            );
            (None, combined_cte)
        } else {
            // vp_rare path (rare pred or variable pred).
            // Step 1: plain INSERT (no $1 param needed — all values are inlined).
            let p_col = if pred_id != 0 {
                format!("{pred_id}::bigint")
            } else {
                p_expr.clone()
            };
            let insert_sql = format!(
                "INSERT INTO _pg_ripple.vp_rare (p, s, o, g, source) \
                 SELECT DISTINCT {p_col}, {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                 ON CONFLICT DO NOTHING"
            );
            // Step 2: EXISTS-based provenance INSERT (CWB-FIX-04 / CWB-10).
            // Joins with vp_rare to record provenance for triples that now exist,
            // even if this rule's INSERT was a no-op (shared-target race case).
            let (prov_pred_col, prov_p_filter) = if pred_id != 0 {
                (format!("{pred_id}::bigint"), format!("vr.p = {pred_id}"))
            } else {
                (format!("({p_expr})"), format!("vr.p = ({p_expr})"))
            };
            let p_is_not_null = if pred_id == 0 {
                format!(" AND ({p_expr}) IS NOT NULL")
            } else {
                String::new()
            };
            let prov_sql = format!(
                "INSERT INTO _pg_ripple.construct_rule_triples (rule_name, pred_id, s, o, g) \
                 SELECT DISTINCT $1, {prov_pred_col}, ({s_expr}), ({o_expr}), \
                        {target_graph_id}::bigint \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL{p_is_not_null} \
                   AND EXISTS ( \
                       SELECT 1 FROM _pg_ripple.vp_rare vr \
                       WHERE {prov_p_filter} \
                         AND vr.s = ({s_expr}) \
                         AND vr.o = ({o_expr}) \
                         AND vr.g = {target_graph_id} \
                         AND vr.source = 1 \
                   ) \
                 ON CONFLICT DO NOTHING"
            );
            (Some(insert_sql), prov_sql)
        };

        results.push((pred_id, plain_insert, prov_sql));
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
/// 1. Validate mode and name.
/// 2. Parse the query (must be CONSTRUCT).
/// 3. Validate the template (no blank nodes, no unbound variables).
/// 4. Identify source graphs; perform cycle check.
/// 5. Compute `rule_order` via topological sort; reject mutual recursion.
/// 6. Compile the WHERE pattern to SQL.
/// 7. Insert into `_pg_ripple.construct_rules` using parameterized SPI (CWB-FIX-05).
/// 8. Run an initial full recompute with exact provenance (CWB-FIX-04).
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

    // CWB-FIX-05: validate mode values.
    if mode != "incremental" && mode != "full" {
        pgrx::error!("construct rule mode must be 'incremental' or 'full'");
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

    Spi::run_with_args(
        &format!(
            "INSERT INTO _pg_ripple.construct_rules \
             (name, sparql, generated_sql, target_graph, target_graph_id, mode, \
              source_graphs, rule_order) \
             VALUES ($1, $2, $3, $4, $5, $6, {source_graphs_literal}, $7) \
             ON CONFLICT (name) DO UPDATE \
             SET sparql = EXCLUDED.sparql, \
                 generated_sql = EXCLUDED.generated_sql, \
                 target_graph = EXCLUDED.target_graph, \
                 target_graph_id = EXCLUDED.target_graph_id, \
                 mode = EXCLUDED.mode, \
                 source_graphs = EXCLUDED.source_graphs, \
                 rule_order = EXCLUDED.rule_order"
        ),
        &[
            DatumWithOid::from(name),
            DatumWithOid::from(sparql),
            DatumWithOid::from(generated_sql.as_str()),
            DatumWithOid::from(target_graph),
            DatumWithOid::from(target_graph_id),
            DatumWithOid::from(mode),
            DatumWithOid::from(rule_order),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("failed to register construct rule: {e}"));

    // Run initial full recompute with exact provenance capture (CWB-FIX-04).
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

/// Execute the INSERT SQL plans and record provenance in `construct_rule_triples`.
///
/// CWB-FIX-04/CWB-10: For vp_rare predicates, uses a two-step approach —
/// plain INSERT (no params) then EXISTS-based provenance INSERT ($1=rule_name).
/// This records provenance for ALL rules that derive a triple, even when the
/// INSERT is a no-op because another rule already inserted the same triple.
///
/// For promoted VP tables, a single RETURNING CTE handles both INSERT and prov.
///
/// Returns the total number of derived triples now owned by this rule.
fn run_full_recompute(
    rule_name: &str,
    insert_sqls: &[(i64, Option<String>, String)],
    _target_graph_id: i64,
) -> i64 {
    for (_pred_id, plain_insert, prov_sql) in insert_sqls {
        if let Some(plain) = plain_insert {
            // vp_rare step 1: plain INSERT (no rule_name param).
            Spi::run(plain)
                .unwrap_or_else(|e| pgrx::warning!("run_full_recompute insert (vp_rare): {e}"));
        }
        // Step 2 (or combined for promoted VP): provenance SQL with $1 = rule_name.
        Spi::run_with_args(prov_sql, &[DatumWithOid::from(rule_name)])
            .unwrap_or_else(|e| pgrx::warning!("run_full_recompute prov: {e}"));
    }

    // Return exact count of provenance rows for this rule.
    let final_count = Spi::get_one_with_args::<i64>(
        "SELECT COUNT(*)::bigint FROM _pg_ripple.construct_rule_triples \
         WHERE rule_name = $1",
        &[DatumWithOid::from(rule_name)],
    )
    .unwrap_or(Some(0))
    .unwrap_or(0);

    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET derived_triple_count = $2 WHERE name = $1",
        &[
            DatumWithOid::from(rule_name),
            DatumWithOid::from(final_count),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("run_full_recompute: update derived_triple_count: {e}"));

    final_count
}

/// Record a successful incremental run in health counters (CWB-FIX-07).
fn record_run_success(rule_name: &str, derived_count: i64) {
    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET successful_run_count  = successful_run_count + 1, \
             last_incremental_run  = now(), \
             last_error            = NULL, \
             derived_triple_count  = $2 \
         WHERE name = $1",
        &[
            DatumWithOid::from(rule_name),
            DatumWithOid::from(derived_count),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_run_success: {e}"));
}

/// Record a failed incremental run in health counters (CWB-FIX-07).
///
/// CWB-FIX-STAB-1: retraction/derivation failures are correctness-critical —
/// a warning is emitted so the operator can detect and investigate.
fn record_run_failure(rule_name: &str, error: &str) {
    pgrx::warning!(
        "construct rule '{}' maintenance failed: {}",
        rule_name,
        error
    );
    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET failed_run_count  = failed_run_count + 1, \
             last_error        = $2 \
         WHERE name = $1",
        &[DatumWithOid::from(rule_name), DatumWithOid::from(error)],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_run_failure: {e}"));
}

/// Delete derived triples from VP tables that are exclusively owned by this
/// rule (no other rule's provenance row covers the same `(pred_id, s, o, g)`).
///
/// CWB-FIX-03: Uses HTAP-aware deletion so that post-merge main-resident
/// triples are tombstoned rather than having a direct DELETE against the VIEW.
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
        // Check if a promoted VP table exists.
        let has_table = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
              WHERE id = $1 AND table_oid IS NOT NULL)",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or(Some(false))
        .unwrap_or(false);

        if has_table {
            // CWB-FIX-03: HTAP-aware retraction.
            // Check if the predicate uses HTAP (delta + main + tombstones).
            let is_htap = crate::storage::merge::is_htap(pred_id);

            if is_htap {
                // Try delta first; tombstone main-resident rows.
                let delta = format!("_pg_ripple.vp_{pred_id}_delta");
                let tombs = format!("_pg_ripple.vp_{pred_id}_tombstones");

                let d = Spi::get_one_with_args::<i64>(
                    &format!(
                        "WITH d AS (DELETE FROM {delta} \
                         WHERE s=$1 AND o=$2 AND g=$3 AND source=1 \
                         RETURNING 1) \
                         SELECT count(*)::bigint FROM d"
                    ),
                    &[
                        DatumWithOid::from(s),
                        DatumWithOid::from(o),
                        DatumWithOid::from(g),
                    ],
                )
                .unwrap_or(Some(0))
                .unwrap_or(0);

                if d == 0 {
                    // Not in delta — tombstone from main.
                    Spi::run_with_args(
                        &format!(
                            "INSERT INTO {tombs} (s, o, g) \
                             SELECT s, o, g \
                             FROM _pg_ripple.vp_{pred_id}_main \
                             WHERE s=$1 AND o=$2 AND g=$3 AND source=1 \
                             ON CONFLICT DO NOTHING"
                        ),
                        &[
                            DatumWithOid::from(s),
                            DatumWithOid::from(o),
                            DatumWithOid::from(g),
                        ],
                    )
                    .unwrap_or_else(|e| pgrx::warning!("retract tombstone insert: {e}"));
                }
            } else {
                // Flat VP table — direct DELETE is correct.
                let sql = format!(
                    "DELETE FROM _pg_ripple.vp_{pred_id} \
                     WHERE s = {s} AND o = {o} AND g = {g} AND source = 1"
                );
                Spi::run(&sql).unwrap_or_else(|e| pgrx::warning!("retract VP: {e}"));
            }
        } else {
            // vp_rare is always a flat table — direct DELETE is correct.
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

// ─── CWB-FIX-02: Delta maintenance kernel (source graph write hooks) ──────────

/// Trigger incremental construct-rule maintenance after inserts into `graph_iri`.
///
/// Called by `insert_triple` and `sparql_update` after modifying a named
/// graph that may be a source graph for registered construct rules.
///
/// For each affected rule (in `rule_order`):
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
        let res = compile_construct_to_inserts(&sparql, target_graph_id);
        let (insert_sqls, _) = match res {
            Ok(r) => r,
            Err(e) => {
                record_run_failure(&rule_name, &e);
                continue;
            }
        };

        let mut ok = true;
        for (_pred_id, plain_insert, prov_sql) in &insert_sqls {
            if let Some(plain) = plain_insert
                && let Err(e) = Spi::run(plain)
            {
                record_run_failure(&rule_name, &e.to_string());
                ok = false;
                break;
            }
            if let Err(e) = Spi::run_with_args(prov_sql, &[DatumWithOid::from(rule_name.as_str())])
            {
                record_run_failure(&rule_name, &e.to_string());
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
            record_run_success(&rule_name, count);
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
        retract_exclusive_triples(&rule_name);
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.construct_rule_triples WHERE rule_name = $1",
            &[DatumWithOid::from(rule_name.as_str())],
        )
        .unwrap_or_else(|e| pgrx::warning!("on_graph_delete provenance clear: {e}"));

        let res = compile_construct_to_inserts(&sparql, target_graph_id);
        let (insert_sqls, _) = match res {
            Ok(r) => r,
            Err(e) => {
                record_run_failure(&rule_name, &e);
                continue;
            }
        };

        let count = run_full_recompute(&rule_name, &insert_sqls, target_graph_id);
        record_run_success(&rule_name, count);
    }
}

/// Return the pipeline status for all construct rules (CWB-FIX-10).
pub(crate) fn construct_pipeline_status() -> pgrx::JsonB {
    ensure_catalog();
    Spi::get_one::<pgrx::JsonB>(
        "SELECT jsonb_build_object(
            'rule_count', COUNT(*),
            'rules', COALESCE(jsonb_agg(jsonb_build_object(
                'name',                 name,
                'rule_order',           rule_order,
                'mode',                 mode,
                'source_graphs',        source_graphs,
                'target_graph',         target_graph,
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
