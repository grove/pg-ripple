[← Back to Blog Index](README.md)

# Natural Language to SPARQL

## Ask your knowledge graph a question in English and get a real query back

---

SPARQL is powerful. It's also a language that nobody outside the Semantic Web community writes fluently. Your knowledge graph has the answers. Your users don't speak SPARQL.

pg_ripple bridges this gap with `sparql_from_nl()` — a SQL function that sends a natural language question to an LLM and returns a valid SPARQL query, informed by your graph's actual schema.

---

## The Function

```sql
SELECT pg_ripple.sparql_from_nl(
  'Which employees in the engineering department were hired after 2023?'
);
```

Returns:

```sparql
SELECT ?employee ?name ?hireDate WHERE {
  ?employee rdf:type ex:Employee ;
            ex:department ex:Engineering ;
            foaf:name ?name ;
            ex:hireDate ?hireDate .
  FILTER(?hireDate > "2023-01-01"^^xsd:date)
}
ORDER BY ?hireDate
```

That's a single function call. The returned SPARQL query can be passed directly to `pg_ripple.sparql()` for execution.

---

## How It Works

`sparql_from_nl()` doesn't just send your question to an LLM and hope. It constructs a context-aware prompt:

### 1. Schema Extraction

The function reads your graph's actual predicates, classes, and SHACL shapes from the catalog:

```
Available predicates: ex:department, ex:hireDate, foaf:name, ex:salary, ...
Available classes: ex:Employee, ex:Department, ex:Project, ...
SHACL constraints: ex:Employee must have exactly 1 foaf:name (xsd:string),
                   ex:Employee must have exactly 1 ex:hireDate (xsd:date), ...
```

This schema context tells the LLM what predicates and classes actually exist in the graph. Without it, the LLM would guess predicate names — and guess wrong.

### 2. Few-Shot Examples

pg_ripple maintains a table of example question→query pairs:

```sql
-- Add domain-specific examples
SELECT pg_ripple.llm_add_example(
  question => 'Find all managers',
  query    => 'SELECT ?m ?name WHERE { ?m rdf:type ex:Manager ; foaf:name ?name . }'
);
```

These examples are included in the prompt as demonstrations. Three to five good examples dramatically improve the LLM's accuracy for domain-specific queries.

### 3. Prompt Construction

The prompt combines the user's question, the schema context, the few-shot examples, and instructions specific to pg_ripple's SPARQL dialect (e.g., using `pg:similar()` for vector search, named graph syntax for multi-graph queries).

### 4. LLM Call

The assembled prompt is sent to the configured endpoint:

```sql
-- Configure the LLM endpoint
SET pg_ripple.llm_endpoint = 'https://api.openai.com/v1/chat/completions';
SET pg_ripple.llm_model = 'gpt-4o';
SET pg_ripple.llm_api_key = 'sk-...';
```

Any OpenAI-compatible API works: OpenAI, Azure OpenAI, Ollama, vLLM, LiteLLM.

### 5. Validation

The returned SPARQL is parsed with `spargebra` before being returned. If parsing fails, the function retries with the error message appended to the prompt (the auto-repair loop). If it still fails after 3 attempts, it returns an error rather than an invalid query.

---

## The Mock Endpoint

For testing and CI, `sparql_from_nl()` supports a mock mode:

```sql
SET pg_ripple.llm_endpoint = 'mock';
```

In mock mode, the function returns a deterministic SPARQL query generated from pattern matching on the question text. No network call, no API key needed. This lets you test the NL→SPARQL pipeline in CI without LLM dependencies.

---

## Auto-Repair

What happens when the LLM generates invalid SPARQL? The `repair_sparql()` function closes the loop:

```sql
-- Try to execute a generated query
BEGIN;
  SELECT pg_ripple.sparql(generated_query);
EXCEPTION WHEN OTHERS THEN
  -- Auto-repair: send the error back to the LLM
  SELECT pg_ripple.repair_sparql(
    query         => generated_query,
    error_message => SQLERRM
  );
END;
```

`repair_sparql()` sends the broken query and the error message to the LLM with instructions to fix it. Common fixes: wrong predicate names (the LLM guessed), missing prefix declarations, incorrect filter syntax.

Input sanitization is applied: null bytes are rejected, queries are capped at 32 KiB, and prompt-injection markers are stripped. The LLM is a tool, not a trusted input source.

---

## When NL-to-SPARQL Works Well

- **Domain-specific graphs with consistent naming.** If your predicates are `ex:hireDate`, `ex:department`, `foaf:name`, the LLM maps natural language to these predicates reliably.
- **Simple to moderate queries.** SELECT with filters, basic joins, ORDER BY, LIMIT. The LLM handles these well.
- **Good few-shot examples.** Five well-chosen examples cover most query patterns your users will ask.

## When It Doesn't

- **Complex aggregations.** "What's the average salary by department, excluding the top 5% earners?" requires nested subqueries that LLMs generate incorrectly ~40% of the time.
- **Property paths.** "Find all ancestors of this category" requires `skos:broader+` syntax that LLMs sometimes get wrong.
- **Ambiguous questions.** "Show me recent changes" could mean new triples, updated triples, or CDC events. Without context, the LLM guesses.

For complex queries, `sparql_from_nl()` is a starting point, not a final answer. Generate the query, review it, refine it. It's still faster than writing SPARQL from scratch if you're not fluent.

---

## The Bigger Picture

`sparql_from_nl()` is one piece of a larger NL→answer pipeline:

```
User question
    → sparql_from_nl()     → SPARQL query
    → sparql()             → result set
    → rag_context()        → structured context
    → LLM                  → natural language answer
```

The knowledge graph provides the factual grounding. The LLM provides the natural language interface. pg_ripple connects them with SQL functions that any application can call.
