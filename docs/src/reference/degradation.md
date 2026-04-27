# Optional-Feature Degradation Semantics

> **v0.64.0 TRUTH-10**: This page documents the expected degraded behavior for
> every optional feature in pg_ripple. Operators can use
> `pg_ripple.feature_status()` to check the live status at runtime.

---

## Overview

pg_ripple distinguishes between **required core features** (always active) and
**optional features** that depend on external extensions, configuration, or
future development milestones. When an optional feature is unavailable, pg_ripple
degrades gracefully — it does not panic, it does not silently return wrong results,
and it always emits a warning or structured error.

The `pg_ripple.feature_status()` SQL function returns one row per major capability
with an honest status value. Integrate this into your monitoring pipeline:

```sql
SELECT feature_name, status, degraded_reason
FROM pg_ripple.feature_status()
WHERE status IN ('degraded', 'stub', 'planned')
ORDER BY feature_name;
```

The `pg_ripple_http /ready` endpoint includes a `partial_features` array in its
response body, which lists all non-`implemented` features so operators can
assess readiness before routing production traffic.

---

## Feature Degradation Reference

### Arrow Flight (`arrow_flight`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `stub` |
| Return on missing dependency | `{"status":"stub","message":"Arrow IPC streaming not yet implemented"}` |
| Warning code | None (HTTP 200 with stub body) |
| Readiness behavior | Reported as `stub` in `/ready` `partial_features` |
| Planned implementation | v0.66.0 |

**Detail**: The `/flight/do_get` endpoint in `pg_ripple_http` returns a JSON stub
body instead of Arrow IPC data. This is not a crash or error — it is a placeholder
until the Arrow IPC serialization layer is implemented. Do not rely on Arrow Flight
output for production data pipelines until v0.66.0.

---

### WCOJ / Worst-Case Optimal Joins (`wcoj`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `planner_hint` |
| Behavior | Cyclic BGP join reordering at plan time; no custom executor |
| Warning code | None |
| Readiness behavior | Reported as `planner_hint` in `/ready` |
| Planned implementation | v0.66.0 (true Leapfrog Triejoin executor) |

**Detail**: WCOJ is implemented as a SPARQL-to-SQL planner optimization that
reorders cyclic-pattern joins to reduce intermediate result sizes. A true
Leapfrog Triejoin executor that intersects sorted iterators at runtime is not
implemented. For cyclic BGPs (e.g. triangle queries), pg_ripple uses PostgreSQL's
native hash-join or merge-join, which may be slower than a true WCOJ executor
for highly cyclic graphs.

---

### SHACL-SPARQL Rules (`shacl_sparql_rule`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `planned` |
| Return on invocation | Rule is stored but never fires |
| Warning code | `WARNING` level log when a `sh:SPARQLRule` shape is loaded |
| Readiness behavior | Reported as `planned` in `/ready` |
| Planned implementation | v0.65.0 |

**Detail**: `sh:SPARQLRule` shapes are parsed, stored in the SHACL catalog, and
associated with their target classes. However, they are not routed through the
derivation kernel and will not fire during inference or validation runs. Use
`sh:SPARQLConstraint` (which IS implemented) for SPARQL-backed validation.

---

### CONSTRUCT Writeback (`construct_writeback`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `manual_refresh` |
| Behavior | Rules apply only when explicitly invoked via `pg_ripple.apply_construct_rules()` |
| Warning code | None |
| Readiness behavior | Reported as `manual_refresh` in `/ready` |
| Planned implementation | v0.65.0 (incremental delta maintenance) |

**Detail**: CONSTRUCT writeback rules are correct — they produce the right output
when invoked. However, they do not maintain derived triples incrementally when
source data changes. After a bulk load or SPARQL UPDATE, operators must manually
call `pg_ripple.apply_construct_rules()` to refresh derived triples.
Incremental delta maintenance (triggered automatically on insert/delete) is
planned for v0.65.0.

---

### Citus SERVICE Pruning (`citus_service_pruning`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `planned` |
| Return on invocation | Query executes without shard pruning (correct but slower) |
| Warning code | None |
| Readiness behavior | Reported as `planned` in `/ready` |
| Planned implementation | v0.66.0 |

