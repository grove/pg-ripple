# SPARQL Query Debugger — EXPLAIN SPARQL

pg_ripple v0.50.0 extends `pg_ripple.explain_sparql()` with an interactive query debugger mode (`analyze := true`) that surfaces the algebra tree, generated SQL, plan-cache status, and per-operator row counts as a structured JSONB document.

---

## Function Signature

```sql
pg_ripple.explain_sparql(
    query       TEXT,
    analyze     BOOL DEFAULT FALSE
) RETURNS JSONB
```

---

## Output Schema

| Key | Type | Description |
|-----|------|-------------|
| `algebra` | text | spargebra algebra tree (string representation) |
| `sql` | text | Generated SQL sent to PostgreSQL |
| `plan` | text | PostgreSQL EXPLAIN output (JSON format) |
| `cache_status` | text | `"hit"`, `"miss"`, or `"bypass"` |
| `cache_hit` | bool | Legacy alias for `cache_status = "hit"` |
| `actual_rows` | array | Per-operator actual row counts (only when `analyze = true`) |
| `topn_applied` | bool | Whether TopN push-down optimisation was applied |
| `encode_calls` | int | Dictionary encode calls during translation |

### `cache_status` values

| Value | Meaning |
|-------|---------|
| `"hit"` | SQL was served from the per-backend plan cache |
| `"miss"` | SQL was freshly compiled; query not yet cached |
| `"bypass"` | Plan caching is disabled (`pg_ripple.plan_cache_size = 0`) |

### `actual_rows`

Only present when `analyze := true`. Contains a flat array of actual row counts extracted from PostgreSQL's `EXPLAIN ANALYZE` JSON output — one integer per plan node, in plan-tree order.

---

## Examples

### Basic explain (no execution)

```sql
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE { ?s <http://schema.org/name> ?name }',
    false
) AS explain_out;
```

### Interactive debugger (with row counts)

```sql
WITH result AS (
    SELECT pg_ripple.explain_sparql(
        'SELECT ?name WHERE { ?s <http://schema.org/name> ?name }',
        true
    ) AS j
)
SELECT
    j->>'cache_status'  AS cache_status,
    j->>'sql'           AS generated_sql,
    j->'actual_rows'    AS actual_rows_per_operator
FROM result;
```

### All four query types

```sql
-- SELECT
SELECT pg_ripple.explain_sparql('SELECT * WHERE { ?s ?p ?o } LIMIT 5', true);

-- ASK
SELECT pg_ripple.explain_sparql('ASK { <http://example.org/a> ?p ?o }', true);

-- CONSTRUCT
SELECT pg_ripple.explain_sparql(
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 5', true);

-- DESCRIBE (returns algebra + synthetic SQL stub; no error)
SELECT pg_ripple.explain_sparql('DESCRIBE <http://example.org/a>', false);
```

---

## Interpreting the Algebra Tree

The `algebra` field is the string representation of the internal `spargebra` algebra tree. It shows how the SPARQL engine parsed and structured your query before SQL generation. Common nodes:

- `Project` — variable projection
- `Filter` — FILTER expression
- `Join` / `LeftJoin` — triple-pattern joins / OPTIONAL
- `BGP` — basic graph pattern (list of triple patterns)
- `Union` — UNION clause
- `Extend` — BIND expression

---

## Tuning with EXPLAIN ANALYZE

Use `actual_rows` to compare estimated vs actual row counts:

```sql
WITH debug AS (
    SELECT pg_ripple.explain_sparql(
        'SELECT ?s WHERE { ?s <http://schema.org/type> <http://schema.org/Person> }',
        true
    ) AS j
)
SELECT
    jsonb_array_length(j->'actual_rows') AS num_plan_nodes,
    j->'actual_rows'                      AS row_counts
FROM debug;
```

If row estimates are far from actuals, run `ANALYZE` on the VP tables:

```sql
ANALYZE _pg_ripple.vp_rare;
```

---

## Configuration

| GUC | Default | Effect on explain |
|-----|---------|-------------------|
| `pg_ripple.plan_cache_size` | `256` | Set to `0` to force `cache_status = "bypass"` |
| `pg_ripple.max_path_depth` | `10` | Affects property-path SQL depth; changes cache key |
