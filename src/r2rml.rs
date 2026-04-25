//! R2RML Direct Mapping (v0.56.0 Feature L-7.3).
//!
//! Implements the W3C R2RML 2012 core mapping vocabulary.

use pgrx::prelude::*;

const RR_TRIPLES_MAP: &str = "http://www.w3.org/ns/r2rml#TriplesMap";
const RR_LOGICAL_TABLE: &str = "http://www.w3.org/ns/r2rml#logicalTable";
const RR_TABLE_NAME: &str = "http://www.w3.org/ns/r2rml#tableName";
const RR_SQL_QUERY: &str = "http://www.w3.org/ns/r2rml#sqlQuery";
const RR_SUBJECT_MAP: &str = "http://www.w3.org/ns/r2rml#subjectMap";
const RR_PREDICATE_OBJECT_MAP: &str = "http://www.w3.org/ns/r2rml#predicateObjectMap";
const RR_PREDICATE_MAP: &str = "http://www.w3.org/ns/r2rml#predicateMap";
const RR_OBJECT_MAP: &str = "http://www.w3.org/ns/r2rml#objectMap";
const RR_TEMPLATE: &str = "http://www.w3.org/ns/r2rml#template";
const RR_COLUMN: &str = "http://www.w3.org/ns/r2rml#column";
const RR_CLASS: &str = "http://www.w3.org/ns/r2rml#class";
const RR_CONSTANT: &str = "http://www.w3.org/ns/r2rml#constant";
const RR_TERM_TYPE: &str = "http://www.w3.org/ns/r2rml#termType";
const RR_IRI: &str = "http://www.w3.org/ns/r2rml#IRI";
const RR_LITERAL: &str = "http://www.w3.org/ns/r2rml#Literal";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Look up the first object value for (subject_id, predicate_iri) using vp_rare.
fn lookup_object(subject_id: i64, predicate_iri: &str) -> Option<String> {
    let pred_id = crate::dictionary::lookup_iri(predicate_iri)?;
    Spi::connect(|c| {
        c.select(
            "SELECT d.value FROM _pg_ripple.vp_rare vp \
             JOIN _pg_ripple.dictionary d ON d.id = vp.o \
             WHERE vp.s = $1 AND vp.p = $2 LIMIT 1",
            Some(1),
            &[
                pgrx::datum::DatumWithOid::from(subject_id),
                pgrx::datum::DatumWithOid::from(pred_id),
            ],
        )
        .ok()
        .and_then(|mut r| r.next())
        .and_then(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
    })
}

fn lookup_objects(subject_id: i64, predicate_iri: &str) -> Vec<String> {
    let Some(pred_id) = crate::dictionary::lookup_iri(predicate_iri) else {
        return vec![];
    };
    Spi::connect(|c| {
        c.select(
            "SELECT d.value FROM _pg_ripple.vp_rare vp \
             JOIN _pg_ripple.dictionary d ON d.id = vp.o \
             WHERE vp.s = $1 AND vp.p = $2",
            None,
            &[
                pgrx::datum::DatumWithOid::from(subject_id),
                pgrx::datum::DatumWithOid::from(pred_id),
            ],
        )
        .ok()
        .map(|rows| {
            rows.filter_map(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default()
    })
}

fn find_instances_of(class_iri: &str) -> Vec<i64> {
    let Some(type_pred_id) = crate::dictionary::lookup_iri(RDF_TYPE) else {
        return vec![];
    };
    let Some(class_id) = crate::dictionary::lookup_iri(class_iri) else {
        return vec![];
    };
    Spi::connect(|c| {
        c.select(
            "SELECT s FROM _pg_ripple.vp_rare WHERE p = $1 AND o = $2",
            None,
            &[
                pgrx::datum::DatumWithOid::from(type_pred_id),
                pgrx::datum::DatumWithOid::from(class_id),
            ],
        )
        .ok()
        .map(|rows| {
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect()
        })
        .unwrap_or_default()
    })
}

fn apply_template(template: &str, columns: &std::collections::HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (col, val) in columns {
        let encoded = percent_encode(val);
        result = result.replace(&format!("{{{col}}}"), &encoded);
    }
    result
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b'/' => {
                out.push(byte as char)
            }
            b => {
                out.push('%');
                out.push(
                    char::from_digit((b >> 4) as u32, 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
                out.push(
                    char::from_digit((b & 0xf) as u32, 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    out
}

fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn quote_table_name(name: &str) -> String {
    name.splitn(2, '.')
        .map(|p| format!("\"{}\"", p.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".")
}

/// Execute an R2RML mapping loaded at `mapping_iri` and insert generated triples.
/// Returns the number of triples inserted.
pub fn r2rml_load(mapping_iri: &str) -> i64 {
    if crate::dictionary::lookup_iri(mapping_iri).is_none() {
        pgrx::error!(
            "r2rml_load: mapping IRI <{mapping_iri}> not found; load it first with load_turtle()"
        );
    }
    let triples_maps = find_instances_of(RR_TRIPLES_MAP);
    if triples_maps.is_empty() {
        pgrx::warning!("r2rml_load: no rr:TriplesMap instances found");
        return 0;
    }
    let mut total: i64 = 0;
    for tm_id in triples_maps {
        let Some(lt_iri) = lookup_object(tm_id, RR_LOGICAL_TABLE) else {
            continue;
        };
        let Some(lt_id) = crate::dictionary::lookup_iri(&lt_iri) else {
            continue;
        };
        let source_sql = if let Some(t) = lookup_object(lt_id, RR_TABLE_NAME) {
            format!("SELECT * FROM {}", quote_table_name(&t))
        } else if let Some(q) = lookup_object(lt_id, RR_SQL_QUERY) {
            q
        } else {
            continue;
        };
        let Some(sm_iri) = lookup_object(tm_id, RR_SUBJECT_MAP) else {
            continue;
        };
        let Some(sm_id) = crate::dictionary::lookup_iri(&sm_iri) else {
            continue;
        };
        let subject_template = lookup_object(sm_id, RR_TEMPLATE);
        let subject_column = lookup_object(sm_id, RR_COLUMN);
        let subject_constant = lookup_object(sm_id, RR_CONSTANT);
        let subject_classes = lookup_objects(sm_id, RR_CLASS);
        let subject_term_type =
            lookup_object(sm_id, RR_TERM_TYPE).unwrap_or_else(|| RR_IRI.to_string());

        let pom_iris = lookup_objects(tm_id, RR_PREDICATE_OBJECT_MAP);
        let mut pom_list: Vec<(String, String, bool)> = Vec::new();
        for pom_iri in &pom_iris {
            let Some(pom_id) = crate::dictionary::lookup_iri(pom_iri) else {
                continue;
            };
            let pred = if let Some(pm_iri) = lookup_object(pom_id, RR_PREDICATE_MAP) {
                crate::dictionary::lookup_iri(&pm_iri)
                    .and_then(|pm_id| lookup_object(pm_id, RR_CONSTANT))
            } else {
                None
            };
            let Some(pred) = pred else {
                continue;
            };
            if let Some(om_iri) = lookup_object(pom_id, RR_OBJECT_MAP)
                && let Some(om_id) = crate::dictionary::lookup_iri(&om_iri)
            {
                let is_lit = lookup_object(om_id, RR_TERM_TYPE).is_none_or(|tt| tt == RR_LITERAL);
                let spec = lookup_object(om_id, RR_TEMPLATE)
                    .or_else(|| lookup_object(om_id, RR_COLUMN))
                    .or_else(|| lookup_object(om_id, RR_CONSTANT))
                    .unwrap_or_default();
                pom_list.push((pred, spec, is_lit));
            }
        }

        let ntriples = Spi::connect(|c| {
            let rows_result = c.select(&source_sql, None, &[]);
            let table = match rows_result {
                Ok(r) => r,
                Err(e) => {
                    pgrx::warning!("r2rml_load source query error: {e}");
                    return String::new();
                }
            };
            // Collect column names from the SpiTupleTable before consuming it.
            let col_count = table.columns().ok().unwrap_or(0);
            let col_names: Vec<String> = (1..=col_count)
                .map(|i| table.column_name(i).unwrap_or_else(|_| format!("col{i}")))
                .collect();
            let mut buf = String::new();
            for row in table {
                let mut cols: std::collections::HashMap<String, String> = Default::default();
                for (i, name) in col_names.iter().enumerate() {
                    let val = row
                        .get::<&str>(i + 1)
                        .ok()
                        .flatten()
                        .unwrap_or("")
                        .to_string();
                    cols.insert(name.clone(), val);
                }
                let subj = if let Some(ref t) = subject_template {
                    format!("<{}>", apply_template(t, &cols))
                } else if let Some(ref col) = subject_column {
                    let v = cols.get(col.as_str()).map_or("", |s| s.as_str());
                    if subject_term_type == RR_IRI {
                        format!("<{}>", percent_encode(v))
                    } else {
                        format!("\"{}\"", escape_literal(v))
                    }
                } else if let Some(ref c) = subject_constant {
                    format!("<{c}>")
                } else {
                    continue;
                };
                for cls in &subject_classes {
                    buf.push_str(&format!("{subj} <{RDF_TYPE}> <{cls}> .\n"));
                }
                for (pred, spec, is_lit) in &pom_list {
                    let obj = if spec.contains('{') {
                        let expanded = apply_template(spec, &cols);
                        if *is_lit {
                            format!("\"{}\"", escape_literal(&expanded))
                        } else {
                            format!("<{expanded}>")
                        }
                    } else if let Some(v) = cols.get(spec.as_str()) {
                        if *is_lit {
                            format!("\"{}\"", escape_literal(v))
                        } else {
                            format!("<{}>", percent_encode(v))
                        }
                    } else if *is_lit {
                        format!("\"{}\"", escape_literal(spec))
                    } else {
                        format!("<{spec}>")
                    };
                    buf.push_str(&format!("{subj} <{pred}> {obj} .\n"));
                }
            }
            buf
        });

        if !ntriples.is_empty() {
            total += crate::bulk_load::load_ntriples(&ntriples, false);
        }
    }
    total
}
