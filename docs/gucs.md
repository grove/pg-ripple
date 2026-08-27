# pg_ripple GUC Reference

For the complete reference, see [the reference GUC documentation](src/reference/guc-reference.md).

`pg_ripple.llm_api_key_env` accepts only environment-variable names matching
`^[A-Z_][A-Z0-9_]*$`. Raw values are rejected. During a pre-0.131 migration,
a superuser may temporarily set `pg_ripple.llm_api_key_env_allow_raw = on`.

> **Version**: v0.116.0  
> All parameters are prefixed with `pg_ripple.` and configurable via `SET`, `postgresql.conf`, or `ALTER DATABASE/ROLE SET`.

## Categories

- [Storage & VP Tables](#storage--vp-tables)
- [Dictionary & Caching](#dictionary--caching)
- [SPARQL Engine](#sparql-engine)
- [Datalog & Inference](#datalog--inference)
- [Proof Trees & Rule Explanation](#proof-trees--rule-explanation)  *(v0.116.0)*
- [SHACL Validation](#shacl-validation)
- [owl:sameAs & Entity Resolution](#owlsameas--entity-resolution)
- [ER Monitoring](#er-monitoring)  *(v0.116.0)*
- [Federation](#federation)
- [CDC & Replication](#cdc--replication)
- [Bidirectional Relay](#bidirectional-relay)  *(v0.116.0)*
- [Probabilistic & Bayesian Reasoning](#probabilistic--bayesian-reasoning)  *(v0.116.0)*
- [LLM / Embedding & RAG](#llm--embedding--rag)
- [PageRank & Centrality](#pagerank--centrality)
- [Observability & Tracing](#observability--tracing)
- [Security & Access Control](#security--access-control)
- [HTTP Companion (Arrow Flight)](#http-companion-arrow-flight)
- [Citus Integration](#citus-integration)
- [Background Workers](#background-workers)

---

## Storage & VP Tables

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `vp_promotion_threshold` | integer | 1000 | Suset | Minimum triple count before a rare-predicate is promoted to its own VP table |
| `vp_promotion_batch_size` | integer | 10000 | Suset | Batch size for bulk VP promotion |
| `columnar_threshold` | integer | -1 | Suset | Row count above which main partitions use columnar storage (-1 = disabled) |
| `adaptive_indexing_enabled` | bool | off | Suset | Allow the merge worker to create secondary indexes dynamically |
| `dedup_on_merge` | bool | off | Suset | Deduplicate triples during HTAP merge |
| `tombstone_gc_enabled` | bool | on | Suset | Enable background GC of tombstone rows |
| `tombstone_gc_threshold` | integer | 10000 | Suset | Number of tombstones that trigger a GC cycle |
| `tombstone_retention_seconds` | integer | 0 | Suset | Seconds to retain tombstones before GC eligibility (0 = immediate) |
| `delta_index_threshold` | integer | 5000 | Suset | Delta-partition row count triggering automatic index creation |
| `dict_vacuum_threshold` | integer | 10000 | Suset | Dictionary table dead-tuple threshold before auto-vacuum |
| `vacuum_dict_batch_size` | integer | 200 | Suset | Batch size for incremental dictionary vacuum |
| `auto_analyze` | bool | on | Suset | Enable automatic ANALYZE on VP tables after merge |
| `stats_refresh_interval_seconds` | integer | 300 | Suset | Seconds between background stats-refresh cycles |
| `stats_scan_limit` | integer | 1000 | Suset | Maximum rows sampled per VP table during stats refresh |
| `bulk_load_use_copy` | bool | on | Suset | Use COPY instead of INSERT for bulk RDF loads |
| `export_batch_size` | integer | 10000 | Userset | Rows per batch when serializing RDF exports |
| `export_max_rows` | integer | 1000000 | Userset | Hard cap on rows returned by export functions |
| `export_confidence` | real | 0.0 | Userset | Minimum confidence score for triples included in exports |

---

## Dictionary & Caching

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `dictionary_cache_size` | integer | 65536 | Postmaster | Per-process LRU capacity for the IRI/literal → i64 cache |
| `dictionary_tier_threshold` | integer | 1000000 | Suset | Entry count above which the dictionary switches to tiered storage |
| `strict_dictionary` | bool | off | Userset | Reject insertions of IRIs that fail RFC 3987 validation |
| `predicate_cache_enabled` | bool | on | Suset | Cache predicate → VP-table-OID lookups in shared memory |
| `cache_budget` | integer | 256 | Suset | Shared memory pages reserved for in-memory predicate-stats cache |
| `plan_cache_capacity` | integer | 512 | Suset | Maximum number of compiled SPARQL plans held in the plan cache |
| `plan_cache_size` | integer | 8192 | Suset | Maximum byte size of the SPARQL plan cache (KB) |
| `rule_plan_cache` | bool | on | Suset | Enable plan-level caching for Datalog rule SQL |
| `rule_plan_cache_size` | integer | 256 | Suset | Maximum number of Datalog rule SQL plans cached |

---

## SPARQL Engine

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `sparql_max_rows` | integer | 10000 | Userset | Maximum result rows returned by any SPARQL SELECT |
| `sparql_max_algebra_depth` | integer | 64 | Userset | Maximum depth of the SPARQL algebra tree before the query is rejected |
| `sparql_max_triple_patterns` | integer | 256 | Userset | Maximum number of triple patterns in a single BGP |
| `sparql_overflow_action` | string | `'error'` | Userset | Action on row-limit overflow: `'error'`, `'truncate'`, or `'warn'` |
| `sparql_strict` | bool | off | Userset | Reject queries that reference undefined prefixes or unknown predicates |
| `strict_sparql_filters` | bool | off | Userset | Raise an error (instead of returning NULL) on ill-typed FILTER arguments |
| `bgp_reorder` | bool | on | Userset | Enable basic graph pattern join-order optimization |
| `star_join_collapse` | bool | on | Userset | Collapse star patterns (same subject) into a single VP scan |
| `topn_pushdown` | bool | on | Userset | Push `LIMIT` into VP scans for ORDER BY queries |
| `parallel_query_min_joins` | integer | 4 | Userset | Minimum joins in a BGP before parallel scan is considered |
| `all_nodes_predicate_limit` | integer | 100 | Userset | Maximum predicates scanned in `ALL_PREDICATES()` expressions |
| `approx_distinct` | bool | off | Userset | Use HyperLogLog approximation for `COUNT(DISTINCT ...)` |
| `describe_strategy` | string | `'cbd'` | Userset | DESCRIBE form: `'cbd'` (concise bounded description) or `'symcbd'` |
| `describe_form` | string | `'turtle'` | Userset | Serialization format for DESCRIBE responses |
| `describe_max_depth` | integer | 16 | Userset | Maximum hop depth for symmetric CBD DESCRIBE |
| `default_graph` | string | `''` | Userset | Named graph IRI used when no GRAPH clause is specified (empty = default graph) |
| `use_graph_context` | bool | on | Userset | Include graph column in SPARQL result bindings |

---

## Datalog & Inference

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `datalog_max_depth` | integer | 64 | Userset | Maximum recursion depth for Datalog fixpoint evaluation |
| `datalog_max_derived` | integer | 1000000 | Userset | Maximum derived triples before an inference run is aborted |
| `datalog_parallel_workers` | integer | 4 | Suset | Number of parallel workers for stratum evaluation |
| `datalog_parallel_threshold` | integer | 10000 | Suset | Minimum triple count to trigger parallel stratum evaluation |
| `datalog_sequence_batch` | integer | 1000 | Suset | Batch size for sequential Datalog stratum evaluation |
| `datalog_antijoin_threshold` | integer | 100 | Userset | Estimated right-side rows below which an anti-join uses a hash strategy |
| `datalog_cost_reorder` | bool | on | Userset | Enable cost-based join reordering for Datalog rule bodies |
| `datalog_cost_bound_s_divisor` | real | 10.0 | Userset | Divisor applied to subject-bound cardinality estimates |
| `datalog_cost_bound_so_divisor` | real | 100.0 | Userset | Divisor applied to subject+object-bound cardinality estimates |
| `datalog_citus_dispatch` | bool | off | Suset | Dispatch Datalog SQL to all Citus worker shards |
| `wcoj_enabled` | bool | on | Userset | Enable worst-case optimal join (leapfrog triejoin) |
| `wcoj_min_tables` | integer | 3 | Userset | Minimum number of join tables before WCOJ is considered |
| `wcoj_min_cardinality` | integer | 1000 | Userset | Minimum estimated cardinality before WCOJ is considered |
| `tabling` | bool | on | Userset | Enable tabling (memoization) for recursive Datalog |
| `tabling_ttl` | integer | 3600 | Userset | Seconds before a tabled result is considered stale |
| `dred_enabled` | bool | on | Suset | Enable Delete–Rederive (DReD) for incremental retraction |
| `dred_batch_size` | integer | 1000 | Suset | Batch size for DReD retraction cycles |
| `wfs_max_iterations` | integer | 100 | Userset | Maximum iterations for well-founded semantics computation |
| `demand_transform` | bool | on | Userset | Apply magic-sets demand transformation to recursive rules |
| `rule_graph_scope` | string | `'default'` | Userset | Which graphs rules fire in: `'default'` or `'all'` |
| `rule_conflict_check_on_load` | bool | on | Suset | Check for conflicting rule heads when rules are loaded |
| `record_derivations` | bool | off | Userset | Write derivation provenance to `_pg_ripple.derivations` |
| `strict_goal_validation` | bool | off | Userset | Reject Datalog goal atoms that reference unregistered predicates |
| `suggest_rules_max_candidates` | integer | 20 | Userset | Maximum candidate rules returned by `suggest_rules()` |

---

## Proof Trees & Rule Explanation

> **New in v0.116.0** (M16-07, M16-19)

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `proof_tree_max_depth` | integer | 64 | Userset | Maximum depth of a proof tree built by `justify()`/`build_proof_tree()`. Exceeding this limit emits a `PT0480` warning and truncates the tree |
| `proof_tree_max_nodes` | integer | 10000 | Userset | Maximum total nodes across all branches of a proof tree. Exceeding this limit emits a `PT0481` warning and stops expansion |
| `rule_explanation_cache_max_entries` | integer | 1000 | Userset | Capacity of the per-process LRU cache for `explain_rule()` results. Set to 0 to disable |

---

## SHACL Validation

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `shacl_mode` | string | `'warn'` | Userset | SHACL validation mode: `'warn'`, `'error'`, or `'off'` |
| `shacl_rule_cwb` | bool | on | Suset | Apply closed-world behaviour to SHACL rule evaluation |
| `shacl_rule_max_iterations` | integer | 100 | Suset | Maximum SHACL rule fixpoint iterations |
| `shacl_score_log_retention_days` | integer | 30 | Suset | Days to retain SHACL validation score history |
| `enforce_constraints` | bool | on | Userset | Raise errors for SHACL constraint violations at INSERT time |

---

## owl:sameAs & Entity Resolution

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `sameas_reasoning` | bool | on | Userset | Enable owl:sameAs canonicalization and cluster merging |
| `sameas_max_cluster_size` | integer | 10000 | Suset | Maximum cluster size before owl:sameAs merging is blocked |
| `sameas_apply_rate_limit` | integer | 100 | Suset | Maximum owl:sameAs merges per second |
| `record_sameas_anomalies` | bool | on | Suset | Log anomalous owl:sameAs pairs to `_pg_ripple.sameas_anomalies` |
| `sameas_anomaly_log_retention` | integer | 30 | Suset | Days to retain owl:sameAs anomaly log entries |
| `default_fuzzy_threshold` | real | 0.85 | Userset | Default similarity threshold for `fuzzy_match()` |
| `string_similarity_extensions_ok` | bool | on | Userset | Allow string-similarity UDFs to call `pg_trgm` / `fuzzystrmatch` |
| `bloom_max_input_length` | integer | 4096 | Userset | Maximum byte length of strings passed to Bloom-filter deduplication |

---

## ER Monitoring

> **New in v0.116.0** (M16-01)

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `er_monitoring_retention_days` | integer | 30 | Suset | Days to retain ER monitoring stream-table rows before automatic pruning by the background worker. Range: 1–3650 |

---

## Federation

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `federation_allowed_endpoints` | string | `''` | Suset | Comma-separated allow-list of service endpoint URL prefixes |
| `federation_allow_private` | bool | off | Suset | Allow federation to private-range IP addresses |
| `federation_adaptive_timeout` | bool | on | Userset | Dynamically adjust per-endpoint timeout based on observed latency |
| `allow_unregistered_service_endpoints` | bool | off | Suset | Permit SERVICE calls to endpoints not listed in `federation_endpoints` |
| `vector_federation_timeout_ms` | integer | 5000 | Userset | Timeout (ms) for remote vector-search federation calls |

---

## CDC & Replication

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `cdc_bridge_enabled` | bool | off | Suset | Enable the CDC outbox bridge for downstream consumers |
| `cdc_bridge_batch_size` | integer | 100 | Suset | CDC notifications batched before a pg_tide outbox flush |
| `cdc_bridge_flush_ms` | integer | 200 | Suset | Maximum milliseconds between CDC outbox flushes |
| `cdc_bridge_outbox_table` | string | `''` | Suset | Fully-qualified name of the CDC outbox table |
| `cdc_slot_idle_timeout_seconds` | integer | 3600 | Suset | Idle timeout (s) before orphaned CDC slots are dropped |
| `cdc_watermark_batch_size` | integer | 100 | Suset | CDC watermark rows processed per cycle |
| `cdc_watermark_flush_interval_ms` | integer | 50 | Suset | Watermark flush interval (ms) |
| `temporal_cdc_enabled` | bool | on | Suset | Enable CDC support for temporal (valid-time) triple versions |
| `temporal_data_model` | string | `'point'` | Suset | Temporal data model: `'point'` or `'interval'` |
| `enable_temporal_operators` | bool | off | Userset | Enable temporal SPARQL extensions (`VALID_TIME`, `AS OF`) |
| `replication_enabled` | bool | off | Suset | Enable built-in logical replication between pg_ripple instances |
| `replication_batch_size` | integer | 100 | Suset | Triples per logical-replication batch |
| `replication_batch_interval_ms` | integer | 500 | Suset | Interval (ms) between replication batch flushes |
| `replication_conflict_strategy` | string | `'last_writer_wins'` | Suset | Logical apply conflict strategy; current worker uses `'last_writer_wins'` |
| `read_replica_dsn` | string | `''` | Suset | Connection string for directing read-only queries to a replica |
| `trickle_integration` | bool | on | Userset | Legacy relay integration switch; disables the pg_tide CDC bridge when set to `off` |

---

## Bidirectional Relay

> **New in v0.116.0** (M16-11)

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `bidi_relay_max_inflight` | integer | 1000 | Suset | Maximum in-flight messages in the bidi relay channel before overflow handling triggers |
| `bidi_relay_drop_policy` | string | `NULL` | Userset | Overflow drop policy for the bidi relay channel. `NULL` = block (default), `'drop-oldest'` = silently discard the oldest message, `'drop-newest'` = discard the incoming message |

---

## Probabilistic & Bayesian Reasoning

> **`bayesian_propagation_max_depth` new in v0.116.0** (M16-20)

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `probabilistic_datalog` | bool | off | Userset | Enable probabilistic Datalog evaluation (noisy-OR model) |
| `confidence_propagation_max_depth` | integer | 10 | Userset | Maximum depth for confidence propagation via `update_confidence()` |
| `bayesian_propagation_max_depth` | integer | 10 | Userset | Maximum depth for Bayesian downstream propagation in `propagate_downstream()`. Replaces the hard-coded default for the Bayesian path only |
| `confidence_batch_size` | integer | 500 | Suset | Triples processed per batch in the async confidence pipeline |
| `confidence_reprocessing_interval` | integer | 3600 | Suset | Seconds between scheduled confidence re-computation cycles |
| `confidence_update_strategy` | string | `'lazy'` | Suset | Confidence update strategy: `'lazy'` or `'eager'` |
| `conflict_confidence_penalty` | real | 0.1 | Userset | Confidence multiplier applied to conflicting triples |
| `cwb_confidence_propagation` | bool | on | Suset | Apply CWB (closed-world bias) during confidence propagation |
| `prob_datalog_max_iterations` | integer | 100 | Userset | Maximum fixpoint iterations for probabilistic Datalog |
| `prob_datalog_convergence_delta` | real | 0.001 | Userset | Convergence threshold for probabilistic Datalog fixpoint |
| `prob_datalog_cyclic` | bool | on | Userset | Allow cyclic dependencies in probabilistic Datalog programs |
| `prob_datalog_cyclic_strict` | bool | off | Userset | Raise an error instead of warning when cyclic probabilistic rules are detected |
| `prov_enabled` | bool | off | Userset | Enable provenance tracking (PROV-O) for all triple insertions |
| `prov_confidence` | real | 1.0 | Userset | Default confidence assigned to triples with no explicit confidence |
| `evidence_log_retention` | integer | 30 | Suset | Days to retain provenance evidence log entries |

---

## LLM / Embedding & RAG

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `embedding_model` | string | `''` | Userset | Embedding model identifier (e.g. `'text-embedding-3-small'`) |
| `embedding_api_url` | string | `''` | Suset | Base URL of the embedding API endpoint |
| `embedding_api_key` | string | `''` | Suset | API key for the embedding service (stored in shared memory, not logs) |
| `embedding_dimensions` | integer | 1536 | Userset | Embedding vector dimension |
| `embedding_batch_size` | integer | 64 | Userset | Number of literals sent per embedding API request |
| `embedding_index_type` | string | `'ivfflat'` | Suset | pgvector index type: `'ivfflat'` or `'hnsw'` |
| `embedding_precision` | string | `'float4'` | Suset | Storage precision for embedding vectors: `'float4'` or `'float2'` |
| `pgvector_enabled` | bool | on | Suset | Enable pgvector integration for hybrid SPARQL + vector search |
| `auto_embed` | bool | off | Userset | Automatically embed new literals when they are inserted |

---

## PageRank & Centrality

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `pagerank_rules` | string | `'default'` | Userset | Named rule set used for PageRank weight computation |
| `pagerank_wcoj_threshold` | integer | 10000 | Userset | Minimum edge count before WCOJ is used in PageRank |
| `pagerank_selective_threshold` | real | 0.01 | Userset | Minimum personalization weight for selective PageRank |
| `pagerank_temp_threshold` | integer | 50000 | Suset | Row count above which PageRank uses temporary tables |
| `pagerank_shacl_threshold` | real | 0.5 | Userset | Minimum PageRank score for SHACL-constrained node filtering |
| `pagerank_trickle_confidence_attenuation` | bool | on | Userset | Attenuate incremental K-hop PageRank deltas by edge confidence |
| `pagerank_sketch_width` | integer | 2048 | Suset | Width of the Count-Min sketch for approximate PageRank |
| `pagerank_sketch_depth` | integer | 5 | Suset | Depth of the Count-Min sketch for approximate PageRank |

---

## Observability & Tracing

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `tracing_enabled` | bool | off | Suset | Enable OpenTelemetry distributed tracing |
| `tracing_exporter` | string | `'otlp'` | Suset | Trace exporter backend: `'otlp'` or `'stdout'` |
| `tracing_otlp_endpoint` | string | `''` | Suset | OTLP exporter endpoint URL |
| `tracing_traceparent` | string | `''` | Userset | W3C `traceparent` header value for the current session |
| `audit_log_enabled` | bool | off | Suset | Enable audit logging of all SPARQL/Datalog operations |
| `audit_retention` | integer | 90 | Suset | Days to retain audit log entries |
| `explanation_cache_ttl` | integer | 3600 | Userset | Seconds before a cached `explain_rule()` DB entry is considered stale |
| `rule_explanation_cache_ttl` | integer | 3600 | Userset | Alias for `explanation_cache_ttl` (DB-side TTL for rule explanations) |

---

## Security & Access Control

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `rls_bypass` | bool | off | Userset | Allow superusers to bypass RLS policies on VP tables |
| `copy_rdf_allowed_paths` | string | `''` | Suset | Comma-separated path prefixes permitted for `COPY`-based RDF loads |
| `block_on_conflict` | bool | off | Userset | Block inserts that conflict with existing triples (instead of ON CONFLICT DO NOTHING) |

---

## HTTP Companion (Arrow Flight)

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `arrow_batch_size` | integer | 1000 | Userset | Rows per Arrow record batch in bulk export |
| `arrow_flight_secret` | string | `''` | Suset | HMAC secret used to sign Arrow Flight session tickets |
| `arrow_flight_expiry_secs` | integer | 3600 | Suset | Expiry time (s) for Arrow Flight session tickets |
| `arrow_unsigned_tickets_allowed` | bool | off | Suset | Accept Arrow Flight tickets without an HMAC signature (testing only) |

---

## Citus Integration

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `citus_sharding_enabled` | bool | off | Suset | Enable Citus distributed sharding for VP tables |
| `citus_service_pruning` | bool | off | Suset | Enable predicate-level shard pruning for SERVICE queries |
| `citus_prune_carry_max` | integer | 1000 | Userset | Maximum number of shard bindings carried across joins during pruning |
| `citus_trickle_compat` | bool | off | Userset | Legacy compatibility mode using `colocate_with => 'none'` for Citus CDC/IVM integrations |

---

## Background Workers

| GUC | Type | Default | Context | Description |
|-----|------|---------|---------|-------------|
| `worker_database` | string | `''` | Postmaster | Target database name for the HTAP merge and housekeeping background workers |

---

## v0.116.0 New GUCs Summary

The following GUCs were added in v0.116.0 (A16 milestone):

| GUC | Category | Ticket |
|-----|----------|--------|
| `er_monitoring_retention_days` | ER Monitoring | M16-01 |
| `proof_tree_max_depth` | Proof Trees | M16-07 |
| `proof_tree_max_nodes` | Proof Trees | M16-07 |
| `rule_explanation_cache_max_entries` | Proof Trees / Rule Explanation | M16-19 |
| `bayesian_propagation_max_depth` | Probabilistic Reasoning | M16-20 |
| `bidi_relay_drop_policy` | Bidirectional Relay | M16-11 |

---

## Advisory Lifecycle Policy

GUC-related security advisories are tracked in `audit.toml` under a quarterly review schedule:

1. **Open advisory** → triage within one sprint.
2. **Ignore with expiry** → add `[ignore]` entry with `expires` at most 12 months out.
3. **Quarterly review** → when an expiry approaches, re-evaluate the threat model.
4. **Expiry forces re-decision** → expired entries are treated as new findings.

See [`audit.toml`](../audit.toml) for the full advisory ignore list.
