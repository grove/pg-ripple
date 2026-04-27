# OpenTelemetry Observability Guide

pg_ripple emits OpenTelemetry spans for every SPARQL query, Datalog inference
run, SHACL validation, and merge cycle. This page documents the span names,
attributes, and example Prometheus/Grafana queries.

## Distributed Tracing (v0.61.0+)

Starting with v0.61.0, the `pg_ripple_http` HTTP service extracts the
[W3C `traceparent`](https://www.w3.org/TR/trace-context/) header from
incoming requests and forwards it through the `pg_ripple.tracing_traceparent`
session GUC into the extension. Every span emitted during that request is
tagged with the originating trace ID, giving an unbroken trace from the load
balancer through the HTTP service into the query engine.

### Enabling traceparent propagation

The `pg_ripple_http` service reads the `traceparent` header automatically.
No configuration is required. To verify propagation is working, send a
request with a `traceparent` header and check your tracing backend for the
correlated span.

```bash
curl -H "traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01" \
     -H "Content-Type: application/sparql-query" \
     --data "SELECT * WHERE { ?s ?p ?o } LIMIT 10" \
     http://localhost:8080/sparql
```

## Span Name Reference

| Span Name | Description | Key Attributes |
|---|---|---|
| `sparql.select` | SPARQL SELECT query execution | `query`, `result_count`, `duration_ms` |
| `sparql.update` | SPARQL UPDATE execution | `operation_type`, `triples_modified` |
| `sparql.ask` | SPARQL ASK query | `result` |
| `sparql.construct` | SPARQL CONSTRUCT query | `result_count` |
| `sparql.describe` | SPARQL DESCRIBE query | `subject_iri` |
| `datalog.infer` | Datalog inference run | `rule_set`, `new_triples`, `iterations` |
| `datalog.retract` | DRed retraction run | `rule_set`, `retracted_triples` |
| `shacl.validate` | SHACL validation pass | `shapes_count`, `violations_count` |
| `shacl.rule.compile` | SHACL-AF rule compilation | `rule_count`, `compiled_count` |
| `htap.merge` | HTAP merge cycle | `predicate_id`, `rows_merged`, `duration_ms` |
| `federation.call` | Federated SPARQL endpoint call | `endpoint`, `duration_ms`, `error` |
| `bulk_load` | N-Triples / Turtle bulk load | `format`, `triple_count`, `duration_ms` |

## Standard Attributes

All pg_ripple spans include these attributes:

| Attribute | Type | Description |
|---|---|---|
| `db.system` | string | Always `"postgresql"` |
| `db.name` | string | PostgreSQL database name |
| `pg_ripple.version` | string | Extension version (e.g. `"0.61.0"`) |
| `pg_ripple.traceparent` | string | W3C traceparent forwarded from HTTP layer (if set) |

## Example Prometheus Queries

### Query latency p95 (5-minute window)

```promql
histogram_quantile(0.95,
  sum(rate(pg_ripple_span_duration_ms_bucket{span="sparql.select"}[5m])) by (le)
)
```

### Federation error rate by endpoint

```promql
sum(rate(pg_ripple_federation_errors_total[5m])) by (endpoint)
/
sum(rate(pg_ripple_federation_calls_total[5m])) by (endpoint)
```

### HTAP merge throughput (rows/sec)

```promql
sum(rate(pg_ripple_htap_merge_rows_total[1m]))
```

### SHACL violation rate

```promql
sum(rate(pg_ripple_shacl_violations_total[5m])) by (shape)
```

## Grafana Dashboard

A sample Grafana dashboard JSON is available at
`docs/fixtures/grafana_pg_ripple_dashboard.json`. Import it via
**Dashboards → Import → Upload JSON file**.

## Tracing Backends

pg_ripple's OTel exporter supports any OTLP-compatible backend:

| Backend | Configuration |
|---|---|
| Datadog | Set `OTEL_EXPORTER_OTLP_ENDPOINT=https://trace.agent.datadoghq.com` |
| Honeycomb | Set `OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io` and `OTEL_EXPORTER_OTLP_HEADERS=x-honeycomb-team=<API_KEY>` |
| Grafana Tempo | Set `OTEL_EXPORTER_OTLP_ENDPOINT=http://tempo:4317` |
| Jaeger | Set `OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317` |

These environment variables are read by the `pg_ripple_http` service at startup.
