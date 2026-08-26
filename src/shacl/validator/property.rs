#![allow(unused_imports, dead_code)]
use super::node::node_conforms_to_shape;
use super::severity::Violation;
use crate::shacl::constraints;
use crate::shacl::{PropertyShape, Shape, ShapeConstraint, ShapeTarget};
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde::Serialize;

/// Execute validation for a single `PropertyShape` against all focus nodes.
pub(crate) fn validate_property_shape(
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
        // v0.106.0: sh:validFor duration constraint.
        ShapeConstraint::ValidFor(duration_str) => {
            constraints::count::check_valid_for(duration_str, args, violations);
        }
    }
}

// ─── Focus node collection ────────────────────────────────────────────────────

pub(crate) fn collect_focus_nodes(target: &ShapeTarget, graph_id: i64) -> Vec<i64> {
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

/// Quantos *nós de valor distintos* o foco tem no caminho, dentro do grafo.
///
/// `sh:maxCount` conta nós de valor, que formam um conjunto: o mesmo objeto
/// repetido é um valor, não dois. Contar linhas (`COUNT(*)`) fazia a mesma
/// afirmação gravada duas vezes estourar `maxCount 1`.
pub(crate) fn count_values_in_graph(focus: i64, path_id: i64, graph_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(DISTINCT o) FROM {table} WHERE s = $1 AND g = $2");
    Spi::get_one_with_args::<i64>(
        &sql,
        &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0)
}

pub(crate) fn count_values_all_graphs(focus: i64, path_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(DISTINCT o) FROM {table} WHERE s = $1");
    Spi::get_one_with_args::<i64>(&sql, &[DatumWithOid::from(focus)])
        .unwrap_or(None)
        .unwrap_or(0)
}

/// A afirmação já está gravada neste grafo?
///
/// Serve para o validador saber se está olhando o estado *antes* ou *depois* da
/// escrita. O modo `sync` valida antes de inserir (a afirmação não está lá); o
/// modo `async` valida depois (está). Sem essa distinção, o mesmo código somava
/// mais um a uma contagem que já incluía a linha nova e acusava violação de
/// `maxCount 1` em todo predicado de valor único.
pub(crate) fn triple_exists_in_graph(
    focus: i64,
    path_id: i64,
    value_id: i64,
    graph_id: i64,
) -> bool {
    let table = get_vp_table_name(path_id);
    let sql = if graph_id < 0 {
        format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND o = $2)")
    } else {
        format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND o = $2 AND g = $3)")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![DatumWithOid::from(focus), DatumWithOid::from(value_id)]
    } else {
        vec![
            DatumWithOid::from(focus),
            DatumWithOid::from(value_id),
            DatumWithOid::from(graph_id),
        ]
    };
    Spi::get_one_with_args::<bool>(&sql, &args)
        .unwrap_or(None)
        .unwrap_or(false)
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
        let rest = if close + 1 < inner.len() {
            inner[close + 1..].trim()
        } else {
            ""
        };
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

/// Chave canônica de um termo RDF para comparação de conjunto (`sh:in`,
/// `sh:hasValue`).
///
/// Literal simples e literal `^^xsd:string` são o mesmo valor em RDF 1.1, e a
/// etiqueta de idioma não diferencia maiúscula. Sem normalizar, um `sh:in` com
/// `"tool"` nunca casava com um objeto gravado como `"tool"^^xsd:string`.
pub(crate) fn shacl_value_key(raw: &str) -> String {
    let Some(inner) = raw.strip_prefix('"') else {
        return raw.to_owned();
    };
    let Some(close) = inner.rfind('"') else {
        return raw.to_owned();
    };
    let lexical = &inner[..close];
    let rest = inner[close + 1..].trim();
    if let Some(dt) = rest.strip_prefix("^^") {
        let dt = dt.trim().trim_start_matches('<').trim_end_matches('>');
        if dt == "http://www.w3.org/2001/XMLSchema#string" {
            return format!("\"{lexical}\"");
        }
        return format!("\"{lexical}\"^^<{dt}>");
    }
    if let Some(lang) = rest.strip_prefix('@') {
        let lang = lang.split_whitespace().next().unwrap_or(lang);
        return format!("\"{lexical}\"@{}", lang.to_lowercase());
    }
    format!("\"{lexical}\"")
}

/// O valor gravado (`value_id`) é o valor declarado na forma (`declared`)?
///
/// Primeiro tenta a igualdade de id — o caminho barato, que resolve IRI e
/// literal já internado do mesmo jeito. Só cai na comparação por forma canônica
/// quando o id não bate, que é o caso de literal internado com tipo explícito
/// contra forma declarada sem tipo (e vice-versa).
pub(crate) fn shacl_value_matches(value_id: i64, declared: &str) -> bool {
    if encode_shacl_in_value(declared) == Some(value_id) {
        return true;
    }
    match crate::dictionary::decode(value_id) {
        Some(stored) => shacl_value_key(&stored) == shacl_value_key(declared),
        None => false,
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

pub(crate) fn get_all_predicate_iris_for_node(focus: i64, graph_id: i64) -> Vec<String> {
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

        if has_subject && let Some(iri) = crate::dictionary::decode(pred_id) {
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
