# Performance and Conformance Results

A summary of every benchmark and conformance test pg_ripple runs in CI, with links to the full result pages.

---

## W3C standards conformance

| Suite | Result | Reference |
|---|---|---|
| **W3C SPARQL 1.1** | 100 % (smoke gate; full suite runs informationally) | [SPARQL Compliance Matrix](../reference/sparql-compliance.md) |
| **W3C SHACL Core** | 100 % | [W3C Conformance](../reference/w3c-conformance.md) |
| **W3C OWL 2 RL** | 100 % | [OWL 2 RL Results](../reference/owl2rl-results.md) |
| **Apache Jena edge cases** (~1,000 tests) | Tracked in CI; informational until ≥95 % | [W3C Conformance](../reference/w3c-conformance.md) |

The first three are *blocking* CI gates — a release cannot ship if any drops below 100 %. The Apache Jena suite stays informational until pg_ripple confidently passes ≥ 95 %; reporting is honest about the current state.

---

## Performance benchmarks

| Benchmark | What it measures | Result |
|---|---|---|
| **WatDiv 10 M / 100 M** | SPARQL correctness + latency across 100 query templates (star, chain, snowflake, complex) | 100 % correctness; latency competitive with Virtuoso open-source on the same hardware. See [WatDiv Results](../reference/watdiv-results.md). |
| **LUBM** | OWL RL inference correctness across 14 canonical queries | 14 / 14 pass. See [LUBM Results](../reference/lubm-results.md). |
| **BSBM** | E-commerce-style mixed query workload | Regression gate in CI; numbers tracked per commit. |
| **Bulk load** | Triples per second on commodity hardware | > 100 K triples/sec on a 4-core / 16 GB machine. |
| **SPARQL latency** | Typical star pattern p50 | < 10 ms on a 100 M-triple store with warm cache. |

---

## Why the results matter

### 100 % conformance is the floor, not the ceiling

A triple store that gets 95 % of W3C tests right has a 5 % chance of returning a wrong result for *your* query. That is not an academic concern — it is the difference between "always correct" and "occasionally surprising". pg_ripple's CI fails the build before merging anything that drops below 100 %.

### Performance numbers come from real machines, not marketing

Every benchmark on this page is run in GitHub Actions on a known instance type. The configuration, dataset, and harness are reproducible from the [`benchmarks/`](https://github.com/grove/pg-ripple/tree/main/benchmarks) directory. If you cannot reproduce a number, that is a bug — file an issue.

### What we have *not* benchmarked

- Distributed (Citus) clusters at production scale. CI runs a small four-worker cluster; production-scale numbers are pending.
- Federation latency to remote SPARQL endpoints. The variance is dominated by the remote endpoint, not by pg_ripple.
- LLM end-to-end latency for `rag_context()` + chat completion. The LLM dominates; pg_ripple's contribution is sub-100 ms.

---

## Running the benchmarks yourself

```bash
# WatDiv (10 M triples)
cd benchmarks/watdiv && ./run.sh

# LUBM (14 queries)
cd benchmarks && ./lubm.sh

# Bulk load
cd benchmarks && bash ci_benchmark.sh insert_throughput.sql

# Vector index comparison
cd benchmarks && bash ci_benchmark.sh vector_index_compare.sql
```

Most benchmarks run in under five minutes on a developer laptop; the full WatDiv 100 M takes ~30 minutes.

---

## See also

- [Reference → SPARQL compliance matrix](../reference/sparql-compliance.md)
- [Reference → WatDiv results](../reference/watdiv-results.md)
- [Reference → LUBM results](../reference/lubm-results.md)
- [Reference → OWL 2 RL results](../reference/owl2rl-results.md)
