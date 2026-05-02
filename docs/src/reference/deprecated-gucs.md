# Deprecated GUCs

> **A13-04 (v0.86.0)**: this page lists all deprecated GUC parameters in pg_ripple, their replacement names, and their scheduled removal versions.

Setting a deprecated GUC raises `PT501` with a descriptive message pointing to the replacement.

---

## Currently Deprecated

| Deprecated GUC | Introduced | Replacement | Removal |
|---|---|---|---|
| `pg_ripple.property_path_max_depth` | v0.24.0 | `pg_ripple.max_path_depth` | v0.56.0 (already removed) |

---

## Removed GUCs

These GUCs have already been removed. Attempting to set them raises a `PT501` error.

| Removed GUC | Removed In | Replacement |
|---|---|---|
| `pg_ripple.property_path_max_depth` | v0.56.0 | `pg_ripple.max_path_depth` |

---

## Planned Deprecations

| GUC | Planned Deprecation | Reason | Replacement |
|---|---|---|---|
| `pg_ripple.describe_strategy` | v0.86.0 (soft) | Superseded by `describe_form` with W3C-aligned values | `pg_ripple.describe_form` |

---

## Migration Notes

### `pg_ripple.property_path_max_depth` → `pg_ripple.max_path_depth`

Renamed in v0.56.0. Update `postgresql.conf` and `ALTER SYSTEM SET` calls:

```sql
-- Before (raises PT501):
SET pg_ripple.property_path_max_depth = 200;

-- After:
SET pg_ripple.max_path_depth = 200;
```

### `pg_ripple.describe_strategy` → `pg_ripple.describe_form`

The `describe_strategy` GUC uses values `cbd`, `scbd`, `simple`. The new `describe_form` GUC
uses W3C-aligned names `cbd`, `scbd`, `symmetric` (`symmetric` is an alias for `scbd`).
When `describe_form` is set, it takes precedence over `describe_strategy`.

```sql
-- Before:
SET pg_ripple.describe_strategy = 'scbd';

-- After (preferred):
SET pg_ripple.describe_form = 'symmetric';  -- same as 'scbd'
```

Both GUCs remain supported in v0.86.0. `describe_strategy` may be formally deprecated in v0.87.0.
