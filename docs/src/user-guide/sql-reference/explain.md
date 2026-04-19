# Explain API

pg_ripple exposes two `explain_sparql` overloads and `explain_datalog` for introspecting query plans.

---

## `explain_sparql(query, format)` — text output (v0.23.0)

Returns a human-readable or structured text representation of a SPARQL query plan.

```sql
pg_ripple.explain_sparql(query TEXT, format TEXT DEFAULT 'text') RETURNS TEXT
```

**Format options:**

| Format | Description |
|--------|-------------|
| `'text'` (default) | Runs `EXPLAIN` on the generated SQL and returns the plan as text |
| `'json'` | Runs `EXPLAIN FORMAT JSON` and returns the plan as JSON text |
| `'sql'` | Returns the generated SQL without running `EXPLAIN` |
| `'sparql_algebra'` | Returns the parsed SPARQL algebra tree (Debug format) |

**Example:**

```sql
SELECT pg_ripple.explain_sparql(
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o }',
    'sql'
);
```

---

## `explain_sparql(query, analyze)` — JSONB output (v0.40.0)

Returns a machine-readable JSONB document containing the full explain pipeline.

```sql
pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN DEFAULT false) RETURNS JSONB
```

**Return structure:**

```json
{
  "algebra":   "<spargebra debug output>",
  "sql":       "<generated SQL>",
  "plan":      "<EXPLAIN [ANALYZE] output>",
  "cache_hit": true,
  "encode_calls": 0
}
```

| Field | Type | Description |
|-------|------|-------------|
| `algebra` | string | Parsed SPARQL algebra tree in Rust Debug format |
| `sql` | string | Generated SQL that will be executed |
| `plan` | string | PostgreSQL `EXPLAIN [ANALYZE]` output |
| `cache_hit` | boolean | Whether the compiled plan was served from the plan cache |
| `encode_calls` | number | Dictionary encoder invocations (0 when using plan cache) |

When `analyze = true`, `EXPLAIN ANALYZE` is run and the plan includes actual timing.

**Example:**

```sql
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE { ?s <https://schema.org/name> ?name }',
    false
) ->> 'sql';
```

---

## `explain_datalog(rule_set_name)` — JSONB output (v0.40.0)

Returns a JSONB introspection document for a named Datalog rule set.

```sql
pg_ripple.explain_datalog(rule_set_name TEXT) RETURNS JSONB
```

**Return structure:**

```json
{
  "strata": [["rule1", "rule2"], ["rule3"]],
  "rules": ["head(?x, ?y) :- body(?x, ?y) ."],
  "sql_per_rule": ["INSERT INTO ... SELECT ..."],
  "last_run_stats": [{"rule_set": "...", "derived_count": 42, "elapsed_ms": 3}]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `strata` | array of arrays | Stratification result — each inner array is one stratum |
| `rules` | array of strings | Rule text as stored in `_pg_ripple.rules` |
| `sql_per_rule` | array of strings | Compiled SQL for each rule in the same order |
| `last_run_stats` | array of objects | Statistics from the most recent `infer()` run (from `_pg_ripple.inference_stats`) |

Returns `{"strata": [], "rules": [], "sql_per_rule": [], "last_run_stats": []}` when the rule set does not exist.

**Example:**

```sql
SELECT jsonb_pretty(pg_ripple.explain_datalog('my_rules'));
```

---

## See also

- [Streaming Cursor API](cursor-api.md) — stream large result sets
- [Observability](../../reference/observability.md) — tracing and cache statistics
- [GUC Reference](../../reference/guc-reference.md) — `pg_ripple.sparql_plan_cache`, `pg_ripple.rule_plan_cache`
