#![allow(unused_imports, dead_code)]
use super::property::{
    collect_focus_nodes, get_all_predicate_iris_for_node, get_value_ids, validate_property_shape,
};
use super::severity::Violation;
use super::sparql::validate_sync_with_shapes;
use crate::shacl::constraints;
use crate::shacl::{PropertyShape, Shape, ShapeConstraint, ShapeTarget};
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde::Serialize;

/// Check whether `node_id` conforms to the shape `shape_iri`.
/// Depth-limited to 32 levels to prevent infinite recursion.
pub(crate) fn node_conforms_to_shape(
    node_id: i64,
    shape_iri: &str,
    graph_id: i64,
    all_shapes: &[Shape],
) -> bool {
    node_conforms_to_shape_depth(node_id, shape_iri, graph_id, all_shapes, 0)
}

pub fn node_conforms_to_shape_depth(
    node_id: i64,
    shape_iri: &str,
    graph_id: i64,
    all_shapes: &[Shape],
    depth: u32,
) -> bool {
    if depth > 32 {
        return true; // cycle guard
    }
    let shape = match all_shapes.iter().find(|s| s.shape_iri == shape_iri) {
        Some(s) => s,
        None => return true,
    };
    if shape.deactivated {
        return true;
    }

    for c in &shape.constraints {
        if !node_satisfies_constraint(node_id, c, graph_id, all_shapes, depth) {
            return false;
        }
    }

    for ps in &shape.properties {
        let viols = validate_property_shape_depth(
            ps,
            &[node_id],
            graph_id,
            shape_iri,
            all_shapes,
            depth + 1,
        );
        if !viols.is_empty() {
            return false;
        }
    }

    true
}

fn node_satisfies_constraint(
    node_id: i64,
    constraint: &ShapeConstraint,
    graph_id: i64,
    all_shapes: &[Shape],
    depth: u32,
) -> bool {
    match constraint {
        ShapeConstraint::Or(shape_iris) => shape_iris
            .iter()
            .any(|s| node_conforms_to_shape_depth(node_id, s, graph_id, all_shapes, depth + 1)),
        ShapeConstraint::And(shape_iris) => shape_iris
            .iter()
            .all(|s| node_conforms_to_shape_depth(node_id, s, graph_id, all_shapes, depth + 1)),
        ShapeConstraint::Not(shape_iri) => {
            !node_conforms_to_shape_depth(node_id, shape_iri, graph_id, all_shapes, depth + 1)
        }
        _ => true,
    }
}

fn validate_property_shape_depth(
    ps: &PropertyShape,
    focus_nodes: &[i64],
    graph_id: i64,
    shape_iri: &str,
    all_shapes: &[Shape],
    _depth: u32,
) -> Vec<Violation> {
    validate_property_shape(ps, focus_nodes, graph_id, shape_iri, all_shapes)
}

// ─── Property shape validation ────────────────────────────────────────────────

