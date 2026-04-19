# Upgrading Safely

pg_ripple follows PostgreSQL's standard extension upgrade mechanism. Each release ships a migration script that `ALTER EXTENSION pg_ripple UPDATE` executes automatically, walking the version chain from your current version to the target.

---

## How Extension Upgrades Work

PostgreSQL extensions use a chain of migration scripts to move between versions. pg_ripple provides a script for every consecutive version pair:

```
pg_ripple--0.1.0--0.2.0.sql
pg_ripple--0.2.0--0.3.0.sql
pg_ripple--0.3.0--0.4.0.sql
...
pg_ripple--0.31.0--0.32.0.sql
```

When you run `ALTER EXTENSION pg_ripple UPDATE`, PostgreSQL finds the shortest path from your current version to the latest and executes each script in sequence.

```admonish info title="Migration scripts are idempotent"
Each migration script uses `IF NOT EXISTS`, `CREATE OR REPLACE`, and similar guards. If a migration is partially applied (e.g., due to a crash), re-running it is safe.
```

---

## Pre-Upgrade Checklist

### 1. Check Your Current Version

```sql
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';
```

### 2. Back Up the Database

```bash
pg_dump -Fc -f pre_upgrade_backup.dump mydb
```

```admonish danger title="Always back up before upgrading"
While migration scripts are tested, a backup lets you restore to the pre-upgrade state if anything goes wrong. This is especially important for major feature releases that add schema changes.
```

### 3. Review the Changelog

Read the [Release Notes](../reference/changelog.md) for every version between your current version and the target. Pay attention to:

- **Breaking changes**: renamed functions, changed return types, removed GUC parameters
- **Schema changes**: new columns on internal tables, new indexes
- **New dependencies**: additional `shared_preload_libraries` entries

### 4. Check for Active Connections

```sql
SELECT count(*) FROM pg_stat_activity
WHERE datname = current_database()
  AND pid != pg_backend_pid();
```

Disconnect all application connections before upgrading. The upgrade modifies extension catalog entries and may need exclusive locks on internal tables.

### 5. Verify the New Package Is Installed

The new `.so` (shared library) and SQL migration files must be present in the PostgreSQL extension directory before running `ALTER EXTENSION`:

```bash
# Check that the target version's migration script exists
ls $(pg_config --sharedir)/extension/pg_ripple--*

# Check that the shared library is updated
ls -la $(pg_config --pkglibdir)/pg_ripple.so
```

---

## Performing the Upgrade

### Step 1: Install the New Package

```bash
# From source
cargo pgrx install --pg-config $(pg_config) --release

# Or from a pre-built package
# dpkg -i pg_ripple-0.32.0-pg18.deb
```

### Step 2: Schedule a Maintenance Window

```admonish warning title="No zero-downtime upgrades"
pg_ripple does not yet support zero-downtime upgrades. Schedule the upgrade during a maintenance window. If you have read replicas, route read traffic to a replica during the upgrade window, but note that the replica will also need the new shared library installed before promotion.
```

### Step 3: Restart PostgreSQL (If Required)

Some releases update the shared library or change shared memory layout. Check the release notes — if they mention shared memory changes or new background workers, restart PostgreSQL:

```bash
pg_ctl restart -D $PGDATA
```

### Step 4: Run the Migration

```sql
-- Upgrade to the latest installed version
ALTER EXTENSION pg_ripple UPDATE;

-- Or upgrade to a specific version
ALTER EXTENSION pg_ripple UPDATE TO '0.32.0';
```

PostgreSQL will execute each intermediate migration script in order:

```
NOTICE:  updating extension "pg_ripple" from version "0.30.0" to "0.31.0"
NOTICE:  updating extension "pg_ripple" from version "0.31.0" to "0.32.0"
```

### Step 5: Verify

```sql
-- Confirm the new version
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';

-- Run the health check
SELECT pg_ripple.canary();

-- Verify stats
SELECT pg_ripple.stats();
```

---

## Post-Upgrade Verification

Run these checks after every upgrade:

