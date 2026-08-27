// @allow-large-file: direct writeback SQL and validation stay together

fn fetch_writeback_config(mapping: &str) -> (String, String, Vec<String>, String) {
    let row: Option<(Option<String>, String, Option<String>, String)> =
        pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT writeback_table, writeback_schema, \
                        to_json(writeback_key_columns)::text, \
                        writeback_conflict_policy \
                 FROM _pg_ripple.json_mappings WHERE name = $1",
                    None,
                    &[pgrx::datum::DatumWithOid::from(mapping)],
                )
                .unwrap_or_else(|e| pgrx::error!("writeback config SPI error: {e}"))
                .next()
                .map(|row| {
                    let wt: Option<String> = row.get(1).ok().flatten();
                    let ws: String = row
                        .get(2)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "public".to_string());
                    let wk: Option<String> = row.get(3).ok().flatten();
                    let wp: String = row
                        .get(4)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "replace".to_string());
                    (wt, ws, wk, wp)
                })
        });

    let (wt, ws, wk_json, wp) = row.unwrap_or_else(|| {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        )
    });

    let writeback_table = wt.unwrap_or_else(|| {
        pgrx::error!(
            "PT0550: json mapping writeback target not configured; \
             call register_json_mapping(…, writeback_table => '…')"
        )
    });

    let key_columns: Vec<String> = wk_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    if key_columns.is_empty() {
        pgrx::error!(
            "PT0550: json mapping writeback target not configured; \
             call register_json_mapping(…, writeback_table => '…')"
        );
    }

    (writeback_table, ws, key_columns, wp)
}

fn pg_quote_ident(ident: &str) -> String {
    pgrx::Spi::get_one_with_args::<String>(
        "SELECT quote_ident($1)",
        &[pgrx::datum::DatumWithOid::from(ident)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| format!("\"{}\"", ident.replace('"', "\"\"")))
}

fn table_exists_in_schema(schema: &str, table: &str) -> bool {
    pgrx::Spi::get_one_with_args::<bool>(
        "SELECT EXISTS( \
             SELECT 1 FROM information_schema.tables \
             WHERE table_schema = $1 AND table_name = $2)",
        &[
            pgrx::datum::DatumWithOid::from(schema),
            pgrx::datum::DatumWithOid::from(table),
        ],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

fn require_key_columns_present(
    mapping: &str,
    key_columns: &[String],
    term_values: &std::collections::HashMap<String, String>,
) {
    let missing: Vec<&str> = key_columns
        .iter()
        .filter(|c| !term_values.contains_key(c.as_str()))
        .map(|c| c.as_str())
        .collect();
    if !missing.is_empty() {
        pgrx::error!(
            "PT0552: json mapping {:?} writeback key column(s) [{}] have no \
             asserted value for this subject; ensure every writeback_key_columns \
             predicate is present in the mapping context and has been ingested \
             for this subject before calling writeback",
            mapping,
            missing.join(", ")
        );
    }
}

fn fetch_or_compute_column_casts(
    mapping: &str,
    schema: &str,
    table: &str,
) -> std::collections::HashMap<String, String> {
    let cached: Option<pgrx::JsonB> = pgrx::Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT writeback_column_casts FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None);

    if let Some(pgrx::JsonB(serde_json::Value::Object(obj))) = cached
        && !obj.is_empty()
    {
        return obj
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect();
    }

    let casts = compute_column_casts(schema, table);

    if !casts.is_empty() {
        let casts_json = serde_json::Value::Object(
            casts
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        );
        pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.json_mappings SET writeback_column_casts = $2 WHERE name = $1",
            &[
                pgrx::datum::DatumWithOid::from(mapping),
                pgrx::datum::DatumWithOid::from(pgrx::JsonB(casts_json)),
            ],
        )
        .unwrap_or_else(|e| pgrx::warning!("could not cache writeback column casts: {e}"));
    }

    casts
}

fn compute_column_casts(schema: &str, table: &str) -> std::collections::HashMap<String, String> {
    pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT a.attname::text, a.atttypid::regtype::text \
                 FROM pg_catalog.pg_attribute a \
                 JOIN pg_catalog.pg_class c ON c.oid = a.attrelid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 \
                   AND a.attnum > 0 AND NOT a.attisdropped \
                   AND a.attgenerated = '' AND a.attidentity = ''",
                None,
                &[
                    pgrx::datum::DatumWithOid::from(schema),
                    pgrx::datum::DatumWithOid::from(table),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("writeback column list SPI error: {e}"))
            .filter_map(|row| {
                let name: String = row.get(1).ok().flatten()?;
                let ty: String = row.get(2).ok().flatten()?;
                Some((name, ty))
            })
            .collect()
    })
}

