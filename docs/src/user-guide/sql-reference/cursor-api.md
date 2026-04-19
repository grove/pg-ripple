# Streaming Cursor API

When processing millions of triples, materialising an entire SPARQL result set in one call can exhaust memory or hit statement-level row limits. The streaming cursor API returns results in batches via PostgreSQL `SETOF` functions.

---

## Functions

### `sparql_cursor(query TEXT) RETURNS SETOF JSONB` (v0.40.0)

Streams the output of a SPARQL `SELECT` or `ASK` query as a sequence of JSONB binding rows.

```sql
SELECT * FROM pg_ripple.sparql_cursor($$
    SELECT ?s ?label WHERE { ?s <https://schema.org/name> ?label }
$$);
```

Equivalent to `sparql()` but avoids full materialisation. Each row is a JSONB object with one key per projected variable.

### `sparql_cursor_turtle(query TEXT) RETURNS SETOF TEXT` (v0.40.0)

Streams the output of a SPARQL `CONSTRUCT` query as Turtle text chunks.

```sql
COPY (
    SELECT result FROM pg_ripple.sparql_cursor_turtle($$
        CONSTRUCT { ?s <https://schema.org/name> ?name }
        WHERE    { ?s <https://schema.org/name> ?name }
    $$)
) TO '/tmp/dump.ttl';
```

Each returned `TEXT` value is a complete, self-contained Turtle serialisation of one batch (up to 1 024 triples).

### `sparql_cursor_jsonld(query TEXT) RETURNS SETOF TEXT` (v0.40.0)

Streams the output of a SPARQL `CONSTRUCT` query as JSON-LD expanded-form chunks.

```sql
SELECT string_agg(result, E'\n')
FROM pg_ripple.sparql_cursor_jsonld($$
    CONSTRUCT { ?s ?p ?o } WHERE { GRAPH <https://my.graph/> { ?s ?p ?o } }
$$);
```

---

## Overflow control GUCs

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.sparql_max_rows` | `0` (unlimited) | Maximum rows returned by `sparql()` and `sparql_cursor()`. When exceeded, behaviour is controlled by `sparql_overflow_action`. |
| `pg_ripple.export_max_rows` | `0` (unlimited) | Maximum rows returned by Turtle/N-Triples/JSON-LD export functions. When exceeded, a PT642 WARNING is emitted and the result is truncated. |
| `pg_ripple.sparql_overflow_action` | `''` (warn) | `'warn'` — emit PT640 WARNING and truncate. `'error'` — raise PT640 ERROR. |
| `pg_ripple.datalog_max_derived` | `0` (unlimited) | Maximum derived facts produced by a single `infer()` call. When exceeded, a PT641 WARNING is emitted. |

**Example — limit a large export to 50 000 rows:**

```sql
SET pg_ripple.export_max_rows = 50000;
SELECT string_agg(result, E'\n')
FROM pg_ripple.sparql_cursor_turtle('CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }');
```

**Example — error on overflow instead of silent truncation:**

```sql
SET pg_ripple.sparql_max_rows = 100000;
SET pg_ripple.sparql_overflow_action = 'error';
SELECT * FROM pg_ripple.sparql_cursor('SELECT ?s WHERE { ?s ?p ?o }');
```

---

## Performance notes

- Batches of 1 024 rows are processed per iteration; memory footprint is `O(batch_size)`.
- For very large CONSTRUCT exports, consider using `COPY ... TO` with `sparql_cursor_turtle` to avoid buffering in the client.
- The cursor functions call the same SPARQL→SQL pipeline as `sparql()`; plan cache hits apply.

---

## See also

- [Explain API](explain.md) — introspect SPARQL query plans
- [Observability](../../reference/observability.md) — tracing and cache statistics
- [Error Reference](../../reference/error-reference.md) — PT640, PT641, PT642
