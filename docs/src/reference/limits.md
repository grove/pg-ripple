# Limits and Quotas

This page documents the hard limits, default quotas, and tunable caps in pg_ripple. All GUC parameters listed here can be adjusted; see the [GUC Reference](guc-reference.md) for full details.

---

## Query Limits

| Limit | Default | GUC | Error code |
|---|---|---|---|
| Max SPARQL result rows | unlimited | `pg_ripple.sparql_max_rows` | PT640 |
| SPARQL overflow action | `truncate` | `pg_ripple.sparql_overflow_action` | — |
| Max algebra tree depth | 256 | `pg_ripple.sparql_max_algebra_depth` | PT440 |
| Max triple patterns per query | 4096 | `pg_ripple.sparql_max_triple_patterns` | PT440 |
| Max DESCRIBE CBD depth | 16 | `pg_ripple.describe_max_depth` | — |
| Fuzzy match input length | 4096 chars | `pg_ripple.fuzzy_max_input_length` | PT0308 |
| All-nodes predicate expansion cap | 500 predicates | `pg_ripple.all_nodes_predicate_limit` | — |

## Export Limits

| Limit | Default | GUC | Error code |
|---|---|---|---|
| Export function max rows (Turtle/N-Triples/JSON-LD) | unlimited | `pg_ripple.export_max_rows` | PT642 |
| Arrow Flight batch size | 1000 rows/batch | `pg_ripple.arrow_batch_size` | — |

## Inference Limits

| Limit | Default | GUC | Error code |
|---|---|---|---|
| Datalog max depth | unlimited | `pg_ripple.datalog_max_depth` | — |
| Datalog max derived facts | unlimited | `pg_ripple.datalog_max_derived` | — |
| Well-founded semantics max rounds | 100 | `pg_ripple.wfs_max_iterations` | PT520 |
| SHACL rule max iterations | 100 | `pg_ripple.shacl_rule_max_iterations` | PT301 |
| Lattice inference max iterations | 1000 | `pg_ripple.lattice_max_iterations` | PT540 |
| Probabilistic Datalog max iterations | 100 | `pg_ripple.prob_datalog_max_iterations` | — |
| SHACL score log retention | 30 days | `pg_ripple.shacl_score_log_retention_days` | — |

## Federation Limits

| Limit | Default | GUC | Error code |
|---|---|---|---|
| Per-SERVICE timeout | 30 s | `pg_ripple.federation_timeout` | PT214 |
| Connect timeout | 10 s | `pg_ripple.federation_connect_timeout_secs` | PT214 |
| Max rows per SERVICE call | 10,000 | `pg_ripple.federation_max_results` | — |
| Max response body per endpoint | 100 MiB | `pg_ripple.federation_max_response_bytes` | PT215 |
| Partial recovery max bytes | 64 KiB | `pg_ripple.federation_partial_recovery_max_bytes` | — |
| Circuit breaker threshold | 5 failures | `pg_ripple.federation_circuit_breaker_threshold` | PT217 |
| Parallel SERVICE workers | 4 | `pg_ripple.federation_parallel_max` | — |

## PageRank Limits

| Limit | Default | GUC | Error code |
|---|---|---|---|
| Max PageRank iterations | 100 | `pg_ripple.pagerank_max_iterations` | — |
| Max seed IRIs per call | 1024 | `pg_ripple.pagerank_max_seeds` | PT0411 |
| Convergence check norm | L1 | `pg_ripple.pagerank_convergence_norm` | — |

## Storage Limits

| Limit | Default | GUC | Notes |
|---|---|---|---|
| VP promotion threshold | 1,000 triples | `pg_ripple.vp_promotion_threshold` | Below threshold: stored in `vp_rare` |
| Merge batch size | 1,000,000 rows | `pg_ripple.merge_batch_size` | Per-merge INSERT…SELECT |
| Merge fence lock timeout | 5,000 ms | `pg_ripple.merge_lock_timeout_ms` | — |
| CDC watermark batch size | 100 events | `pg_ripple.cdc_watermark_batch_size` | — |
| VP promotion batch size | 10,000 rows | `pg_ripple.vp_promotion_batch_size` | — |
| Bidi relay max in-flight | 1,000 ops | `pg_ripple.bidi_relay_max_inflight` | Drop-oldest policy |
| Dictionary vacuum threshold | 10,000 terms | `pg_ripple.dict_vacuum_threshold` | Post-encode auto-VACUUM |

