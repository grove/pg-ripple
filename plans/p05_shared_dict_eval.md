# P-05 Evaluation: Shared-Memory Dictionary LRU Cache

**Status**: Closed — not worth implementing in v0.83.0 (see rationale below)  
**Roadmap ID**: P-05-EVAL (v0.83.0)  
**Author**: pg_ripple engineering  
**Date**: 2026-04-22

---

## Problem Statement

The dictionary decode LRU cache (IRI/blank-node/literal → i64) is currently maintained as a **per-backend in-process cache** backed by the `lru` crate. This means:

1. Each PostgreSQL backend that queries pg_ripple allocates its own cache up to `pg_ripple.dictionary_cache_size` entries.
2. Cache entries are not shared between concurrent backends; a busy cluster with 100 connections may redundantly decode the same popular IRIs 100 times.
3. On backend restart (e.g., connection pool recycle), the entire per-backend cache is discarded and must be rebuilt on the first queries.

---

## Current Architecture

```
┌─────────────────────┐
│  Backend A          │
│  LRU<i64, String>   │  (heap-allocated, ~1 MB default)
└─────────────────────┘

┌─────────────────────┐
│  Backend B          │
│  LRU<i64, String>   │  (same data, different allocation)
└─────────────────────┘

       ↓ every decode miss goes to ↓

┌─────────────────────────────────────────────────────┐
│  _pg_ripple.dictionary  (PostgreSQL shared buffers) │
└─────────────────────────────────────────────────────┘
```

**GUC**: `pg_ripple.dictionary_cache_size` (default 65 536 entries, ~8 MB per backend for typical IRI lengths).

---

## Proposed Alternative: PgSharedMem-backed Cache

PostgreSQL exposes shared memory for extensions via `PgSharedMem` (pgrx) / `dsm_segment` (C API). The idea is:

```
┌─────────────────────────────────────────────────────────────────┐
│  Shared Memory Segment                                          │
│  LWLock-protected hashtable: i64 → (offset, len) into string pool │
│  String pool: mmap'd region, bump-allocated, append-only        │
└─────────────────────────────────────────────────────────────────┘
          ↑ read by all backends, write on miss
```

---

## Evaluation

### 1. Benefit: Reduced Redundant Decoding

| Workload | Per-backend (current) | Shared (proposed) |
|---|---|---|
| 1 backend, warm cache | 0 DB roundtrips | 0 DB roundtrips |
| 10 backends, warm per-backend caches | 0 DB roundtrips | 0 DB roundtrips |
| 10 backends, cold start | 10× decodes for common IRIs | 1× decode, 9× shared hits |
| Connection pool churn (100 reconnects/hr) | 100× cold-cache rebuilds | Shared cache survives reconnects |

For **typical OLTP workloads** (few thousand distinct IRIs, stable connection pools), the benefit is marginal — backends quickly warm their own caches.

For **analytics workloads** with high connection churn or short-lived backends (e.g., pg_ripple used from pgbouncer with transaction-mode pooling), a shared cache would meaningfully reduce SPI overhead.

### 2. Complexity Cost

| Concern | Risk |
|---|---|
| LWLock contention on every cache hit | Medium — requires per-shard locks (a single LWLock serialises all reads) |
| String pool eviction | Hard — LRU eviction with a pointer-based pool requires compaction or a two-level scheme |
| pgrx `PgSharedMem` API maturity | Medium — `PgSharedMem` in pgrx 0.18 is stable but size must be declared at `_PG_init` time; dynamic growth is not supported without `dsm_segment` |
| Cross-version compat of shared layout | High — upgrading the extension requires flushing shared memory, adding complexity to migration scripts |
| Bug surface | Higher — race conditions in shared-memory code are hard to reproduce and diagnose |

### 3. Benchmarking Baseline

Running `pgbench -c 20 -j 4 -T 60` with a SPARQL SELECT workload against a 10 000-IRI dataset:

| Cache strategy | Decode throughput (queries/s) |
|---|---|
| Per-backend LRU (current) | ~18 000 |
| No cache (all SPI decode) | ~12 000 |
| Estimated shared-mem (modelled) | ~19 500 (8% gain) |

The **8% throughput gain** does not justify the engineering and maintenance cost at this stage.

### 4. Alternative: Increase Default Cache Size

A simpler alternative is to increase `pg_ripple.dictionary_cache_size` from 65 536 to 131 072 or 262 144 entries when IRI diversity is known to be high. This is a one-line GUC change with no complexity.

---

## Decision

**Close with "not worth it" for v0.83.0.**

Rationale:

1. The modelled throughput gain is ≤10% for typical workloads.
2. Shared-memory LRU with string pool eviction adds significant complexity to a correctness-critical path.
3. pgrx `PgSharedMem` layout changes would require migration script coordination on every bump.
4. The same goal (warm cache surviving reconnects) can be achieved more safely by switching to connection-pool `session`-mode pooling, which preserves backend memory.

**Revisit if**:

- Benchmarks show >20% decode overhead on a production workload.
- pgrx ships a `dsm`-backed dynamic hashtable with built-in eviction.
- Transaction-mode pooling (pgbouncer) becomes the dominant deployment pattern.

---

## Follow-On Work (Future)

If a shared dictionary cache is eventually implemented, the recommended approach is:

1. Use PostgreSQL's `dshash_table` (available in PG 10+) via a `unsafe` C wrapper, which handles LWLock-per-partition automatically.
2. Store string data in a `dsm_segment`-backed ring buffer with generation counters for safe GC.
3. Keep the per-backend LRU as an L1 cache in front of the shared L2 to avoid LWLock on every read.

Estimated effort: ~3 engineer-weeks including testing.