/// Validate and atomically store the public writeback configuration.
pub fn configure_json_writeback_impl(
    mapping: &str,
    schema: &str,
    table: &str,
    key_columns: &[String],
    conflict_policy: &str,
) {
    super::require_mapping_exists(mapping);
    if schema.trim().is_empty() || table.trim().is_empty() {
        pgrx::error!("configure_json_writeback: target schema and table are required");
    }
    if !matches!(conflict_policy, "replace" | "skip" | "error") {
        pgrx::error!(
            "configure_json_writeback: conflict_policy must be 'replace', 'skip', or 'error'"
        );
    }
    if key_columns.is_empty() {
        pgrx::error!("configure_json_writeback: key_columns must not be empty");
    }
    let mut seen = std::collections::HashSet::new();
    if key_columns
        .iter()
        .any(|column| column.trim().is_empty() || !seen.insert(column))
    {
        pgrx::error!("configure_json_writeback: key_columns must contain unique, non-empty names");
    }

    let relation: Option<(i64, String, bool, bool)> = pgrx::Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT c.oid::bigint, c.relkind::text, \
                        has_table_privilege(session_user, c.oid, 'INSERT'), \
                        has_table_privilege(session_user, c.oid, 'UPDATE') \
                 FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2",
                Some(1),
                &[
                    pgrx::datum::DatumWithOid::from(schema),
                    pgrx::datum::DatumWithOid::from(table),
                ],
            )
            .unwrap_or_else(|e| {
                pgrx::error!("configure_json_writeback: relation lookup failed: {e}")
            });
        if rows.is_empty() {
            return None;
        }
        let row = rows.first();
        Some((
            row.get::<i64>(1).ok().flatten()?,
            row.get::<String>(2).ok().flatten()?,
            row.get::<bool>(3).ok().flatten()?,
            row.get::<bool>(4).ok().flatten()?,
        ))
    });
    let (target_oid, relkind, can_insert, can_update) = relation.unwrap_or_else(|| {
        pgrx::error!(
            "configure_json_writeback: target relation {}.{} does not exist",
            schema,
            table
        )
    });
    if !matches!(relkind.as_str(), "r" | "p") {
        pgrx::error!(
            "configure_json_writeback: target {}.{} must be a table or partitioned table",
            schema,
            table
        );
    }
    if !can_insert || (conflict_policy == "replace" && !can_update) {
        pgrx::error!(
            "configure_json_writeback: current role lacks required privileges on {}.{}",
            schema,
            table
        );
    }

    let columns: Vec<(String, String, String, String)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT a.attname::text, a.atttypid::regtype::text, \
                        a.attgenerated::text, a.attidentity::text \
                 FROM pg_catalog.pg_attribute a \
                 WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped",
                None,
                &[pgrx::datum::DatumWithOid::from(target_oid)],
            )
            .unwrap_or_else(|e| pgrx::error!("configure_json_writeback: column lookup failed: {e}"))
            .filter_map(|row| {
                Some((
                    row.get(1).ok().flatten()?,
                    row.get(2).ok().flatten()?,
                    row.get(3).ok().flatten()?,
                    row.get(4).ok().flatten()?,
                ))
            })
            .collect()
    });
    for key in key_columns {
        let Some((_, _, generated, identity)) = columns.iter().find(|column| column.0 == *key)
        else {
            pgrx::error!(
                "configure_json_writeback: key column {:?} does not exist on {}.{}",
                key,
                schema,
                table
            );
        };
        if !generated.is_empty() || !identity.is_empty() {
            pgrx::error!(
                "configure_json_writeback: key column {:?} is generated or identity and is not insertable",
                key
            );
        }
    }

    let casts = compute_column_casts(schema, table);
    let casts_json = serde_json::Value::Object(
        casts
            .into_iter()
            .map(|(name, ty)| (name, serde_json::Value::String(ty)))
            .collect(),
    );
    pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_mappings \
         SET writeback_schema = $2, writeback_table = $3, writeback_key_columns = $4, \
             writeback_conflict_policy = $5, writeback_column_casts = $6, writeback_enabled = false \
         WHERE name = $1",
        &[
            pgrx::datum::DatumWithOid::from(mapping),
            pgrx::datum::DatumWithOid::from(schema),
            pgrx::datum::DatumWithOid::from(table),
            pgrx::datum::DatumWithOid::from(
                key_columns.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            pgrx::datum::DatumWithOid::from(conflict_policy),
            pgrx::datum::DatumWithOid::from(pgrx::JsonB(casts_json)),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("configure_json_writeback: catalog update failed: {e}"));
}

