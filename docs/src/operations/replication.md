# Logical Replication

pg_ripple v0.54.0 adds RDF logical replication: a primary database streams its
RDF triple changes to one or more replica databases in near-real-time using
PostgreSQL's built-in logical decoding infrastructure.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  PRIMARY                                                          │
│  ┌──────────────┐   INSERT/DELETE   ┌────────────────────────┐   │
│  │  VP delta    │ ──────────────→  │  WAL (logical decoding)│   │
│  │  tables      │                   │  slot: pg_ripple_sub   │   │
│  └──────────────┘                   └───────────┬────────────┘   │
└─────────────────────────────────────────────────│────────────────┘
                                                  │ streaming replication
┌─────────────────────────────────────────────────│────────────────┐
│  REPLICA                                         ↓               │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  _pg_ripple.replication_status (pending batches)         │    │
│  └─────────────────────────┬────────────────────────────────┘    │
│                             ↓ logical_apply_worker               │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  pg_ripple.load_ntriples() — applies triples in order    │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

The logical decoding slot (`pg_ripple_sub`) captures every `INSERT` and `DELETE`
on the `_pg_ripple` schema's VP delta tables.  The changes are decoded into
N-Triples format and written to `_pg_ripple.replication_status` on the replica.
The `logical_apply_worker` background process (enabled when
`pg_ripple.replication_enabled = on`) reads these pending batches and applies
them via `load_ntriples()`.

## Setup Walkthrough

### 1. Primary configuration

In `postgresql.conf` on the primary:

```conf
wal_level = logical
max_replication_slots = 4
max_wal_senders = 4
```

Create the publication after `CREATE EXTENSION pg_ripple`:

```sql
CREATE PUBLICATION pg_ripple_pub
  FOR ALL TABLES IN SCHEMA _pg_ripple;
```

### 2. Replica configuration

Add to `postgresql.conf` on the replica:

```conf
pg_ripple.replication_enabled = on
pg_ripple.replication_conflict_strategy = 'last_writer_wins'
```

Create the extension and subscription on the replica:

```sql
CREATE EXTENSION pg_ripple;

CREATE SUBSCRIPTION pg_ripple_sub
  CONNECTION 'host=primary-host port=5432 dbname=mydb user=replication password=secret'
  PUBLICATION pg_ripple_pub;
```

### 3. Verify replication is running

```sql
SELECT * FROM pg_ripple.replication_stats();
```

```
   slot_name    | lag_bytes | last_applied_lsn | last_applied_at
----------------+-----------+------------------+---------------------
 pg_ripple_sub  |         0 | 0/15000A8        | 2026-04-24 12:00:01
```

A `lag_bytes` of 0 means the replica is fully caught up.

## Lag Monitoring

Query replication lag in bytes at any time:

```sql
SELECT slot_name, lag_bytes
FROM pg_ripple.replication_stats()
WHERE lag_bytes > 1000000;  -- alert if > 1 MB behind
```

Integrate with Prometheus via `pg_stat_statements` or the built-in OTEL tracing
endpoint (`pg_ripple.tracing_otlp_endpoint`).

## Conflict Resolution

The `last_writer_wins` strategy (the only strategy in v0.54.0) keeps the triple
with the highest Statement ID (SID) when two replicas receive the same `(s, p, g)`
triple with different objects.  This matches eventual-consistency semantics for
typical RDF workloads.

Set the strategy via:

```sql
SET pg_ripple.replication_conflict_strategy = 'last_writer_wins';
```

## Failover Procedure

1. Stop writes to the primary (or wait for the replica lag to reach 0).
2. Promote the replica: `pg_ctl promote -D /var/lib/postgresql/18/main`
3. Update your application connection string to point to the new primary.
4. Re-create the replication subscription on any new replicas.

## GUC Reference

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.replication_enabled` | `off` | Enable the logical apply worker |
| `pg_ripple.replication_conflict_strategy` | `last_writer_wins` | Conflict resolution strategy |

See also the full [GUC reference](../reference/guc-reference.md).
