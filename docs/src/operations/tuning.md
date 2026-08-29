# GUC Tuning Guide

This page maps each `pg_ripple` GUC parameter to workload characteristics.
Use it as a starting point for tuning your deployment.

> See [Configuration Reference](configuration.md) for the full GUC list and default values.

---

## Workload-Class Tuning Matrix

The table below shows recommended GUC settings for five common deployment profiles.
Values shown override the default; omit the row to keep the default.

| GUC | OLTP (write-heavy) | SPARQL Analytics | Datalog/Reasoning | Federation | Development |
|---|---|---|---|---|---|
| `htap_delta_max_rows` | `500 000` | `1 000 000` | `200 000` | `200 000` | `10 000` |
| `merge_interval_secs` | `30` | `300` | `60` | `120` | `10` |
| `auto_analyze` | `on` | `on` | `on` | `off` | `on` |
| `plan_cache_size` | `256` | `2 048` | `512` | `128` | `64` |
| `sparql_max_algebra_depth` | `64` | `512` | `256` | `128` | `256` |
| `sparql_max_triple_patterns` | `1 024` | `8 192` | `4 096` | `2 048` | `4 096` |
| `max_path_depth` | `32` | `128` | `64` | `32` | `64` |
| `export_batch_size` | `5 000` | `50 000` | `10 000` | `10 000` | `10 000` |
| `dictionary_cache_size` | `131 072` | `262 144` | `65 536` | `65 536` | `16 384` |
| `datalog_parallel_workers` | `1` | `1` | `4` | `1` | `1` |
| `datalog_parallel_threshold` | _(n/a)_ | _(n/a)_ | `10 000` | _(n/a)_ | _(n/a)_ |
| `tracing_exporter` | `none` | `none` | `none` | `otlp` | `stdout` |
| `tracing_otlp_endpoint` | _(n/a)_ | _(n/a)_ | _(n/a)_ | `http://jaeger:4317` | _(n/a)_ |

### Profile Descriptions

**OLTP (write-heavy)**
Continuous triple ingestion at high rate. Short merge intervals and a smaller delta
threshold ensure that the merge worker keeps up. `plan_cache_size` is moderate;
reduce `sparql_max_triple_patterns` to reject accidental full-scan queries.

**SPARQL Analytics**
Complex SELECT/CONSTRUCT queries on a large, mostly-static dataset. Large plan cache
reduces translation overhead. High `sparql_max_triple_patterns` allows complex queries.
Infrequent merges reduce background I/O.

**Datalog/Reasoning**
OWL RL / custom rule materialisation. Enable parallel workers and a high threshold to
parallelise independent strata. Keep `merge_interval_secs` moderate — the merge worker
competes with inference for I/O.

**Federation**
Heavy `SERVICE {}` usage querying remote SPARQL endpoints. Enable OTLP tracing to
observe per-endpoint latency. Reduce local `plan_cache_size` since remote queries
vary widely in shape.

**Development**
Local developer machine. Small caches, short merge intervals for fast feedback.
Enable `tracing_exporter = 'stdout'` for easy debugging without an APM stack.

---

## Security Limits

Always set these in production to protect against runaway or malicious queries:

```sql
-- Reject deeply-nested algebra trees (e.g. injected via user input)
SET pg_ripple.sparql_max_algebra_depth = 256;

-- Reject queries with an unreasonable number of triple patterns
SET pg_ripple.sparql_max_triple_patterns = 4096;

-- Cap recursive property path depth
SET pg_ripple.max_path_depth = 64;
```

See [Security Hardening](security.md) for additional recommendations.

## HTTP streaming

Streaming-safe SPARQL Results JSON, CSV, TSV, and N-Triples responses use one
checked-out read-only PostgreSQL connection and a bounded response coalescer.
Tune `PG_RIPPLE_HTTP_STREAM_CHUNK_BYTES` for network efficiency and keep
`PG_RIPPLE_HTTP_STREAM_MAX_ROW_BYTES` bounded when terms are supplied by users.
Set `PG_RIPPLE_HTTP_QUERY_TIMEOUT_MS` below its `900000` ms maximum for tenant
workloads, and use `PG_RIPPLE_HTTP_STREAM_IDLE_TIMEOUT_MS` to release slow or
stalled streams. Disconnects and deadline expiry cancel the PostgreSQL query;
the connection is discarded when rollback or cancellation cannot be proven.

