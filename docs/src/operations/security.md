# Security

pg_ripple provides multiple layers of security: PostgreSQL's native authentication and authorization, named-graph row-level security (RLS), SQL injection prevention through dictionary encoding, and secure configuration of the `pg_ripple_http` companion service.

---

## Authentication and Authorization

pg_ripple relies entirely on PostgreSQL's built-in authentication (`pg_hba.conf`) and role-based access control. There is no separate user database.

### Minimum Privileges for SPARQL Queries

```sql
-- Create a read-only role
CREATE ROLE sparql_reader LOGIN PASSWORD 'strong_password';
GRANT USAGE ON SCHEMA pg_ripple TO sparql_reader;
GRANT USAGE ON SCHEMA _pg_ripple TO sparql_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA _pg_ripple TO sparql_reader;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO sparql_reader;
```

### Minimum Privileges for Data Loading

```sql
-- Create a writer role
CREATE ROLE sparql_writer LOGIN PASSWORD 'strong_password';
GRANT USAGE ON SCHEMA pg_ripple TO sparql_writer;
GRANT USAGE ON SCHEMA _pg_ripple TO sparql_writer;
GRANT SELECT, INSERT, DELETE ON ALL TABLES IN SCHEMA _pg_ripple TO sparql_writer;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA _pg_ripple TO sparql_writer;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO sparql_writer;
```

```admonish warning title="Default privileges"
Use `ALTER DEFAULT PRIVILEGES` to ensure newly created VP tables (created when new predicates are encountered) inherit the correct grants:
```

```sql
ALTER DEFAULT PRIVILEGES IN SCHEMA _pg_ripple
  GRANT SELECT ON TABLES TO sparql_reader;
ALTER DEFAULT PRIVILEGES IN SCHEMA _pg_ripple
  GRANT SELECT, INSERT, DELETE ON TABLES TO sparql_writer;
```

---

## Named-Graph Row-Level Security

pg_ripple supports fine-grained access control at the named-graph level using PostgreSQL's row-level security (RLS) infrastructure. This allows different users to see different subsets of the knowledge graph.

### Enabling Graph RLS

```sql
-- Enable RLS on all VP tables
SELECT pg_ripple.enable_graph_rls();
```

This creates RLS policies on every VP table (including `vp_rare`) that filter rows based on the `g` (graph) column.

### Granting Graph Access

```sql
-- Grant a role access to a specific named graph
SELECT pg_ripple.grant_graph('sparql_reader', 'http://example.org/confidential');

-- Grant access to the default graph (g = 0)
SELECT pg_ripple.grant_graph('sparql_reader', '');

-- Grant access to all graphs
SELECT pg_ripple.grant_graph('sparql_reader', '*');
```

### Revoking Graph Access

```sql
-- Revoke access to a specific graph
SELECT pg_ripple.revoke_graph('sparql_reader', 'http://example.org/confidential');
```

### How It Works

When graph RLS is enabled:

1. Each VP table gets an RLS policy that checks the `g` column against the user's allowed graph IDs
2. The dictionary encodes graph IRIs to `i64` identifiers
3. An internal mapping table (`_pg_ripple.graph_grants`) stores `(role, graph_id)` pairs
4. PostgreSQL enforces the policy transparently — SPARQL queries automatically filter results

```admonish note title="Superuser bypass"
PostgreSQL superusers bypass RLS by default. To enforce graph security even for superusers, the user must explicitly `SET row_security = on` and not be a table owner. For production, use non-superuser roles for application connections.
```

### Example: Multi-Tenant Knowledge Graph

```sql
-- Create tenant roles
CREATE ROLE tenant_a LOGIN PASSWORD 'pw_a';
CREATE ROLE tenant_b LOGIN PASSWORD 'pw_b';

-- Grant base access
GRANT USAGE ON SCHEMA pg_ripple TO tenant_a, tenant_b;
GRANT USAGE ON SCHEMA _pg_ripple TO tenant_a, tenant_b;
GRANT SELECT ON ALL TABLES IN SCHEMA _pg_ripple TO tenant_a, tenant_b;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO tenant_a, tenant_b;

-- Enable graph RLS
SELECT pg_ripple.enable_graph_rls();

-- Tenant A sees only their graph
SELECT pg_ripple.grant_graph('tenant_a', 'http://example.org/tenant-a');

-- Tenant B sees only their graph
SELECT pg_ripple.grant_graph('tenant_b', 'http://example.org/tenant-b');

-- Both see shared reference data
SELECT pg_ripple.grant_graph('tenant_a', 'http://example.org/shared');
SELECT pg_ripple.grant_graph('tenant_b', 'http://example.org/shared');
```

