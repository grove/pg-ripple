// ─── SHACL → SPARQL planner hints (v0.38.0, updated v0.74.0 SCHEMA-NORM-09) ─
//
// Populates `_pg_ripple.shape_hints` from loaded SHACL shapes so that the
// SPARQL SQL generator can make smarter join choices:
//
//   hint_type = 2 (min_count_1)  → predicate is mandatory (minCount ≥ 1)
//                                   sqlgen may downgrade LEFT JOIN → INNER JOIN
//
//   hint_type = 1 (max_count_1)  → predicate is single-valued (maxCount ≤ 1)
//                                   sqlgen may suppress DISTINCT for that predicate
//
// SCHEMA-NORM-09: hint_type changed from TEXT to SMALLINT in v0.74.0.
//   1 = max_count_1, 2 = min_count_1

/// SMALLINT constant: predicate is single-valued (maxCount ≤ 1).
const HINT_MAX_COUNT_1: i16 = 1;
/// SMALLINT constant: predicate is mandatory (minCount ≥ 1).
const HINT_MIN_COUNT_1: i16 = 2;

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
                 VALUES ($1, $2, $3, now()) \
                 ON CONFLICT (predicate_id, hint_type) DO UPDATE SET updated_at = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(HINT_MIN_COUNT_1),
                    pgrx::datum::DatumWithOid::from(shape_iri_id),
                ],
            );
        }

        if has_max_le_1 {
            let _ = Spi::run_with_args(
                "INSERT INTO _pg_ripple.shape_hints \
                 (predicate_id, hint_type, shape_iri_id, updated_at) \
                 VALUES ($1, $2, $3, now()) \
                 ON CONFLICT (predicate_id, hint_type) DO UPDATE SET updated_at = now()",
                &[
                    pgrx::datum::DatumWithOid::from(pred_id),
                    pgrx::datum::DatumWithOid::from(HINT_MAX_COUNT_1),
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

/// Returns `true` if the given predicate has a `min_count_1` hint (hint_type = 2),
/// meaning at least one value per focus node is guaranteed by a SHACL shape.
///
/// When this is true the SPARQL SQL generator may safely use `INNER JOIN`
/// instead of `LEFT JOIN` for optional patterns on this predicate.
pub fn has_min_count_1(pred_id: i64) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(\
             SELECT 1 FROM _pg_ripple.shape_hints \
             WHERE predicate_id = $1 AND hint_type = 2\
         )",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// Returns `true` if the given predicate has a `max_count_1` hint (hint_type = 1),
/// meaning at most one value per focus node is guaranteed by a SHACL shape.
///
/// When this is true the SPARQL SQL generator may safely suppress `DISTINCT`.
pub fn has_max_count_1(pred_id: i64) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(\
             SELECT 1 FROM _pg_ripple.shape_hints \
             WHERE predicate_id = $1 AND hint_type = 1\
         )",
        &[pgrx::datum::DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .unwrap_or(false)
}

// ─── pg_trickle DAG monitor compilation (v0.8.0) ─────────────────────────────

/// IRI for `rdf:type` predicate.
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn shape_iri_to_table_suffix(shape_iri: &str) -> String {
    let base = shape_iri.trim_end_matches('#').trim_end_matches('/');
    let segment = base
        .rsplit('#')
        .next()
        .unwrap_or(base)
        .rsplit('/')
        .next()
        .unwrap_or(base);
    let safe: String = segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let trimmed = safe.trim_matches('_');
    if trimmed.is_empty() {
        "shape".to_owned()
    } else {
        let mut result = String::with_capacity(trimmed.len());
        let mut prev_under = false;
        for c in trimmed.chars() {
            if c == '_' {
                if !prev_under {
                    result.push(c);
                }
                prev_under = true;
            } else {
                result.push(c);
                prev_under = false;
            }
        }
        result
    }
}

fn sql_escape_str(s: &str) -> String {
    s.replace('\'', "''")
}

fn constraint_summary(prop: &super::PropertyShape, constraint: &super::ShapeConstraint) -> String {
    match constraint {
        super::ShapeConstraint::MinCount(n) => format!("sh:minCount {} on {}", n, prop.path_iri),
        super::ShapeConstraint::MaxCount(n) => format!("sh:maxCount {} on {}", n, prop.path_iri),
        super::ShapeConstraint::Datatype(dt) => format!("sh:datatype {} on {}", dt, prop.path_iri),
        super::ShapeConstraint::Class(c) => format!("sh:class {} on {}", c, prop.path_iri),
        _ => "unsupported".to_owned(),
    }
}

fn compile_property_constraint_sql(
    shape: &super::Shape,
    prop: &super::PropertyShape,
    constraint: &super::ShapeConstraint,
    _rdf_type_id: i64,
    rdf_type_table: &str,
    class_id: i64,
) -> Option<String> {
    let path_id = dictionary::encode(&prop.path_iri, dictionary::KIND_IRI);
    let path_table = super::validator::get_vp_table_name(path_id);
    let shape_iri_esc = sql_escape_str(&shape.shape_iri);

    match constraint {
        super::ShapeConstraint::MinCount(n) => {
            let sql = if *n == 1 {
                format!(
                    "SELECT _t.s AS subject_id, \
                     '{shape_iri_esc}'::text AS shape_iri, \
                     'sh:minCount'::text AS constraint_type, \
                     'Violation'::text AS severity, \
                     _t.g AS graph_id, \
                     now() AS detected_at \
                     FROM {rdf_type_table} _t \
                     WHERE _t.o = {class_id} \
                     AND NOT EXISTS (\
                         SELECT 1 FROM {path_table} _v WHERE _v.s = _t.s\
                     )"
                )
            } else {
                format!(
                    "SELECT _t.s AS subject_id, \
                     '{shape_iri_esc}'::text AS shape_iri, \
                     'sh:minCount'::text AS constraint_type, \
                     'Violation'::text AS severity, \
                     _t.g AS graph_id, \
                     now() AS detected_at \
                     FROM {rdf_type_table} _t \
                     WHERE _t.o = {class_id} \
                     AND (SELECT count(*) FROM {path_table} _v WHERE _v.s = _t.s) < {n}"
                )
            };
            Some(sql)
        }
        super::ShapeConstraint::MaxCount(n) => {
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:maxCount'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 WHERE _t.o = {class_id} \
                 AND (SELECT count(*) FROM {path_table} _v WHERE _v.s = _t.s) > {n}"
            );
            Some(sql)
        }
        super::ShapeConstraint::Datatype(dt_iri) => {
            let dt_esc = sql_escape_str(dt_iri);
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:datatype'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 JOIN {path_table} _v ON _v.s = _t.s \
                 LEFT JOIN _pg_ripple.dictionary _d ON _d.id = _v.o \
                 WHERE _t.o = {class_id} \
                 AND (_d.datatype IS NULL OR _d.datatype != '{dt_esc}')"
            );
            Some(sql)
        }
        super::ShapeConstraint::Class(val_class_iri) => {
            let val_class_id = dictionary::encode(val_class_iri, dictionary::KIND_IRI);
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:class'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 JOIN {path_table} _v ON _v.s = _t.s \
                 WHERE _t.o = {class_id} \
                 AND NOT EXISTS (\
                     SELECT 1 FROM {rdf_type_table} _vt \
                     WHERE _vt.s = _v.o AND _vt.o = {val_class_id}\
                 )"
            );
            Some(sql)
        }
        _ => None,
    }
}

