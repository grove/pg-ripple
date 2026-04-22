//! GRAPH pattern and SERVICE (federation) translators.

use spargebra::algebra::GraphPattern;
use spargebra::term::NamedNodePattern;

use crate::sparql::federation;
use crate::sparql::sqlgen::{Ctx, Fragment};

/// Translate a GRAPH { ... } pattern (named-graph scoped).
pub(crate) fn translate_graph(
    name: &NamedNodePattern,
    inner: &GraphPattern,
    ctx: &mut Ctx,
) -> Fragment {
    match name {
        NamedNodePattern::NamedNode(nn) => match ctx.encode_iri(nn.as_str()) {
            Some(gid) => {
                let saved = ctx.graph_filter;
                ctx.graph_filter = Some(gid);
                let frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);
                ctx.graph_filter = saved;
                frag
            }
            None => {
                let mut frag = Fragment::empty();
                frag.conditions.push("FALSE".to_owned());
                frag
            }
        },
        NamedNodePattern::Variable(v) => {
            let vname = v.as_str().to_owned();
            let saved_vg = ctx.variable_graph;
            ctx.variable_graph = true;
            let mut frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);
            ctx.variable_graph = saved_vg;
            if let Some((alias, _)) = frag.from_items.first() {
                let gcol = format!("{alias}.g");
                frag.conditions.push(format!("{gcol} <> 0"));
                let other_g_conditions: Vec<String> = frag
                    .from_items
                    .iter()
                    .skip(1)
                    .filter(|(_, sql)| sql.contains("_pg_ripple.vp_"))
                    .map(|(a, _)| format!("{a}.g = {gcol}"))
                    .collect();
                frag.conditions.extend(other_g_conditions);
                if let Some(existing) = frag.bindings.get(&vname) {
                    let existing = existing.clone();
                    frag.conditions.push(format!("{gcol} = {existing}"));
                } else {
                    frag.bindings.insert(vname, gcol);
                }
            }
            frag
        }
    }
}

// ─── Batch SERVICE translator (v0.19.0) ────────────────────────────────────────