## Dictionary and Encoding

| Limit | Default | GUC | Notes |
|---|---|---|---|
| Dictionary LRU cache size | 65,536 entries | `pg_ripple.dictionary_cache_size` | XXH3-128 hash map |
| Shared memory budget | 64 MiB | `pg_ripple.cache_budget_mb` | `postmaster`-scoped |

## HTTP API Limits

| Limit | Default | Config | Notes |
|---|---|---|---|
| Max request body size | 10 MiB | `PG_RIPPLE_HTTP_MAX_BODY_BYTES` | Applies to SPARQL UPDATE body |
| Rate limit | unlimited | `PG_RIPPLE_HTTP_RATE_LIMIT` | Per source IP, req/s |
| Arrow Flight ticket expiry | 3,600 s | `pg_ripple.arrow_flight_expiry_secs` | Signed HMAC tickets |
| Connection pool size | 16 | `PG_RIPPLE_HTTP_POOL_SIZE` | Postgres connections |
| Streaming query timeout | 300,000 ms | `PG_RIPPLE_HTTP_QUERY_TIMEOUT_MS` | Capped by `PG_RIPPLE_HTTP_QUERY_TIMEOUT_MAX_MS` |
| Streaming idle timeout | 60,000 ms | `PG_RIPPLE_HTTP_STREAM_IDLE_TIMEOUT_MS` | Maximum wait between rows |
| Streaming chunk size | 65,536 B | `PG_RIPPLE_HTTP_STREAM_CHUNK_BYTES` | Bounded response coalescing |
| Streaming row size | 1 MiB | `PG_RIPPLE_HTTP_STREAM_MAX_ROW_BYTES` | Encoded row cap |

## Audit and Retention

| Limit | Default | GUC | Notes |
|---|---|---|---|
| Event audit retention | 90 days | `pg_ripple.audit_retention_days` | `_pg_ripple.event_audit` |
| SHACL score log retention | 30 days | `pg_ripple.shacl_score_log_retention_days` | `_pg_ripple.shacl_score_log` |
| CDC slot idle timeout | 3,600 s | `pg_ripple.cdc_slot_idle_timeout_seconds` | Orphan slot cleanup |
| VACUUM dict batch size | 200 entries | `pg_ripple.vacuum_dict_batch_size` | `vacuum_dictionary()` |

---

## Hard Limits (Not Configurable)

These limits are baked into the implementation:

| Constraint | Value | Notes |
|---|---|---|
| Dictionary hash space | 2^64 (XXH3-128) | Collision probability negligible in practice |
| Maximum SID (statement ID) | 2^63 − 1 (i64) | PostgreSQL sequence maximum |
| Maximum named graph ID | 2^63 − 1 (i64) | Same sequence namespace |
| Maximum predicate ID | 2^63 − 1 (i64) | Same sequence namespace |
| SPARQL 1.1 spec compliance | Full | SELECT, CONSTRUCT, DESCRIBE, ASK, UPDATE, LOAD, CLEAR, DROP, ADD, MOVE, COPY |
| PostgreSQL target | 18.x only | pgrx 0.18, no older PG support |

---

## Recommended Limits for Production

For a 100 GB RDF dataset on a 32-core / 128 GB RAM server:

```ini
# postgresql.conf overrides
pg_ripple.dictionary_cache_size     = 1000000
pg_ripple.cache_budget_mb           = 512
pg_ripple.sparql_max_rows           = 100000
pg_ripple.export_max_rows           = 500000
pg_ripple.merge_threshold           = 50000
pg_ripple.merge_workers             = 4
pg_ripple.datalog_parallel_workers  = 8
pg_ripple.federation_timeout        = 60
pg_ripple.federation_parallel_max   = 8
pg_ripple.pagerank_max_iterations   = 200
```

See [Performance Tuning](../operations/tuning.md) for a full tuning guide.
