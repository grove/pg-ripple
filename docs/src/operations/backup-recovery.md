# Backup and Disaster Recovery

pg_ripple stores all data in standard PostgreSQL tables within the `_pg_ripple` schema. This means **every PostgreSQL backup tool works out of the box** — VP tables, the dictionary, the predicates catalog, SHACL constraints, Datalog rules, and inferred triples are all captured by `pg_dump`, WAL archiving, and streaming replication.

```admonish info title="No special export needed"
Unlike triple stores that require a separate RDF dump/reload cycle, pg_ripple data is just PostgreSQL data. Your existing backup infrastructure already covers it.
```

---

## What Gets Backed Up

| Object | Schema | Captured by pg_dump? | Notes |
|---|---|---|---|
| Dictionary table | `_pg_ripple.dictionary` | Yes | All IRI, blank node, and literal mappings |
| Predicates catalog | `_pg_ripple.predicates` | Yes | Predicate → VP table OID mapping |
| VP tables (main + delta + tombstones) | `_pg_ripple.vp_{id}_*` | Yes | One table set per predicate |
| Rare predicates table | `_pg_ripple.vp_rare` | Yes | Consolidated low-cardinality predicates |
| SHACL constraints | `_pg_ripple.shacl_*` | Yes | Shape definitions and validation state |
| Datalog rules | `_pg_ripple.rules` | Yes | Rule text and compiled plans |
| Inferred triples | VP tables, `source = 1` | Yes | Materialized inference results |
| Extension metadata | `pg_catalog` | Yes | Extension version and control file |
| Shared memory state | In-memory only | **No** | Dictionary LRU cache, merge worker counters |

```admonish note title="Shared memory state"
The dictionary LRU cache and merge worker counters live in shared memory and are **not** persisted to disk. They are rebuilt automatically on PostgreSQL restart. This is by design — the cache warms up quickly from normal query traffic.
```

---

## Logical Backup with pg_dump

### Full Database Dump

```bash
# Custom format (recommended — compressed, parallel-restore capable)
pg_dump -Fc -f pg_ripple_backup.dump mydb

# Plain SQL (human-readable, useful for auditing)
pg_dump -Fp -f pg_ripple_backup.sql mydb
```

### Extension-Only Dump

To back up only pg_ripple data without the rest of the database:

```bash
pg_dump -Fc \
  --schema=_pg_ripple \
  --schema=pg_ripple \
  -f pg_ripple_only.dump mydb
```

```admonish warning title="Include both schemas"
Always include **both** `_pg_ripple` (internal storage) and `pg_ripple` (public API functions). Restoring one without the other leaves the extension in an inconsistent state.
```

### Parallel Dump for Large Datasets

For databases with millions of triples, use parallel workers:

```bash
# Directory format required for parallel dump
pg_dump -Fd -j 4 -f pg_ripple_backup_dir/ mydb
```

The dictionary table and large VP tables will be dumped in parallel, significantly reducing backup time.

---

## Restoring from Backup

### Full Restore to a New Database

```bash
# Create the target database
createdb mydb_restored

# Restore (custom format)
pg_restore -d mydb_restored -Fc pg_ripple_backup.dump

# Restore (directory format, parallel)
pg_restore -d mydb_restored -Fd -j 4 pg_ripple_backup_dir/
```

### Restore from Plain SQL

```bash
psql -d mydb_restored -f pg_ripple_backup.sql
```

### Post-Restore Verification

After restoring, verify the extension is intact:

```sql
-- Check extension version
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';

-- Verify triple count
SELECT pg_ripple.stats();

-- Run the health check
SELECT pg_ripple.canary();

-- Spot-check a SPARQL query
SELECT pg_ripple.sparql($$
  SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }
$$);
```

```admonish tip title="Do VP tables survive dump/restore?"
**Yes.** VP tables are standard PostgreSQL heap tables with B-tree or BRIN indexes. `pg_dump` captures them exactly like any other table. The HTAP delta/main/tombstone split, indexes, and the merge worker view definitions are all preserved. After restore, the merge worker resumes normal operation once `shared_preload_libraries` includes `pg_ripple`.
```