pub(crate) fn translate_service_batched(
    url: &str,
    inner_l: &GraphPattern,
    inner_r: &GraphPattern,
    silent: bool,
    ctx: &mut Ctx,
) -> Option<Fragment> {
    if !federation::is_endpoint_allowed(url) {
        return None;
    }
    if !federation::is_endpoint_healthy(url) {
        return None;
    }
    if federation::get_local_view(url).is_some() {
        return None;
    }

    let mut vars_l: Vec<String> = federation::collect_pattern_variables(inner_l)
        .into_iter()
        .collect();
    vars_l.sort();
    let mut vars_r: Vec<String> = federation::collect_pattern_variables(inner_r)
        .into_iter()
        .collect();
    vars_r.sort();

    let mut all_vars: Vec<String> = vars_l.iter().chain(vars_r.iter()).cloned().collect();
    all_vars.sort();
    all_vars.dedup();

    let projection = if all_vars.is_empty() {
        "*".to_owned()
    } else {
        all_vars
            .iter()
            .map(|v| format!("?{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let combined_text =
        format!("SELECT {projection} WHERE {{ {{ {inner_l} }} UNION {{ {inner_r} }} }}");
    pgrx::debug1!("batch SERVICE {url}: {combined_text}");

    let timeout_secs = federation::effective_timeout_secs(url);
    let max_results = crate::FEDERATION_MAX_RESULTS.get();
    let on_partial = crate::FEDERATION_ON_PARTIAL.get();
    let on_partial_str = on_partial
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("empty");

    let start = std::time::Instant::now();
    let result = if on_partial_str == "use" {
        federation::execute_remote_partial(url, &combined_text, timeout_secs, max_results)
    } else {
        federation::execute_remote(url, &combined_text, timeout_secs, max_results)
    };
    let latency_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok((variables, rows)) => {
            federation::record_health(url, true, latency_ms);
            if variables.is_empty() || rows.is_empty() {
                return Some(Fragment::zero_rows());
            }
            let (variables, encoded_rows) = federation::encode_results(variables, rows);
            Some(translate_service_values(&variables, &encoded_rows, ctx))
        }
        Err(e) => {
            federation::record_health(url, false, latency_ms);
            if silent {
                pgrx::warning!("batch SERVICE {url} failed (returning empty): {e}");
                return Some(Fragment::zero_rows());
            }
            pgrx::warning!("batch SERVICE {url} failed, falling back to sequential: {e}");
            None
        }
    }
}

// ─── SERVICE translator (v0.16.0, enhanced v0.19.0) ───────────────────────────

pub(crate) fn translate_service(
    name: &NamedNodePattern,
    inner: &GraphPattern,
    silent: bool,
    ctx: &mut Ctx,
) -> Fragment {
    let service_silent_fallback = |ctx: &mut Ctx| -> Fragment {
        let alias = ctx.next_alias();
        let mut frag = Fragment::empty();
        frag.from_items
            .push((alias, "(SELECT 1 AS _dummy)".to_owned()));
        frag
    };

    let url = match name {
        NamedNodePattern::NamedNode(nn) => nn.as_str().to_string(),
        NamedNodePattern::Variable(v) => {
            let vname = v.as_str().to_owned();
            let endpoints = federation::get_all_graph_endpoints();
            if endpoints.is_empty() {
                pgrx::warning!(
                    "SERVICE with variable endpoint ?{} — no registered graph endpoints; returning empty",
                    v.as_str()
                );
                let mut frag = Fragment::empty();
                frag.conditions.push("FALSE".to_owned());
                return frag;
            }
            let mut arms: Vec<Fragment> = Vec::new();
            for (ep_url, graph_iri) in &endpoints {
                let url_id = match ctx.encode_iri(ep_url) {
                    Some(id) => id,
                    None => continue,
                };
                let gid = match ctx.encode_iri(graph_iri) {
                    Some(id) => id,
                    None => continue,
                };
                let saved = ctx.graph_filter;
                ctx.graph_filter = Some(gid);
                let mut arm_frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);
                ctx.graph_filter = saved;
                if let Some(existing) = arm_frag.bindings.get(&vname).cloned() {
                    arm_frag.conditions.push(format!("{existing} = {url_id}"));
                } else {
                    arm_frag.bindings.insert(vname.clone(), url_id.to_string());
                }
                arms.push(arm_frag);
            }
            if arms.is_empty() {
                let mut frag = Fragment::empty();
                frag.conditions.push("FALSE".to_owned());
                return frag;
            }
            if arms.len() == 1
                && let Some(arm) = arms.pop()
            {
                return arm;
            }
            let all_vars: Vec<String> = {
                let mut vars: std::collections::HashSet<String> = std::collections::HashSet::new();
                for arm in &arms {
                    vars.extend(arm.bindings.keys().cloned());
                }
                let mut v: Vec<String> = vars.into_iter().collect();
                v.sort();
                v
            };
            let union_arms: Vec<String> = arms
                .iter()
                .map(|arm| {
                    let cols: Vec<String> = all_vars
                        .iter()
                        .map(|var| {
                            arm.bindings
                                .get(var)
                                .map(|col| format!("{col} AS _sv_{var}"))
                                .unwrap_or_else(|| format!("NULL::bigint AS _sv_{var}"))
                        })
                        .collect();
                    let cols_str = if cols.is_empty() {
                        "1 AS _dummy".to_owned()
                    } else {
                        cols.join(", ")
                    };
                    format!(
                        "SELECT {cols_str} FROM {} {}",
                        arm.build_from(),
                        arm.build_where()
                    )
                })
                .collect();
            let union_subq = format!("({})", union_arms.join(" UNION ALL "));
            let alias = ctx.next_alias();
            let mut frag = Fragment::empty();
            frag.from_items.push((alias.clone(), union_subq));
            for var in &all_vars {
                frag.bindings
                    .insert(var.clone(), format!("{alias}._sv_{var}"));
            }
            return frag;
        }
    };

    if !federation::is_endpoint_allowed(&url) {
        if silent {
            pgrx::warning!("SERVICE endpoint not registered (SILENT skipping): {url}");
            return service_silent_fallback(ctx);
        }
        pgrx::error!(
            "federation endpoint not registered: {}; use pg_ripple.register_endpoint() to allow it",
            url
        );
    }

    if !federation::is_endpoint_healthy(&url) {
        if silent {
            pgrx::warning!("SERVICE endpoint {url} is unhealthy (success_rate < 10%); skipping");
            return service_silent_fallback(ctx);
        }
        pgrx::warning!("SERVICE endpoint {url} is unhealthy; proceeding anyway");
    }

    if let Some(stream_table) = federation::get_local_view(&url) {
        return translate_service_local(&stream_table, ctx);
    }

    if let Some(graph_iri) = federation::get_graph_iri(&url) {
        if let Some(gid) = ctx.encode_iri(&graph_iri) {
            let saved = ctx.graph_filter;
            ctx.graph_filter = Some(gid);
            let frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);
            ctx.graph_filter = saved;
            return frag;
        }
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }

    let inner_vars: Vec<String> = {
        let mut vars: Vec<String> = federation::collect_pattern_variables(inner)
            .into_iter()
            .collect();
        vars.sort();
        vars
    };
    let projection = if inner_vars.is_empty() {
        "*".to_owned()
    } else {
        inner_vars
            .iter()
            .map(|v| format!("?{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let inner_text = format!("SELECT {projection} WHERE {{ {inner} }}");

    let timeout_secs = federation::effective_timeout_secs(&url);
    let max_results = crate::FEDERATION_MAX_RESULTS.get();
    let start = std::time::Instant::now();

    let on_partial = crate::FEDERATION_ON_PARTIAL.get();
    let on_partial_str = on_partial
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("empty");

    let result = if on_partial_str == "use" {
        federation::execute_remote_partial(&url, &inner_text, timeout_secs, max_results)
    } else {
        federation::execute_remote(&url, &inner_text, timeout_secs, max_results)
    };

    let latency_ms = start.elapsed().as_millis() as i64;

    let (variables, rows) = match result {
        Ok(r) => {
            federation::record_health(&url, true, latency_ms);
            r
        }
        Err(e) => {
            federation::record_health(&url, false, latency_ms);
            let on_error = crate::FEDERATION_ON_ERROR.get();
            let on_error_str = on_error
                .as_ref()
                .and_then(|c| c.to_str().ok())
                .unwrap_or("warning");
            if silent || on_error_str == "empty" {
                pgrx::warning!("SERVICE {url} failed (returning empty): {e}");
                if silent {
                    return service_silent_fallback(ctx);
                }
                return Fragment::zero_rows();
            } else if on_error_str == "error" {
                pgrx::error!("SERVICE {url} failed: {e}");
            } else {
                pgrx::warning!("SERVICE {url} failed (returning empty): {e}");
                return Fragment::zero_rows();
            }
        }
    };

    if variables.is_empty() || rows.is_empty() {
        return Fragment::zero_rows();
    }
    let (variables, encoded_rows) = federation::encode_results(variables, rows);
    translate_service_values(&variables, &encoded_rows, ctx)
}

fn translate_service_local(stream_table: &str, ctx: &mut Ctx) -> Fragment {
    let vars = federation::get_view_variables(stream_table);
    if vars.is_empty() {
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }
    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    let qualified = if stream_table.contains('.') {
        stream_table.to_owned()
    } else {
        format!("_pg_ripple.{stream_table}")
    };
    frag.from_items.push((alias.clone(), qualified));
    for v in &vars {
        frag.bindings.insert(v.clone(), format!("{alias}._v_{v}"));
    }
    frag
}

pub(crate) fn translate_service_values(
    variables: &[String],
    encoded_rows: &[Vec<Option<i64>>],
    ctx: &mut Ctx,
) -> Fragment {
    if variables.is_empty() || encoded_rows.is_empty() {
        return Fragment::empty();
    }

    let inline_max = crate::FEDERATION_INLINE_MAX_ROWS.get() as usize;
    let row_count = encoded_rows.len();

    if inline_max > 0 && row_count > inline_max {
        pgrx::info!(
            "PT620: SERVICE result set ({row_count} rows) exceeds \
             pg_ripple.federation_inline_max_rows ({inline_max}); \
             spooling to temporary table"
        );
        return translate_service_values_spool(variables, encoded_rows, ctx);
    }

    let col_names: Vec<String> = variables.iter().map(|v| format!("_svc_{v}")).collect();
    let col_names_str = col_names.join(", ");
    let rows_sql: Vec<String> = encoded_rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| match cell {
                    None => "NULL::bigint".to_owned(),
                    Some(id) => id.to_string(),
                })
                .collect();
            format!("({})", cells.join(", "))
        })
        .collect();

    let n = ctx.alias_counter;
    ctx.alias_counter += 1;
    let values_expr = format!(
        "(SELECT * FROM (VALUES {}) AS _svi{n}({col_names_str}))",
        rows_sql.join(", ")
    );

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), values_expr));
    for v in variables {
        frag.bindings.insert(v.clone(), format!("{alias}._svc_{v}"));
    }
    frag
}

