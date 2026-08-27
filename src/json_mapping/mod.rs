mod export;
mod ingest;
mod queue;
mod registry;
mod triggers;
mod writeback;

pub use export::export_json_node_impl;
pub use ingest::ingest_json_impl;
pub use queue::{drain_json_writeback_queue, json_writeback_status_impl};
pub use registry::{fetch_mapping_context, register_mapping_impl, require_mapping_exists};
pub use triggers::{
    disable_json_writeback_impl, enable_json_writeback_impl,
    install_writeback_triggers_after_promotion,
};
pub use writeback::{
    configure_json_writeback_impl, writeback_inspect_impl, writeback_json_row_delete_impl,
    writeback_json_row_impl,
};
#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;
    #[pg_extern]
    #[allow(clippy::too_many_arguments)]
    pub fn register_json_mapping(
        name: &str,
        context: pgrx::JsonB,
        shape_iri: default!(Option<&str>, "NULL"),
        default_graph_iri: default!(Option<&str>, "NULL"),
        timestamp_path: default!(Option<&str>, "NULL"),
        timestamp_predicate: default!(Option<&str>, "'http://www.w3.org/ns/prov#generatedAtTime'"),
        iri_template: default!(Option<&str>, "NULL"),
        iri_match_pattern: default!(Option<&str>, "NULL"),
    ) {
        crate::json_mapping::register_mapping_impl(
            name,
            &context.0,
            shape_iri,
            default_graph_iri,
            timestamp_path,
            timestamp_predicate,
            iri_template,
            iri_match_pattern,
        );
    }

    #[pg_extern]
    pub fn ingest_json(
        payload: pgrx::JsonB,
        subject_iri: &str,
        mapping: &str,
        graph_iri: default!(Option<&str>, "NULL"),
        mode: default!(&str, "'append'"),
        source_timestamp: default!(Option<pgrx::datum::Timestamp>, "NULL"),
    ) -> i64 {
        match mode {
            "append" => {
                crate::json_mapping::ingest_json_impl(&payload.0, subject_iri, mapping, graph_iri)
            }
            "upsert" => {
                crate::bidi::ingest_json_upsert_impl(&payload.0, subject_iri, mapping, graph_iri)
            }
            "diff" => crate::bidi::ingest_json_diff_impl(
                &payload.0,
                subject_iri,
                mapping,
                graph_iri,
                source_timestamp,
            ),
            other => pgrx::error!(
                "ingest_json: unknown mode '{}'; valid values: append, upsert, diff",
                other
            ),
        }
    }

    #[pg_extern]
    pub fn export_json_node(
        subject_id: i64,
        mapping: &str,
        strip: default!(Vec<String>, "ARRAY['@type','@id']::TEXT[]"),
    ) -> Option<pgrx::JsonB> {
        crate::json_mapping::export_json_node_impl(subject_id, mapping, strip)
    }

    #[pg_extern]
    pub fn writeback_json_row(mapping: &str, subject_iri: &str) -> i64 {
        crate::json_mapping::writeback_json_row_impl(mapping, subject_iri)
    }

    #[pg_extern]
    pub fn writeback_json_row_delete(mapping: &str, subject_iri: &str) -> i64 {
        crate::json_mapping::writeback_json_row_delete_impl(mapping, subject_iri)
    }

    #[pg_extern]
    pub fn enable_json_writeback(mapping: &str) {
        crate::json_mapping::enable_json_writeback_impl(mapping)
    }

    #[pg_extern]
    pub fn disable_json_writeback(mapping: &str) {
        crate::json_mapping::disable_json_writeback_impl(mapping)
    }

    #[allow(clippy::type_complexity)]
    #[pg_extern]
    pub fn json_writeback_status() -> pgrx::iter::TableIterator<
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
        crate::json_mapping::json_writeback_status_impl()
    }

    #[pg_extern(security_definer)]
    #[search_path(pg_catalog, _pg_ripple, public)]
    pub fn configure_json_writeback(
        mapping_name: &str,
        target_schema: &str,
        target_table: &str,
        key_columns: Vec<String>,
        conflict_policy: default!(&str, "'error'"),
    ) {
        crate::json_mapping::configure_json_writeback_impl(
            mapping_name,
            target_schema,
            target_table,
            &key_columns,
            conflict_policy,
        );
    }

    #[allow(clippy::type_complexity)]
    #[pg_extern]
    pub fn writeback_inspect(
        mapping_name: &str,
    ) -> pgrx::iter::TableIterator<
        'static,
        (
            pgrx::name!(target_schema, String),
            pgrx::name!(target_table, String),
            pgrx::name!(key_columns, Vec<String>),
            pgrx::name!(conflict_policy, String),
            pgrx::name!(writeback_enabled, bool),
            pgrx::name!(trigger_count, i32),
            pgrx::name!(queue_depth, i64),
        ),
    > {
        crate::json_mapping::writeback_inspect_impl(mapping_name)
    }
}
