# Backup and Restore

pg_ripple stores all data in standard PostgreSQL tables inside the `_pg_ripple` schema. This means standard PostgreSQL backup tools (`pg_dump`, `pg_restore`, continuous archiving, and PITR) work without modification.

---

## pg_dump / pg_restore

### Full database backup

```bash
pg_dump -Fc -f backup.dump mydb
pg_restore -d newdb backup.dump
```

After restore, run:

```sql
-- Ensure extension is loaded
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Reconcile the predicates catalog
SELECT pg_ripple.promote_rare_predicates();
```

### Extension schema only (structure)

```bash
pg_dump --schema=_pg_ripple --schema-only -f schema.sql mydb
```

### Data only

```bash
pg_dump --schema=_pg_ripple --data-only -Fc -f data.dump mydb
```

---

## VP table considerations

Each predicate with enough triples gets its own VP table in the `_pg_ripple` schema (`_pg_ripple.vp_{id}`, `_pg_ripple.vp_{id}_delta`, `_pg_ripple.vp_{id}_main`). `pg_dump` includes all of these automatically when you dump the `_pg_ripple` schema.

**After restore:**

1. Run `pg_ripple.compact()` to flush any lingering delta rows to main.
2. Run `ANALYZE _pg_ripple.*` (or `pg_ripple.vacuum()`) to refresh planner statistics.
3. Optionally run `pg_ripple.reindex()` to rebuild indices.

---

## PITR with WAL

pg_ripple relies entirely on standard PostgreSQL WAL for crash recovery. Enable WAL archiving in `postgresql.conf`:

```ini
wal_level = replica
archive_mode = on
archive_command = 'cp %p /archive/%f'
```

Restore to a point-in-time using `pg_basebackup` and a recovery configuration. No pg_ripple-specific steps are required.

---

## Logical replication

pg_ripple's VP tables can be included in a logical replication publication:

```sql
-- On the primary
CREATE PUBLICATION pg_ripple_pub
    FOR TABLES IN SCHEMA _pg_ripple;

-- On the replica
CREATE SUBSCRIPTION pg_ripple_sub
    CONNECTION 'host=primary dbname=mydb'
    PUBLICATION pg_ripple_pub;
```

> **Note**: The merge background worker should be disabled on the replica (`SET pg_ripple.merge_threshold = 0`) to avoid write conflicts.

---

## Dictionary integrity

The `_pg_ripple.dictionary` table maps every IRI, blank node, and literal to an `i64` identifier. VP tables store only these integer IDs; the dictionary is required to decode query results.

**Always back up `_pg_ripple.dictionary` together with VP tables.** A VP-table backup without the dictionary is unreadable.

After a partial restore (e.g., restoring only some VP tables), run:

```sql
SELECT pg_ripple.vacuum_dictionary();
```

This removes dictionary entries that are no longer referenced by any VP table.

---

## Exporting to RDF files

For portable backups that are independent of the PostgreSQL version:

```sql
-- Export all graphs as N-Quads
COPY (SELECT pg_ripple.export_nquads(NULL)) TO '/backup/export.nq';
```

Or use the streaming exporter for large datasets:

```sql
COPY (
    SELECT line FROM pg_ripple.export_turtle_stream(NULL)
) TO '/backup/export.ttl';
```

To restore from an RDF export:

```sql
SELECT pg_ripple.load_nquads(pg_read_file('/backup/export.nq'));
-- or
SELECT pg_ripple.load_turtle(pg_read_file('/backup/export.ttl'));
```

---

## Crash recovery (v0.20.0)

pg_ripple relies entirely on PostgreSQL's WAL-based crash recovery. No additional steps are required after an unexpected shutdown (power failure, `kill -9`, OOM kill).

### What happens on restart after a crash

1. **PostgreSQL replays WAL** — all committed transactions are recovered; uncommitted transactions are rolled back.
2. **The HTAP merge worker restarts** — the background worker re-attaches to shared memory and resumes from a clean state. Any partial merge is discarded; the delta tables retain all committed but un-merged triples.
3. **The dictionary is consistent** — dictionary inserts use `ON CONFLICT DO NOTHING … RETURNING`, which is atomic. No orphaned or duplicate entries survive a crash.
4. **The predicates catalog is consistent** — `_pg_ripple.predicates` is updated within the same transaction as the VP table write, so the counts are always coherent.

### Verifying recovery

Run the following after a suspected crash:

```sql
-- 1. Check for negative triple counts (should always be 0)
SELECT count(*) FROM _pg_ripple.predicates WHERE triple_count < 0;

-- 2. Check for duplicate dictionary entries (should always be 0)
SELECT count(*) FROM (
    SELECT value, kind, count(*) AS n
    FROM _pg_ripple.dictionary
    GROUP BY value, kind
    HAVING count(*) > 1
) dups;

-- 3. Reconcile the predicates catalog
SELECT pg_ripple.promote_rare_predicates();

-- 4. Refresh planner statistics
SELECT pg_ripple.vacuum();
```

### Running the crash recovery test suite

The automated crash recovery tests (introduced in v0.20.0) simulate `kill -9` during merge, bulk load, and validation, then verify all assertions above:

```bash
# Requires: cargo pgrx start pg18
just test-crash-recovery
```

Individual scripts:

```bash
bash tests/crash_recovery/merge_during_kill.sh
bash tests/crash_recovery/dict_during_kill.sh
bash tests/crash_recovery/shacl_during_violation.sh
```

### WAL replay and PITR

pg_ripple is fully compatible with PostgreSQL Point-in-Time Recovery (PITR). Enable WAL archiving and use `pg_basebackup` as you would for any PostgreSQL database. The `_pg_ripple` schema is recovered along with all other schema objects.

See the [PITR with WAL](#pitr-with-wal) section above for configuration details.
