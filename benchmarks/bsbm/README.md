# Berlin SPARQL Benchmark (BSBM) for pg_ripple

This directory contains BSBM-derived benchmark scripts adapted for pg_ripple's
`pg_ripple.sparql()` SQL interface.

## What is BSBM?

The Berlin SPARQL Benchmark (BSBM) is a standard benchmark for SPARQL endpoints
developed by Christian Bizer and Andreas Schultz. It models an e-commerce scenario
with products, vendors, offers, reviews, and reviewers.

Reference: <http://wifo5-03.informatik.uni-mannheim.de/bizer/berlinsparqlbenchmark/>

## Files

| File | Purpose |
|---|---|
| `bsbm_load.sql` | Generate BSBM-scale data (scale factor 1 = 1,000 products) |
| `bsbm_queries.sql` | BSBM query mix Q1–Q12 via `pg_ripple.sparql()` |
| `bsbm_htap.sql` | HTAP concurrent workload: insert + SPARQL under load |
| `bsbm_pgbench.sql` | pgbench custom script for sustained HTAP throughput test |

## Running the Benchmark

### 1. Load BSBM data

```sql
\i benchmarks/bsbm/bsbm_load.sql
```

By default loads scale factor 1 (~1,000 products, ~10,000 triples).
Set `:scale` to a higher value for larger datasets:

```bash
psql -v scale=10 -f benchmarks/bsbm/bsbm_load.sql
```

### 2. Run the BSBM query mix

```sql
\i benchmarks/bsbm/bsbm_queries.sql
```

The script executes all 12 BSBM query types and reports row counts and timing.

### 3. Run the HTAP concurrent workload

```sql
\i benchmarks/bsbm/bsbm_htap.sql
```

This runs the BSBM query mix while concurrently inserting new triples, verifying
that the HTAP delta/main split allows non-blocking concurrent reads and writes.

### 4. pgbench sustained throughput

```bash
pgbench -f benchmarks/bsbm/bsbm_pgbench.sql -c 8 -j 4 -T 60 <dbname>
```

Targets: >100K triples/sec bulk insert, <500 ms query latency under load.

## Baseline Comparison

| Metric | v0.5.1 (single-table) | v0.6.0 (HTAP) | Target |
|---|---|---|---|
| Bulk insert throughput | baseline | ≥ v0.5.1 | >100K triples/sec |
| Q1 latency (1M triples) | baseline | ≤ v0.5.1 | <10 ms |
| Q4 latency (1M triples) | baseline | ≤ v0.5.1 | <20 ms |
| Insert + query concurrency | serial | non-blocking | reads unblocked |
| Merge worker latency | N/A | <30 s | per threshold cycle |

Actual measured values should be recorded in `CHANGELOG.md` when the v0.6.0
release is tagged.
