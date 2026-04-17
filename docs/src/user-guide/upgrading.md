# Upgrading

This page documents the upgrade path between pg_ripple versions. Each section covers what changed, whether any action is required, and whether the change is safe for existing deployments.

---

## v0.21.0 → v0.22.0

**Status:** Safe for all existing deployments. No data migration required.

### Upgrade procedure

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.22.0';
```

The migration script `sql/pg_ripple--0.21.0--0.22.0.sql` runs automatically and applies the privilege revocations described below.

### What changed

#### Privilege model hardening

The internal `_pg_ripple` schema and all its tables and sequences are now explicitly inaccessible to unprivileged roles:

```sql
REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA _pg_ripple FROM PUBLIC;
```

**Impact:** If any application role was previously reading internal tables directly (e.g. `SELECT * FROM _pg_ripple.dictionary`), those queries will fail with `permission denied` after the upgrade.

**Recommendation:** Use only the public `pg_ripple.*` API. Direct access to `_pg_ripple.*` tables was always unsupported. The public API functions are `SECURITY DEFINER` and continue to work for all roles.

#### Dictionary cache rollback safety

Rolled-back `insert_triple` calls no longer leave stale term IDs in the shared-memory encode cache. This is a correctness fix — no configuration change required. See [Operations — Rollback safety guarantee](operations.md#rollback-safety-guarantee-v0220) for details.

#### Merge race fixes

Two race conditions in the background merge worker are closed:

- **Tombstone resurrection**: Deleted triples can no longer reappear after a concurrent merge.
- **View-rename race**: Concurrent queries no longer see `relation does not exist` during a merge cycle.

Both fixes are automatic — no configuration change required.

#### Rare-predicate promotion atomicity

The promotion of a predicate from `vp_rare` to its own VP table is now atomic. Previously, a narrow window existed where concurrent inserts could be orphaned during promotion. This fix is automatic — no configuration change required.

#### GUC bounds enforcement

`pg_ripple.vp_promotion_threshold` now enforces `min = 10` and `max = 10,000,000`. Values outside this range were previously accepted (and could cause catalog explosion at `threshold = 1` or permanent `vp_rare` lock-in at `threshold = INT_MAX`).

If your configuration sets a value below 10, PostgreSQL will clamp it to 10 after the upgrade.

#### HTTP companion service security hardening

`pg_ripple_http` receives several security improvements:

- **Rate limiting**: set `PG_RIPPLE_HTTP_RATE_LIMIT=100` (req/s per IP) to enable.
- **Error redaction**: internal database details are no longer returned in error responses.
- **Constant-time auth**: bearer token comparison is now timing-safe.
- **URL scheme enforcement**: `register_endpoint()` rejects non-http/https URLs.

See [Security — Phase 2](../reference/security.md#security-review-phase-2-v0220) for details.

---

## v0.20.0 → v0.21.0

No schema changes. No action required.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.21.0';
```

---

## v0.19.0 → v0.20.0

No schema changes. No action required.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.20.0';
```

---

## v0.18.0 → v0.19.0

No schema changes. No action required.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.19.0';
```

---

## General upgrade guidance

### Safe pattern for all versions

```sql
-- 1. Take a base backup or logical dump first (optional but recommended).
-- 2. Stop the application.
-- 3. Run the upgrade:
ALTER EXTENSION pg_ripple UPDATE;
-- 4. Restart the application.
```

### Checking the installed version

```sql
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';
```

### Upgrading across multiple versions

PostgreSQL's `ALTER EXTENSION ... UPDATE` follows the migration chain automatically. To upgrade from v0.19.0 directly to v0.22.0:

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.22.0';
```

PostgreSQL will apply the chain `0.19.0→0.20.0→0.21.0→0.22.0` automatically.

### If something goes wrong

If the upgrade fails midway, restore from your backup. Migration scripts are designed to be idempotent (safe to re-run) but a failed mid-migration state is best recovered from backup rather than manual patching.
