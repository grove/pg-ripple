# Observability Reference

This page is the reference for pg_ripple's observability and monitoring features.

## Overview

pg_ripple exposes monitoring data via:
- A Prometheus metrics endpoint (`/metrics`) in the `pg_ripple_http` companion service
- OpenTelemetry (OTLP) distributed tracing
- `pg_stat_statements` integration for query-level statistics
- `explain_sparql()` with `analyze := true` for interactive query debugging
- `explain_datalog()` for Datalog inference plan visualization
- `explain_inference()` for derivation tree inspection

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE '%observ%' OR feature_name LIKE '%explain%' OR feature_name LIKE '%monitor%';
```

## Prometheus Metrics

The `pg_ripple_http` companion service exposes `/metrics` in Prometheus text format. Key metrics include:

| Metric | Description |
|---|---|
| `pg_ripple_sparql_queries_total` | Total SPARQL queries executed |
| `pg_ripple_sparql_query_duration_seconds` | Histogram of query durations |
| `pg_ripple_triple_count` | Total triples per graph |
| `pg_ripple_merge_operations_total` | Background merge operations |
| `pg_ripple_dictionary_cache_hits_total` | Dictionary LRU cache hit rate |
| `pg_ripple_construct_rule_firings_total` | CONSTRUCT writeback rule invocations |
| `pg_ripple_datalog_materialize_duration_seconds` | Datalog inference wall time |

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN) → TEXT` | JSON explain plan for a SPARQL query |
| `pg_ripple.explain_datalog(rule_set TEXT) → TEXT` | Execution plan for Datalog rule set |
| `pg_ripple.explain_inference(rule_set TEXT, triple TEXT) → TEXT` | Derivation tree for a specific triple |
| `pg_ripple.feature_status() → SETOF record` | Current status of all implemented features |

## OpenTelemetry Tracing

When `pg_ripple.otlp_endpoint` is set, spans are exported for each SPARQL
query execution. The `traceparent` header from `pg_ripple_http` is propagated
to the extension for end-to-end traces.

## Query Debugging

`explain_sparql(query, analyze := true)` executes the query and returns a JSON
object containing:
- The SPARQL algebra tree (post-optimization)
- The generated SQL plan (from `EXPLAIN ANALYZE`)
- Per-step row counts and actual vs. estimated rows
- Total wall time

## Related Pages

- [Audit Log](audit-log.md)
- [GUC Reference](guc-reference.md)
- [HTTP API Reference](http-api.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