fn translate_service_values_spool(
    variables: &[String],
    encoded_rows: &[Vec<Option<i64>>],
    ctx: &mut Ctx,
) -> Fragment {
    let n = ctx.alias_counter;
    ctx.alias_counter += 1;
    let temp_table = format!("_pg_ripple_svc_spool_{n}");

    let col_defs: Vec<String> = variables
        .iter()
        .map(|v| format!("_svc_{v} bigint"))
        .collect();
    let create_sql = format!(
        "CREATE TEMP TABLE IF NOT EXISTS {temp_table} ({}) ON COMMIT DROP",
        col_defs.join(", ")
    );

    if let Err(e) = pgrx::Spi::run(&create_sql) {
        pgrx::log!("SERVICE spool: failed to create temp table {temp_table}: {e}");
        let max = crate::FEDERATION_INLINE_MAX_ROWS.get() as usize;
        let truncated = &encoded_rows[..max.min(encoded_rows.len())];
        return translate_service_values(variables, truncated, ctx);
    }

    let batch_size = 1000usize;
    let col_names: Vec<String> = variables.iter().map(|v| format!("_svc_{v}")).collect();
    let col_names_str = col_names.join(", ");

    for chunk in encoded_rows.chunks(batch_size) {
        let rows_sql: Vec<String> = chunk
            .iter()
            .map(|row| {
                let cells: Vec<String> = row
                    .iter()
                    .map(|cell| match cell {
                        None => "NULL::bigint".to_owned(),
                        Some(id) => id.to_string(),
                    })
                    .collect();
                format!("({})", cells.join(", "))
            })
            .collect();
        let insert_sql = format!(
            "INSERT INTO {temp_table} ({col_names_str}) VALUES {}",
            rows_sql.join(", ")
        );
        if let Err(e) = pgrx::Spi::run(&insert_sql) {
            pgrx::log!("SERVICE spool: INSERT error for {temp_table}: {e}");
        }
    }

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), temp_table));
    for v in variables {
        frag.bindings.insert(v.clone(), format!("{alias}._svc_{v}"));
    }
    frag
}