---

## WAL-Based Continuous Archiving

For point-in-time recovery (PITR), configure WAL archiving:

### Enable WAL Archiving

In `postgresql.conf`:

```ini
wal_level = replica
archive_mode = on
archive_command = 'cp %p /backup/wal_archive/%f'
max_wal_senders = 3
```

### Take a Base Backup

```bash
pg_basebackup -D /backup/base -Ft -z -P
```

### Point-in-Time Recovery

Create a `recovery.signal` file and configure the restore target:

```ini
# postgresql.conf (or postgresql.auto.conf)
restore_command = 'cp /backup/wal_archive/%f %p'
recovery_target_time = '2026-04-19 14:30:00'
```

Start PostgreSQL — it will replay WAL up to the specified time.

```admonish warning title="HTAP merge and PITR"
If you recover to a point mid-merge, the merge worker will detect the incomplete state and re-run the merge on startup. No manual intervention is needed, but the first merge cycle after recovery may take longer than usual.
```

---

## Streaming Replication

pg_ripple works transparently with PostgreSQL streaming replication:

```bash
# On the replica
pg_basebackup -h primary-host -D /var/lib/postgresql/18/main -R -P
```

The `-R` flag writes the `standby.signal` and connection parameters. All VP tables, dictionary data, and HTAP state replicate via WAL.

```admonish note title="Merge worker on replicas"
The background merge worker does **not** run on read replicas. Replicas receive merged state via WAL replay from the primary. This is correct behavior — replicas should never write.
```

---

## Backup Strategy Recommendations

### Small Datasets (< 1M triples)

| Component | Recommendation |
|---|---|
| Method | `pg_dump -Fc` nightly |
| Retention | 7 daily + 4 weekly |
| RPO | 24 hours |
| RTO | Minutes |

### Medium Datasets (1M – 100M triples)

| Component | Recommendation |
|---|---|
| Method | WAL archiving + daily base backup |
| Retention | 7 daily base + continuous WAL |
| RPO | Seconds (WAL) |
| RTO | Minutes to hours |

### Large Datasets (> 100M triples)

| Component | Recommendation |
|---|---|
| Method | WAL archiving + pgBackRest or Barman |
| Retention | Incremental base + continuous WAL |
| RPO | Seconds (WAL) |
| RTO | Proportional to dataset size |

```admonish tip title="Test your restores"
Schedule monthly restore drills. A backup that has never been tested is not a backup. Automate the verification queries shown above as part of the drill.
```

---

## Disaster Recovery Checklist

1. **Before disaster**: WAL archiving enabled, base backups on schedule, replication lag monitored
2. **During incident**: identify the failure scope (single table, full database, or host loss)
3. **Recovery steps**:
   - Host loss → promote replica or restore from base backup + WAL
   - Corruption → PITR to last known good time
   - Accidental deletion → PITR to just before the DROP/DELETE
4. **Post-recovery**:
   - Run `SELECT pg_ripple.canary()` to verify health
   - Check `pg_ripple.stats()` for expected triple counts
   - Verify the merge worker is running (`merge_worker_pid > 0`)
   - Run representative SPARQL queries to confirm data integrity
   - Resume WAL archiving and replication

---

## Common Pitfalls

```admonish danger title="Don't forget shared_preload_libraries"
After restoring to a fresh PostgreSQL instance, ensure `shared_preload_libraries = 'pg_ripple'` is set in `postgresql.conf` **before** starting the server. Without it, the merge worker will not start, the dictionary cache will be unavailable, and queries will fall back to uncached dictionary lookups.
```

- **Schema ownership**: the restoring user must be a superuser or own both `_pg_ripple` and `pg_ripple` schemas
- **Sequence values**: `pg_dump` captures sequence state — statement IDs (`i` column) will continue from the correct value after restore
- **Tablespace placement**: if you used custom tablespaces for VP tables, ensure they exist on the target server before restoring
