# Vector Search Reference

This page is the reference for pg_ripple's vector + SPARQL hybrid search.

## Overview

pg_ripple integrates with pgvector to provide semantic similarity search over
RDF entities. Entity embeddings are stored in `_pg_ripple.embeddings` with
HNSW or IVFFlat indices. The `pg:similar()` SPARQL function queries the
vector index and returns results ranked by cosine similarity.

Hybrid retrieval combines vector similarity ranking with SPARQL graph
constraints, enabling queries like: "find entities semantically similar to X
that also satisfy SHACL shape Y".

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE '%vector%' OR feature_name LIKE '%embed%' OR feature_name LIKE '%kge%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.embed_entities(graph_iri TEXT, model TEXT) → BIGINT` | Bulk-embed all entities in a graph |
| `pg_ripple.similar_entities(iri TEXT, k INT, model TEXT) → SETOF TEXT` | Find k nearest-neighbor entities by embedding |
| `pg_ripple.suggest_sameas(iri TEXT, k INT) → SETOF TEXT` | Suggest `owl:sameAs` candidates via cosine similarity |

## SPARQL `pg:similar()` Function

Use the `pg:similar()` extension function inside SPARQL queries for inline
vector search:

```sparql
PREFIX pg: <http://pg_ripple.io/fn/>
SELECT ?entity ?score WHERE {
  ?entity pg:similar("machine learning", 10) ?score .
  ?entity a <http://example.org/Paper> .
}
ORDER BY DESC(?score)
```

## Embedding Models

Embeddings are generated via the configured LLM embedding endpoint. Each
entity-model pair is stored once in `_pg_ripple.embeddings`. The incremental
embedding worker runs in the background and embeds new entities as they are
inserted.

## Knowledge Graph Embeddings (KGE)

Graph-structure embeddings (TransE, RotatE) are computed by `src/kge.rs`
and stored alongside text embeddings. KGE embeddings capture structural
relationship patterns and complement text-based semantic similarity.

## Index Configuration

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.vector_index_type` | `'hnsw'` | Index type: `hnsw` or `ivfflat` |
| `pg_ripple.hnsw_m` | `16` | HNSW M parameter |
| `pg_ripple.hnsw_ef_construction` | `64` | HNSW ef_construction parameter |
| `pg_ripple.vector_dimensions` | `1536` | Embedding vector dimensions |

## Related Pages

- [Embedding Functions](embedding-functions.md)
- [GraphRAG Reference](graphrag.md)
- [Vector Index Trade-offs](vector-index-tradeoffs.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
