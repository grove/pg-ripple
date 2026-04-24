# CloudNativePG Deployment

[CloudNativePG (CNP)](https://cloudnative-pg.io) is a Kubernetes operator for
managing PostgreSQL clusters.  pg_ripple v0.54.0 ships a pre-built extension
image for CNP ≥ 1.24, allowing operators to install pg_ripple into a managed
cluster with **no custom PostgreSQL container image** and no custom build step.

## Prerequisites

- CloudNativePG operator ≥ 1.24
- Kubernetes ≥ 1.25
- The `Image Volume` feature gate enabled in CNP (enabled by default in CNP ≥ 1.24)

## How It Works

CloudNativePG ≥ 1.24 supports [extension images](https://cloudnative-pg.io/documentation/current/extension_volumes/):
a minimal OCI image whose only purpose is to supply pre-compiled `.so` and SQL
files.  CNP mounts the image as an init-container volume and copies the files
into the PostgreSQL container at startup.

pg_ripple publishes two images per release:

| Image | Contents |
|-------|----------|
| `ghcr.io/grove/pg_ripple:<version>` | Full batteries-included image |
| `ghcr.io/grove/pg_ripple:<version>-cnpg` | Extension volume for CloudNativePG |

The `-cnpg` image contains pg_ripple and pgvector compiled for PostgreSQL 18 at
the paths expected by CNP:

```
/var/lib/postgresql/extension-files/lib/pg_ripple.so
/var/lib/postgresql/extension-files/ext/pg_ripple.control
/var/lib/postgresql/extension-files/ext/pg_ripple--*.sql
/var/lib/postgresql/extension-files/lib/vector.so
/var/lib/postgresql/extension-files/ext/vector.control
```

## Cluster Manifest Walkthrough

The example manifest is at `examples/cloudnativepg_cluster.yaml`:

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: pg-ripple-cluster
spec:
  imageName: ghcr.io/cloudnative-pg/postgresql:18
  instances: 3

  postgresql:
    extensionImages:
      - name: pg-ripple-ext
        image: ghcr.io/grove/pg_ripple:0.54.0-cnpg   # ← extension volume
    parameters:
      allow_system_table_mods: "on"
      shared_preload_libraries: "pg_ripple"

  storage:
    size: 20Gi

  superuserSecret:
    name: pg-ripple-superuser

  bootstrap:
    initdb:
      database: postgres
      postInitSQL:
        - "CREATE EXTENSION IF NOT EXISTS pg_ripple;"
        - "CREATE EXTENSION IF NOT EXISTS vector;"
```

Key points:
- `imageName` is the standard CNP base image — **not** a custom build.
- `extensionImages` lists the pg_ripple extension volume.  CNP mounts it and
  copies files before PostgreSQL starts.
- `postInitSQL` runs `CREATE EXTENSION` on first cluster startup.

## Deploying

```bash
# Create the superuser secret first
kubectl create secret generic pg-ripple-superuser \
  --from-literal=username=postgres \
  --from-literal=password=your-secure-password

# Apply the cluster manifest
kubectl apply -f examples/cloudnativepg_cluster.yaml

# Wait for all instances to be ready
kubectl wait --for=condition=Ready cluster/pg-ripple-cluster --timeout=120s
```

## Post-Deploy Verification

```bash
kubectl exec -it pg-ripple-cluster-1 -- psql -U postgres -c \
  "SELECT extname, extversion FROM pg_extension WHERE extname IN ('pg_ripple', 'vector');"
```

Expected output:

```
  extname  | extversion
-----------+------------
 pg_ripple | 0.54.0
 vector    | 0.7.4
```

Load a test triple and run a SPARQL query:

```bash
kubectl exec -it pg-ripple-cluster-1 -- psql -U postgres -c \
  "SELECT pg_ripple.load_ntriples('<https://example.org/s> <https://example.org/p> <https://example.org/o> .');"

kubectl exec -it pg-ripple-cluster-1 -- psql -U postgres -c \
  "SELECT * FROM pg_ripple.sparql('SELECT ?s ?p ?o WHERE { ?s ?p ?o }');"
```

## Upgrade Procedure

Upgrading pg_ripple is a one-line change in the cluster manifest — bump the
extension image tag and apply:

```bash
# Edit the manifest to change the image tag
sed -i 's/pg_ripple:0.54.0-cnpg/pg_ripple:0.55.0-cnpg/' \
  examples/cloudnativepg_cluster.yaml

kubectl apply -f examples/cloudnativepg_cluster.yaml

# Once the rolling restart completes, run the migration
kubectl exec -it pg-ripple-cluster-1 -- psql -U postgres \
  -c "ALTER EXTENSION pg_ripple UPDATE TO '0.55.0';"
```

CNP handles the rolling restart automatically, ensuring zero downtime.

## High Availability

CloudNativePG provides built-in HA: the primary is automatically elected from
the standby instances if the current primary fails.  pg_ripple's shared memory
and background workers (merge worker, apply worker) are automatically restarted
by PostgreSQL on the new primary.

For RDF logical replication across CNP clusters, see
[Logical Replication](replication.md).