pub type WritebackInspectRow = (
    pgrx::name!(target_schema, String),
    pgrx::name!(target_table, String),
    pgrx::name!(key_columns, Vec<String>),
    pgrx::name!(conflict_policy, String),
    pgrx::name!(writeback_enabled, bool),
    pgrx::name!(trigger_count, i32),
    pgrx::name!(queue_depth, i64),
);

pub fn writeback_inspect_impl(
    mapping: &str,
) -> pgrx::iter::TableIterator<'static, WritebackInspectRow> {
    type Row = (String, String, Vec<String>, String, bool, i32, i64);
    let rows: Vec<Row> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT COALESCE(m.writeback_schema, 'public'), \
                        COALESCE(m.writeback_table, ''), \
                        COALESCE(m.writeback_key_columns, '{}'::text[]), \
                        COALESCE(m.writeback_conflict_policy, 'error'), \
                        m.writeback_enabled, \
                        (SELECT COUNT(*)::int \
                         FROM pg_catalog.pg_trigger t \
                         WHERE NOT t.tgisinternal \
                           AND pg_catalog.starts_with( \
                               t.tgname, \
                               'pg_ripple_jwb_' || \
                               pg_catalog.regexp_replace(m.name, '[^[:alnum:]]', '_', 'g') || '_')), \
                        COUNT(q.id) FILTER (WHERE q.processed_at IS NULL)::bigint \
                 FROM _pg_ripple.json_mappings m \
                 LEFT JOIN _pg_ripple.json_writeback_queue q ON q.mapping_name = m.name \
                 WHERE m.name = $1 \
                 GROUP BY m.name, m.writeback_schema, m.writeback_table, m.writeback_key_columns, \
                          m.writeback_conflict_policy, m.writeback_enabled",
                None,
                &[pgrx::datum::DatumWithOid::from(mapping)],
            )
            .unwrap_or_else(|e| pgrx::error!("writeback_inspect: SPI error: {e}"))
            .filter_map(|row| {
                Some((
                    row.get(1).ok().flatten()?,
                    row.get(2).ok().flatten()?,
                    row.get(3).ok().flatten().unwrap_or_default(),
                    row.get(4).ok().flatten().unwrap_or_default(),
                    row.get(5).ok().flatten().unwrap_or(false),
                    row.get(6).ok().flatten().unwrap_or(0),
                    row.get(7).ok().flatten().unwrap_or(0),
                ))
            })
            .collect()
    });
    if rows.is_empty() {
        super::require_mapping_exists(mapping);
    }
    pgrx::iter::TableIterator::new(rows)
}

