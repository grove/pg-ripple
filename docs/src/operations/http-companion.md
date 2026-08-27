# HTTP companion

`pg_ripple_http` exposes the SPARQL HTTP endpoint for a PostgreSQL instance
that has the `pg_ripple` extension installed.

## Production configuration

Set `PG_RIPPLE_HTTP_AUTH_TOKEN` from a secret. Do not put the token in a
Compose file, Dockerfile, Helm values file, or Kubernetes manifest.

```bash
export AUTH_TOKEN="$(openssl rand -base64 32)"
docker run --rm -p 7878:7878 \
  -e PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@postgres:5432/mydb" \
  -e PG_RIPPLE_HTTP_AUTH_TOKEN="${AUTH_TOKEN}" \
  -e PG_RIPPLE_HTTP_RATE_LIMIT="100" \
  -e PG_RIPPLE_HTTP_CORS_ORIGINS="https://app.example.com" \
  ghcr.io/trickle-labs/pg-ripple-http:0.131.0
```

Send the token in the `Authorization` header:

```bash
curl -H "Authorization: Bearer ${AUTH_TOKEN}" \
  http://localhost:7878/health
```

The default rate limit is 100 requests per second per client IP. The default
CORS allowlist is empty, so the service does not allow cross-origin requests.

## Local development

To run without a token, set `PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1` explicitly.
Use this setting only for local development.

```bash
docker run --rm -p 7878:7878 \
  -e PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@postgres:5432/mydb" \
  -e PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1 \
  ghcr.io/trickle-labs/pg-ripple-http:0.131.0
```

## Helm

Create the token outside the chart and reference the existing Secret:

```bash
kubectl create secret generic my-ripple-http \
  --from-literal=auth-token="$AUTH_TOKEN"
helm install my-ripple ./charts/pg_ripple \
  --set http.authTokenSecret.name=my-ripple-http
```

For local development, set `http.allowUnauthenticated=true` instead. The chart
sets the rate limit to `100` and the CORS allowlist to an empty string by
default.
