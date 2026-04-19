# Observability

pg_ripple exposes cache statistics, plan introspection, and an optional OpenTelemetry tracing facade.

---

## Cache statistics

```sql
pg_ripple.cache_stats() RETURNS JSONB
```

Returns a JSONB document with statistics for all internal caches (added in v0.40.0, replacing the earlier `plan_cache_stats()` + `dict_cache_stats()` pair):

```json
{
  "plan_cache": {
    "hits":    42,
    "misses":  7,
    "entries": 12
  },
  "dict_cache": {
    "hits":      100345,
    "misses":    1023,
    "evictions": 5
  },
  "federation_cache": {
    "entries": 3
  }
}
```

**Reset statistics:**

```sql
SELECT pg_ripple.reset_cache_stats();
```

Resets `dict_cache` hit/miss/eviction counters to zero. Plan cache entries are not evicted; use `SELECT pg_ripple.plan_cache_evict()` for that.

---

## pg_stat_statements integration

When `pg_stat_statements` is installed, the view `pg_ripple.stat_statements_decoded` exposes the query text alongside statement statistics:

```sql
SELECT query_decoded, calls, total_exec_time
FROM   pg_ripple.stat_statements_decoded
ORDER  BY total_exec_time DESC
LIMIT  10;
```

The view is created automatically if `pg_stat_statements` is available in the search path.

---

## OpenTelemetry tracing (v0.40.0)

pg_ripple includes a lightweight tracing facade. When enabled, each SPARQL parse/translate/execute cycle, merge worker iteration, federation call, and Datalog inference step emits a span.

### GUCs

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.tracing_enabled` | `off` | Master on/off switch. When off, the tracing facade is a no-op with zero overhead. |
| `pg_ripple.tracing_exporter` | `''` (stdout) | Exporter backend: `'stdout'` writes spans as JSON lines to the PostgreSQL log at `DEBUG5`. |

### Enable tracing

```sql
SET pg_ripple.tracing_enabled = on;
SET pg_ripple.tracing_exporter = 'stdout';
SET client_min_messages = debug5;

-- Run a query — spans appear in the log.
SELECT * FROM pg_ripple.sparql('SELECT ?s WHERE { ?s ?p ?o } LIMIT 1');
```

### Span format (stdout exporter)

Each span is emitted as a single `DEBUG5` log line:

```
[pg_ripple span] name="sparql.execute" elapsed_us=1234
```

### Performance

The tracing facade adds **zero overhead** when `tracing_enabled = off`. When enabled with the `stdout` exporter, each span adds a `pgrx::log!` call — negligible for most workloads.

---

## See also

- [Explain API](../sql-reference/explain.md) — plan introspection
- [Streaming Cursor API](../sql-reference/cursor-api.md) — overflow control
- [GUC Reference](guc-reference.md) — full GUC listing
- [Error Reference](error-reference.md) — PT640–PT642 overflow errors