pub fn writeback_json_row_impl(mapping: &str, subject_iri: &str) -> i64 {
    let (writeback_table, writeback_schema, key_columns, conflict_policy) =
        fetch_writeback_config(mapping);

    let context = super::fetch_mapping_context(mapping);
    let ctx_obj = match context.as_object() {
        Some(o) => o.clone(),
        None => return 0,
    };
    let mut iri_to_term: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (term, iri_val) in &ctx_obj {
        if term.starts_with('@') {
            continue;
        }
        let iri = match iri_val {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(meta) => {
                meta.get("@id").and_then(|v| v.as_str()).unwrap_or("")
            }
            _ => "",
        };
        if !iri.is_empty() && !iri.starts_with('@') {
            iri_to_term.insert(iri.to_string(), term.clone());
        }
    }

    let sparql = format!(
        "CONSTRUCT {{ <{0}> ?p ?o }} WHERE {{ <{0}> ?p ?o }}",
        subject_iri.replace('\\', "\\\\").replace('>', "\\>")
    );
    let triples = crate::sparql::sparql_construct_rows(&sparql);

    if triples.is_empty() {
        return 0;
    }

    let mut term_values: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_s_id, p_id, o_id) in &triples {
        let pred_iri = match crate::dictionary::decode(*p_id) {
            Some(s) => s,
            None => continue,
        };
        let term = match iri_to_term.get(&pred_iri) {
            Some(t) => t.clone(),
            None => continue,
        };
        let obj_str = match crate::dictionary::decode(*o_id) {
            Some(s) => {
                if s.starts_with('"') {
                    let inner = s.trim_start_matches('"');
                    if let Some(end) = inner.find('"') {
                        inner[..end].to_string()
                    } else {
                        inner.to_string()
                    }
                } else if s.starts_with('<') && s.ends_with('>') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                }
            }
            None => continue,
        };
        term_values.insert(term, obj_str);
    }

    if term_values.is_empty() {
        return 0;
    }

    require_key_columns_present(mapping, &key_columns, &term_values);

    let column_casts = fetch_or_compute_column_casts(mapping, &writeback_schema, &writeback_table);

    if column_casts.is_empty() {
        pgrx::error!(
            "writeback_json_row: target table {}.{} not found or has no columns; \
             check writeback_schema and writeback_table in the mapping",
            writeback_schema,
            writeback_table
        );
    }

    let mut insert_cols: Vec<String> = Vec::new();
    let mut insert_vals: Vec<String> = Vec::new();
    let mut insert_types: Vec<String> = Vec::new();

    for (col, pg_type) in &column_casts {
        if let Some(val) = term_values.get(col.as_str()) {
            insert_cols.push(col.clone());
            insert_vals.push(val.clone());
            insert_types.push(pg_type.clone());
        }
    }

    if insert_cols.is_empty() {
        return 0;
    }

    if conflict_policy == "error" {
        let q_schema = pg_quote_ident(&writeback_schema);
        let q_table = pg_quote_ident(&writeback_table);
        let where_clause: Vec<String> = key_columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", pg_quote_ident(col), i + 1))
            .collect();
        let check_sql = format!(
            "SELECT COUNT(*) FROM {q_schema}.{q_table} WHERE {}",
            where_clause.join(" AND ")
        );
        let key_vals: Vec<pgrx::datum::DatumWithOid> = key_columns
            .iter()
            .map(|col| {
                pgrx::datum::DatumWithOid::from(
                    term_values.get(col.as_str()).map_or("", String::as_str),
                )
            })
            .collect();
        let count: i64 = pgrx::Spi::get_one_with_args::<i64>(&check_sql, &key_vals)
            .unwrap_or(None)
            .unwrap_or(0);
        if count > 0 {
            pgrx::error!(
                "PT0551: json mapping writeback conflict on mapping {:?} subject {:?}; \
                 policy is 'error'",
                mapping,
                subject_iri
            );
        }
    }

    let q_schema = pg_quote_ident(&writeback_schema);
    let q_table = pg_quote_ident(&writeback_table);
    let q_cols: Vec<String> = insert_cols.iter().map(|c| pg_quote_ident(c)).collect();
    let q_key_cols: Vec<String> = key_columns.iter().map(|c| pg_quote_ident(c)).collect();

    let cols_list = q_cols.join(", ");

    let conflict_clause = match conflict_policy.as_str() {
        "skip" => "ON CONFLICT DO NOTHING".to_string(),
        "error" => "".to_string(),
        _ => {
            let update_cols: Vec<String> = q_cols
                .iter()
                .zip(insert_cols.iter())
                .filter(|(_, col)| !key_columns.contains(col))
                .map(|(qc, _)| format!("{qc} = EXCLUDED.{qc}"))
                .collect();
            if update_cols.is_empty() || q_key_cols.is_empty() {
                "ON CONFLICT DO NOTHING".to_string()
            } else {
                let key_list = q_key_cols.join(", ");
                let set_list = update_cols.join(", ");
                format!("ON CONFLICT ({key_list}) DO UPDATE SET {set_list}")
            }
        }
    };

    let select_vals: Vec<String> = insert_types
        .iter()
        .enumerate()
        .map(|(i, pg_type)| format!("CAST(${} AS {pg_type})", i + 1))
        .collect();
    let select_vals_list = select_vals.join(", ");

    let insert_select_sql = format!(
        "WITH ins AS ( \
             INSERT INTO {q_schema}.{q_table} ({cols_list}) \
             SELECT {select_vals_list} {conflict_clause} \
             RETURNING 1 \
         ) SELECT count(*) FROM ins"
    );

    let spi_args: Vec<pgrx::datum::DatumWithOid> = insert_vals
        .iter()
        .map(|s| pgrx::datum::DatumWithOid::from(s.as_str()))
        .collect();

    pgrx::Spi::get_one_with_args::<i64>(&insert_select_sql, &spi_args)
        .unwrap_or_else(|e| pgrx::error!("writeback_json_row: INSERT failed: {e}"))
        .unwrap_or(0)
}

