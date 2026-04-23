# RAG Pipeline — `rag_context()`

pg_ripple v0.50.0 introduces `pg_ripple.rag_context()` — a single SQL function that assembles a retrieval-augmented generation (RAG) context string from your knowledge graph, ready for use as an LLM system prompt or user message.

---

## Function Signature

```sql
pg_ripple.rag_context(
    question    TEXT,
    k           INT DEFAULT 10
) RETURNS TEXT
```

### Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `question` | (required) | Natural-language question to retrieve context for |
| `k` | `10` | Maximum number of entities to include in context |

---

## How It Works

The function executes a five-step pipeline entirely inside PostgreSQL:

```
question TEXT
    │
    ▼  Step 1: Embed question
   HNSW cosine search on _pg_ripple.embeddings
    │
    ▼  Step 2: Vector recall
   Top-k most similar entities
    │
    ▼  Step 3: SPARQL graph expansion
   1-hop neighbourhood for each entity (labels, types, properties, neighbors)
    │
    ▼  Step 4: Assemble context
   JSON-LD fragments joined into a plain-text context string
    │
    ▼  Step 5 (optional): NL→SPARQL execution
   If pg_ripple.llm_endpoint is set, execute sparql_from_nl(question)
   and append the SPARQL result set
    │
    ▼
context TEXT
```

---

## Prerequisites

`rag_context()` requires:

1. **pgvector** extension installed (`CREATE EXTENSION vector`)
2. **`pg_ripple.pgvector_enabled = on`** (default: `on`)
3. Entities loaded with embeddings via `pg_ripple.embed_entities()` or manually into `_pg_ripple.embeddings`

When pgvector is absent or the embeddings table is empty, the function degrades gracefully and returns an empty string with a WARNING rather than raising an ERROR.

---

## Examples

### Basic context retrieval

```sql
-- Retrieve context for a question (returns plain text)
SELECT pg_ripple.rag_context(
    'What drugs are used to treat headaches?',
    k := 5
);
```

### Use the context as an LLM system prompt

```sql
-- Assemble context and pass to sparql_from_nl
SELECT pg_ripple.sparql_from_nl(
    'What drugs treat headaches? Use the context: ' ||
    pg_ripple.rag_context('What treats headaches?', k := 5)
);
```

### End-to-end RAG with automatic SPARQL execution

When `pg_ripple.llm_endpoint` is configured, `rag_context()` automatically calls `sparql_from_nl()` and appends the SPARQL query result:

```sql
-- Set the LLM endpoint (once per session or in postgresql.conf)
SET pg_ripple.llm_endpoint = 'https://api.openai.com/v1';
SET pg_ripple.llm_api_key_env = 'OPENAI_API_KEY';

-- rag_context now includes vector context + SPARQL result
SELECT pg_ripple.rag_context('Who are the key authors in the knowledge graph?', k := 10);
```

---

## Tuning

### Adjusting `k`

Larger `k` returns more context but increases token usage. Start with `k = 5`–`10` for most use cases.

```sql
-- Narrow context: k=3
SELECT pg_ripple.rag_context('What is aspirin?', k := 3);

-- Wide context: k=20
SELECT pg_ripple.rag_context('Give me a broad overview of drug interactions', k := 20);
```

### Embedding freshness

Context quality depends on the embeddings being up to date. Run `embed_entities()` periodically or after bulk loads:

```sql
-- Re-embed all entities in the default graph
SELECT pg_ripple.embed_entities(graph_iri := NULL, model := NULL, batch_size := 100);
```

### GUC settings

| GUC | Default | Effect |
|-----|---------|--------|
| `pg_ripple.pgvector_enabled` | `on` | Set to `off` to disable pgvector (returns empty context) |
| `pg_ripple.llm_endpoint` | `''` | When set, enables Step 5 (NL→SPARQL) |
| `pg_ripple.llm_model` | `'gpt-4o'` | LLM model name for Step 5 |

---

## Output Format

The context string has the following structure for each entity:

```
Entity: https://example.org/aspirin
Label: aspirin
Context:
{
  "label": "aspirin",
  "types": ["https://pharma.example/Drug"],
  "properties": [
    {"predicate": "...", "object": "..."}
  ],
  "neighbors": ["https://pharma.example/Ibuprofen"]
}

---

Entity: https://example.org/ibuprofen
...
```

When Step 5 executes a SPARQL query, the result is appended:

```
---

SPARQL Result for: What treats headaches?
[{"?drug": "<https://pharma.example/aspirin>"}]
```

---

## Graceful Degradation

| Condition | Behaviour |
|-----------|-----------|
| pgvector not installed | WARNING + empty string |
| `pgvector_enabled = off` | WARNING + empty string |
| Embeddings table empty | Empty string (no WARNING) |
| `llm_endpoint` not set | Steps 1–4 only; no SPARQL execution |
