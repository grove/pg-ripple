//! pg_ripple SQL API — Export (Turtle/N-Triples/JSON-LD), GraphRAG BYOG, JSON-LD Framing

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Export ────────────────────────────────────────────────────────────────

    /// Export triples to N-Triples format.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_ntriples(graph: Option<&str>) -> String {
        crate::export::export_ntriples(graph)
    }

    /// Export triples to N-Quads format.
    /// Pass a graph IRI to export a specific graph, or NULL to export all graphs.
    #[pg_extern]
    fn export_nquads(graph: Option<&str>) -> String {
        crate::export::export_nquads(graph)
    }

    /// Export triples as Turtle text.
    ///
    /// Groups triples by subject and emits compact Turtle blocks.  Includes
    /// all `@prefix` declarations from the prefix registry.
    /// RDF-star quoted triples are serialized in Turtle-star `<< s p o >>` notation.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_turtle(graph: default!(Option<&str>, "NULL")) -> String {
        crate::export::export_turtle(graph)
    }

    /// Export triples as JSON-LD (expanded form).
    ///
    /// Returns a JSON-LD document as a JSONB array where each element represents
    /// one subject with all its predicates and objects.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_jsonld(graph: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        pgrx::JsonB(crate::export::export_jsonld(graph))
    }

    /// Streaming Turtle export — returns one `TEXT` row per triple.
    ///
    /// Yields `@prefix` declarations first, then one flat Turtle triple per line.
    /// Suitable for large graphs where buffering the full document would be too
    /// memory-intensive.
    #[pg_extern]
    fn export_turtle_stream(
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let lines = crate::export::export_turtle_stream(graph);
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    /// Streaming JSON-LD export — returns one NDJSON line per subject.
    ///
    /// Each row is a JSON string representing one subject's complete node object.
    /// Suitable for large graphs where buffering the full document is undesirable.
    #[pg_extern]
    fn export_jsonld_stream(
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let lines = crate::export::export_jsonld_stream(graph);
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    // ── GraphRAG BYOG Parquet export (v0.26.0) ────────────────────────────────

    /// Export all `gr:Entity` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `title`, `type`, `description`, `text_unit_ids`, `frequency`, `degree`.
    ///
    /// `graph_iri` is the named graph IRI (without angle brackets), or an empty
    /// string to query the default graph.
    ///
    /// Requires superuser.  Returns the number of entity rows written.
    ///
    /// The output file is compatible with `pyarrow.parquet.read_table()` and
    /// can be fed directly to GraphRAG's BYOG `entity_table_path` option.
    #[pg_extern]
    fn export_graphrag_entities(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_entities(graph_iri, output_path)
    }

    /// Export all `gr:Relationship` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `source`, `target`, `description`, `weight`, `combined_degree`, `text_unit_ids`.
    ///
    /// Requires superuser.  Returns the number of relationship rows written.
    #[pg_extern]
    fn export_graphrag_relationships(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_relationships(graph_iri, output_path)
    }

    /// Export all `gr:TextUnit` nodes from a named graph to a Parquet file.
    ///
    /// Writes a Parquet file at `output_path` with columns:
    /// `id`, `text`, `n_tokens`, `document_id`, `entity_ids`, `relationship_ids`.
    ///
    /// Requires superuser.  Returns the number of text unit rows written.
    #[pg_extern]
    fn export_graphrag_text_units(graph_iri: &str, output_path: &str) -> i64 {
        crate::export::export_graphrag_text_units(graph_iri, output_path)
    }

    // ── JSON-LD Framing (v0.17.0) ─────────────────────────────────────────────

    /// Translate a JSON-LD frame to a SPARQL CONSTRUCT query string.
    ///
    /// Primary inspection and debugging tool: shows the generated CONSTRUCT
    /// query without executing it. `graph` restricts to a named graph when set.
    #[pg_extern]
    fn jsonld_frame_to_sparql(frame: pgrx::JsonB, graph: default!(Option<&str>, "NULL")) -> String {
        let val = &frame.0;
        crate::framing::frame_to_sparql(val, graph).unwrap_or_else(|e| pgrx::error!("{}", e))
    }

    /// Primary end-user function: translate a JSON-LD frame into a SPARQL
    /// CONSTRUCT query, execute it, apply the W3C embedding algorithm, compact
    /// with the frame's `@context`, and return the framed JSON-LD document.
    #[pg_extern]
    fn export_jsonld_framed(
        frame: pgrx::JsonB,
        graph: default!(Option<&str>, "NULL"),
        embed: default!(&str, "'@once'"),
        explicit: default!(bool, "false"),
        ordered: default!(bool, "false"),
    ) -> pgrx::JsonB {
        let val = &frame.0;
        let result = crate::framing::frame_and_execute(val, graph, embed, explicit, ordered)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        pgrx::JsonB(result)
    }

    /// Streaming variant of `export_jsonld_framed` — returns one NDJSON line
    /// per matched root node. Avoids buffering large framed documents in memory.
    #[pg_extern]
    fn export_jsonld_framed_stream(
        frame: pgrx::JsonB,
        graph: default!(Option<&str>, "NULL"),
    ) -> TableIterator<'static, (name!(line, String),)> {
        let val = frame.0.clone();
        let lines = crate::framing::execute_framed_stream(&val, graph)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    /// General-purpose framing primitive: apply the W3C JSON-LD Framing
    /// embedding algorithm to any already-expanded JSON-LD JSONB document.
    ///
    /// `input` is expected to be a JSON-LD array of expanded node objects.
    /// Useful for framing SPARQL CONSTRUCT results obtained via other means.
    #[pg_extern]
    fn jsonld_frame(
        input: pgrx::JsonB,
        frame: pgrx::JsonB,
        embed: default!(&str, "'@once'"),
        explicit: default!(bool, "false"),
        ordered: default!(bool, "false"),
    ) -> pgrx::JsonB {
        let result = crate::framing::frame_jsonld(&input.0, &frame.0, embed, explicit, ordered)
            .unwrap_or_else(|e| pgrx::error!("{}", e));
        pgrx::JsonB(result)
    }
}