Now SPARQL queries run by `tenant_a` will only see triples in `tenant-a` and `shared` graphs, with no application-level filtering required.

---

## SQL Injection Prevention

pg_ripple's architecture provides strong defense against SQL injection by design.

### Dictionary Encoding as a Security Layer

All SPARQL queries go through a multi-step translation pipeline:

1. **Parse**: SPARQL text is parsed by `spargebra` into an abstract algebra tree
2. **Encode**: All bound constants (IRIs, literals) are dictionary-encoded to `i64` integers *before* SQL generation
3. **Generate**: SQL is constructed using parameterized queries with integer placeholders
4. **Execute**: SQL runs via `pgrx::SpiClient` with bound parameters

```admonish success title="No raw strings in VP queries"
Because VP tables store only `BIGINT` columns (`s`, `o`, `g`, `i`, `source`), there is no surface for string-based SQL injection. Even if a malicious IRI is passed in a SPARQL query, it is hashed to an integer before any SQL is generated.
```

### Table Name Safety

VP table references use OID lookups from `_pg_ripple.predicates`, not string concatenation:

```rust
// Internal: table names are never interpolated from user input
let table_oid = predicates::get_table_oid(predicate_id)?;
// SQL uses the OID directly: FROM pg_class WHERE oid = $1
```

### User-Facing Function Safety

Functions that accept text input (like `pg_ripple.sparql()`) parse the SPARQL text through `spargebra`, which rejects anything that is not valid SPARQL. No raw SQL is passed through.

---

## File-Path Loaders and Superuser Requirement

Functions that read from the server's filesystem require superuser privileges:

| Function | Requires Superuser | Reason |
|---|---|---|
| `pg_ripple.load_turtle_file(path)` | **Yes** | Reads arbitrary filesystem paths |
| `pg_ripple.load_ntriples_file(path)` | **Yes** | Reads arbitrary filesystem paths |
| `pg_ripple.load_rdfxml_file(path)` | **Yes** | Reads arbitrary filesystem paths |
| `pg_ripple.load_turtle(text)` | No | Parses in-memory text only |
| `pg_ripple.load_ntriples(text)` | No | Parses in-memory text only |

```admonish danger title="Filesystem access"
File-path loaders can read any file the PostgreSQL process has access to. Never grant superuser to application roles. Instead, load data as a superuser and grant read access to application roles via schema permissions.
```

### Safe Bulk Load Pattern

```sql
-- As superuser: load the data
SELECT pg_ripple.load_turtle_file('/data/import/dataset.ttl');

-- As superuser: grant access to the app role
GRANT SELECT ON ALL TABLES IN SCHEMA _pg_ripple TO app_role;
```

---

## pg_ripple_http Security

The `pg_ripple_http` companion service exposes a SPARQL Protocol endpoint over HTTP. Secure it appropriately.

### TLS Configuration

Always run `pg_ripple_http` behind TLS in production:

```toml
# pg_ripple_http.toml
[server]
bind = "0.0.0.0:8443"
tls_cert = "/etc/ssl/certs/pg_ripple_http.crt"
tls_key = "/etc/ssl/private/pg_ripple_http.key"
```

```admonish danger title="Never expose HTTP without TLS"
SPARQL queries may contain sensitive data patterns. Without TLS, queries and results are transmitted in plaintext. Always terminate TLS either at the service or at a reverse proxy.
```

### Authentication

Configure `pg_ripple_http` to authenticate incoming requests:

```toml
[auth]
# HTTP Basic authentication backed by PostgreSQL roles
method = "pg_role"
# Or use a static API key
# method = "api_key"
# api_key = "your-secret-key-here"
```

