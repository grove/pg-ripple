# pg_upgrade Compatibility

This page documents how to upgrade a PostgreSQL installation that has pg_ripple
installed using `pg_upgrade`.

## Supported Upgrade Paths

| From PG version | To PG version | pg_ripple compatibility |
|----------------|---------------|------------------------|
| PostgreSQL 18.x | PostgreSQL 18.x (patch) | ✅ Fully supported — use `ALTER EXTENSION pg_ripple UPDATE` |
| PostgreSQL 17.x | PostgreSQL 18.x | ⛔ pg_ripple requires PostgreSQL 18 only; clean install required |

> **Note:** pg_ripple targets PostgreSQL 18 exclusively. If you are upgrading
> from an older PostgreSQL major version, you must dump and restore your data
> rather than using `pg_upgrade`.

## Pre-upgrade Steps

Before running `pg_upgrade`:

1. **Check migration path** — ensure `sql/pg_ripple--<old>--<new>.sql` exists
   for each version step between your installed version and the target version.

2. **Verify extension version** — confirm the installed version:
   ```sql
   SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';
   ```

3. **Back up the database** — always back up before any upgrade:
   ```bash
   pg_dump -Fc -f pg_ripple_backup.dump <database>
   ```

4. **Run `cargo pgrx install`** — install the new pg_ripple version into the
   new PostgreSQL 18 instance:
   ```bash
   cargo pgrx install --pg-config /path/to/new/pg_config
   ```

## Upgrading the Extension (same PostgreSQL major version)

After installing a new pg_ripple binary:

```sql
-- Apply all migrations in sequence automatically
ALTER EXTENSION pg_ripple UPDATE;

-- Check the new version
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';
```

`ALTER EXTENSION pg_ripple UPDATE` follows the chain of migration scripts
in `sql/pg_ripple--<from>--<to>.sql` automatically. Each script is
idempotent for schema changes and applies in order.

## Migration Script Chain

All available migration paths as of v0.48.0:

| From | To | Script |
|------|----|--------|
| 0.1.0 | 0.2.0 | `sql/pg_ripple--0.1.0--0.2.0.sql` |
| 0.2.0 | 0.3.0 | `sql/pg_ripple--0.2.0--0.3.0.sql` |
| … | … | … |
| 0.47.0 | 0.48.0 | `sql/pg_ripple--0.47.0--0.48.0.sql` |

To upgrade across multiple versions (e.g., 0.45.0 → 0.48.0):
```sql
ALTER EXTENSION pg_ripple UPDATE;  -- PostgreSQL follows the chain automatically
```

## Full Dump/Restore (cross-major-version or fresh install)

If you cannot use `ALTER EXTENSION pg_ripple UPDATE`, perform a dump/restore:

```bash
# 1. Dump all data (including _pg_ripple internal tables)
pg_dump -Fc --schema=pg_ripple --schema=_pg_ripple -f ripple_data.dump <database>

# 2. Install pg_ripple fresh in the new database
psql -c "CREATE EXTENSION pg_ripple" <new_database>

# 3. Restore data (use --no-owner if roles differ)
pg_restore --no-owner -d <new_database> ripple_data.dump
```

## Troubleshooting

### "extension has no update path from version X to version Y"

This means a migration script is missing. Check:
```bash
ls sql/pg_ripple--*.sql | sort
```
If a step is missing, contact the maintainers or manually apply the DDL
changes documented in the CHANGELOG for that version.

### "could not load library"

The new pg_ripple `.so`/`.dylib` binary is not installed in PostgreSQL's
`pkglibdir`. Run:
```bash
cargo pgrx install --pg-config /path/to/pg_config
```
