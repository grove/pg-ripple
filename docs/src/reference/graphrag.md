# GraphRAG Reference

This page is the reference for pg_ripple's GraphRAG (Retrieval-Augmented Generation) integration.

## Overview

pg_ripple provides GraphRAG capabilities that combine RDF knowledge graph
retrieval with large language model (LLM) generation. The system uses
pgvector-based semantic search, SPARQL query generation, RAG context assembly,
and entity alignment to support knowledge-graph-grounded question answering.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE '%rag%' OR feature_name LIKE 'graphrag%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.rag_context(question TEXT, k INT) → TEXT` | Retrieve top-k relevant graph context for a question |
| `pg_ripple.sparql_from_nl(question TEXT) → TEXT` | Generate SPARQL from natural language (requires LLM endpoint) |
| `pg_ripple.suggest_sameas(iri TEXT, k INT) → SETOF TEXT` | Suggest `owl:sameAs` candidates via embedding similarity |
| `pg_ripple.graphrag_export(graph_iri TEXT, format TEXT) → TEXT` | Export graph in GraphRAG-compatible format (Parquet or JSON-LD) |

## RAG Pipeline

1. **Embedding**: Subject IRIs are embedded via the configured LLM embedding endpoint.
2. **Retrieval**: Given a question, its embedding is compared to entity embeddings
   using HNSW/IVFFlat vector search (`pg_ripple.rag_context()`).
3. **Context assembly**: Top-k entities are expanded to their triples via SPARQL
   (`DESCRIBE` or a custom query template).
4. **Generation**: The assembled context is passed to the LLM for answer generation.

## Microsoft GraphRAG Integration

`graphrag_export()` produces output compatible with Microsoft GraphRAG:
- **Parquet format**: entity and relationship tables with embeddings
- **JSON-LD format**: context-annotated graph nodes

## LLM Configuration

Set LLM endpoint parameters via GUCs:

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.llm_endpoint` | `''` | Base URL for LLM API (OpenAI-compatible) |
| `pg_ripple.llm_api_key` | `''` | API key (use a secret, not plaintext) |
| `pg_ripple.llm_model` | `'gpt-4o'` | Model name for generation |
| `pg_ripple.embedding_model` | `'text-embedding-3-small'` | Model for embeddings |

## Related Pages

- [Vector Search Reference](vector-search.md)
- [GraphRAG Functions](graphrag-functions.md)
- [GraphRAG Ontology](graphrag-ontology.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
