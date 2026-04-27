# Cookbook: SPARQL Repair Workflow

**Goal.** Build an iterative loop where an LLM proposes a SPARQL query, pg_ripple tells it what went wrong (parse error, missing prefix, no rows, schema mismatch), and the LLM tries again. Most "no result" cases self-heal in one or two iterations.

**Why pg_ripple.** `sparql_from_nl()` and `explain_sparql()` were designed to compose. Combine them with the [error catalog](../reference/error-catalog.md) and you get an automated SPARQL pair-programmer.

**Time to first result.** ~10 minutes.

---

## The loop

```
   user question (NL)
        │
        ▼
   sparql_from_nl(question)
        │
        ├── parse fails (PT702) ──► describe error → LLM retry
        │
        ▼
   sparql(query)
        │
        ├── exception (PT…)     ──► describe error → LLM retry
        │
        ▼
   row count == 0 ?
        │
        ├── yes ──► explain_sparql(query, analyze:=true) → LLM retry
        │
        ▼
   answer
```

Maximum iterations: 3. Anything worse than that is usually a data-quality problem and should escalate to a human.

---

## Step 1 — Configure NL → SPARQL

```sql
ALTER SYSTEM SET pg_ripple.llm_endpoint    = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.llm_api_key_env = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.llm_model       = 'gpt-4o';
ALTER SYSTEM SET pg_ripple.llm_include_shapes = on;
SELECT pg_reload_conf();
```

`llm_include_shapes = on` ships your active SHACL shapes with every prompt — the LLM sees the schema and produces queries that respect it.

## Step 2 — Add few-shot examples for your vocabulary

```sql
SELECT pg_ripple.add_llm_example(
    'List all proteins that interact with insulin',
    'PREFIX bio: <https://bio.example/>
     SELECT ?p WHERE {
       ?p a bio:Protein ; bio:interactsWith bio:Insulin .
     }'
);
```

Five to ten examples per domain is usually enough.

## Step 3 — The repair function

```sql
CREATE OR REPLACE FUNCTION sparql_repair(question TEXT, max_attempts INT DEFAULT 3)
RETURNS TABLE (attempt INT, query TEXT, status TEXT, rows JSONB) AS $$
DECLARE
    q TEXT;
    feedback TEXT := '';
    n INT := 0;
    err TEXT;
    row_count INT;
    out_rows JSONB;
BEGIN
    LOOP
        n := n + 1;
        EXIT WHEN n > max_attempts;

        -- 1. Generate a query.
        BEGIN
            q := pg_ripple.sparql_from_nl(question || E'\n' || feedback);
        EXCEPTION WHEN OTHERS THEN
            feedback := format('Previous attempt failed to generate SPARQL: %s. Try again.', SQLERRM);
            CONTINUE;
        END;

        -- 2. Try to execute it.
        BEGIN
            SELECT count(*), jsonb_agg(s) INTO row_count, out_rows
            FROM   pg_ripple.sparql(q) s;
        EXCEPTION WHEN OTHERS THEN
            err := SQLERRM;
            attempt := n; query := q; status := 'parse_or_runtime_error: ' || err;
            rows := NULL;
            RETURN NEXT;
            feedback := format('Previous SPARQL caused error: %s. The query was: %s', err, q);
            CONTINUE;
        END;

        -- 3. If empty, ask the LLM to broaden.
        IF row_count = 0 THEN
            attempt := n; query := q; status := 'empty_result_set'; rows := NULL;
            RETURN NEXT;
            feedback := format(
                'Previous query returned zero rows: %s. Loosen FILTERs or remove restrictive prefixes.',
                q
            );
            CONTINUE;
        END IF;

        -- 4. 
        attempt := n; query := q; status := 'ok'; rows := out_rows;
        RETURN NEXT;
        RETURN;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

## Step 4 — Use it

```sql
SELECT * FROM sparql_repair('Which proteins interact with the gene encoding insulin?');
```

Output:

```
attempt | query                                           | status                 | rows
--------+-------------------------------------------------+------------------------+-----
1       | SELECT ?p WHERE { ?p bio:interactsWith ...      | parse_or_runtime_error | NULL
2       | PREFIX bio: <https://bio.example/> SELECT ...   | empty_result_set       | NULL
3       | PREFIX bio: <https://bio.example/> SELECT ...   | ok                     | [...]
```

Three attempts, automated. The user gets the right answer; the audit log captures every attempted query for later analysis.

---

## Telemetry

Wire `sparql_repair()` into your application telemetry:

- **Average attempts per question** — > 2 means you need more few-shot examples.
- **Most common error code** — likely a vocabulary gap.
- **Retry depth** — > 3 means the LLM is going in circles; escalate to a human.

---

## Hardening

- **Set `pg_ripple.sparql_max_algebra_depth`** to reject pathological generated queries before they execute.
- **Set `pg_ripple.sparql_query_timeout`** so a bad generation cannot wedge the database.
- **Cache successful (question → query) pairs** — the LLM does not need to be called for repeats.

---

## See also

- [NL → SPARQL](../features/nl-to-sparql.md)
- [SPARQL Query Debugger — explain_sparql](../user-guide/explain-sparql.md)
- [Error catalog](../reference/error-catalog.md)
