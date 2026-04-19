# Installation

pg_ripple is a PostgreSQL 18 extension written in Rust. Choose the installation method that fits your environment.

## Docker (recommended)

The fastest path to a working pg_ripple instance. No build tools required.

```bash
# Start pg_ripple with Docker Compose
docker compose up -d

# Connect
psql -h localhost -p 5432 -U postgres -d pg_ripple
```

The `docker-compose.yml` in the repository root starts PostgreSQL 18 with pg_ripple pre-installed and the extension created in the default database.

### Verify the installation

```sql
SELECT pg_ripple.triple_count();
```

The result should be `0` — the extension is installed and ready.

## From source (cargo pgrx)

Build and install directly into a local PostgreSQL 18 instance.

### Prerequisites

- Rust (stable, edition 2024)
- PostgreSQL 18 development headers
- `cargo-pgrx` 0.17

```bash
# Install cargo-pgrx
cargo install cargo-pgrx --version 0.17 --locked

# Initialize pgrx with PostgreSQL 18
cargo pgrx init --pg18 $(which pg_config)

# Build and install
cargo pgrx install --release --pg-config $(which pg_config)
```

### Create the extension

Connect to your database and run:

```sql
CREATE EXTENSION pg_ripple;
```

### Verify

```sql
SELECT pg_ripple.triple_count();
```

## Configuration

pg_ripple works out of the box with default settings. For production deployments, you may want to adjust GUC parameters — see [Configuration and Tuning](../operations/configuration.md).

For HTAP storage (background merge worker) and shared-memory dictionary cache, add pg_ripple to `shared_preload_libraries` in `postgresql.conf`:

```
shared_preload_libraries = 'pg_ripple'
```

Restart PostgreSQL after this change.

## Troubleshooting

### Wrong PostgreSQL version

pg_ripple requires PostgreSQL 18. Check your version:

```bash
pg_config --version
```

### Missing shared_preload_libraries

If you see errors about shared memory or the merge worker not starting, ensure `pg_ripple` is in `shared_preload_libraries` and PostgreSQL has been restarted.

### pgrx version mismatch

pg_ripple requires `cargo-pgrx` 0.17. If you have an older version:

```bash
cargo install cargo-pgrx --version 0.17 --locked --force
```

### Extension not found after install

If `CREATE EXTENSION pg_ripple` fails with "extension not found", verify that the extension files were installed to the correct PostgreSQL directory:

```bash
pg_config --sharedir
ls $(pg_config --sharedir)/extension/pg_ripple*
```

### Docker container fails to start

Check logs:

```bash
docker compose logs pg_ripple
```

Common causes: port 5432 already in use (change the port mapping), insufficient memory (pg_ripple recommends at least 512MB).

## Next steps

- [Hello World — Five-Minute Walkthrough](hello-world.md) — load and query your first triples
- [Guided Tutorial](tutorial.md) — build a knowledge graph in 30 minutes
