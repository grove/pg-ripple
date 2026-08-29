# HTTP companion

`pg_ripple_http` exposes the SPARQL HTTP endpoint for a PostgreSQL instance
that has the `pg_ripple` extension installed.

## Production configuration

The service defaults to production mode, binds to loopback, and requires a
read token. Set `PG_RIPPLE_HTTP_AUTH_TOKEN_FILE` from a mounted secret when
possible. Do not put the token in a
Compose file, Dockerfile, Helm values file, or Kubernetes manifest.

```bash
export AUTH_TOKEN="$(openssl rand -base64 32)"
docker run --rm -p 7878:7878 \
  -e PG_RIPPLE_HTTP_BIND="0.0.0.0:7878" \
  -e PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@postgres:5432/mydb" \
  -e PG_RIPPLE_HTTP_AUTH_TOKEN="${AUTH_TOKEN}" \
  -e PG_RIPPLE_HTTP_PG_SSLMODE="verify-full" \
  -e PG_RIPPLE_HTTP_PG_CA_FILE="/run/secrets/pg-ca.pem" \
  -e PG_RIPPLE_HTTP_RATE_LIMIT="100" \
  -e PG_RIPPLE_HTTP_CORS_ORIGINS="https://app.example.com" \
  ghcr.io/trickle-labs/pg-ripple-http:0.134.0
```

Send the token in the `Authorization` header:

```bash
curl -H "Authorization: Bearer ${AUTH_TOKEN}" \
  http://localhost:7878/health
```

The default rate limit is 100 requests per second per client IP. The default
CORS allowlist is empty, so the service does not allow cross-origin requests.

Secret values can be supplied with `_FILE` variants for the read, write, admin,
metrics, and PostgreSQL password settings. Values are loaded once at startup
and are never included in diagnostic responses. `PG_RIPPLE_HTTP_BIND` defaults
to `127.0.0.1:7878`; set it explicitly for a public listener.

## Local development

To run without a token, set both `PG_RIPPLE_HTTP_MODE=development` and
`PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1` explicitly. Production mode rejects
that override, including when it is inherited from a container environment.

```bash
docker run --rm -p 7878:7878 \
  -e PG_RIPPLE_HTTP_BIND="0.0.0.0:7878" \
  -e PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@postgres:5432/mydb" \
  -e PG_RIPPLE_HTTP_MODE=development \
  -e PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1 \
  ghcr.io/trickle-labs/pg-ripple-http:0.134.0
```

## Helm

Create the token outside the chart and reference the existing Secret:

```bash
kubectl create secret generic my-ripple-http \
  --from-literal=auth-token="$AUTH_TOKEN"
helm install my-ripple ./charts/pg_ripple \
  --set http.authTokenSecret.name=my-ripple-http
```

Optional write, admin, and metrics credentials can be supplied with
`http.writeTokenSecret`, `http.adminTokenSecret`, and `http.metricsTokenSecret`.

For local development, set `http.mode=development` and
`http.allowUnauthenticated=true`. The chart rejects an unauthenticated
production configuration. Set `networkPolicy.enabled=true` and provide
explicit `networkPolicy.egress` entries for federation or LLM destinations.

For PostgreSQL TLS, set `PG_RIPPLE_HTTP_PG_SSLMODE` to `require`, `verify-ca`,
or `verify-full`, and mount the CA file through the pod. Use the dedicated
`pg_ripple_http` role from `sql/roles/pg_ripple_http.sql`; set its password or
configure certificate authentication before using it. It has no direct
privileges on `_pg_ripple`.