/// Run offline validation of all data against all active SHACL shapes.
pub fn run_validate(graph: Option<&str>) -> pgrx::JsonB {
    let graph_id: i64 = match graph {
        None | Some("") => 0,
        Some("*") => -1,
        Some(g) => {
            let g_clean = if g.starts_with('<') && g.ends_with('>') {
                &g[1..g.len() - 1]
            } else {
                g
            };
            crate::dictionary::lookup_iri(g_clean).unwrap_or(0)
        }
    };

    let shapes = crate::shacl::spi::load_shapes();
    let mut all_violations: Vec<serde_json::Value> = Vec::new();
    let mut conforms = true;

    for shape in &shapes {
        if shape.deactivated {
            continue;
        }

        let focus_nodes = collect_focus_nodes(&shape.target, graph_id);

        for c in &shape.constraints {
            match c {
                ShapeConstraint::Or(shape_iris) => {
                    for &focus in &focus_nodes {
                        let ok = shape_iris
                            .iter()
                            .any(|s| node_conforms_to_shape(focus, s, graph_id, &shapes));
                        if !ok {
                            conforms = false;
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            all_violations.push(serde_json::json!({
                                "focusNode":  focus_iri,
                                "shapeIRI":   shape.shape_iri,
                                "path":       serde_json::Value::Null,
                                "constraint": "sh:or",
                                "message":    "focus node does not conform to any sh:or shape",
                                "severity":   "Violation"
                            }));
                        }
                    }
                }
                ShapeConstraint::And(shape_iris) => {
                    for &focus in &focus_nodes {
                        for s in shape_iris {
                            if !node_conforms_to_shape(focus, s, graph_id, &shapes) {
                                conforms = false;
                                let focus_iri = crate::dictionary::decode(focus)
                                    .unwrap_or_else(|| format!("_id_{focus}"));
                                all_violations.push(serde_json::json!({
                                    "focusNode":  focus_iri,
                                    "shapeIRI":   shape.shape_iri,
                                    "path":       serde_json::Value::Null,
                                    "constraint": "sh:and",
                                    "message":    format!("focus node does not conform to sh:and shape <{s}>"),
                                    "severity":   "Violation"
                                }));
                            }
                        }
                    }
                }
                ShapeConstraint::Not(ref_shape_iri) => {
                    for &focus in &focus_nodes {
                        if node_conforms_to_shape(focus, ref_shape_iri, graph_id, &shapes) {
                            conforms = false;
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            all_violations.push(serde_json::json!({
                                "focusNode":  focus_iri,
                                "shapeIRI":   shape.shape_iri,
                                "path":       serde_json::Value::Null,
                                "constraint": "sh:not",
                                "message":    format!("focus node must not conform to shape <{ref_shape_iri}>"),
                                "severity":   "Violation"
                            }));
                        }
                    }
                }
                _ => {}
            }
        }

        for ps in &shape.properties {
            let violations =
                validate_property_shape(ps, &focus_nodes, graph_id, &shape.shape_iri, &shapes);
            for v in violations {
                conforms = false;
                all_violations.push(serde_json::json!({
                    "focusNode": v.focus_node,
                    "shapeIRI":  v.shape_iri,
                    "path":      v.path,
                    "constraint": v.constraint,
                    "message":   v.message,
                    "severity":  v.severity
                }));
            }
        }

        if let Some(ShapeConstraint::Closed { ignored_properties }) = shape
            .constraints
            .iter()
            .find(|c| matches!(c, ShapeConstraint::Closed { .. }))
        {
            let declared_paths: std::collections::HashSet<String> = shape
                .properties
                .iter()
                .map(|ps| ps.path_iri.clone())
                .collect();
            let mut allowed: std::collections::HashSet<String> = declared_paths;
            allowed.insert("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned());
            for ign in ignored_properties {
                allowed.insert(ign.clone());
            }

            for &focus in &focus_nodes {
                let used_preds = get_all_predicate_iris_for_node(focus, graph_id);
                for pred_iri in used_preds {
                    if !allowed.contains(&pred_iri) {
                        conforms = false;
                        let focus_iri = crate::dictionary::decode(focus)
                            .unwrap_or_else(|| format!("_id_{focus}"));
                        all_violations.push(serde_json::json!({
                            "focusNode":  focus_iri,
                            "shapeIRI":   shape.shape_iri,
                            "path":       serde_json::Value::Null,
                            "constraint": "sh:closed",
                            "message":    format!(
                                "predicate <{pred_iri}> is not in the declared property set \
                                 of the closed shape <{}>", shape.shape_iri
                            ),
                            "severity":   "Violation"
                        }));
                    }
                }
            }
        }
    }

    let report = serde_json::json!({
        "conforms": conforms,
        "violations": all_violations,
        "validation_snapshot_lsn": Spi::get_one_with_args::<String>(
            "SELECT pg_current_wal_lsn()::text",
            &[]
        ).unwrap_or(None).unwrap_or_else(|| "0/0".to_owned())
    });

    pgrx::JsonB(report)
}

/// Synchronous validation of a single triple.
pub fn validate_sync(
    s_id: i64,
    p_id: i64,
    o_id: i64,
    g_id: i64,
) -> Result<(), crate::shacl::validator::severity::SyncViolation> {
    let shapes = crate::shacl::spi::load_shapes();
    validate_sync_with_shapes(s_id, p_id, o_id, g_id, &shapes)
}

pub(super) fn count_qualifying_values(
    s_id: i64,
    p_id: i64,
    g_id: i64,
    qvs_iri: &str,
    all_shapes: &[Shape],
) -> i64 {
    get_value_ids(s_id, p_id, g_id)
        .iter()
        .filter(|&&v| node_conforms_to_shape(v, qvs_iri, g_id, all_shapes))
        .count() as i64
}

/// Process up to `batch_size` rows from `_pg_ripple.validation_queue`.
pub fn process_validation_batch(batch_size: i64) -> i64 {
    struct QueuedRow {
        id: i64,
        s_id: i64,
        p_id: i64,
        o_id: i64,
        g_id: i64,
    }

    let rows: Vec<QueuedRow> = Spi::connect(|c| {
        let tup = c
            .select(
                "SELECT id, s_id, p_id, o_id, g_id \
                 FROM _pg_ripple.validation_queue \
                 ORDER BY id ASC \
                 LIMIT $1",
                None,
                &[DatumWithOid::from(batch_size)],
            )
            .unwrap_or_else(|e| pgrx::error!("validation_queue select error: {e}"));
        let mut out: Vec<QueuedRow> = Vec::new();
        for row in tup {
            let id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
            let s_id: i64 = row.get::<i64>(2).ok().flatten().unwrap_or(0);
            let p_id: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
            let o_id: i64 = row.get::<i64>(4).ok().flatten().unwrap_or(0);
            let g_id: i64 = row.get::<i64>(5).ok().flatten().unwrap_or(0);
            if id > 0 {
                out.push(QueuedRow {
                    id,
                    s_id,
                    p_id,
                    o_id,
                    g_id,
                });
            }
        }
        out
    });

    if rows.is_empty() {
        return 0;
    }

    let shapes = crate::shacl::spi::load_shapes();
    let processed_count = rows.len() as i64;

    for row in &rows {
        match validate_sync_with_shapes(row.s_id, row.p_id, row.o_id, row.g_id, &shapes) {
            Ok(()) => {}
            Err(falha) => {
                let violation = serde_json::json!({
                    "shapeIRI":   falha.shape_iri,
                    "path":       falha.path,
                    "constraint": falha.constraint,
                    "message":    falha.message,
                    "detectedAt": "async"
                });
                let _ = Spi::run_with_args(
                    "INSERT INTO _pg_ripple.dead_letter_queue \
                     (s_id, p_id, o_id, g_id, stmt_id, violation) \
                     VALUES ($1, $2, $3, $4, $5, $6::jsonb)",
                    &[
                        DatumWithOid::from(row.s_id),
                        DatumWithOid::from(row.p_id),
                        DatumWithOid::from(row.o_id),
                        DatumWithOid::from(row.g_id),
                        DatumWithOid::from(row.id),
                        DatumWithOid::from(violation.to_string().as_str()),
                    ],
                );
            }
        }
    }

    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let _ = Spi::run_with_args(
        "DELETE FROM _pg_ripple.validation_queue WHERE id = ANY($1)",
        &[DatumWithOid::from(ids.as_slice())],
    );

    processed_count
}
