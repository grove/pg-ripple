//! pg_ripple SQL API — Vector embedding, Hybrid search, GraphRAG RAG pipeline (v0.27.0+)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Vector embedding (v0.27.0) ────────────────────────────────────────────

    /// Store a user-supplied embedding vector for an entity IRI.
    ///
    /// `embedding` is a `FLOAT8[]` array upserted into `_pg_ripple.embeddings`.
    /// Its length must match `pg_ripple.embedding_dimensions` (default: 1536).
    ///
    /// Raises a WARNING (not an ERROR) when pgvector is absent or the array
    /// length does not match the configured dimension count (PT602 / PT603).
    #[pg_extern]
    fn store_embedding(
        entity_iri: &str,
        embedding: Vec<f64>,
        model: default!(Option<&str>, "NULL"),
    ) {
        crate::sparql::embedding::store_embedding(entity_iri, embedding, model)
    }

    /// Return the k nearest entities to `query_text` by cosine distance.
    ///
    /// Encodes `query_text` via the configured embedding API and queries
    /// `_pg_ripple.embeddings` using the pgvector `<=>` cosine distance
    /// operator.  Returns results sorted by ascending distance.
    ///
    /// Returns zero rows when pgvector is absent, `pgvector_enabled = false`,
    /// or `embedding_api_url` is not configured.
    #[pg_extern]
    fn similar_entities(
        query_text: &str,
        k: default!(i32, 10),
        model: default!(Option<&str>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_id, i64),
            name!(entity_iri, String),
            name!(distance, f64),
        ),
    > {
        let rows = crate::sparql::embedding::similar_entities(query_text, k, model);
        TableIterator::new(rows)
    }

    /// Batch-embed entities from a graph using the configured embedding API.
    ///
    /// Collects entity IRIs + their `rdfs:label` (or IRI local name) and calls
    /// the OpenAI-compatible API at `pg_ripple.embedding_api_url`.  Results are
    /// upserted into `_pg_ripple.embeddings`.
    ///
    /// `graph_iri` — restrict to a named graph; NULL embeds entities from all graphs.
    /// `model` — override `pg_ripple.embedding_model`.
    /// `batch_size` — API call batch size (default: 100).
    ///
    /// Returns total embeddings stored.
    #[pg_extern]
    fn embed_entities(
        graph_iri: default!(Option<&str>, "NULL"),
        model: default!(Option<&str>, "NULL"),
        batch_size: default!(i32, 100),
    ) -> i64 {
        crate::sparql::embedding::embed_entities(graph_iri, model, batch_size)
    }

    /// Refresh stale embeddings after label updates.
    ///
    /// Identifies entities whose `rdfs:label` triple was inserted after
    /// `_pg_ripple.embeddings.updated_at` and re-embeds them.  When `force =
    /// true`, re-embeds all entities regardless of staleness.
    ///
    /// Returns the count of re-embedded entities.  Emits a NOTICE when no
    /// stale embeddings are found (PT606).
    #[pg_extern]
    fn refresh_embeddings(
        graph_iri: default!(Option<&str>, "NULL"),
        model: default!(Option<&str>, "NULL"),
        force: default!(bool, false),
    ) -> i64 {
        crate::sparql::embedding::refresh_embeddings(graph_iri, model, force)
    }

    // ── v0.28.0: Advanced Hybrid Search & RAG Pipeline ────────────────────────

    /// Enumerate all embedding models stored in `_pg_ripple.embeddings`.
    ///
    /// Returns one row per model with the entity count and vector dimension.
    /// Returns zero rows when pgvector is absent.
    #[pg_extern]
    fn list_embedding_models() -> TableIterator<
        'static,
        (
            name!(model, String),
            name!(entity_count, i64),
            name!(dimensions, i32),
        ),
    > {
        let rows = crate::sparql::embedding::list_embedding_models();
        TableIterator::new(rows)
    }

    /// Materialise `pg:hasEmbedding` triples for entities in `_pg_ripple.embeddings`.
    ///
    /// Inserts `<entity_iri> <pg:hasEmbedding> "true"^^xsd:boolean` for every
    /// embedded entity.  This makes embedding completeness checkable via SHACL.
    ///
    /// Returns the count of newly inserted triples.
    #[pg_extern]
    fn add_embedding_triples() -> i64 {
        crate::sparql::embedding::add_embedding_triples()
    }

    /// Produce a text representation of an entity's RDF neighborhood for embedding.
    ///
    /// Gathers the entity's label, type(s), and neighboring entity labels within
    /// `depth` hops (up to `max_neighbors`).  Returns a plain-text string suitable
    /// for passing to an embedding API.
    #[pg_extern]
    fn contextualize_entity(
        entity_iri: &str,
        depth: default!(i32, 1),
        max_neighbors: default!(i32, 20),
    ) -> String {
        crate::sparql::embedding::contextualize_entity(entity_iri, depth, max_neighbors)
    }

    /// Hybrid search using Reciprocal Rank Fusion of SPARQL and vector results.
    ///
    /// Executes `sparql_query` (a SPARQL SELECT returning `?entity`) for the
    /// SPARQL-ranked candidate set, then executes `similar_entities(query_text)`
    /// for the vector-ranked set.  Applies RRF with k_rrf = 60.
    ///
    /// `alpha` controls weighting: 0.0 = vector only, 1.0 = SPARQL only, 0.5 = equal.
    ///
    /// Returns zero rows when pgvector is absent (PT603 WARNING).
    #[pg_extern]
    fn hybrid_search(
        sparql_query: &str,
        query_text: &str,
        k: default!(i32, 10),
        alpha: default!(f64, 0.5),
        model: default!(Option<&str>, "NULL"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_id, i64),
            name!(entity_iri, String),
            name!(rrf_score, f64),
            name!(sparql_rank, i32),
            name!(vector_rank, i32),
        ),
    > {
        let rows =
            crate::sparql::embedding::hybrid_search(sparql_query, query_text, k, alpha, model);
        TableIterator::new(rows)
    }

    /// End-to-end RAG retrieval: find k nearest entities to `question`, collect context.
    ///
    /// Step 1: vector search for `k` candidates.
    /// Step 2: apply optional `sparql_filter` WHERE clause on candidates.
    /// Step 3: contextualize each surviving entity.
    /// Step 4: return rows with `entity_iri`, `label`, `context_json`, `distance`.
    ///
    /// `output_format`: `'jsonb'` (default) or `'jsonld'`.  When `'jsonld'`,
    /// `context_json` includes `@type` and `@context` keys.
    ///
    /// Returns zero rows when pgvector is absent (PT603 WARNING).
    #[pg_extern]
    fn rag_retrieve(
        question: &str,
        sparql_filter: default!(Option<&str>, "NULL"),
        k: default!(i32, 5),
        model: default!(Option<&str>, "NULL"),
        output_format: default!(&str, "'jsonb'"),
    ) -> TableIterator<
        'static,
        (
            name!(entity_iri, String),
            name!(label, String),
            name!(context_json, pgrx::JsonB),
            name!(distance, f64),
        ),
    > {
        let rows = crate::sparql::embedding::rag_retrieve(
            question,
            sparql_filter,
            k,
            model,
            output_format,
        );
        TableIterator::new(rows)
    }

    /// Register an external vector service endpoint for SPARQL SERVICE federation.
    ///
    /// `api_type` must be one of `'pgvector'`, `'weaviate'`, `'qdrant'`, or `'pinecone'`.
    ///
    /// Registered endpoints can be queried via `SERVICE <url> { ?e pg:similarTo "text" }`.
    #[pg_extern]
    fn register_vector_endpoint(url: &str, api_type: &str) {
        crate::sparql::federation::register_vector_endpoint(url, api_type)
    }
}
