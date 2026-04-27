# Audit Log

When `pg_ripple.audit_log_enabled = on`, every SPARQL UPDATE — `INSERT DATA`, `DELETE DATA`, `INSERT { … } WHERE`, `DELETE { … } WHERE`, `LOAD`, `CLEAR`, `MOVE`, `COPY`, `ADD` — is captured to `_pg_ripple.audit_log`. The audit log is the workhorse of compliance, debugging, and tenant attribution.

For a higher-level view of the audit story (PROV-O, RDF-star, point-in-time replay), see [Temporal & Provenance](../features/temporal-and-provenance.md).

---

## Schema

```sql
CREATE TABLE _pg_ripple.audit_log (
    id          BIGSERIAL    PRIMARY KEY,
    ts          TIMESTAMPTZ  NOT NULL DEFAULT now(),
    role        NAME         NOT NULL DEFAULT current_user,
    txid        BIGINT       NOT NULL DEFAULT txid_current(),
    operation   TEXT         NOT NULL,    -- 'INSERT DATA' | 'DELETE DATA' | …
    query       TEXT         NOT NULL,
    triple_delta INTEGER     NOT NULL     -- net change in triple count
);
```

The table is partitioned by month if `pg_ripple.audit_log_partition_monthly = on` is set at extension creation time.

---

## Configuration

| GUC | Default | Effect |
|---|---|---|
| `pg_ripple.audit_log_enabled` | `off` | Master switch |
| `pg_ripple.audit_log_payload_max` | `8192` | Truncate captured queries longer than this many characters |
| `pg_ripple.audit_log_partition_monthly` | `off` | Create monthly child partitions on extension create |

Toggle the GUC per database according to the compliance posture of that database. There is no global on/off — different tenants on the same instance can have different policies.

---

## Querying the log

```sql
-- Recent activity by role.
SELECT role, count(*) AS ops, sum(triple_delta) AS net_triples
FROM   _pg_ripple.audit_log
WHERE  ts > now() - interval '24 hours'
GROUP  BY role
ORDER  BY ops DESC;

-- All deletions in a transaction.
SELECT id, ts, role, query
FROM   _pg_ripple.audit_log
WHERE  txid = 12345678
ORDER  BY id;

-- Find the SPARQL UPDATE that introduced a specific triple.
-- (Combined with point_in_time(), reconstructs the change history.)
SELECT id, ts, role, query
FROM   _pg_ripple.audit_log
WHERE  query ILIKE '%<https://example.org/secret>%'
ORDER  BY ts DESC
LIMIT 10;
```

---

## Retention and cleanup

`purge_audit_log(before TIMESTAMPTZ)` removes old entries:

```sql
SELECT pg_ripple.purge_audit_log(before := now() - interval '90 days');
-- Returns the count of rows deleted.
```

For partitioned audit logs, drop whole partitions instead:

```sql
DROP TABLE _pg_ripple.audit_log_2025_q4;
```

---

## Shipping to a SIEM

The audit log is a regular PostgreSQL table — every shipping mechanism that works for tables works for it:

- **Logical replication** to a centralised audit warehouse.
- **Foreign data wrapper** to expose it inside a SIEM.
- **Trigger + `pg_notify`** to push entries into a SOC pipeline in real time.
- **CDC subscription** with `pg_ripple_http`'s WebSocket exposure for real-time streaming.

---

## Performance notes

- Logging is synchronous within the UPDATE statement. The cost is dominated by the `INSERT` into `_pg_ripple.audit_log` — roughly the same as one ordinary triple insert.
- For OLTP workloads with high UPDATE rates, enable partitioning and add a `BRIN(ts)` index. The default `id PRIMARY KEY` is sufficient for most cases.
- The log is written even when the SPARQL UPDATE itself is later rolled back — to capture *attempted* changes for forensics. Use `pg_ripple.audit_log_only_committed = on` to log only on commit.

---

## See also

- [Temporal & Provenance](../features/temporal-and-provenance.md)
- [Multi-Tenant Graphs](../features/multi-tenant-graphs.md)
- [Operations → Security](../operations/security.md)
