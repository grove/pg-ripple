# Kubernetes Deployment

pg_ripple ships a Helm chart (`charts/pg_ripple/`) that deploys the
batteries-included image — PostgreSQL 18 with pg_ripple, PostGIS, and pgvector
pre-installed — on any Kubernetes cluster.

## Prerequisites

- Kubernetes ≥ 1.25
- Helm ≥ 3.10
- Persistent volume provisioner (any cloud provider or `local-path-provisioner`)

## Installation

### Add the Helm repository

```bash
helm repo add pg-ripple https://grove.github.io/pg-ripple/charts
helm repo update
```

### Install with defaults

```bash
helm install pg-ripple pg-ripple/pg-ripple \
  --set postgres.password=mysecretpassword
```

### Install from source

```bash
helm install pg-ripple ./charts/pg_ripple \
  --set postgres.password=mysecretpassword
```

### Verify deployment

```bash
kubectl get pods -l app.kubernetes.io/name=pg-ripple
kubectl exec -it <pod-name> -- psql -U postgres \
  -c "SELECT extname, extversion FROM pg_extension WHERE extname = 'pg_ripple';"
```

## Values Reference

| Key | Default | Description |
|-----|---------|-------------|
| `replicaCount` | `1` | Number of PostgreSQL Pods |
| `image.repository` | `ghcr.io/grove/pg_ripple` | Image repository |
| `image.tag` | `0.54.0` | Image tag |
| `image.pullPolicy` | `IfNotPresent` | Image pull policy |
| `postgres.password` | `ripple` | Superuser password (use a Secret in production) |
| `postgres.database` | `postgres` | Database to create |
| `persistence.enabled` | `true` | Enable persistent storage |
| `persistence.size` | `10Gi` | PVC size |
| `persistence.storageClass` | `""` | StorageClass (empty = cluster default) |
| `service.type` | `ClusterIP` | PostgreSQL service type |
| `service.port` | `5432` | PostgreSQL port |
| `http.enabled` | `true` | Enable SPARQL HTTP sidecar |
| `http.service.type` | `ClusterIP` | SPARQL HTTP service type |
| `http.service.port` | `7878` | SPARQL HTTP port |
| `ripple.federationEndpoints` | `[]` | SPARQL federation endpoints |
| `ripple.shacl.shapesConfigMap` | `""` | SHACL shapes ConfigMap |
| `ripple.llm.apiKeySecret` | `""` | LLM API key Secret name |

## Common Configurations

### Expose SPARQL HTTP externally (LoadBalancer)

```bash
helm upgrade pg-ripple ./charts/pg_ripple \
  --set http.service.type=LoadBalancer \
  --set http.service.port=7878
```

### Increase storage

```bash
helm upgrade pg-ripple ./charts/pg_ripple \
  --set persistence.size=100Gi \
  --set persistence.storageClass=premium-ssd
```

### Configure federation endpoints

```bash
helm upgrade pg-ripple ./charts/pg_ripple \
  --set 'ripple.federationEndpoints[0].name=wikidata' \
  --set 'ripple.federationEndpoints[0].url=https://query.wikidata.org/sparql'
```

### Enable Prometheus monitoring

```bash
helm upgrade pg-ripple ./charts/pg_ripple \
  --set metrics.enabled=true \
  --set metrics.serviceMonitor.enabled=true
```

## Health Probes

The chart configures both liveness and readiness probes using `pg_isready`:

```yaml
livenessProbe:
  exec:
    command: ["pg_isready", "-U", "postgres"]
  initialDelaySeconds: 30
  periodSeconds: 10

readinessProbe:
  exec:
    command: ["pg_isready", "-U", "postgres"]
  initialDelaySeconds: 5
  periodSeconds: 5
```

## Prometheus Integration

When `metrics.serviceMonitor.enabled = true`, the chart creates a
`ServiceMonitor` resource for the Prometheus Operator.  pg_ripple exposes
query stats via `pg_stat_statements` and OTEL tracing via the
`pg_ripple.tracing_otlp_endpoint` GUC.

Configure the OTEL endpoint:

```bash
kubectl exec -it <pod-name> -- psql -U postgres \
  -c "SET pg_ripple.tracing_otlp_endpoint = 'http://otel-collector:4318';"
```

## Future: Kubernetes Operator

A future release will provide a Go operator built with `controller-runtime` that
manages pg_ripple clusters as first-class Kubernetes resources — similar to
CloudNativePG but tailored to the RDF workload lifecycle (bulk load, VP merge
scheduling, SHACL validation pipelines).

The operator will provide:
- `RDFTripleStore` custom resource with declarative schema management
- Automated rolling upgrades with zero downtime
- Built-in Prometheus metrics CRDs
- Automated SHACL shape deployment via ConfigMap reference

## Pre-Installed Extensions

The batteries-included image pre-installs:

| Extension | Version | Activate with |
|-----------|---------|---------------|
| pg_ripple | 0.54.0 | `CREATE EXTENSION pg_ripple;` |
| PostGIS | 3.4.3 | `CREATE EXTENSION postgis;` |
| pgvector | 0.7.4 | `CREATE EXTENSION vector;` |
