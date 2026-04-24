# High Availability Decision Tree

This page helps you choose the right HA topology for your pg_ripple deployment.

## Decision Tree

```
Do you need sub-second read scalability across multiple nodes?
├─ YES → Use streaming replication (primary + read replicas)
│        + pg_ripple logical replication for RDF-specific apply
└─ NO  → Single node with good hardware is likely sufficient

Are you running on Kubernetes?
├─ YES → Use CloudNativePG operator (see cloudnativepg.md)
│        or the pg_ripple Helm chart (see kubernetes.md)
└─ NO  → Self-managed PostgreSQL with streaming replication

Do you need the replica to run SPARQL queries against RDF data?
├─ YES → Enable pg_ripple.replication_enabled = on on the replica
│        so the logical apply worker keeps the dictionary + VP tables in sync
└─ NO  → Standard PostgreSQL streaming replication is sufficient
         (the replica can still serve SELECT queries via PG's built-in machinery)
```

## Supported Topologies

### 1. Single Node

For workloads up to ~50 M triples and moderate write rates.  No HA — use
pg_ripple's built-in WAL + periodic backups for durability.

```
  [Client] → [pg_ripple primary]
```

### 2. Streaming Replication + RDF Logical Apply

The recommended topology for production HA.  PostgreSQL streaming replication
keeps the replica byte-for-byte identical.  pg_ripple's logical apply worker
additionally decodes VP-table changes into N-Triples and re-applies them so the
replica's dictionary and VP tables remain queryable independently.

```
  [Writes] → [pg_ripple primary] ──streaming──→ [pg_ripple replica]
                                  ──logical──→  [logical_apply_worker]
```

**Requirements:**
- `wal_level = logical` on the primary
- `pg_ripple.replication_enabled = on` on the replica
- One replication slot per replica

**Lag target:** < 1 second at 10 k-triple/s insert rate.

### 3. CloudNativePG

The recommended topology for Kubernetes environments.  CNP manages the primary
election, failover, and rolling upgrades automatically.  Use the extension image
volume to avoid maintaining a custom PostgreSQL container.

```
  [Writes] → [CNP primary Pod] ── CNP streaming ──→ [CNP standby Pods ×2]
```

**Requirements:** CloudNativePG operator ≥ 1.24.
See [cloudnativepg.md](cloudnativepg.md) for setup.

### 4. Multi-Region Federated Query

For globally distributed data, keep separate pg_ripple instances per region and
use SPARQL `SERVICE` federation to query across them:

```sparql
SELECT ?s ?p ?o WHERE {
  SERVICE <https://us.example.com/sparql> { ?s ?p ?o }
  UNION
  SERVICE <https://eu.example.com/sparql> { ?s ?p ?o }
}
```

Each regional instance is independently HA via topology 2 or 3.

## Trade-offs

| Topology | Setup complexity | Failover RTO | Write scale-out | SPARQL on replicas |
|----------|-----------------|-------------|-----------------|-------------------|
| Single node | Low | N/A (manual restore) | No | N/A |
| Streaming + logical apply | Medium | ~30s (manual failover) | No | Yes |
| CloudNativePG | Medium | ~10s (automatic) | No | Yes |
| Multi-region federation | High | Per-region | Yes (writes to local) | Yes |

## pg_ripple vs Standard PG Streaming Replication

Standard PostgreSQL streaming replication copies the raw WAL bytes — including
internal storage-format details.  This is sufficient for read replicas, but the
replica's pg_ripple state may not be independently queryable via SPARQL without
the logical apply worker (which reconstructs the dictionary and VP tables from
decoded N-Triples changes).

Enable `pg_ripple.replication_enabled = on` on the replica to activate the
logical apply worker and ensure full SPARQL query capability.

## Monitoring Replication Lag

```sql
-- On the replica
SELECT * FROM pg_ripple.replication_stats();

-- On the primary (standard PG view)
SELECT slot_name, active, lag
FROM pg_replication_slots;
```

Set up alerting when `lag_bytes > 10 MB` (roughly 1–2 s at typical write rates).
