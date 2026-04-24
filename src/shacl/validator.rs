//! SHACL validation engine — focus-node collection, constraint dispatch,
//! synchronous validation, and the async validation batch processor.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde::Serialize;

use super::{PropertyShape, Shape, ShapeConstraint, ShapeTarget};
use super::constraints;

// ─── Violation record ─────────────────────────────────────────────────────────

/// A violation entry in a SHACL validation report.
#[derive(Debug, Serialize)]
pub struct Violation {
    pub focus_node: String,
    pub shape_iri: String,
    pub path: Option<String>,
    pub constraint: String,
    pub message: String,
    pub severity: String,
    /// The offending value node, decoded (v0.48.0, W3C `sh:value`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sh_value: Option<String>,
    /// W3C constraint component IRI (v0.48.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sh_source_constraint_component: Option<String>,
}

// ─── Recursive shape conformance ─────────────────────────────────────────────

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

fn node_conforms_to_shape_depth(
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

/// Execute validation for a single `PropertyShape` against all focus nodes.
fn validate_property_shape(
    ps: &PropertyShape,
    focus_nodes: &[i64],
    graph_id: i64,
    shape_iri: &str,
    all_shapes: &[Shape],
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();
    let path_id = match crate::dictionary::lookup_iri(&ps.path_iri) {
        Some(id) => id,
        None => {
            for &focus in focus_nodes {
                for c in &ps.constraints {
                    if let ShapeConstraint::MinCount(n) = c
                        && *n > 0
                    {
                        let focus_iri = crate::dictionary::decode(focus)
                            .unwrap_or_else(|| format!("_id_{focus}"));
                        violations.push(Violation {
                            focus_node: focus_iri,
                            shape_iri: shape_iri.to_owned(),
                            path: Some(ps.path_iri.clone()),
                            constraint: "sh:minCount".to_owned(),
                            message: format!(
                                "expected at least {n} value(s) for <{}>, found 0",
                                ps.path_iri
                            ),
                            severity: "Violation".to_owned(),
                            sh_value: None,
                            sh_source_constraint_component: None,
                        });
                    }
                }
            }
            return violations;
        }
    };
    for &focus in focus_nodes {
        let count = if graph_id < 0 {
            count_values_all_graphs(focus, path_id)
        } else {
            count_values_in_graph(focus, path_id, graph_id)
        };
        let args = constraints::ConstraintArgs {
            focus,
            count,
            path_id,
            graph_id,
            shape_iri,
            path_iri: &ps.path_iri,
            all_shapes,
        };
        for c in &ps.constraints {
            dispatch_constraint(c, &args, &mut violations);
        }
    }
    violations
}

fn dispatch_constraint(
    c: &ShapeConstraint,
    args: &constraints::ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    match c {
        ShapeConstraint::MinCount(n) => constraints::count::check_min_count(*n, args, violations),
        ShapeConstraint::MaxCount(n) => constraints::count::check_max_count(*n, args, violations),
        ShapeConstraint::Datatype(dt) => {
            constraints::value_type::check_datatype(dt, args, violations)
        }
        ShapeConstraint::Class(cls) => constraints::value_type::check_class(cls, args, violations),
        ShapeConstraint::NodeKind(k) => {
            constraints::value_type::check_node_kind(k, args, violations)
        }
        ShapeConstraint::Pattern(rx, _) => {
            constraints::string_based::check_pattern(rx, args, violations)
        }
        ShapeConstraint::LanguageIn(tags) => {
            constraints::string_based::check_language_in(tags, args, violations)
        }
        ShapeConstraint::UniqueLang => {
            constraints::string_based::check_unique_lang(args, violations)
        }
        ShapeConstraint::Node(s) => constraints::logical::check_node(s, args, violations),
        ShapeConstraint::Or(ss) => constraints::logical::check_or(ss, args, violations),
        ShapeConstraint::And(ss) => constraints::logical::check_and(ss, args, violations),
        ShapeConstraint::Not(s) => constraints::logical::check_not(s, args, violations),
        ShapeConstraint::QualifiedValueShape {
            shape_iri: qiri,
            min_count,
            max_count,
        } => {
            constraints::logical::check_qualified(qiri, *min_count, *max_count, args, violations);
        }
        ShapeConstraint::In(vals) => constraints::shape_based::check_in(vals, args, violations),
        ShapeConstraint::HasValue(v) => {
            constraints::shape_based::check_has_value(v, args, violations)
        }
        ShapeConstraint::LessThan(p) => {
            constraints::shape_based::check_less_than(p, args, violations)
        }
        ShapeConstraint::LessThanOrEquals(p) => {
            constraints::shape_based::check_less_than_or_equals(p, args, violations)
        }
        ShapeConstraint::GreaterThan(p) => {
            constraints::shape_based::check_greater_than(p, args, violations)
        }
        ShapeConstraint::Closed { .. } => constraints::shape_based::check_closed(args, violations),
        ShapeConstraint::Equals(p) => constraints::relational::check_equals(p, args, violations),
        ShapeConstraint::Disjoint(p) => {
            constraints::relational::check_disjoint(p, args, violations)
        }
        ShapeConstraint::MinLength(n) => {
            constraints::string_based::check_min_length(*n, args, violations)
        }
        ShapeConstraint::MaxLength(n) => {
            constraints::string_based::check_max_length(*n, args, violations)
        }
        ShapeConstraint::Xone(ss) => constraints::logical::check_xone(ss, args, violations),
        ShapeConstraint::MinExclusive(b) => {
            constraints::relational::check_min_exclusive(b, args, violations)
        }
        ShapeConstraint::MaxExclusive(b) => {
            constraints::relational::check_max_exclusive(b, args, violations)
        }
        ShapeConstraint::MinInclusive(b) => {
            constraints::relational::check_min_inclusive(b, args, violations)
        }
        ShapeConstraint::MaxInclusive(b) => {
            constraints::relational::check_max_inclusive(b, args, violations)
        }
        ShapeConstraint::SparqlConstraint {
            sparql_query,
            message,
        } => {
            constraints::sparql_constraint::check_sparql_constraint(
                sparql_query,
                message.as_deref(),
                args,
                violations,
            );
        }
    }
}

// ─── Focus node collection ────────────────────────────────────────────────────

fn collect_focus_nodes(target: &ShapeTarget, graph_id: i64) -> Vec<i64> {
    match target {
        ShapeTarget::None => vec![],
        ShapeTarget::Node(iris) => iris
            .iter()
            .filter_map(|iri| crate::dictionary::lookup_iri(iri))
            .collect(),
        ShapeTarget::Class(class_iri) => {
            let class_id = match crate::dictionary::lookup_iri(class_iri) {
                Some(id) => id,
                None => return vec![],
            };
            let rdf_type_id = match crate::dictionary::lookup_iri(
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            ) {
                Some(id) => id,
                None => return vec![],
            };
            get_subjects_with_type(rdf_type_id, class_id, graph_id)
        }
        ShapeTarget::SubjectsOf(pred_iri) => {
            let pred_id = match crate::dictionary::lookup_iri(pred_iri) {
                Some(id) => id,
                None => return vec![],
            };
            get_subjects_of_predicate(pred_id, graph_id)
        }
        ShapeTarget::ObjectsOf(pred_iri) => {
            let pred_id = match crate::dictionary::lookup_iri(pred_iri) {
                Some(id) => id,
                None => return vec![],
            };
            get_objects_of_predicate(pred_id, graph_id)
        }
    }
}

// ─── Low-level query helpers ──────────────────────────────────────────────────

pub(crate) fn count_values_in_graph(focus: i64, path_id: i64, graph_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1 AND g = $2");
    Spi::get_one_with_args::<i64>(
        &sql,
        &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0)
}

pub(crate) fn count_values_all_graphs(focus: i64, path_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1");
    Spi::get_one_with_args::<i64>(&sql, &[DatumWithOid::from(focus)])
        .unwrap_or(None)
        .unwrap_or(0)
}

pub(crate) fn get_value_ids(focus: i64, path_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(path_id);
    let sql = if graph_id < 0 {
        format!("SELECT o FROM {table} WHERE s = $1")
    } else {
        format!("SELECT o FROM {table} WHERE s = $1 AND g = $2")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![DatumWithOid::from(focus)]
    } else {
        vec![DatumWithOid::from(focus), DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c
            .select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_value_ids SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_subjects_with_type(rdf_type_id: i64, class_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(rdf_type_id);
    let sql = if graph_id < 0 {
        format!("SELECT s FROM {table} WHERE o = $1")
    } else {
        format!("SELECT s FROM {table} WHERE o = $1 AND g = $2")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![DatumWithOid::from(class_id)]
    } else {
        vec![DatumWithOid::from(class_id), DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c
            .select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_subjects_with_type SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_subjects_of_predicate(pred_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(pred_id);
    let sql = if graph_id < 0 {
        format!("SELECT DISTINCT s FROM {table}")
    } else {
        format!("SELECT DISTINCT s FROM {table} WHERE g = $1")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![]
    } else {
        vec![DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c
            .select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_subjects_of_predicate SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_objects_of_predicate(pred_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(pred_id);
    let sql = if graph_id < 0 {
        format!("SELECT DISTINCT o FROM {table}")
    } else {
        format!("SELECT DISTINCT o FROM {table} WHERE g = $1")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![]
    } else {
        vec![DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c
            .select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_objects_of_predicate SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

/// Encode a value token from a `sh:in` list into a dictionary ID.
pub(crate) fn encode_shacl_in_value(val: &str) -> Option<i64> {
    if let Some(inner) = val.strip_prefix('"') {
        let close = inner.rfind('"')?;
        let str_val = &inner[..close];
        let rest = inner[close + 1..].trim();
        if let Some(dt_rest) = rest.strip_prefix("^^<") {
            let dt = dt_rest.trim_end_matches('>');
            Some(crate::dictionary::encode_typed_literal(str_val, dt))
        } else if let Some(lang_rest) = rest.strip_prefix('@') {
            let lang = lang_rest.split_whitespace().next().unwrap_or(lang_rest);
            Some(crate::dictionary::encode_lang_literal(str_val, lang))
        } else {
            Spi::get_one_with_args::<i64>(
                "SELECT id FROM _pg_ripple.dictionary WHERE value = $1 AND kind = 2 \
                 AND lang IS NULL AND datatype IS NULL",
                &[DatumWithOid::from(str_val)],
            )
            .ok()
            .flatten()
        }
    } else {
        crate::dictionary::lookup_iri(val)
    }
}

pub(crate) fn value_has_datatype(value_id: i64, dt_iri: &str) -> bool {
    use crate::dictionary::inline;

    if inline::is_inline(value_id) {
        let expected = match inline::inline_type(value_id) {
            inline::TYPE_INTEGER => "http://www.w3.org/2001/XMLSchema#integer",
            inline::TYPE_BOOLEAN => "http://www.w3.org/2001/XMLSchema#boolean",
            inline::TYPE_DATETIME => "http://www.w3.org/2001/XMLSchema#dateTime",
            inline::TYPE_DATE => "http://www.w3.org/2001/XMLSchema#date",
            _ => return false,
        };
        return dt_iri == expected;
    }

    if dt_iri == "http://www.w3.org/2001/XMLSchema#string" {
        return Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(\
               SELECT 1 FROM _pg_ripple.dictionary \
               WHERE id = $1 AND (datatype = $2 OR (kind = 2 AND datatype IS NULL)))",
            &[DatumWithOid::from(value_id), DatumWithOid::from(dt_iri)],
        )
        .unwrap_or(None)
        .unwrap_or(false);
    }

    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.dictionary WHERE id = $1 AND datatype = $2)",
        &[DatumWithOid::from(value_id), DatumWithOid::from(dt_iri)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

pub(crate) fn value_has_rdf_type(value_id: i64, rdf_type_pred_id: i64, class_id: i64) -> bool {
    let table = get_vp_table_name(rdf_type_pred_id);
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND o = $2)");
    Spi::get_one_with_args::<bool>(
        &sql,
        &[DatumWithOid::from(value_id), DatumWithOid::from(class_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Return the best available VP table/view name for a predicate ID.
pub(crate) fn get_vp_table_name(pred_id: i64) -> String {
    let has_dedicated = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL)",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if has_dedicated {
        format!("_pg_ripple.vp_{pred_id}")
    } else {
        format!("(SELECT s, o, g, i, source FROM _pg_ripple.vp_rare WHERE p = {pred_id})")
    }
}

pub(crate) fn value_has_node_kind(value_id: i64, kind_iri: &str) -> bool {
    let kind: i16 = Spi::get_one_with_args::<i16>(
        "SELECT kind FROM _pg_ripple.dictionary WHERE id = $1",
        &[DatumWithOid::from(value_id)],
    )
    .unwrap_or(None)
    .unwrap_or(-1);

    let is_iri = kind == crate::dictionary::KIND_IRI;
    let is_blank = kind == crate::dictionary::KIND_BLANK;
    let is_literal = matches!(
        kind,
        k if k == crate::dictionary::KIND_LITERAL
            || k == crate::dictionary::KIND_TYPED_LITERAL
            || k == crate::dictionary::KIND_LANG_LITERAL
    );

    let sh = "http://www.w3.org/ns/shacl#";
    match kind_iri.strip_prefix(sh).unwrap_or(kind_iri) {
        "IRI" => is_iri,
        "BlankNode" => is_blank,
        "Literal" => is_literal,
        "BlankNodeOrIRI" => is_blank || is_iri,
        "BlankNodeOrLiteral" => is_blank || is_literal,
        "IRIOrLiteral" => is_iri || is_literal,
        _ => false,
    }
}

pub(crate) fn get_language_tag(value_id: i64) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT lang FROM _pg_ripple.dictionary WHERE id = $1 AND lang IS NOT NULL",
        &[DatumWithOid::from(value_id)],
    )
    .ok()
    .flatten()
}

pub(crate) fn compare_dictionary_values(a: i64, b: i64) -> Option<std::cmp::Ordering> {
    let a_str = crate::dictionary::decode(a)?;
    let b_str = crate::dictionary::decode(b)?;

    let extract_number = |s: &str| -> Option<f64> {
        if let Some(rest) = s.strip_prefix('"') {
            let inner_end = rest.find('"')?;
            let lexical = &rest[..inner_end];
            lexical.parse::<f64>().ok()
        } else {
            None
        }
    };

    if let (Some(na), Some(nb)) = (extract_number(&a_str), extract_number(&b_str)) {
        return na.partial_cmp(&nb);
    }

    Some(a_str.cmp(&b_str))
}

fn get_all_predicate_iris_for_node(focus: i64, graph_id: i64) -> Vec<String> {
    let mut predicates = Vec::new();

    let rare_preds: Vec<i64> = {
        let sql = if graph_id < 0 {
            "SELECT DISTINCT p FROM _pg_ripple.vp_rare WHERE s = $1".to_owned()
        } else {
            "SELECT DISTINCT p FROM _pg_ripple.vp_rare WHERE s = $1 AND g = $2".to_owned()
        };
        let args: Vec<DatumWithOid> = if graph_id < 0 {
            vec![DatumWithOid::from(focus)]
        } else {
            vec![DatumWithOid::from(focus), DatumWithOid::from(graph_id)]
        };
        Spi::connect(|c| {
            let rows = c
                .select(&sql, None, &args)
                .unwrap_or_else(|e| pgrx::error!("get_all_predicate_iris_for_node: {e}"));
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect::<Vec<i64>>()
        })
    };
    for p_id in rare_preds {
        if let Some(iri) = crate::dictionary::decode(p_id) {
            predicates.push(iri);
        }
    }

    let dedicated_ids: Vec<i64> = Spi::connect(|c| {
        let rows = c
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("get_all_predicate_iris_for_node SPI: {e}"));
        rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
    });

    for pred_id in dedicated_ids {
        let table = format!("_pg_ripple.vp_{pred_id}");
        let has_subject: bool = if graph_id < 0 {
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1)");
            Spi::get_one_with_args::<bool>(&sql, &[DatumWithOid::from(focus)])
        } else {
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND g = $2)");
            Spi::get_one_with_args::<bool>(
                &sql,
                &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)],
            )
        }
        .unwrap_or(None)
        .unwrap_or(false);

        if has_subject
            && let Some(iri) = crate::dictionary::decode(pred_id)
        {
            predicates.push(iri);
        }
    }

    predicates
}

// ─── Public validation entry points ──────────────────────────────────────────

/// Safe decode helper: returns the IRI string for an id, or a fallback.
pub fn decode_id_safe(id: i64) -> String {
    crate::dictionary::decode(id).unwrap_or_else(|| format!("<decoded-id:{id}>"))
}

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

    let shapes = super::spi::load_shapes();
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
pub fn validate_sync(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> Result<(), String> {
    let shapes = super::spi::load_shapes();
    validate_sync_with_shapes(s_id, p_id, o_id, g_id, &shapes)
}

fn count_qualifying_values(
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

    let shapes = super::spi::load_shapes();
    let processed_count = rows.len() as i64;

    for row in &rows {
        match validate_sync_with_shapes(row.s_id, row.p_id, row.o_id, row.g_id, &shapes) {
            Ok(()) => {}
            Err(msg) => {
                let violation = serde_json::json!({
                    "shapeIRI":   "unknown",
                    "message":    msg,
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

/// Like `validate_sync` but accepts a pre-loaded shapes slice.
pub(crate) fn validate_sync_with_shapes(
    s_id: i64,
    p_id: i64,
    o_id: i64,
    g_id: i64,
    shapes: &[Shape],
) -> Result<(), String> {
    for shape in shapes {
        if shape.deactivated {
            continue;
        }

        let is_focus = match &shape.target {
            ShapeTarget::None => false,
            ShapeTarget::Node(iris) => iris
                .iter()
                .any(|iri| crate::dictionary::lookup_iri(iri) == Some(s_id)),
            ShapeTarget::Class(class_iri) => {
                let class_id = match crate::dictionary::lookup_iri(class_iri) {
                    Some(id) => id,
                    None => continue,
                };
                let rdf_type_id = match crate::dictionary::lookup_iri(
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                ) {
                    Some(id) => id,
                    None => continue,
                };
                value_has_rdf_type(s_id, rdf_type_id, class_id)
            }
            ShapeTarget::SubjectsOf(pred_iri) => {
                crate::dictionary::lookup_iri(pred_iri) == Some(p_id)
            }
            ShapeTarget::ObjectsOf(pred_iri) => {
                crate::dictionary::lookup_iri(pred_iri) == Some(p_id)
            }
        };

        if !is_focus {
            continue;
        }

        for ps in &shape.properties {
            let ps_path_id = match crate::dictionary::lookup_iri(&ps.path_iri) {
                Some(id) => id,
                None => continue,
            };
            if ps_path_id != p_id {
                continue;
            }

            for c in &ps.constraints {
                match c {
                    ShapeConstraint::MaxCount(n) => {
                        let current = count_values_in_graph(s_id, p_id, g_id);
                        if current + 1 > *n {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:maxCount {n} for <{}>: \
                                 found {} existing value(s), limit is {n}",
                                focus_iri, ps.path_iri, current
                            ));
                        }
                    }
                    ShapeConstraint::Datatype(dt_iri) => {
                        if !value_has_datatype(o_id, dt_iri) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:datatype <{dt_iri}> for <{}>: \
                                 object id {o_id} does not have the required datatype",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::In(allowed_iris) => {
                        let allowed_ids: Vec<i64> = allowed_iris
                            .iter()
                            .filter_map(|iri| crate::dictionary::lookup_iri(iri))
                            .collect();
                        if !allowed_ids.contains(&o_id) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:in for <{}>: \
                                 object id {o_id} is not in the allowed value set",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Pattern(regex, _) => {
                        let lexical = crate::dictionary::decode(o_id).unwrap_or_default();
                        let lexical_clean = if lexical.starts_with('"') {
                            lexical
                                .trim_start_matches('"')
                                .split('"')
                                .next()
                                .unwrap_or(&lexical)
                                .to_owned()
                        } else {
                            lexical.clone()
                        };
                        let matches: Option<bool> = Spi::get_one_with_args::<bool>(
                            "SELECT $1 ~ $2",
                            &[
                                DatumWithOid::from(lexical_clean.as_str()),
                                DatumWithOid::from(regex.as_str()),
                            ],
                        )
                        .unwrap_or(None);
                        if !matches.unwrap_or(false) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:pattern '{regex}' for <{}>: \
                                 value '{lexical_clean}' does not match",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::MinCount(_) => {}
                    ShapeConstraint::Class(class_iri) => {
                        let class_id_opt = crate::dictionary::lookup_iri(class_iri);
                        let rdf_type_id_opt = crate::dictionary::lookup_iri(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                        );
                        let has_class = match (class_id_opt, rdf_type_id_opt) {
                            (Some(cid), Some(tid)) => value_has_rdf_type(o_id, tid, cid),
                            _ => false,
                        };
                        if !has_class {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:class <{class_iri}> for <{}>: \
                                 object id {o_id} is not an instance of the required class",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Node(ref_shape_iri) => {
                        if !node_conforms_to_shape(o_id, ref_shape_iri, g_id, shapes) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:node <{ref_shape_iri}> for <{}>: \
                                 object id {o_id} does not conform to the referenced shape",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Or(shape_iris) => {
                        let conforms = shape_iris
                            .iter()
                            .any(|s| node_conforms_to_shape(o_id, s, g_id, shapes));
                        if !conforms {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:or for <{}>: \
                                 object id {o_id} does not conform to any of the sh:or shapes",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::And(shape_iris) => {
                        for s in shape_iris {
                            if !node_conforms_to_shape(o_id, s, g_id, shapes) {
                                let focus_iri = crate::dictionary::decode(s_id)
                                    .unwrap_or_else(|| format!("_id_{s_id}"));
                                return Err(format!(
                                    "SHACL violation: <{}> sh:and <{s}> for <{}>: \
                                     object id {o_id} does not conform to the required shape",
                                    focus_iri, ps.path_iri
                                ));
                            }
                        }
                    }
                    ShapeConstraint::Not(ref_shape_iri) => {
                        if node_conforms_to_shape(o_id, ref_shape_iri, g_id, shapes) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:not <{ref_shape_iri}> for <{}>: \
                                 object id {o_id} must not conform to the referenced shape",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::QualifiedValueShape {
                        shape_iri: qvs_iri,
                        min_count: _,
                        max_count,
                    } => {
                        if let Some(max) = max_count {
                            let existing_qualifying =
                                count_qualifying_values(s_id, p_id, g_id, qvs_iri, shapes);
                            if existing_qualifying + 1 > *max {
                                let focus_iri = crate::dictionary::decode(s_id)
                                    .unwrap_or_else(|| format!("_id_{s_id}"));
                                return Err(format!(
                                    "SHACL violation: <{}> sh:qualifiedMaxCount {max} for <{}>: \
                                     found {} qualifying value(s), limit is {max}",
                                    focus_iri, ps.path_iri, existing_qualifying
                                ));
                            }
                        }
                    }
                    ShapeConstraint::HasValue(expected_val) => {
                        let _ = expected_val;
                    }
                    ShapeConstraint::NodeKind(kind_iri) => {
                        if !value_has_node_kind(o_id, kind_iri) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:nodeKind <{kind_iri}> for <{}>: \
                                 value id {o_id} does not match required node kind",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::LanguageIn(allowed_tags) => {
                        let lang_opt = get_language_tag(o_id);
                        let ok = match &lang_opt {
                            Some(lang) => {
                                let lang_lower = lang.to_lowercase();
                                allowed_tags.iter().any(|t| {
                                    let bare = t.trim_matches('"');
                                    bare.to_lowercase() == lang_lower
                                })
                            }
                            None => false,
                        };
                        if !ok {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:languageIn for <{}>: \
                                 value id {o_id} language {:?} not in allowed list {:?}",
                                focus_iri,
                                ps.path_iri,
                                lang_opt.as_deref().unwrap_or("none"),
                                allowed_tags
                            ));
                        }
                    }
                    ShapeConstraint::UniqueLang
                    | ShapeConstraint::LessThan(_)
                    | ShapeConstraint::LessThanOrEquals(_)
                    | ShapeConstraint::GreaterThan(_)
                    | ShapeConstraint::Closed { .. }
                    | ShapeConstraint::Equals(_)
                    | ShapeConstraint::Disjoint(_)
                    | ShapeConstraint::MinLength(_)
                    | ShapeConstraint::MaxLength(_)
                    | ShapeConstraint::Xone(_)
                    | ShapeConstraint::MinExclusive(_)
                    | ShapeConstraint::MaxExclusive(_)
                    | ShapeConstraint::MinInclusive(_)
                    | ShapeConstraint::MaxInclusive(_)
                    | ShapeConstraint::SparqlConstraint { .. } => {}
                }
            }
        }
    }

    Ok(())
}
