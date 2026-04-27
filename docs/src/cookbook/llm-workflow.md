# Cookbook: LLM Workflow — Natural Language to Knowledge Graph Answer

**Goal.** Build a complete end-to-end workflow where a user asks a question in plain English, an LLM generates a SPARQL query, the query runs against your knowledge graph, and the results are returned to the user as a fluent natural-language answer.

**Why pg_ripple.** All four steps run inside one SQL call chain — from NL question to SPARQL to result to summarised answer. No separate orchestration layer required.

**Time to first result.** ~10 minutes.

---

## The four-step workflow

```
   User question (NL)
          │
          ▼
   sparql_from_nl()   ← LLM generates SPARQL from your schema + few-shot examples
          │
          ▼
   sparql()           ← execute the SPARQL against the VP tables
          │
          ▼
   sparql_construct_turtle()  ← optionally fetch the context subgraph as Turtle
          │
          ▼
   LLM summarises results into a fluent answer
```

pg_ripple handles steps 1–3 natively. Step 4 is your LLM call.

---

## Step 1 — Set up the LLM endpoint

```sql
ALTER SYSTEM SET pg_ripple.llm_endpoint    = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.llm_api_key_env = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.llm_model       = 'gpt-4o';

-- (Optional) embed entities for vector-assisted SPARQL generation.
ALTER SYSTEM SET pg_ripple.embedding_api_url     = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key_env = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.embedding_model       = 'text-embedding-3-small';

SELECT pg_reload_conf();
```

## Step 2 — Load a sample graph

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:     <https://example.org/> .
@prefix schema: <https://schema.org/> .

ex:Alice  a schema:Person ;
    schema:name     "Alice Smith" ;
    schema:jobTitle "Senior Engineer" ;
    schema:knows    ex:Bob .

ex:Bob  a schema:Person ;
    schema:name     "Bob Jones" ;
    schema:jobTitle "Data Scientist" ;
    schema:knows    ex:Carol .

ex:Carol  a schema:Person ;
    schema:name     "Carol Kim" ;
    schema:jobTitle "VP Engineering" .
$TTL$);
```

## Step 3 — Add few-shot examples (recommended)

Domain-specific examples dramatically improve `sparql_from_nl` accuracy:

```sql
SELECT pg_ripple.add_llm_example(
    'Who does Alice know?',
    'PREFIX ex: <https://example.org/>
     PREFIX schema: <https://schema.org/>
     SELECT ?name WHERE {
         ex:Alice schema:knows ?p .
         ?p schema:name ?name .
     }'
);

SELECT pg_ripple.add_llm_example(
    'What is Bob''s job title?',
    'PREFIX ex: <https://example.org/>
     PREFIX schema: <https://schema.org/>
     SELECT ?title WHERE {
         ex:Bob schema:jobTitle ?title .
     }'
);
```

## Step 4 — Generate and run a SPARQL query

```sql
-- Generate SPARQL from a natural-language question.
SELECT pg_ripple.sparql_from_nl('Who does Bob know?') AS generated_sparql;
```

Returns:
```sparql
PREFIX ex:     <https://example.org/>
PREFIX schema: <https://schema.org/>
SELECT ?name WHERE {
    ex:Bob schema:knows ?person .
    ?person schema:name ?name .
}
```

Execute it:
```sql
SELECT result->>'name' AS person_name
FROM pg_ripple.sparql(
    pg_ripple.sparql_from_nl('Who does Bob know?')
);
```

## Step 5 — Fetch the relevant subgraph as context

For richer answers, pass the whole subgraph to the LLM rather than just the result rows:

```sql
SELECT pg_ripple.sparql_construct_turtle($Q$
    CONSTRUCT { ?s ?p ?o }
    WHERE {
        ?s ?p ?o .
        FILTER(?s IN (<https://example.org/Alice>, <https://example.org/Bob>))
    }
$Q$) AS context_turtle;
```

The Turtle block goes into the LLM prompt alongside the SPARQL result rows and the user question.

## Step 6 — Combine with hybrid vector search

For questions where the relevant entity cannot be named precisely ("who works on machine learning"), combine the NL→SPARQL path with a vector similarity pre-filter:

```sql
-- Embed all entities first (once per load cycle).
SELECT pg_ripple.embed_entities();

-- Hybrid: find similar entities, then expand via SPARQL.
SELECT * FROM pg_ripple.sparql($$
    PREFIX schema: <https://schema.org/>
    SELECT ?name ?title WHERE {
        ?p schema:name ?name ;
           schema:jobTitle ?title .
        FILTER(?p IN (
            SELECT s FROM pg_ripple.similar_entities('machine learning engineer', k := 5)
        ))
    }
$$);
```

## Step 7 — A complete Python application

```python
import psycopg, openai

DB_URL = "postgresql://..."
client = openai.OpenAI()

SYSTEM_PROMPT = """
You are a helpful assistant. You have been given the result of a SPARQL query against
a knowledge graph, as well as the subgraph context as Turtle. Answer the user's
question concisely using only the provided data. If the data does not contain the
answer, say so.
""".strip()

def answer_question(question: str) -> str:
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()

        # Step A: generate SPARQL and run it.
        cur.execute(
            "SELECT jsonb_agg(row_to_json(r)) FROM pg_ripple.sparql(pg_ripple.sparql_from_nl(%s)) r",
            (question,)
        )
        sparql_rows = cur.fetchone()[0] or []

        # Step B: fetch the context subgraph (optional but improves answer quality).
        # For simplicity we pull the 1-hop neighbourhood of every ?s in the result.
        turtle_context = ""
        if sparql_rows:
            subjects = [row.get("s") or row.get("p") or "" for row in sparql_rows if row]
            if subjects:
                subj_filter = ", ".join(f"<{s}>" for s in subjects if s.startswith("http"))
                if subj_filter:
                    cur.execute(
                        "SELECT pg_ripple.sparql_construct_turtle("
                        "  'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o . FILTER(?s IN (" + subj_filter + ")) }'"
                        ")"
                    )
                    turtle_context = cur.fetchone()[0] or ""

    prompt = (
        f"{SYSTEM_PROMPT}\n\n"
        f"=== SPARQL RESULT ===\n{sparql_rows}\n\n"
        f"=== GRAPH CONTEXT (Turtle) ===\n{turtle_context}\n\n"
        f"=== QUESTION ===\n{question}"
    )
    resp = client.chat.completions.create(
        model="gpt-4o",
        messages=[{"role": "user", "content": prompt}],
        temperature=0.0,
    )
    return resp.choices[0].message.content

if __name__ == "__main__":
    print(answer_question("Who does Bob know and what are their job titles?"))
```

---

## When to use each approach

| Approach | Best for |
|---|---|
| `sparql_from_nl()` alone | Precise factual questions ("how many", "list all") |
| `rag_context()` alone | Open-ended, multi-hop questions ("tell me about X") |
| Both combined (this recipe) | Production agents that need both precision and breadth |
| `sparql_from_nl()` + Turtle context | Scenarios where the answer needs rich narrative explanation |

---

## See also

- [NL → SPARQL](../features/nl-to-sparql.md) — `sparql_from_nl` deep dive and tuning guide.
- [AI Overview](../features/ai-overview.md) — decision tree for all AI features.
- [Cookbook: Grounded Chatbot](grounded-chatbot.md) — simpler single-pass variant.
- [Cookbook: SPARQL Repair Workflow](sparql-repair.md) — what to do when NL→SPARQL fails.
- [AI Agent Integration](../features/ai-agent-integration.md) — LangChain / LlamaIndex wiring.
