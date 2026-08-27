use pgrx::prelude::*;

pub fn ingest_json_impl(
    payload: &serde_json::Value,
    subject_iri: &str,
    mapping: &str,
    graph_iri: Option<&str>,
) -> i64 {
    let context = super::fetch_mapping_context(mapping);
    let ntriples = crate::bulk_load::json_to_ntriples(payload, subject_iri, None, Some(&context));
    if ntriples.is_empty() {
        return 0;
    }

    let default_graph: Option<String> = if graph_iri.is_none() {
        Spi::get_one_with_args::<String>(
            "SELECT default_graph_iri FROM _pg_ripple.json_mappings WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(mapping)],
        )
        .unwrap_or(None)
    } else {
        None
    };
    let resolved_graph = graph_iri.or(default_graph.as_deref());
    let (inserted, graph_id) = match resolved_graph {
        None | Some("") => (crate::bulk_load::load_ntriples(&ntriples, false), 0),
        Some(graph) => {
            let graph_id = crate::dictionary::encode(
                graph.trim_matches(|c| c == '<' || c == '>'),
                crate::dictionary::KIND_IRI,
            );
            (
                crate::bulk_load::load_ntriples_into_graph(&ntriples, graph_id),
                graph_id,
            )
        }
    };
    if inserted > 0 {
        crate::bidi::update_graph_metrics_triple_count(graph_id, inserted);
    }
    inserted
}
