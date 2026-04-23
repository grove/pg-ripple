# Natural Language to SPARQL

pg_ripple v0.49.0 adds `pg_ripple.sparql_from_nl()`, a SQL function that converts a plain-English question into a SPARQL SELECT query using any configured OpenAI-compatible LLM endpoint.

## Quick Start

```sql
-- Configure an OpenAI-compatible endpoint
SET pg_ripple.llm_endpoint = 'https://api.openai.com/v1';
SET pg_ripple.llm_model    = 'gpt-4o';
-- API key is read from the named environment variable, not stored inline:
SET pg_ripple.llm_api_key_env = 'OPENAI_API_KEY';  -- default

-- Generate a SPARQL query from plain English
SELECT pg_ripple.sparql_from_nl('List all people and their email addresses');
```

```
SELECT ?person ?email WHERE {
  ?person a <http://xmlns.com/foaf/0.1/Person> .
  ?person <http://xmlns.com/foaf/0.1/mbox> ?email .
}
```

The returned string is a valid, parseable SPARQL 1.1 query that you can pass directly to `pg_ripple.sparql()`.

## Configuring the LLM Endpoint

pg_ripple supports any OpenAI-compatible `/v1/chat/completions` API, including:

| Provider | Example `llm_endpoint` |
|----------|------------------------|
| OpenAI | `https://api.openai.com/v1` |
| Azure OpenAI | `https://<resource>.openai.azure.com/openai/deployments/<deployment>` |
| Ollama (local) | `http://localhost:11434/v1` |
| vLLM | `http://localhost:8000/v1` |
| Together AI | `https://api.together.xyz/v1` |

### GUC Parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.llm_endpoint` | string | `''` (disabled) | Base URL for the OpenAI-compatible API. Set to `'mock'` for testing without a real LLM. |
| `pg_ripple.llm_model` | string | `gpt-4o` | Model identifier passed in the request body. |
| `pg_ripple.llm_api_key_env` | string | `PG_RIPPLE_LLM_API_KEY` | Name of the environment variable holding the API key. The key is never stored in the database. |
| `pg_ripple.llm_include_shapes` | bool | `on` | When `on`, active SHACL shapes are included in the LLM prompt as schema context. |

### Setting the API Key Securely

pg_ripple never stores the API key in the database. Instead, it reads the value from a named environment variable at call time:

```bash
# In your shell or service environment:
export PG_RIPPLE_LLM_API_KEY="sk-..."
```

```sql
-- Tell pg_ripple which environment variable to read (default is PG_RIPPLE_LLM_API_KEY):
ALTER SYSTEM SET pg_ripple.llm_api_key_env = 'PG_RIPPLE_LLM_API_KEY';
SELECT pg_reload_conf();
```

## How It Works

For each call to `sparql_from_nl(question)`:

1. **VoID context**: pg_ripple builds a compact description of the graph — the predicate count and the most-frequent predicates — as context for the LLM.
2. **SHACL context** (when `llm_include_shapes = on`): active SHACL shapes are appended to the prompt.
3. **Few-shot examples**: any rows in `_pg_ripple.llm_examples` are included as question/SPARQL pairs.
4. **LLM call**: the prompt is sent to `/v1/chat/completions` with `temperature = 0.0`.
5. **Extraction**: the SPARQL string is extracted from the response and stripped of any markdown fencing.
6. **Validation**: `spargebra` parses the query. If parsing fails, PT702 is raised so callers can handle the error.

## Adding Few-Shot Examples

Few-shot examples improve accuracy significantly for domain-specific vocabularies:

```sql
SELECT pg_ripple.add_llm_example(
    'Find all proteins that interact with insulin',
    'SELECT ?protein WHERE {
       ?protein <https://bio.ontology.org/interactsWith>
                <https://bio.ontology.org/Insulin> .
     }'
);

SELECT pg_ripple.add_llm_example(
    'Which drugs target EGFR?',
    'SELECT ?drug WHERE {
       ?drug <https://bio.ontology.org/targets>
             <https://bio.ontology.org/EGFR> .
     }'
);
```

Examples are stored in `_pg_ripple.llm_examples` and automatically included in every subsequent `sparql_from_nl()` call. Re-calling `add_llm_example()` with the same question updates the stored example (upsert behaviour).

## Testing Without a Real LLM

Set `pg_ripple.llm_endpoint = 'mock'` to use the built-in test mock. The mock bypasses the HTTP call and returns a simple `SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10` query, allowing you to test downstream code (result processing, SPARQL execution) without an external LLM dependency.

```sql
SET pg_ripple.llm_endpoint = 'mock';
SELECT pg_ripple.sparql_from_nl('anything') LIKE 'SELECT%';  -- t
```

## Error Handling

| Code | Condition | Remedy |
|------|-----------|--------|
| PT700 | `llm_endpoint` is empty or the HTTP call fails | Set a valid endpoint URL; check network access and API key |
| PT701 | The LLM response did not contain a SPARQL query | Improve the prompt with few-shot examples; switch to a more capable model |
| PT702 | The generated SPARQL could not be parsed | Add a few-shot example for this question pattern; or use a model fine-tuned for SPARQL |

## Pipeline Pattern

A common pattern is to generate a query, log it, and execute it in one step:

```sql
DO $$
DECLARE
    sparql_q TEXT;
    result   TEXT;
BEGIN
    sparql_q := pg_ripple.sparql_from_nl(
        'Find all companies founded after 2010 with more than 500 employees'
    );
    RAISE NOTICE 'Generated SPARQL: %', sparql_q;
    -- Execute the generated query
    SELECT json_agg(row_to_json(t))::text
    INTO result
    FROM pg_ripple.sparql(sparql_q) t;
    RAISE NOTICE 'Results: %', result;
END;
$$;
```
