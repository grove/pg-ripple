# pg_ripple Helm Chart

This Helm chart deploys a PostgreSQL 18 instance with the `pg_ripple` extension (RDF triple store, SPARQL 1.1, Datalog, SHACL, HTAP) and optionally the `pg_ripple_http` sidecar.

## Prerequisites

- Kubernetes 1.24+
- Helm 3.10+

## Installation

```bash
kubectl create secret generic my-ripple-http \
  --from-literal=auth-token="$AUTH_TOKEN"
helm install my-ripple ./charts/pg_ripple \
  --set http.authTokenSecret.name=my-ripple-http
```

For local development only, allow unauthenticated HTTP explicitly:

```bash
helm install my-ripple ./charts/pg_ripple \
  --set http.allowUnauthenticated=true
```

With custom values:

```bash
helm install my-ripple ./charts/pg_ripple --values my-values.yaml
```

## Configuration

See `values.yaml` for all available configuration options.

### Key options

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of PostgreSQL pods | `1` |
| `image.tag` | pg_ripple image tag | `"0.133.0"` |
| `postgres.password` | PostgreSQL superuser password; empty generates and persists one | `""` |
| `podDisruptionBudget.enabled` | Enable PodDisruptionBudget | `true` |
| `podDisruptionBudget.minAvailable` | Minimum available pods during disruptions | `1` |
| `http.authTokenSecret.name` | Existing Secret containing the HTTP bearer token | `""` |
| `http.allowUnauthenticated` | Allow unauthenticated HTTP for local development | `false` |
| `http.rateLimit` | Requests per second per client IP | `100` |
| `http.corsOrigins` | Comma-separated CORS allowlist | `""` |

## PodDisruptionBudget (v0.120.0)

The chart ships a `PodDisruptionBudget` (PDB) resource enabled by default:

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 1
```

This ensures at least one pg_ripple pod remains available during voluntary
disruptions (node drains, Kubernetes upgrades, etc.).

For high-availability deployments (3+ replicas), consider:

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 2
```

Or using `maxUnavailable`:

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: ""
  maxUnavailable: 1
```

Disable by setting `podDisruptionBudget.enabled: false`.

## Per-Tenant Helm Values (Feature 9, v0.120.0)

Generate a per-tenant `values-<name>.yaml` fragment suitable for `helm install --values`:

```bash
just generate-helm-values TENANT=acme
# Creates values-acme.yaml in the current directory
```

This queries `_pg_ripple.tenants` and emits Helm-compatible YAML with the
tenant's graph IRI and quota configuration.

## Liveness & Readiness Probes

The chart configures HTTP probes against the `pg_ripple_http` sidecar:

- **Liveness** (`/health`): Is the process alive and can it reach PostgreSQL?
- **Readiness** (`/ready`): Has the process ever successfully connected (safe to route traffic)?

See `values.yaml` for probe tuning parameters.
