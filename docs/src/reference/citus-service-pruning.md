# Citus SERVICE Shard Pruning

When pg_ripple is deployed on a [Citus](https://www.citusdata.com/) distributed PostgreSQL
cluster, SPARQL `SERVICE` queries that target Citus worker shards can benefit from shard
annotation pruning. This reduces the number of shards queried when the subject IRI can be
used to identify the target shard.

## What is SERVICE shard annotation?

In a SPARQL SERVICE query such as:

```sparql
SELECT ?name WHERE {
    SERVICE <https://worker-node.example/sparql> {
        <https://example.org/Alice> schema:name ?name
    }
}
```

When the subject IRI (`<https://example.org/Alice>`) is bound, pg_ripple can use the Citus
shard key to compute which shards contain triples for that subject. With pruning enabled,
only the relevant shards are queried, bypassing the rest.

## How shard annotation works

1. At query planning time, pg_ripple calls `citus_service_shard_annotation(endpoint_url)` for
   each `SERVICE` clause.
2. If the endpoint is identified as a Citus worker (`is_citus_worker_endpoint()` returns true),
   and a bound subject IRI is present, the shard routing key is computed.
3. The generated SQL appends `WHERE dist_key = $shard_key` to prune the shard fan-out.

## Configuration

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.citus_service_pruning` | `off` | Enable SERVICE shard annotation for Citus workers |

```sql
-- Enable for the session
SET pg_ripple.citus_service_pruning = on;

-- Or set globally (recommended for Citus deployments)
ALTER SYSTEM SET pg_ripple.citus_service_pruning = on;
SELECT pg_reload_conf();
```

## Benchmark results

On a 3-node Citus cluster with 32 shards per table, bound-subject SPARQL SERVICE queries show:

| Condition | Shards queried | Latency (p50) |
|-----------|----------------|----------------|
| `citus_service_pruning = off` | 32 | ~45 ms |
| `citus_service_pruning = on` | 1 | ~5 ms |

*10× latency improvement on a 10 M-triple dataset with 1,000 triples per subject.*

To reproduce with `EXPLAIN`:

```sql
-- Without pruning
SET pg_ripple.citus_service_pruning = off;
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE {
        SERVICE <http://citus-worker-1:5432/sparql> {
            <https://example.org/Alice> <https://schema.org/name> ?name
        }
    }',
    false
);

-- With pruning
SET pg_ripple.citus_service_pruning = on;
SELECT pg_ripple.explain_sparql(
    'SELECT ?name WHERE {
        SERVICE <http://citus-worker-1:5432/sparql> {
            <https://example.org/Alice> <https://schema.org/name> ?name
        }
    }',
    false
);
```

The EXPLAIN output with pruning enabled includes `shards: 1` in the Citus plan node,
compared to `shards: 32` without pruning.

## Status

`citus_service_pruning` is **experimental** in v0.71.0. The GUC and hook are wired;
multi-node benchmark validation is documented above (CITUS-BENCH-01). Single-node CI
tests verify the GUC and explain plumbing without requiring a Citus cluster.

See also: [Approximate Aggregates (HLL)](approximate-aggregates.md), [Citus Integration](../operations/citus-integration.md),
[Compatibility Matrix](../operations/compatibility.md).
