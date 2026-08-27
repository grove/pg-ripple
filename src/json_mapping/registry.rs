use pgrx::prelude::*;

/// Register or replace a JSON mapping in the catalog.
#[allow(clippy::too_many_arguments)]
pub fn register_mapping_impl(
    name: &str,
    context: &serde_json::Value,
    shape_iri: Option<&str>,
    default_graph_iri: Option<&str>,
    timestamp_path: Option<&str>,
    timestamp_predicate: Option<&str>,
    iri_template: Option<&str>,
    iri_match_pattern: Option<&str>,
) {
    if !context.is_object() {
        pgrx::error!("register_json_mapping: context must be a JSON object (the @context value)");
    }

    if let Some(tmpl) = iri_template {
        let placeholder_count = tmpl.matches("{id}").count();
        if placeholder_count != 1 {
            pgrx::error!(
                "register_json_mapping: iri_template must contain exactly one {{id}} placeholder; found {} in {:?}",
                placeholder_count,
                tmpl
            );
        }
    }

    let ts_pred = timestamp_predicate.unwrap_or("http://www.w3.org/ns/prov#generatedAtTime");
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.json_mappings \
         (name, context, shape_iri, default_graph_iri, timestamp_path, \
          timestamp_predicate, iri_template, iri_match_pattern) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (name) DO UPDATE SET \
             context = EXCLUDED.context, shape_iri = EXCLUDED.shape_iri, \
             default_graph_iri = EXCLUDED.default_graph_iri, timestamp_path = EXCLUDED.timestamp_path, \
             timestamp_predicate = EXCLUDED.timestamp_predicate, iri_template = EXCLUDED.iri_template, \
             iri_match_pattern = EXCLUDED.iri_match_pattern, created_at = now()",
        &[
            pgrx::datum::DatumWithOid::from(name),
            pgrx::datum::DatumWithOid::from(pgrx::JsonB(context.clone())),
            pgrx::datum::DatumWithOid::from(shape_iri),
            pgrx::datum::DatumWithOid::from(default_graph_iri),
            pgrx::datum::DatumWithOid::from(timestamp_path),
            pgrx::datum::DatumWithOid::from(ts_pred),
            pgrx::datum::DatumWithOid::from(iri_template),
            pgrx::datum::DatumWithOid::from(iri_match_pattern),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("register_json_mapping: catalog insert failed: {e}"));

    if let Some(siri) = shape_iri {
        check_mapping_consistency(name, context, siri);
    }
}

/// Fetch a mapping context, raising an error when the mapping is unknown.
pub fn fetch_mapping_context(mapping: &str) -> serde_json::Value {
    let ctx_jsonb = Spi::get_one_with_args::<pgrx::JsonB>(
        "SELECT context FROM _pg_ripple.json_mappings WHERE name = $1",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or_else(|| {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        )
    });
    ctx_jsonb.0
}

fn check_mapping_consistency(mapping_name: &str, context: &serde_json::Value, shape_iri: &str) {
    let ctx_terms: std::collections::HashMap<String, String> = context
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| !k.starts_with('@'))
                .filter_map(|(k, v)| {
                    let iri = match v {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Object(meta) => {
                            meta.get("@id").and_then(|id| id.as_str()).map(String::from)
                        }
                        _ => None,
                    };
                    iri.map(|i| (k.clone(), i))
                })
                .collect()
        })
        .unwrap_or_default();

    let sparql = format!(
        "SELECT ?path ?name WHERE {{ \
             <{shape_iri}> <http://www.w3.org/ns/shacl#property> ?prop . \
             ?prop <http://www.w3.org/ns/shacl#path> ?path . \
             OPTIONAL {{ ?prop <http://www.w3.org/ns/shacl#name> ?name }} \
         }}"
    );
    let shape_props = crate::sparql::sparql(&sparql);
    let shape_iris: std::collections::HashMap<String, Option<String>> = shape_props
        .iter()
        .filter_map(|row| {
            let obj = row.0.as_object()?;
            let path = obj.get("path")?.as_str()?.trim_matches('"').to_string();
            let path = path
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            let name = obj
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.trim_matches('"').to_string());
            Some((path, name))
        })
        .collect();

    for (term, iri) in &ctx_terms {
        if !shape_iris.contains_key(iri) {
            pgrx::warning!(
                "register_json_mapping {:?}: context term {:?} (IRI {}) has no corresponding sh:property in shape {}; field will be ingested but not validated",
                mapping_name,
                term,
                iri,
                shape_iri
            );
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.json_mapping_warnings \
                 (mapping_name, kind, detail) VALUES ($1, 'missing_shape_property', $2) \
                 ON CONFLICT DO NOTHING",
                &[
                    pgrx::datum::DatumWithOid::from(mapping_name),
                    pgrx::datum::DatumWithOid::from(
                        format!(
                            "context term {term:?} (IRI {iri}) has no sh:property in {shape_iri}"
                        )
                        .as_str(),
                    ),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("could not record warning: {e}"));
        }
    }

    let ctx_iris: std::collections::HashSet<&str> =
        ctx_terms.values().map(String::as_str).collect();
    for iri in shape_iris.keys() {
        if !ctx_iris.contains(iri.as_str()) {
            pgrx::warning!(
                "register_json_mapping {:?}: shape {} has sh:property <{}> with no corresponding context term; field will be stored but never appear in outbound documents",
                mapping_name,
                shape_iri,
                iri
            );
            Spi::run_with_args(
                "INSERT INTO _pg_ripple.json_mapping_warnings \
                 (mapping_name, kind, detail) VALUES ($1, 'missing_context_term', $2) \
                 ON CONFLICT DO NOTHING",
                &[
                    pgrx::datum::DatumWithOid::from(mapping_name),
                    pgrx::datum::DatumWithOid::from(
                        format!("shape {shape_iri} has sh:property <{iri}> with no context term")
                            .as_str(),
                    ),
                ],
            )
            .unwrap_or_else(|e| pgrx::warning!("could not record warning: {e}"));
        }
    }
}

/// Require that a mapping exists.
pub fn require_mapping_exists(mapping: &str) {
    let exists: bool = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.json_mappings WHERE name = $1)",
        &[pgrx::datum::DatumWithOid::from(mapping)],
    )
    .unwrap_or(None)
    .unwrap_or(false);
    if !exists {
        pgrx::error!(
            "json mapping {:?} not found; call register_json_mapping() first",
            mapping
        );
    }
}