---

## Memory Footprint Estimation

| Component | Memory | Formula |
|---|---|---|
| Dictionary backend cache | ~`dictionary_cache_size × 80 B` | Per backend connection |
| SPARQL plan cache | ~`plan_cache_size × 2 KB` | Per backend connection |
| Delta tables (HTAP) | ~`htap_delta_max_rows × 24 B` | Per VP predicate |
| Merge worker buffers | ~`export_batch_size × 40 B` | Global (one worker) |

For a deployment with 50 concurrent connections and default GUC values, budget
approximately **400 MB** for pg_ripple's own data structures in addition to
PostgreSQL's `shared_buffers`.

---

## Deprecated GUCs

| Old name | Replacement | Removed in |
|---|---|---|
| `pg_ripple.property_path_max_depth` | `pg_ripple.max_path_depth` | v1.0.0 |

Set `pg_ripple.max_path_depth` going forward. The old name is still accepted
but emits a deprecation notice in the server log.

---

## Ready-to-Use Configuration Profiles

Copy-paste these blocks into `postgresql.conf` (then `SELECT pg_reload_conf()`).

### Small Instance Profile (≤8 cores, ≤32 GB RAM)

Suitable for development, staging, or a small-scale production deployment:

```ini
# pg_ripple — small instance profile
pg_ripple.dictionary_cache_size     = 65536
pg_ripple.cache_budget_mb           = 64
pg_ripple.merge_threshold           = 10000
pg_ripple.merge_workers             = 1
pg_ripple.merge_interval_secs       = 60
pg_ripple.datalog_parallel_workers  = 2
pg_ripple.sparql_max_rows           = 50000
pg_ripple.export_max_rows           = 100000
pg_ripple.plan_cache_capacity       = 256
pg_ripple.federation_timeout        = 30
pg_ripple.federation_parallel_max   = 2
```

### Large Instance Profile (≥32 cores, ≥128 GB RAM)

Suitable for large-scale SPARQL analytics, high-throughput ingestion, or intensive Datalog reasoning:

```ini
# pg_ripple — large instance profile
pg_ripple.dictionary_cache_size     = 1000000
pg_ripple.cache_budget_mb           = 512
pg_ripple.merge_threshold           = 50000
pg_ripple.merge_workers             = 4
pg_ripple.merge_interval_secs       = 120
pg_ripple.datalog_parallel_workers  = 8
pg_ripple.sparql_max_rows           = 500000
pg_ripple.export_max_rows           = 1000000
pg_ripple.plan_cache_capacity       = 2048
pg_ripple.federation_timeout        = 60
pg_ripple.federation_parallel_max   = 8
pg_ripple.pagerank_max_iterations   = 200
pg_ripple.topn_pushdown             = on
```

### High-Security / Multi-Tenant Profile

Applies strict resource limits for shared or multi-tenant deployments:

```ini
# pg_ripple — high-security profile
pg_ripple.sparql_max_rows           = 10000
pg_ripple.sparql_overflow_action    = 'error'
pg_ripple.sparql_max_algebra_depth  = 128
pg_ripple.sparql_max_triple_patterns = 1024
pg_ripple.export_max_rows           = 10000
pg_ripple.federation_endpoint_policy = 'allowlist'
pg_ripple.federation_allow_unregistered_service_endpoints = off
pg_ripple.arrow_unsigned_tickets_allowed = off
pg_ripple.strict_sparql_filters     = on
pg_ripple.datalog_max_derived       = 1000000
pg_ripple.wfs_max_iterations        = 50
pg_ripple.rls_bypass                = off
```

### Datalog Reasoning Profile

Optimized for intensive OWL RL / Datalog materialization workloads:

```ini
# pg_ripple — datalog reasoning profile
pg_ripple.inference_mode            = 'materialized'
pg_ripple.magic_sets                = on
pg_ripple.datalog_cost_reorder      = on
pg_ripple.dred_enabled              = on
pg_ripple.datalog_parallel_workers  = 8
pg_ripple.datalog_parallel_threshold = 5000
pg_ripple.owl_profile               = 'RL'
pg_ripple.tabling                   = on
pg_ripple.tabling_ttl               = 600
pg_ripple.rule_plan_cache           = on
pg_ripple.rule_plan_cache_size      = 128
pg_ripple.wfs_max_iterations        = 200
```