```sql
-- 1. Extension version matches expected
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';

-- 2. Health check passes
SELECT pg_ripple.canary();

-- 3. Triple count is unchanged
SELECT pg_ripple.stats();

-- 4. SPARQL queries work
SELECT pg_ripple.sparql($$
  SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5
$$);

-- 5. Merge worker is running (if shared_preload_libraries is set)
SELECT (pg_ripple.stats()->>'merge_worker_pid')::int > 0 AS merge_worker_ok;

-- 6. Dictionary cache is operational
SELECT
  (s->>'encode_cache_hits')::bigint + (s->>'encode_cache_misses')::bigint > 0
    AS cache_active
FROM pg_ripple.stats() s;
```

```admonish tip title="Automate post-upgrade checks"
Add these verification queries to a script that runs immediately after `ALTER EXTENSION`. If any check fails, the script can alert operators before traffic is routed back to the upgraded instance.
```

---

## Multi-Version Hop Upgrades

PostgreSQL walks the entire migration chain automatically. Upgrading from v0.5.0 directly to v0.32.0 executes all intermediate scripts:

```sql
-- This works — PG finds the path 0.5.0 → 0.5.1 → 0.6.0 → ... → 0.32.0
ALTER EXTENSION pg_ripple UPDATE TO '0.32.0';
```

```admonish note title="Long upgrade chains"
Each migration script is typically fast (milliseconds to seconds). However, scripts that add columns or create indexes on large tables may take longer. For very long hops (10+ versions), expect a few minutes on large datasets. Monitor `pg_stat_activity` for lock waits during the upgrade.
```

---

## Rollback Strategy

There is no built-in downgrade path. If an upgrade causes problems:

### Option A: Restore from Backup

```bash
# Drop the upgraded database
dropdb mydb

# Restore the pre-upgrade backup
createdb mydb
pg_restore -d mydb pre_upgrade_backup.dump
```

This is the safest rollback method — it returns everything to the exact pre-upgrade state.

### Option B: Reinstall the Old Version

```bash
# Install the old shared library
cargo pgrx install --pg-config $(pg_config) --release  # (from old source checkout)

# Restart PostgreSQL
pg_ctl restart -D $PGDATA
```

```admonish danger title="Downgrade limitations"
Reinstalling the old `.so` file works only if the migration scripts did not make irreversible schema changes (e.g., dropping a column). Always check the migration script content before relying on this approach.
```

---

## Upgrading PostgreSQL Itself

When upgrading the PostgreSQL major version (e.g., 17 → 18):

1. **pg_ripple requires PostgreSQL 18**. Earlier versions are not supported.
2. Use `pg_upgrade` as normal — pg_ripple's tables and extension metadata transfer correctly.
3. After `pg_upgrade`, verify the extension:

```bash
psql -d mydb -c "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';"
psql -d mydb -c "SELECT pg_ripple.canary();"
```

```admonish warning title="Recompile the shared library"
After a PostgreSQL major version upgrade, the `pg_ripple.so` shared library must be recompiled against the new PostgreSQL headers. The old binary will not load.
```

---

## Version Compatibility Matrix

| pg_ripple Version | PostgreSQL Version | Notes |
|---|---|---|
| 0.1.0 – 0.32.0 | 18.x | Only supported version |
| Any | < 18 | Not supported |
| Any | 19+ | Not yet tested |

---

## Troubleshooting Upgrades

### "no update path from version X to version Y"

The intermediate migration scripts are missing from the extension directory. Reinstall pg_ripple to ensure all SQL files are present:

```bash
ls $(pg_config --sharedir)/extension/pg_ripple--*.sql | wc -l
```

### "could not open extension control file"

The `pg_ripple.control` file is missing. Reinstall the extension.

### Migration script fails with a lock timeout

Another session holds a lock on an internal table. Ensure all connections are closed before upgrading, or increase `lock_timeout`:

```sql
SET lock_timeout = '60s';
ALTER EXTENSION pg_ripple UPDATE;
```

### Shared library version mismatch

The `.so` file version does not match the SQL migration target. Ensure you installed the matching binary before running `ALTER EXTENSION`:

```bash
cargo pgrx install --pg-config $(pg_config) --release
pg_ctl restart -D $PGDATA
ALTER EXTENSION pg_ripple UPDATE;
```
