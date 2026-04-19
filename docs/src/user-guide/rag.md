# RAG Retrieval with pg_ripple

pg_ripple v0.28.0 provides `pg_ripple.rag_retrieve()` — a single SQL function that takes a natural-language question and returns structured context ready for use as an LLM system prompt.

No separate vector database, no ETL, no eventual consistency. Everything runs inside PostgreSQL.

---

## How It Works

```
question text
      │
      ▼
  vector search (HNSW)
  k nearest entities
      │
      ▼
  SPARQL filter (optional)
  prune candidates to matching subgraph
      │
      ▼
  contextualize_entity()
  gather label, types, neighbors
      │
      ▼
  JSONB output  ─────────►  LLM system prompt
  (or JSON-LD)  ─────────►  OpenAI structured outputs
```

---

## Quick Start

### 1. Set up embeddings (v0.27.0 prerequisite)

```sql
ALTER SYSTEM SET pg_ripple.embedding_api_url = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key  = 'sk-...';
ALTER SYSTEM SET pg_ripple.embedding_model    = 'text-embedding-3-small';
SELECT pg_reload_conf();

-- Embed all entities in your knowledge graph.
SELECT pg_ripple.embed_entities();
```

### 2. Retrieve RAG context

```sql
SELECT entity_iri, label, context_json, distance
FROM pg_ripple.rag_retrieve('what treats headaches?', k := 5);
```

**Sample output:**

| `entity_iri` | `label` | `context_json` | `distance` |
|---|---|---|---|
| `https://pharma.example/aspirin` | aspirin | `{"label": "aspirin", "types": ["Drug", "NSAID"], "properties": [...], "neighbors": [...]}` | 0.12 |
| `https://pharma.example/ibuprofen` | ibuprofen | `{"label": "ibuprofen", ...}` | 0.17 |

### 3. Apply a SPARQL filter

Narrow results to entities matching a SPARQL WHERE clause fragment:

```sql
SELECT entity_iri, label, context_json, distance
FROM pg_ripple.rag_retrieve(
    'what treats headaches?',
    sparql_filter := '?entity a <https://pharma.example/Drug> ;
                               <https://pharma.example/approvedBy> <https://pharma.example/FDA>',
    k := 5
);
```

### 4. JSON-LD output for OpenAI structured outputs

```sql
SELECT entity_iri, context_json
FROM pg_ripple.rag_retrieve(
    'what treats headaches?',
    k := 5,
    output_format := 'jsonld'
);
```

`context_json` in JSON-LD mode contains:

```json
{
  "@context": {"rdfs": "http://www.w3.org/2000/01/rdf-schema#", ...},
  "@id": "<https://pharma.example/aspirin>",
  "@type": ["https://pharma.example/Drug"],
  "rdfs:label": "aspirin",
  "properties": [...],
  "neighbors": [...],
  "contextText": "aspirin. Type: NSAID. Related: headache, fever"
}
```

---

## LangChain Integration

```python
import psycopg2

conn = psycopg2.connect("dbname=mydb user=postgres")
cur = conn.cursor()

def retrieve_context(question: str, k: int = 5) -> str:
    cur.execute(
        "SELECT context_json FROM pg_ripple.rag_retrieve(%s, k := %s)",
        (question, k)
    )
    rows = cur.fetchall()
    return "\n\n".join(str(row[0]) for row in rows)

# Use in a LangChain chain
context = retrieve_context("what treats headaches?")
```

## LlamaIndex Integration

```python
from llama_index.core.retrievers import BaseRetriever
from llama_index.core.schema import NodeWithScore, TextNode
import psycopg2

class PgRippleRetriever(BaseRetriever):
    def __init__(self, conn_str: str, k: int = 5):
        self.conn = psycopg2.connect(conn_str)
        self.k = k

    def _retrieve(self, query_bundle):
        cur = self.conn.cursor()
        cur.execute(
            "SELECT entity_iri, label, context_json FROM pg_ripple.rag_retrieve(%s, k := %s)",
            (query_bundle.query_str, self.k)
        )
        return [
            NodeWithScore(
                node=TextNode(text=str(row[2]), id_=row[0], metadata={"label": row[1]}),
                score=1.0
            )
            for row in cur.fetchall()
        ]
```

---

## pg_ripple_http REST Endpoint

The `pg_ripple_http` sidecar service exposes `rag_retrieve()` via HTTP:

```bash
curl -X POST http://localhost:8080/rag \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"question": "what treats headaches?", "k": 5}'
```

**Response:**

```json
{
  "results": [
    {
      "entity_iri": "https://pharma.example/aspirin",
      "label": "aspirin",
      "context_json": {...},
      "distance": 0.12
    }
  ],
  "context": "aspirin. Type: NSAID. Related: headache, fever.\n\nibuprofen. Type: Drug..."
}
```

For JSON-LD output:

```bash
curl -X POST http://localhost:8080/rag \
  -d '{"question": "what treats headaches?", "k": 5, "output_format": "jsonld"}'
```

---

## Function Reference

### `pg_ripple.rag_retrieve()`

```sql
pg_ripple.rag_retrieve(
    question      TEXT,
    sparql_filter TEXT    DEFAULT NULL,
    k             INT     DEFAULT 5,
    model         TEXT    DEFAULT NULL,
    output_format TEXT    DEFAULT 'jsonb'
) RETURNS TABLE (
    entity_iri   TEXT,
    label        TEXT,
    context_json JSONB,
    distance     FLOAT8
)
```

| Parameter | Description |
|---|---|
| `question` | Natural language question; encoded to a vector for similarity search |
| `sparql_filter` | Optional SPARQL WHERE clause fragment to filter candidates (`?entity` is the bound variable) |
| `k` | Maximum number of results to return |
| `model` | Override `pg_ripple.embedding_model` GUC for this call |
| `output_format` | `'jsonb'` (default) or `'jsonld'`; controls structure of `context_json` |

**Returns zero rows** when pgvector is absent (PT603 WARNING — not an ERROR).

---

## GUC Parameters

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.embedding_api_url` | (none) | OpenAI-compatible embedding API base URL |
| `pg_ripple.embedding_api_key` | (none) | API key (superuser only) |
| `pg_ripple.embedding_model` | `text-embedding-3-small` | Default embedding model |
| `pg_ripple.embedding_dimensions` | `1536` | Embedding vector dimensions |
| `pg_ripple.use_graph_context` | `off` | Use `contextualize_entity()` for richer embeddings |
| `pg_ripple.auto_embed` | `off` | Auto-queue new entities for embedding |
| `pg_ripple.embedding_batch_size` | `100` | Worker embedding batch size |
