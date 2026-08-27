pub fn export_json_node_impl(
    subject_id: i64,
    mapping: &str,
    strip: Vec<String>,
) -> Option<pgrx::JsonB> {
    let context = super::fetch_mapping_context(mapping);
    let mut frame = serde_json::Map::new();
    frame.insert("@context".to_owned(), context.clone());

    if let Some(ctx_obj) = context.as_object() {
        for iri_val in ctx_obj.values() {
            let iri = match iri_val {
                serde_json::Value::String(value) => Some(value.as_str()),
                serde_json::Value::Object(meta) => meta.get("@id").and_then(|v| v.as_str()),
                _ => None,
            };
            if let Some(iri) = iri.filter(|iri| !iri.starts_with('@')) {
                frame.insert(
                    iri.to_owned(),
                    serde_json::Value::Object(Default::default()),
                );
            }
        }
    }

    crate::export::export_jsonld_node_impl(serde_json::Value::Object(frame), subject_id, strip)
        .map(|value| value.map(pgrx::JsonB))
        .unwrap_or_else(|e| pgrx::error!("{e}"))
}