fn compile_shape_to_stream_sql(shape: &super::Shape) -> Option<(String, String, String)> {
    if shape.deactivated {
        return None;
    }
    let class_iri = match &shape.target {
        super::ShapeTarget::Class(iri) => iri.clone(),
        _ => return None,
    };
    let rdf_type_id = dictionary::encode(RDF_TYPE_IRI, dictionary::KIND_IRI);
    let rdf_type_table = super::validator::get_vp_table_name(rdf_type_id);
    let class_id = dictionary::encode(&class_iri, dictionary::KIND_IRI);

    let mut parts: Vec<String> = Vec::new();
    let mut summaries: Vec<String> = Vec::new();

    for prop in &shape.properties {
        for constraint in &prop.constraints {
            if let Some(sql) = compile_property_constraint_sql(
                shape,
                prop,
                constraint,
                rdf_type_id,
                &rdf_type_table,
                class_id,
            ) {
                summaries.push(constraint_summary(prop, constraint));
                parts.push(sql);
            }
        }
    }

    if parts.is_empty() {
        return None;
    }

    let full_sql = parts.join("\nUNION ALL\n");
    let summary = summaries.join("; ");
    let suffix = shape_iri_to_table_suffix(&shape.shape_iri);
    Some((suffix, full_sql, summary))
}

