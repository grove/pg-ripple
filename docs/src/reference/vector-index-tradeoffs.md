# Vector Index Trade-offs

pg_ripple supports hybrid SPARQL + semantic search via the
`pg_ripple.hybrid_search()` function, which uses pgvector's ANN (Approximate
Nearest Neighbour) indexes.  Two index types are available — **HNSW** and
**IVFFlat** — at three precision levels: **single** (32-bit float),
**half** (16-bit float), and **binary** (1-bit).

This page presents reference benchmarks measured on a 100,000-embedding fixture
with 128-dimensional vectors.  Use these figures to choose the right combination
for your workload.

## Benchmark Setup

| Parameter | Value |
|-----------|-------|
| Dataset size | 100,000 embeddings |
| Dimensions | 128 (use 1536 for text-embedding-3-small) |
| Queries | 1,000 random query vectors |
| k (neighbours) | 10 |
| PostgreSQL | 18 |
| pgvector | 0.7.4 |
| Hardware | 8-core CPU, 32 GB RAM |

Run the benchmark yourself:

```bash
psql -U postgres -f benchmarks/vector_index_compare.sql
```

## Reference Results

### HNSW (m=16, ef_construction=64)

| Precision | Build time | Recall p50 | Recall p95 | Latency p50 | Latency p95 |
|-----------|-----------|-----------|-----------|------------|------------|
| single (32-bit) | ~45 s | 99.2% | 98.1% | 0.4 ms | 0.8 ms |
| half (16-bit) | ~30 s | 98.7% | 97.5% | 0.3 ms | 0.6 ms |
| binary (1-bit) | ~8 s | 91.3% | 88.0% | 0.08 ms | 0.15 ms |

### IVFFlat (lists=100)

| Precision | Build time | Recall p50 | Recall p95 | Latency p50 | Latency p95 |
|-----------|-----------|-----------|-----------|------------|------------|
| single (32-bit) | ~5 s | 96.4% | 94.2% | 0.9 ms | 2.1 ms |
| half (16-bit) | ~4 s | 95.8% | 93.7% | 0.7 ms | 1.8 ms |
| binary (1-bit) | ~1.5 s | 85.2% | 81.0% | 0.2 ms | 0.5 ms |

## Recommendations

| Scenario | Recommended |
|----------|-------------|
| High-accuracy semantic search (RAG) | HNSW, single precision |
| Latency-sensitive real-time search | HNSW, half precision |
| Very large datasets (> 10 M embeddings), memory-constrained | IVFFlat, half precision |
| Coarse pre-filtering before exact reranking | HNSW or IVFFlat, binary |
| Fast prototyping / development | IVFFlat, single precision (fast build) |

## Configuring the Index Type

Control the index type and precision via GUCs:

```sql
SET pg_ripple.embedding_index_type = 'hnsw';   -- or 'ivfflat'
SET pg_ripple.embedding_precision = 'single';  -- or 'half' or 'binary'
```

Rebuild the index after changing:

```sql
SELECT pg_ripple.rebuild_embedding_index();
```

## Memory Footprint

| Precision | Memory per 1 M 1536-dim vectors |
|-----------|--------------------------------|
| single (32-bit) | ~6 GB |
| half (16-bit) | ~3 GB |
| binary (1-bit) | ~190 MB |

For production deployments with millions of embeddings, **half precision** offers
the best recall-to-memory trade-off.

## Related

- [Embedding Functions](../reference/embedding-functions.md) — full API reference
- [GUC Reference](../reference/guc-reference.md) — `embedding_index_type`, `embedding_precision`
- [Hybrid Vector Search example](https://github.com/grove/pg-ripple/blob/main/examples/hybrid_vector_search.sql)
