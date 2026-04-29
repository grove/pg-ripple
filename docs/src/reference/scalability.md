# Scalability Reference

pg_ripple supports horizontal scalability via [Citus](https://www.citusdata.com/) for PostgreSQL.

## Citus Integration

When `pg_ripple.citus_sharding_enabled = on` and the Citus extension is installed, pg_ripple
distributes VP tables across Citus worker nodes using the subject (`s`) column as the shard key.

### Features

- **Shard-pruning for bound subjects**: SPARQL queries with a bound subject IRI are rewritten
  to include `WHERE s = <encoded_id>` so Citus routes the query directly to the shard holding
  that subject (v0.59.0).
- **Direct-shard bulk load**: `load_*` functions write triples directly to the physical Citus
  shard tables, bypassing the coordinator routing step (v0.61.0 CITUS-21).
- **BRIN summarise on rebalance**: VP main partitions are BRIN-indexed; after a shard rebalance
  the summarise step is re-run on affected shards (v0.63.0).
- **HyperLogLog COUNT(DISTINCT)**: Citus HLL extension is leveraged for approximate
  `COUNT(DISTINCT ?var)` aggregates in federated queries (v0.63.0, v0.68.0).
- **Per-named-graph RLS propagation**: `grant_graph_access` propagates row-level security
  policies to all Citus worker nodes via `run_command_on_all_nodes` (v0.61.0 CITUS-05).
  Integration test: `tests/integration/citus_rls_propagation.sh` (planned for v0.71.0).

### Configuration

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.citus_sharding_enabled` | `off` | Enable Citus distribution of VP tables |
| `pg_ripple.citus_shard_count` | `32` | Number of shards per VP table |
| `pg_ripple.citus_hll_enabled` | `off` | Use HLL extension for COUNT(DISTINCT) |

### Limitations

- Citus multi-node integration tests are planned for v0.71.0 (CITUS-INT-01).
- Cross-shard property-path queries may not benefit from shard pruning.

See also: [Query Optimization](query-optimization.md), [Architecture](architecture.md).
