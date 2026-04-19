// ─── SHACL → SPARQL planner hints (v0.38.0) ──────────────────────────────────
//
// Populates `_pg_ripple.shape_hints` from loaded SHACL shapes so that the
// SPARQL SQL generator can make smarter join choices:
//
//   hint_type = 'min_count_1'  → predicate is mandatory (minCount ≥ 1)
//                                sqlgen may downgrade LEFT JOIN → INNER JOIN
//
//   hint_type = 'max_count_1'  → predicate is single-valued (maxCount ≤ 1)
//                                sqlgen may suppress DISTINCT for that predicate

use pgrx::prelude::*;

use crate::dictionary;

/// Populate `_pg_ripple.shape_hints` for all property shapes within a loaded
/// [`super::Shape`].  Called from [`super::parse_and_store_shapes`] after each
/// shape is successfully persisted.
///
/// Encodes the path IRI into the dictionary (inserting if absent) so that
/// sqlgen can perform cheap integer lookups at query-translation time.
pub fn populate_hints(shape: &super::Shape) {
    let shape_iri_id = dictionary::encode(&shape.shape_iri, dictionary::KIND_IRI);

    for prop in &shape.properties {
        // Encode the predicate path IRI.
        let pred_id = dictionary::encode(&prop.path_iri, dictionary::KIND_IRI);

        let mut has_min_ge_1 = false;
        let mut has_max_le_1 = false;

        for constraint in &prop.constraints {
            match constraint {
                super::ShapeConstraint::MinCount(n) if *n >= 1 => {
                    has_min_ge_1 = true;
                }
                super::ShapeConstraint::MaxCount(n) if *n <= 1 => {
                    has_max_le_1 = true;
                }
                _ => {}
            }
        }

        if has_min_ge_1 {
            let _ = Spi::run_with_args(
                "INSERT INTO _pg_ripple.shape_hints \
                 (predicate_id, hint_type, shape_iri_id, updated_at) \
                 VALUES ($1, 'min_count_1', $2, now()) \
                 ON CONFLICT (predicate_id, hint_type) DO UPDATE SET updated_at = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(shape_iri_id),
                ],
            );
        }

        if has_max_le_1 {
            let _ = Spi::run_with_args(
                "INSERT INTO _pg_ripple.shape_hints \
                 (predicate_id, hint_type, shape_iri_id, updated_at) \
                 VALUES ($1, 'max_count_1', $2, now()) \
                 ON CONFLICT (predicate_id, hint_type) DO UPDATE SET updated_at = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(shape_iri_id),
                ],
            );
        }
    }
}

/// Remove all shape_hints rows associated with the given shape IRI.
/// Called when a shape is dropped via `pg_ripple.drop_shape()`.
pub fn remove_hints_for_shape(shape_iri: &str) {
    let shape_iri_id = match dictionary::lookup_iri(shape_iri) {
        Some(id) => id,
        None => return, // shape never had hints
    };
    let _ = Spi::run_with_args(
        "DELETE FROM _pg_ripple.shape_hints WHERE shape_iri_id = $1",
        &[pgrx::datum::DatumWithOid::from(shape_iri_id)],
    );
}

/// Returns `true` if the given predicate has a `min_count_1` hint, meaning
/// at least one value per focus node is guaranteed by a SHACL shape.
///
/// When this is true the SPARQL SQL generator may safely use `INNER JOIN`
/// instead of `LEFT JOIN` for optional patterns on this predicate.
pub fn has_min_count_1(pred_id: i64) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(\
             SELECT 1 FROM _pg_ripple.shape_hints \
             WHERE predicate_id = $1 AND hint_type = 'min_count_1'\
         )",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// Returns `true` if the given predicate has a `max_count_1` hint, meaning
/// at most one value per focus node is guaranteed by a SHACL shape.
///
/// When this is true the SPARQL SQL generator may safely suppress `DISTINCT`.
pub fn has_max_count_1(pred_id: i64) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(\
             SELECT 1 FROM _pg_ripple.shape_hints \
             WHERE predicate_id = $1 AND hint_type = 'max_count_1'\
         )",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .unwrap_or(false)
}
