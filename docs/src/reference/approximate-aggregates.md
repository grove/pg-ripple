# Approximate Aggregates (HLL COUNT DISTINCT)

pg_ripple supports approximate `COUNT(DISTINCT ...)` in SPARQL queries using the PostgreSQL
[`hll` extension](https://github.com/citusdata/postgresql-hll) when it is available.

## When HLL is used

HLL is used for `COUNT(DISTINCT ...)` when **both** conditions are met:

1. The GUC `pg_ripple.approx_distinct` is set to `on` (default: `off`).
2. The `hll` PostgreSQL extension is installed in the current database.

When either condition is not met, pg_ripple falls back to exact `COUNT(DISTINCT ...)` SQL.

**Example:**

```sql
-- Enable approximate COUNT(DISTINCT)
SET pg_ripple.approx_distinct = on;

-- This SPARQL query uses HLL when the hll extension is available
SELECT *
FROM pg_ripple.sparql($$
    SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { ?s ?p ?o }
$$);
```

## Error bounds

The `hll` extension uses a HyperLogLog sketch with default precision `log2m = 14`
(approximately 16,384 register buckets). The resulting error characteristics are:

| Cardinality range | Standard error | Typical relative error |
|-------------------|----------------|------------------------|
| < 1,000 | ~3.5% | up to ±5% |
| 1,000 – 10,000 | ~1.5% | typically ±2% |
| ≥ 10,000 | ~0.81% | typically ±1% |
| ≥ 100,000 | ~0.025% | typically ±0.1% |

The standard error for `log2m = 14` is approximately `1.04 / sqrt(2^14) ≈ 0.0081` (0.81%)
for cardinalities in the asymptotic regime (≥ 10,000 distinct values).

## Fallback behavior

When `pg_ripple.approx_distinct = off` or the `hll` extension is not installed:

- `COUNT(DISTINCT ...)` uses exact SQL `COUNT(DISTINCT ...)` semantics.
- No error bounds apply; results are exact.
- This is the default behavior.

## Enabling HLL

```sql
-- Install the hll extension (requires superuser or pg_extension_owner)
CREATE EXTENSION IF NOT EXISTS hll;

-- Enable approximate aggregates for this session
SET pg_ripple.approx_distinct = on;

-- Or set it globally
ALTER SYSTEM SET pg_ripple.approx_distinct = on;
SELECT pg_reload_conf();
```

## Citus / distributed COUNT DISTINCT

On Citus clusters, `COUNT(DISTINCT ...)` is not natively pushable to shards (because shards may
contain overlapping value sets). When `approx_distinct = on` and the `hll` extension is present,
pg_ripple rewrites `COUNT(DISTINCT ?x)` to:

```sql
hll_cardinality(hll_add_agg(hll_hash_bigint(x)))::bigint
```

This allows Citus to aggregate HLL sketches across shards using `hll_union_agg`, giving a
scalable distributed approximate count with the same error bounds as above.

## Checking if HLL is active

```sql
-- Check current setting
SHOW pg_ripple.approx_distinct;

-- Check if hll extension is installed
SELECT count(*) > 0 AS hll_available
FROM pg_available_extensions
WHERE name = 'hll' AND installed_version IS NOT NULL;
```

## Configuration reference

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.approx_distinct` | `off` | Use HLL for `COUNT(DISTINCT ...)` when `hll` is available |

## Testing accuracy

The pg_regress test `hll_accuracy.sql` verifies that approximate `COUNT(DISTINCT ...)` returns
a result within 2% of the exact count for a dataset of 200 distinct subjects. Full accuracy
benchmarks at cardinality 100,000 can be run with the `tests/http_integration/` suite.

See also: [GUC Reference](guc-reference.md), [Citus Integration](../operations/citus-integration.md).