**Detail**: SERVICE result shard pruning routes SPARQL SERVICE calls to the
specific Citus worker shard that holds the relevant data, avoiding full-cluster
fan-out. This optimization is planned but not yet integrated into the
SPARQL-to-SQL translator. Queries return correct results but may scan more
shards than necessary.

---

### Citus HLL COUNT(DISTINCT) (`citus_hll_distinct`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `planned` |
| Return on invocation | COUNT(DISTINCT) uses exact counting (correct but slower) |
| Warning code | None |
| Readiness behavior | Reported as `planned` in `/ready` |
| Planned implementation | v0.66.0 |

**Detail**: HyperLogLog approximate COUNT(DISTINCT) uses a probabilistic sketch
to count distinct values across Citus shards without inter-shard coordination.
The SQL aggregate generation layer does not yet emit HLL function calls.
COUNT(DISTINCT) currently uses exact counting, which is correct but requires
data movement across shards.

---

### SPARQL Cursor Streaming (`sparql_cursor_streaming`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `planned` |
| Behavior | `/sparql/stream` endpoint materializes full result set before streaming |
| Warning code | None |
| Readiness behavior | Reported as `planned` in `/ready` |
| Planned implementation | v0.66.0 |

**Detail**: The `/sparql/stream` endpoint streams responses as chunked
transfer encoding, but the full result set is materialized in memory before
the first chunk is sent. True incremental streaming (where rows are emitted
to the client as they arrive from PostgreSQL) is planned for v0.66.0.
For very large result sets, use `pg_ripple.sparql_cursor()` in SQL instead.

---

### Vector Hybrid Search (`vector_hybrid_search`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `experimental` |
| Dependency | `pgvector` extension |
| Return on missing pgvector | Exact nearest-neighbor search (correct but slower) |
| Warning code | `WARNING: pgvector not installed; falling back to exact similarity search` |
| Readiness behavior | Reported as `experimental` in `/ready` with degraded_reason |

**Detail**: When pgvector is installed, hybrid search uses HNSW approximate
nearest-neighbor indexes for sub-millisecond similarity search. Without pgvector,
pg_ripple falls back to exact cosine distance computation, which is always correct
but O(n) in the number of embeddings. Install pgvector for production vector workloads.

---

### CDC Subscriptions (`cdc_subscriptions`)

| Property | Value |
|---|---|
| Status in v0.63.0 | `experimental` |
| Dependency | `pg_trickle` extension |
| Return on missing pg_trickle | Subscriptions fail with informational error |
| Warning code | `WARNING: pg_trickle is not installed; CDC subscriptions are unavailable` |
| Readiness behavior | Reported as `experimental` in `/ready` with degraded_reason |

**Detail**: Live CDC subscriptions use pg_trickle stream tables for efficient
change propagation. Without pg_trickle, `create_subscription()` returns an
informational message rather than creating a subscription. The extension does
not panic or raise an unrecoverable error.

---

## Checking Feature Status in Code

```sql
-- List all non-implemented features with their degraded reason.
SELECT feature_name, status, degraded_reason
FROM pg_ripple.feature_status()
WHERE status != 'implemented'
ORDER BY status, feature_name;
```

```sql
-- Check a specific feature.
SELECT status, degraded_reason
FROM pg_ripple.feature_status()
WHERE feature_name = 'arrow_flight';
```

```bash
# Check /ready for degraded features (pg_ripple_http).
curl -s http://localhost:7878/ready | jq '.partial_features'
```

---

## Metrics and Log Visibility

Optional-feature degradation is visible through:

1. **`pg_ripple.feature_status()`** — SQL function; one row per feature
2. **`pg_ripple_http /ready`** — includes `partial_features` array
3. **PostgreSQL log** — `WARNING` level messages when optional extensions are
   absent and a feature that depends on them is invoked
4. **Prometheus metrics** — `pg_ripple_http_errors_total` counter increments
   when a stub feature returns an error response

---

*See also: [Feature Status SQL API](../reference/sql-functions.md#feature-status),
[Known Limitations](../../README.md#known-limitations-in-v0630)*