pub fn writeback_json_row_delete_impl(mapping: &str, subject_iri: &str) -> i64 {
    let (writeback_table, writeback_schema, key_columns, _conflict_policy) =
        fetch_writeback_config(mapping);

    let context = super::fetch_mapping_context(mapping);
    let ctx_obj = match context.as_object() {
        Some(o) => o.clone(),
        None => return 0,
    };
    let mut iri_to_term: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (term, iri_val) in &ctx_obj {
        if term.starts_with('@') {
            continue;
        }
        let iri = match iri_val {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(meta) => {
                meta.get("@id").and_then(|v| v.as_str()).unwrap_or("")
            }
            _ => "",
        };
        if !iri.is_empty() && !iri.starts_with('@') {
            iri_to_term.insert(iri.to_string(), term.clone());
        }
    }

    let sparql = format!(
        "CONSTRUCT {{ <{0}> ?p ?o }} WHERE {{ <{0}> ?p ?o }}",
        subject_iri.replace('\\', "\\\\").replace('>', "\\>")
    );
    let triples = crate::sparql::sparql_construct_rows(&sparql);

    if triples.is_empty() {
        return 0;
    }

    let mut term_values: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_s_id, p_id, o_id) in &triples {
        let pred_iri = match crate::dictionary::decode(*p_id) {
            Some(s) => s,
            None => continue,
        };
        let term = match iri_to_term.get(&pred_iri) {
            Some(t) => t.clone(),
            None => continue,
        };
        let obj_str = match crate::dictionary::decode(*o_id) {
            Some(s) => {
                if s.starts_with('"') {
                    let inner = s.trim_start_matches('"');
                    if let Some(end) = inner.find('"') {
                        inner[..end].to_string()
                    } else {
                        inner.to_string()
                    }
                } else if s.starts_with('<') && s.ends_with('>') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                }
            }
            None => continue,
        };
        term_values.insert(term, obj_str);
    }

    require_key_columns_present(mapping, &key_columns, &term_values);

    let column_casts = fetch_or_compute_column_casts(mapping, &writeback_schema, &writeback_table);

    let q_schema = pg_quote_ident(&writeback_schema);
    let q_table = pg_quote_ident(&writeback_table);

    let conditions: Vec<String> = key_columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let pg_type = column_casts
                .get(col.as_str())
                .map_or("text", |s| s.as_str());
            format!("{} = CAST(${} AS {pg_type})", pg_quote_ident(col), i + 1)
        })
        .collect();
    let args_owned: Vec<String> = key_columns
        .iter()
        .map(|col| term_values.get(col.as_str()).cloned().unwrap_or_default())
        .collect();

    let where_clause = conditions.join(" AND ");

    let delete_sql = format!(
        "WITH del AS ( \
             DELETE FROM {q_schema}.{q_table} WHERE {where_clause} RETURNING 1 \
         ) SELECT count(*) FROM del"
    );

    let spi_args: Vec<pgrx::datum::DatumWithOid> = args_owned
        .iter()
        .map(|s| pgrx::datum::DatumWithOid::from(s.as_str()))
        .collect();

    pgrx::Spi::get_one_with_args::<i64>(&delete_sql, &spi_args)
        .unwrap_or_else(|e| pgrx::error!("writeback_json_row_delete: DELETE failed: {e}"))
        .unwrap_or(0)
}

fn context_predicate_iris(context: &serde_json::Value) -> Vec<String> {
    match context.as_object() {
        Some(obj) => obj
            .values()
            .filter_map(|v| match v {
                serde_json::Value::String(s) if s.starts_with("http") => Some(s.clone()),
                serde_json::Value::Object(meta) => meta
                    .get("@id")
                    .and_then(|id| id.as_str())
                    .filter(|s| s.starts_with("http"))
                    .map(String::from),
                _ => None,
            })
            .collect(),
        None => vec![],
    }
}