With `pg_role` authentication, HTTP Basic credentials are forwarded to PostgreSQL. Graph RLS policies apply to the authenticated role.

### Reverse Proxy Setup

For production, place `pg_ripple_http` behind a reverse proxy:

```nginx
# nginx configuration
server {
    listen 443 ssl;
    server_name sparql.example.org;

    ssl_certificate /etc/ssl/certs/sparql.crt;
    ssl_certificate_key /etc/ssl/private/sparql.key;

    location /sparql {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;

        # Rate limiting
        limit_req zone=sparql burst=20 nodelay;
    }
}
```

### CORS Configuration

If the SPARQL endpoint is accessed from browser applications:

```toml
[cors]
allowed_origins = ["https://app.example.org"]
allowed_methods = ["GET", "POST"]
allowed_headers = ["Content-Type", "Authorization"]
max_age = 3600
```

```admonish warning title="Avoid wildcard origins"
Do not set `allowed_origins = ["*"]` in production. This allows any website to send SPARQL queries to your endpoint using the visitor's credentials.
```

---

## Network Isolation

### Production Topology

```
┌─────────────┐     TLS      ┌──────────────────┐     Unix socket     ┌─────────────┐
│  Clients    │ ────────────→ │  pg_ripple_http   │ ──────────────────→ │ PostgreSQL  │
│             │               │  (reverse proxy)  │                     │ (pg_ripple) │
└─────────────┘               └──────────────────┘                     └─────────────┘
```

### Recommendations

1. **PostgreSQL**: bind to `localhost` or a private network interface only. Never expose port 5432 to the public internet.

```ini
# postgresql.conf
listen_addresses = '127.0.0.1'
```

2. **pg_ripple_http**: connect to PostgreSQL via Unix socket for lowest latency and no network exposure.

3. **Firewall rules**: only allow traffic on the HTTPS port (443) from expected client networks.

```bash
# iptables example
iptables -A INPUT -p tcp --dport 443 -s 10.0.0.0/8 -j ACCEPT
iptables -A INPUT -p tcp --dport 443 -j DROP
iptables -A INPUT -p tcp --dport 5432 -j DROP
```

4. **pg_hba.conf**: restrict connections by source IP and authentication method:

```
# TYPE  DATABASE  USER              ADDRESS        METHOD
local   all       postgres                         peer
host    mydb      pg_ripple_http    127.0.0.1/32   scram-sha-256
host    mydb      sparql_reader     10.0.0.0/8     scram-sha-256
host    all       all               0.0.0.0/0      reject
```

```admonish tip title="Use scram-sha-256"
Always use `scram-sha-256` authentication (the default in PostgreSQL 18). Avoid `md5` and never use `trust` in production.
```

---

## Security Checklist

| Item | Status |
|---|---|
| `shared_preload_libraries` includes only trusted extensions | ☐ |
| Non-superuser roles used for all application connections | ☐ |
| Graph RLS enabled for multi-tenant deployments | ☐ |
| `pg_hba.conf` restricts connections to known networks | ☐ |
| TLS enabled on `pg_ripple_http` or reverse proxy | ☐ |
| File-path loaders restricted to superuser only (default) | ☐ |
| `synchronous_commit` enabled for production (not `off`) | ☐ |
| Connection pooler uses `scram-sha-256` | ☐ |
| CORS origins are not wildcarded | ☐ |
| PostgreSQL logs enabled for audit trail | ☐ |
| Regular security updates for PostgreSQL and pg_ripple | ☐ |

---

## Audit Logging

Enable PostgreSQL's logging to maintain an audit trail:

```ini
# postgresql.conf
log_statement = 'all'          # or 'ddl' for schema changes only
log_connections = on
log_disconnections = on
log_line_prefix = '%t [%p] %u@%d '
```

For fine-grained audit logging, consider the `pgaudit` extension alongside pg_ripple.

```admonish note title="SPARQL query logging"
pg_ripple logs the generated SQL via PostgreSQL's standard statement logging. To see the original SPARQL text, enable `log_statement = 'all'` — the SPARQL text appears as the argument to `pg_ripple.sparql()`.
```
