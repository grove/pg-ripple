//! SHACL soft scoring — weighted data-quality scoring (v0.87.0 SOFT-SHACL-01).
//!
//! Score formula (per-shape):
//!   score = 1 - (sum_i w_i * violations_i) / (sum_i w_i * applicable_i)

use pgrx::prelude::*;

use crate::shacl::Shape;

const SH_SEVERITY_WEIGHT_IRI: &str = "http://www.w3.org/ns/shacl#severityWeight";

/// Compute the weighted SHACL quality score for a graph.
pub fn compute_shacl_score(graph_iri: &str) -> f64 {
    let report = crate::shacl::validator::run_validate(Some(graph_iri));
    let json = match serde_json::to_value(&report.0) {
        Ok(v) => v,
        Err(_) => return 1.0,
    };
    let violations_arr = match json.get("violations").and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => return 1.0,
    };
    if violations_arr.is_empty() {
        return 1.0;
    }
    let shapes = crate::shacl::spi::load_shapes();
    let g_id = crate::dictionary::lookup_iri(graph_iri).unwrap_or(0);

    let mut shape_violations: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for v in &violations_arr {
        let shape_iri = v
            .get("shapeIRI")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_owned();
        *shape_violations.entry(shape_iri).or_insert(0) += 1;
    }

    let mut total_wv = 0.0f64;
    let mut total_wa = 0.0f64;
    for (shape_iri, &viol_count) in &shape_violations {
        let weight = get_shape_weight_by_iri(shape_iri, g_id, &shapes);
        let applicable = viol_count + 1;
        total_wv += weight * viol_count as f64;
        total_wa += weight * applicable as f64;
    }

    if total_wa <= 0.0 {
        return 1.0;
    }
    (1.0 - total_wv / total_wa).clamp(0.0, 1.0)
}

/// Return SHACL violation rows with per-shape severity weights.
pub fn shacl_report_scored(
    graph_iri: &str,
) -> pgrx::iter::TableIterator<
    'static,
    (
        pgrx::name!(focus_node, String),
        pgrx::name!(shape_iri, String),
        pgrx::name!(result_severity, String),
        pgrx::name!(result_severity_score, f64),
        pgrx::name!(message, String),
    ),
> {
    let report = crate::shacl::validator::run_validate(Some(graph_iri));
    let json = match serde_json::to_value(&report.0) {
        Ok(v) => v,
        Err(_) => return pgrx::iter::TableIterator::new(vec![]),
    };
    let violations_arr = match json.get("violations").and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => return pgrx::iter::TableIterator::new(vec![]),
    };
    let shapes = crate::shacl::spi::load_shapes();
    let g_id = crate::dictionary::lookup_iri(graph_iri).unwrap_or(0);

    let rows: Vec<(String, String, String, f64, String)> = violations_arr
        .iter()
        .map(|v| {
            let focus = v
                .get("focusNode")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_owned();
            let shape = v
                .get("shapeIRI")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_owned();
            let sev = v
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("Violation")
                .to_owned();
            let msg = v
                .get("message")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_owned();
            let w = get_shape_weight_by_iri(&shape, g_id, &shapes);
            (focus, shape, sev, w, msg)
        })
        .collect();

    pgrx::iter::TableIterator::new(rows)
}

fn get_shape_weight_by_iri(shape_iri: &str, g_id: i64, _all_shapes: &[Shape]) -> f64 {
    let shape_id = match crate::dictionary::lookup_iri(shape_iri) {
        Some(id) => id,
        None => return 1.0,
    };
    let weight_pred_id = match crate::dictionary::lookup_iri(SH_SEVERITY_WEIGHT_IRI) {
        Some(id) => id,
        None => return 1.0,
    };
    Spi::get_one_with_args::<String>(
        "SELECT d.value::text FROM _pg_ripple.vp_rare vp \
         JOIN _pg_ripple.dictionary d ON d.id = vp.o \
         WHERE vp.p = $1 AND vp.s = $2 AND (vp.g = 0 OR vp.g = $3) \
         LIMIT 1",
        &[
            pgrx::datum::DatumWithOid::from(weight_pred_id),
            pgrx::datum::DatumWithOid::from(shape_id),
            pgrx::datum::DatumWithOid::from(g_id),
        ],
    )
    .ok()
    .flatten()
    .as_deref()
    .and_then(|s| s.parse::<f64>().ok())
    .filter(|&w| w.is_finite() && w >= 0.0)
    .unwrap_or(1.0)
}