fn lookup_predicate_id(pred_iri: &str) -> Option<i64> {
    pgrx::Spi::connect(|client| {
        let tbl = client
            .select(
                "SELECT id FROM _pg_ripple.dictionary WHERE value = $1 AND kind = $2",
                Some(1),
                &[
                    pgrx::datum::DatumWithOid::from(pred_iri),
                    pgrx::datum::DatumWithOid::from(crate::dictionary::KIND_IRI),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("writeback predicate lookup SPI error: {e}"));
        if tbl.is_empty() {
            None
        } else {
            tbl.first()
                .get::<i64>(1)
                .unwrap_or_else(|e| pgrx::error!("writeback predicate lookup SPI error: {e}"))
        }
    })
}

fn create_writeback_trigger(
    trigger_name: &str,
    source_table: &str,
    mapping_literal: &str,
    pred_filter: Option<i64>,
) -> Result<(), String> {
    let q_trigger = pg_quote_ident(trigger_name);
    let q_table = format!("_pg_ripple.{}", pg_quote_ident(source_table));
    let args = match pred_filter {
        Some(id) => format!("'{mapping_literal}', '{id}'"),
        None => format!("'{mapping_literal}'"),
    };
    let sql = format!(
        "CREATE OR REPLACE TRIGGER {q_trigger} \
         AFTER INSERT OR DELETE ON {q_table} \
         FOR EACH ROW EXECUTE FUNCTION _pg_ripple.json_writeback_enqueue_fn({args})"
    );
    pgrx::Spi::run_with_args(&sql, &[]).map_err(|e| e.to_string())
}

pub fn install_writeback_triggers_after_promotion(pred_id: i64) {
    let pred_iri = match crate::dictionary::decode(pred_id) {
        Some(v) => v,
        None => return,
    };

    let mappings: Vec<(String, serde_json::Value)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT name, context FROM _pg_ripple.json_mappings WHERE writeback_enabled = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("writeback promotion hook: SPI error: {e}"))
            .filter_map(|row| {
                let name: String = row.get(1).ok().flatten()?;
                let ctx: pgrx::JsonB = row.get(2).ok().flatten()?;
                Some((name, ctx.0))
            })
            .collect()
    });

    for (mapping, context) in mappings {
        if !context_predicate_iris(&context).contains(&pred_iri) {
            continue;
        }

        let safe_mapping = mapping.replace(|c: char| !c.is_alphanumeric(), "_");
        let mapping_literal = mapping.replace('\'', "''");
        let delta_table = format!("vp_{pred_id}_delta");
        let ts_table = format!("vp_{pred_id}_tombstones");
        let delta_trigger = format!("pg_ripple_jwb_{safe_mapping}_{pred_id}");
        let ts_trigger = format!("pg_ripple_jwb_{safe_mapping}_{pred_id}_ts");

        if let Err(e) =
            create_writeback_trigger(&delta_trigger, &delta_table, &mapping_literal, None)
        {
            pgrx::warning!(
                "install_writeback_triggers_after_promotion: could not install delta \
                 trigger for mapping {mapping:?} predicate {pred_id}: {e}"
            );
        }
        if let Err(e) = create_writeback_trigger(&ts_trigger, &ts_table, &mapping_literal, None) {
            pgrx::warning!(
                "install_writeback_triggers_after_promotion: could not install tombstone \
                 trigger for mapping {mapping:?} predicate {pred_id}: {e}"
            );
        }
    }
}

