# Installation

## Prerequisites

| Requirement | Minimum version | Notes |
|---|---|---|
| PostgreSQL | 18.x | pg_ripple targets PG 18 only; PG 17 and earlier are not supported |
| Rust | 1.85 | Edition 2024 features are used |
| pgrx | 0.17.0 | Pinned; must match the version in `Cargo.toml` |
| Cargo | ships with Rust | — |

## Install pgrx

```bash
cargo install --locked cargo-pgrx@0.17.0
cargo pgrx init --pg18 $(which pg_config)
```

`cargo pgrx init` downloads PostgreSQL header files and sets up the local test cluster. This takes a few minutes the first time.

## Build and install the extension

```bash
git clone https://github.com/grove/pg-ripple.git
cd pg-ripple
cargo pgrx install --pg-config $(which pg_config)
```

This compiles the extension and copies the shared library and SQL files into the PostgreSQL extension directory (e.g. `/usr/share/postgresql/18/extension/`).

## Enable the extension

Connect to the target database and run:

```sql
CREATE EXTENSION pg_ripple;
```

User-visible functions land in the `pg_ripple` schema. Internal tables land in `_pg_ripple`. Both schemas are created automatically.

## Verify the installation

```sql
SELECT pg_ripple.triple_count();
-- Returns 0 for an empty store
```

## Upgrading from an earlier version

pg_ripple ships an explicit migration script for every version bump. To upgrade:

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

If you are upgrading across multiple versions (e.g. 0.1.0 → 0.5.0), PostgreSQL chains the migration scripts automatically.

## Uninstalling

```sql
DROP EXTENSION pg_ripple CASCADE;
```

`CASCADE` removes both the `pg_ripple` and `_pg_ripple` schemas and all their objects, including VP tables. All RDF data is permanently deleted.
