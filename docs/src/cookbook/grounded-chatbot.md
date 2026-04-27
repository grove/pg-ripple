# Cookbook: Chatbot Grounded in a Knowledge Graph

**Goal.** Build an LLM-powered question-answering assistant that uses **your knowledge graph** as the source of truth, not the LLM's training data. Hallucinations drop dramatically; every answer is traceable to real triples.

**Why pg_ripple.** A single SQL function call (`rag_context()`) returns an LLM-ready prompt that fuses vector recall, graph expansion, and (optionally) executed SPARQL. No vector store to keep in sync, no orchestration framework to deploy.

**Time to first result.** ~10 minutes.

---

## The pipeline

```
   user question                                   LLM response
        │                                                ▲
        ▼                                                │
   ┌─────────────────────────────────────────────────────┴───────┐
   │  application code (Python / TS / Go)                        │
   │      ┌─────────────────────────────────────────────────┐    │
   │      │  context = pg_ripple.rag_context(question, k=8) │    │
   │      │  prompt  = SYSTEM_PROMPT + context + question   │    │
   │      │  answer  = openai.chat(prompt)                  │    │
   │      └─────────────────────────────────────────────────┘    │
   └─────────────────────────────────────────────────────────────┘
                                                            ▲
                                                            │
                                                       LLM API
```

`rag_context()` does the four things every RAG pipeline must do, all inside one PostgreSQL transaction:

1. Embed the user question (HTTP call to your embedding endpoint).
2. HNSW cosine search to retrieve the top-*k* matching entities.
3. SPARQL graph expansion to gather each entity's 1-hop neighbourhood.
4. Assemble the context as JSON-LD or plain text.

---

## Step 1 — Configure the LLM and embedding endpoints

```sql
ALTER SYSTEM SET pg_ripple.embedding_api_url     = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key_env = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.embedding_model       = 'text-embedding-3-small';

ALTER SYSTEM SET pg_ripple.llm_endpoint          = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.llm_api_key_env       = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.llm_model             = 'gpt-4o';

SELECT pg_reload_conf();
```

API keys are read from environment variables at call time and are never stored in the database.

## Step 2 — Load and embed your knowledge graph

```sql
-- Load whatever you have. Could be a Turtle file, an R2RML mapping, GraphRAG output, …
SELECT pg_ripple.load_turtle_file('/data/medical_kb.ttl');

-- (Recommended) materialise OWL RL inference so 'subClassOf' chains are visible.
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');

-- Embed every labelled entity. Run once after each big load.
SELECT pg_ripple.embed_entities();
```

For very large graphs, set `pg_ripple.use_graph_context = on` so each entity's embedding input includes its 1-hop neighbours — recall jumps significantly on entities whose labels alone are ambiguous.

## Step 3 — Retrieve context for a question

```sql
SELECT pg_ripple.rag_context(
    question := 'What drugs treat moderate hypertension?',
    k        := 8
);
```

Returns a single TEXT block, ready to drop into a prompt:

```
You are answering using the following knowledge graph context:

ENTITY: <https://example.org/lisinopril>
  type: Drug, ACEInhibitor
  rdfs:label: "Lisinopril"
  ex:treats: Hypertension, HeartFailure
  ex:contraindication: Pregnancy

ENTITY: <https://example.org/amlodipine>
  type: Drug, CalciumChannelBlocker
  ...
```

## Step 4 — Combine with NL→SPARQL for fact-style answers

For questions where the answer is a small, well-defined set ("how many", "list the", "who is"), have the LLM also generate a SPARQL query that extracts the precise answer:

```sql
SELECT pg_ripple.sparql_from_nl(
    'How many drugs in the knowledge graph treat hypertension?'
);
-- Returns a parsed, validated SPARQL string.

-- Then execute it.
SELECT * FROM pg_ripple.sparql(
    pg_ripple.sparql_from_nl(
        'How many drugs in the knowledge graph treat hypertension?'
    )
);
```

When `pg_ripple.llm_endpoint` is set, `rag_context()` does this automatically: the assembled context includes both the vector-retrieved neighbourhood **and** the result rows of the auto-generated SPARQL query.

## Step 5 — Wire it into your application

```python
import psycopg
import openai

SYSTEM_PROMPT = """
You are a medical information assistant. Answer ONLY using the
knowledge graph context provided. If the answer is not in the
context, say "I do not have that information." Cite IRIs of
entities you reference.
""".strip()

def answer(question: str) -> str:
    with psycopg.connect("...") as conn:
        cur = conn.cursor()
        cur.execute("SELECT pg_ripple.rag_context(%s, 8)", (question,))
        context = cur.fetchone()[0]
    prompt = f"{SYSTEM_PROMPT}\n\n=== CONTEXT ===\n{context}\n\n=== QUESTION ===\n{question}"
    resp = openai.chat.completions.create(
        model="gpt-4o",
        messages=[{"role": "user", "content": prompt}],
        temperature=0.0,
    )
    return resp.choices[0].message.content
```

---

## Tuning

| Lever | What it does | When to change |
|---|---|---|
| `k` (rag_context) | Number of entities included | More entities = richer context, more tokens |
| `pg_ripple.use_graph_context` | Embed entities with their neighbourhood | Improves recall for ambiguous labels |
| `pg_ripple.llm_include_shapes` | Include SHACL shapes in NL→SPARQL prompt | Improves query accuracy on schemas with many predicates |
| Few-shot examples | Add via `pg_ripple.add_llm_example()` | Domain-specific vocabularies need 5–10 examples |

---

## Why this beats a separate vector DB

- **One transaction.** Loading new triples and updating embeddings happens atomically. There is no half-updated vector store after a crash.
- **No drift.** A separate vector DB has its own schema; over months, the two diverge. Here there is one schema, one source of truth.
- **Multi-tenant by default.** Apply [graph RLS](../features/multi-tenant-graphs.md) and the same `rag_context()` call returns tenant-scoped context with no application-level filter.
- **Audit-ready.** Enable [`audit_log_enabled`](../reference/audit-log.md) and every RAG-time UPDATE is captured. A separate vector DB cannot do that.

---

## See also

- [AI Overview](../features/ai-overview.md)
- [RAG Pipeline reference](../user-guide/rag-pipeline.md)
- [NL → SPARQL](../features/nl-to-sparql.md)