pub fn enable_json_writeback_impl(mapping: &str) {
    let (writeback_table, writeback_schema, _key_columns, _) = fetch_writeback_config(mapping);

    if !table_exists_in_schema(&writeback_schema, &writeback_table) {
        pgrx::error!(
            "enable_json_writeback: target table {}.{} does not exist",
            writeback_schema,
            writeback_table
        );
    }

    disable_json_writeback_impl(mapping);

    let context_json: serde_json::Value = pgrx::Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT context FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .map(|j| j.0)
    .unwrap_or(serde_json::Value::Object(Default::default()));

    let pred_iris: Vec<String> = context_predicate_iris(&context_json);

    let safe_mapping = mapping.replace(|c: char| !c.is_alphanumeric(), "_");
    let mapping_literal = mapping.replace('\'', "''");

    let mut covered_count = 0usize;
    let mut uncovered: Vec<String> = Vec::new();

    for pred_iri in &pred_iris {
        let pred_id = match lookup_predicate_id(pred_iri) {
            Some(id) => id,
            None => {
                uncovered.push(format!("{pred_iri} (no dictionary entry yet)"));
                continue;
            }
        };

        let mut pred_covered = false;
        let mut errs: Vec<String> = Vec::new();

        let delta_table = format!("vp_{pred_id}_delta");
        if table_exists_in_schema("_pg_ripple", &delta_table) {
            let trig = format!("pg_ripple_jwb_{safe_mapping}_{pred_id}");
            match create_writeback_trigger(&trig, &delta_table, &mapping_literal, None) {
                Ok(()) => pred_covered = true,
                Err(e) => errs.push(format!("delta trigger: {e}")),
            }

            let ts_table = format!("vp_{pred_id}_tombstones");
            let ts_trig = format!("pg_ripple_jwb_{safe_mapping}_{pred_id}_ts");
            if let Err(e) = create_writeback_trigger(&ts_trig, &ts_table, &mapping_literal, None) {
                pgrx::warning!(
                    "enable_json_writeback: could not install tombstone trigger for \
                     predicate {pred_iri}: {e}"
                );
            }
        }

        let rare_trig = format!("pg_ripple_jwb_{safe_mapping}_{pred_id}_rare");
        match create_writeback_trigger(&rare_trig, "vp_rare", &mapping_literal, Some(pred_id)) {
            Ok(()) => pred_covered = true,
            Err(e) => errs.push(format!("vp_rare trigger: {e}")),
        }

        if pred_covered {
            covered_count += 1;
        } else {
            uncovered.push(format!("{pred_iri} ({})", errs.join("; ")));
        }
    }

    if covered_count == 0 || !uncovered.is_empty() {
        pgrx::error!(
            "enable_json_writeback: cannot enable — incomplete enqueue coverage \
             ({covered_count} of {} predicate(s) covered); async writeback is unavailable \
             for this mapping until every mapped predicate has been ingested at least \
             once. Uncovered: [{}]. writeback_enabled was left false; use \
             writeback_json_row()/writeback_json_row_delete() for direct writeback \
             in the meantime.",
            pred_iris.len(),
            uncovered.join(", ")
        );
    }

    pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_mappings SET writeback_enabled = true WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or_else(|e| pgrx::error!("enable_json_writeback: catalog update failed: {e}"));
}