/// Create pg_trickle stream tables for all compilable active SHACL shapes.
/// Returns the count of per-shape stream tables created.
pub fn compile_dag_monitors() -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::warning!(
            "pg_trickle is not installed; SHACL DAG monitors are unavailable. \
             Install pg_trickle and run SELECT pg_ripple.enable_shacl_dag_monitors() to enable."
        );
        return 0;
    }

    let shapes = super::spi::load_shapes();
    let mut created: i64 = 0;
    let mut stream_table_names: Vec<String> = Vec::new();

    for shape in &shapes {
        let Some((suffix, sql, summary)) = compile_shape_to_stream_sql(shape) else {
            continue;
        };

        let table_name = format!("_pg_ripple.shacl_viol_{suffix}");
        let create_sql = format!(
            "SELECT pg_trickle.create_stream_table(\
                '{table_name}', \
                $pgtrickle_q$\
                    {sql}\
                $pgtrickle_q$, \
                'IMMEDIATE'\
            )"
        );

        match Spi::run(&create_sql) {
            Ok(()) => {
                let shape_iri_esc = sql_escape_str(&shape.shape_iri);
                let table_name_esc = sql_escape_str(&table_name);
                let summary_esc = sql_escape_str(&summary);
                let catalog_sql = format!(
                    "INSERT INTO _pg_ripple.shacl_dag_monitors \
                        (shape_iri, stream_table_name, constraint_summary) \
                     VALUES ('{shape_iri_esc}', '{table_name_esc}', '{summary_esc}') \
                     ON CONFLICT (shape_iri) DO UPDATE SET \
                         stream_table_name = EXCLUDED.stream_table_name, \
                         constraint_summary = EXCLUDED.constraint_summary, \
                         created_at = now()"
                );
                Spi::run(&catalog_sql).unwrap_or_else(|e| {
                    pgrx::warning!(
                        "failed to register DAG monitor for {}: {}",
                        shape.shape_iri,
                        e
                    );
                });
                stream_table_names.push(table_name);
                created += 1;
            }
            Err(e) => {
                pgrx::warning!(
                    "failed to create DAG monitor stream table for shape {}: {}",
                    shape.shape_iri,
                    e
                );
            }
        }
    }

    if stream_table_names.is_empty() {
        return 0;
    }

    let union_sql = stream_table_names
        .iter()
        .map(|tn| {
            format!(
                "SELECT subject_id, shape_iri, constraint_type, severity, graph_id, detected_at \
                 FROM {tn}"
            )
        })
        .collect::<Vec<_>>()
        .join("\nUNION ALL\n");

    let summary_sql = format!(
        "SELECT shape_iri, constraint_type, severity, graph_id, \
                count(*)       AS violation_count, \
                max(detected_at) AS last_seen \
         FROM (\
             {union_sql}\
         ) _all_violations \
         GROUP BY shape_iri, constraint_type, severity, graph_id"
    );

    let create_summary = format!(
        "SELECT pg_trickle.create_stream_table(\
            '_pg_ripple.violation_summary_dag', \
            $pgtrickle_q$\
                {summary_sql}\
            $pgtrickle_q$, \
            '5s'\
        )"
    );

    Spi::run(&create_summary).unwrap_or_else(|e| {
        pgrx::warning!("failed to create violation_summary_dag stream table: {}", e);
    });

    created
}

/// Drop all pg_trickle SHACL DAG monitor stream tables and clear the catalog.
pub fn drop_dag_monitors() -> i64 {
    Spi::run("SELECT pg_trickle.drop_stream_table('_pg_ripple.violation_summary_dag')").ok();

    let names: Vec<String> = Spi::connect(|client| {
        match client.select(
            "SELECT stream_table_name FROM _pg_ripple.shacl_dag_monitors ORDER BY created_at",
            None,
            &[],
        ) {
            Ok(rows) => rows
                .filter_map(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
                .collect(),
            Err(_) => Vec::new(),
        }
    });

    let count = names.len() as i64;
    for name in &names {
        let drop_sql = format!(
            "SELECT pg_trickle.drop_stream_table('{}')",
            sql_escape_str(name)
        );
        Spi::run(&drop_sql).ok();
    }

    Spi::run("DELETE FROM _pg_ripple.shacl_dag_monitors").unwrap_or_else(|e| {
        pgrx::warning!("failed to clear shacl_dag_monitors catalog: {}", e);
    });

    count
}

/// List all active SHACL DAG monitors.
pub fn list_dag_monitors() -> Vec<(String, String, String)> {
    Spi::connect(|client| {
        match client.select(
            "SELECT shape_iri, stream_table_name, constraint_summary \
             FROM _pg_ripple.shacl_dag_monitors \
             ORDER BY shape_iri",
            None,
            &[],
        ) {
            Ok(rows) => rows
                .filter_map(|row| {
                    let shape_iri = row.get::<&str>(1).ok().flatten()?.to_owned();
                    let table_name = row.get::<&str>(2).ok().flatten()?.to_owned();
                    let summary = row.get::<&str>(3).ok().flatten()?.to_owned();
                    Some((shape_iri, table_name, summary))
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    })
}