pub fn disable_json_writeback_impl(mapping: &str) {
    super::require_mapping_exists(mapping);

    let safe_mapping = mapping.replace(|c: char| !c.is_alphanumeric(), "_");
    let trigger_prefix = format!("pg_ripple_jwb_{safe_mapping}_");

    let like_pattern = format!("{trigger_prefix}%").replace('\'', "''");
    let find_triggers_sql = format!(
        "SELECT string_agg(DISTINCT t.tgname || chr(1) || c.relname, chr(2)) \
         FROM pg_catalog.pg_trigger t \
         JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' \
           AND NOT t.tgisinternal \
           AND t.tgname LIKE '{like_pattern}'"
    );
    let trigger_rows: Vec<(String, String)> = pgrx::Spi::get_one::<String>(&find_triggers_sql)
        .unwrap_or_else(|e| pgrx::error!("disable_json_writeback: SPI error: {e}"))
        .map(|agg| {
            agg.split('\u{2}')
                .filter_map(|entry| entry.split_once('\u{1}'))
                .map(|(tn, tbl)| (tn.to_string(), tbl.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for (trigger_name, table_name) in &trigger_rows {
        let q_trigger = pg_quote_ident(trigger_name);
        let q_table = format!("_pg_ripple.{}", pg_quote_ident(table_name));
        let drop_sql = format!("DROP TRIGGER IF EXISTS {q_trigger} ON {q_table}");
        pgrx::Spi::run_with_args(&drop_sql, &[]).unwrap_or_else(|e| {
            pgrx::warning!(
                "disable_json_writeback: could not drop trigger {}: {e}",
                trigger_name
            )
        });
    }

    pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_mappings SET writeback_enabled = false WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or_else(|e| pgrx::error!("disable_json_writeback: catalog update failed: {e}"));
}

#[allow(clippy::type_complexity)]
pub fn json_writeback_status_impl() -> pgrx::iter::TableIterator<
    'static,
    (
        pgrx::name!(mapping_name, String),
        pgrx::name!(pending, i64),
        pgrx::name!(errors, i64),
        pgrx::name!(last_error, Option<String>),
        pgrx::name!(
            last_processed_at,
            Option<pgrx::datum::TimestampWithTimeZone>
        ),
    ),
> {
    type Row = (
        String,
        i64,
        i64,
        Option<String>,
        Option<pgrx::datum::TimestampWithTimeZone>,
    );

    let rows: Vec<Row> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT \
                     mapping_name, \
                     COUNT(*) FILTER (WHERE processed_at IS NULL)::bigint AS pending, \
                     COUNT(*) FILTER (WHERE error IS NOT NULL)::bigint AS errors, \
                     (SELECT error FROM _pg_ripple.json_writeback_queue q2 \
                      WHERE q2.mapping_name = q.mapping_name \
                        AND q2.error IS NOT NULL \
                      ORDER BY q2.queued_at DESC LIMIT 1) AS last_error, \
                     MAX(processed_at) AS last_processed_at \
                 FROM _pg_ripple.json_writeback_queue q \
                 GROUP BY mapping_name \
                 ORDER BY mapping_name",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("json_writeback_status SPI error: {e}"))
            .filter_map(|row| {
                let mn: String = row.get(1).ok().flatten()?;
                let pending: i64 = row.get(2).ok().flatten().unwrap_or(0);
                let errors: i64 = row.get(3).ok().flatten().unwrap_or(0);
                let last_err: Option<String> = row.get(4).ok().flatten();
                let last_proc: Option<pgrx::datum::TimestampWithTimeZone> =
                    row.get(5).ok().flatten();
                Some((mn, pending, errors, last_err, last_proc))
            })
            .collect()
    });

    pgrx::iter::TableIterator::new(rows)
}

pub fn drain_json_writeback_queue() {
    let batch_size = crate::JSON_WRITEBACK_BATCH_SIZE.get();
    if batch_size <= 0 {
        return;
    }

    let pending_rows: Vec<(i64, String, i64, String)> = pgrx::Spi::connect(|client| {
        client
            .select(
                "SELECT id, mapping_name, subject_id, operation \
                 FROM _pg_ripple.json_writeback_queue \
                 WHERE processed_at IS NULL \
                 ORDER BY queued_at \
                 LIMIT $1",
                None,
                &[pgrx::datum::DatumWithOid::from(batch_size as i64)],
            )
            .unwrap_or_else(|e| pgrx::error!("drain_json_writeback_queue: SPI error: {e}"))
            .filter_map(|row| {
                let id: i64 = row.get(1).ok().flatten()?;
                let mn: String = row.get(2).ok().flatten()?;
                let sid: i64 = row.get(3).ok().flatten()?;
                let op: String = row.get(4).ok().flatten()?;
                Some((id, mn, sid, op))
            })
            .collect()
    });

    for (row_id, mapping_name, subject_id, operation) in &pending_rows {
        let subject_iri_opt: Option<String> = pgrx::Spi::connect(|client| {
            let tbl = client
                .select(
                    "SELECT value FROM _pg_ripple.dictionary WHERE id = $1 AND kind = $2",
                    Some(1),
                    &[
                        pgrx::datum::DatumWithOid::from(*subject_id),
                        pgrx::datum::DatumWithOid::from(crate::dictionary::KIND_IRI),
                    ],
                )
                .unwrap_or_else(|e| {
                    pgrx::error!("drain_json_writeback_queue: dictionary lookup SPI error: {e}")
                });
            if tbl.is_empty() {
                None
            } else {
                tbl.first().get::<String>(1).ok().flatten()
            }
        });

        let subject_iri = match subject_iri_opt {
            Some(s) => s,
            None => {
                mark_queue_row_processed(*row_id, Some("subject_id not found in dictionary"));
                continue;
            }
        };

        let result: Result<(), String> =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if operation == "delete" {
                    writeback_json_row_delete_impl(mapping_name, &subject_iri);
                } else {
                    writeback_json_row_impl(mapping_name, &subject_iri);
                }
            }))
            .map_err(|e| {
                if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "unknown panic in writeback".to_string()
                }
            });

        let error_msg: Option<String> = result.err();
        mark_queue_row_processed(*row_id, error_msg.as_deref());
    }
}

fn mark_queue_row_processed(row_id: i64, error_msg: Option<&str>) {
    let update_result = pgrx::Spi::run_with_args(
        "UPDATE _pg_ripple.json_writeback_queue \
         SET processed_at = now(), error = $2 WHERE id = $1",
        &[
            pgrx::datum::DatumWithOid::from(row_id),
            pgrx::datum::DatumWithOid::from(error_msg),
        ],
    );

    if let Err(e) = update_result {
        pgrx::warning!(
            "drain_json_writeback_queue: status update failed for queue row {row_id}: {e}; \
             leaving row pending with incremented retry_count"
        );
        crate::stats::increment_json_writeback_drain_errors();

        if let Err(e2) = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.json_writeback_queue SET retry_count = retry_count + 1 WHERE id = $1",
            &[pgrx::datum::DatumWithOid::from(row_id)],
        ) {
            pgrx::warning!(
                "drain_json_writeback_queue: retry_count update also failed for queue row {row_id}: {e2}"
            );
        }
    }
}
