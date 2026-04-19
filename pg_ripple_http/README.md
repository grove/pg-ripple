# pg_ripple_http

Standalone HTTP service that exposes a [W3C SPARQL 1.1 Protocol](https://www.w3.org/TR/sparql11-protocol/) endpoint for [pg_ripple](../README.md). Any standard SPARQL client — YASGUI, SPARQLWrapper, Jena, or plain `curl` — can query pg_ripple without a PostgreSQL driver.

## Build

```bash
cargo build --release -p pg_ripple_http
```

The binary is placed at `target/release/pg_ripple_http`.

**Requirements:** Rust 1.88+, and a running PostgreSQL 18 instance with the `pg_ripple` extension installed.

## Run

```bash
./target/release/pg_ripple_http
```

On startup, the service connects to PostgreSQL, verifies that pg_ripple is available, and logs the connection details:

```
INFO pg_ripple_http: connected to postgresql://localhost/postgres (port 7878), triple store contains 12345 triples
INFO pg_ripple_http: pg_ripple_http listening on http://0.0.0.0:7878
```

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://localhost/postgres` | PostgreSQL connection URL |
| `PG_RIPPLE_HTTP_PORT` | `7878` | HTTP listening port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `16` | Database connection pool size |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | (unset) | If set, requests must include `Authorization: Bearer <token>` |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `0` | Max requests/sec per client IP (0 = disabled) |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | Comma-separated allowed origins, or `*` for all |

Example:

```bash
export PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@db-host:5432/mydb"
export PG_RIPPLE_HTTP_PORT=8080
export PG_RIPPLE_HTTP_AUTH_TOKEN="my-secret-token"
./target/release/pg_ripple_http
```

## Endpoints

### `GET /health`

Returns `200 OK` when the service is up and the database is reachable. Use for load balancer health checks.

```bash
curl http://localhost:7878/health
```

### `GET /metrics`

Prometheus-compatible metrics.

```bash
curl http://localhost:7878/metrics
```

### `GET /sparql?query=…`

Run a SPARQL query via URL parameter.

```bash
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"
```

### `POST /sparql`

Run a SPARQL query or update via request body.

| Content-Type | Body |
|---|---|
| `application/sparql-query` | Raw SPARQL SELECT/ASK/CONSTRUCT/DESCRIBE |
| `application/sparql-update` | Raw SPARQL INSERT/DELETE |
| `application/x-www-form-urlencoded` | `query=…` or `update=…` |

```bash
# SELECT
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-query" \
  -d "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"

# Update
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -d 'INSERT DATA { <http://example.org/alice> <http://example.org/name> "Alice" }'
```

## Content negotiation

Set the `Accept` header to control the response format:

| Accept | Format |
|---|---|
| `application/sparql-results+json` *(default for SELECT/ASK)* | SPARQL Results JSON |
| `application/sparql-results+xml` | SPARQL Results XML |
| `text/csv` | CSV |
| `text/tab-separated-values` | TSV |
| `text/turtle` *(default for CONSTRUCT/DESCRIBE)* | Turtle |
| `application/n-triples` | N-Triples |
| `application/ld+json` | JSON-LD |

```bash
curl -G http://localhost:7878/sparql \
  -H "Accept: text/csv" \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5"
```

## Authentication

If `PG_RIPPLE_HTTP_AUTH_TOKEN` is set, every request must include the token:

```bash
curl -G http://localhost:7878/sparql \
  -H "Authorization: Bearer my-secret-token" \
  --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 5"
```

Both `Authorization: Bearer <token>` and `Authorization: Basic <token>` are accepted.

## Docker Compose

The root `docker-compose.yml` runs both PostgreSQL and `pg_ripple_http` together:

```bash
docker compose up
```

Services:

| Service | Port | Description |
|---|---|---|
| `postgres` | 5432 | PostgreSQL 18 + pg_ripple |
| `sparql` | 7878 | SPARQL HTTP endpoint |

---

## Datalog API

Since v0.39.0, `pg_ripple_http` also exposes a `/datalog` REST namespace that lets any HTTP client manage Datalog rule sets, trigger inference, run goal-directed queries, check integrity constraints, and inspect monitoring statistics — without a PostgreSQL driver.

All Datalog endpoints accept and return `application/json` (unless specified otherwise). Rule text uses `text/x-datalog` or `text/plain`.

### Authentication

The same `PG_RIPPLE_HTTP_AUTH_TOKEN` bearer token covers all `/datalog/*` endpoints. Optionally, set `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` to restrict mutating endpoints (`POST`, `PUT`, `DELETE`) to a separate token while keeping read endpoints (inference triggers, monitoring, GET) covered by the main token.

### Phase 1 — Rule management

#### `POST /datalog/rules/{rule_set}`

Load rules from Datalog text. Body: `text/x-datalog`.

```bash
curl -X POST http://localhost:7878/datalog/rules/my-ontology \
  -H "Content-Type: text/x-datalog" \
  -d 'ancestor(?x, ?y) :- parent(?x, ?y).
ancestor(?x, ?z) :- parent(?x, ?y), ancestor(?y, ?z).'
# → {"rule_set": "my-ontology", "rules_loaded": 2}
```

#### `POST /datalog/rules/{rule_set}/builtin`

Load a built-in rule set (`rdfs`, `owl-rl`).

```bash
curl -X POST http://localhost:7878/datalog/rules/rdfs/builtin
# → {"rule_set": "rdfs", "rules_loaded": 13}
```

#### `GET /datalog/rules`

List all rule sets and their rules.

```bash
curl http://localhost:7878/datalog/rules
# → [{"id": 1, "rule_set": "my-ontology", "rule_text": "…", "active": true}, …]
```

#### `DELETE /datalog/rules/{rule_set}`

Delete all rules in a rule set.

```bash
curl -X DELETE http://localhost:7878/datalog/rules/my-ontology
# → {"deleted": 2}
```

#### `POST /datalog/rules/{rule_set}/add`

Add a single rule to an existing rule set. Body: `text/x-datalog`.

```bash
curl -X POST http://localhost:7878/datalog/rules/my-ontology/add \
  -H "Content-Type: text/x-datalog" \
  -d 'sibling(?x, ?y) :- parent(?p, ?x), parent(?p, ?y).'
# → {"rule_set": "my-ontology", "rule_id": 3}
```

#### `DELETE /datalog/rules/{rule_set}/{rule_id}`

Remove a single rule by ID (triggers DRed retraction).

```bash
curl -X DELETE http://localhost:7878/datalog/rules/my-ontology/3
# → {"removed": 1}
```

#### `PUT /datalog/rules/{rule_set}/enable`

Activate a rule set.

```bash
curl -X PUT http://localhost:7878/datalog/rules/my-ontology/enable
# → {"rule_set": "my-ontology", "enabled": true}
```

#### `PUT /datalog/rules/{rule_set}/disable`

Deactivate a rule set.

```bash
curl -X PUT http://localhost:7878/datalog/rules/my-ontology/disable
# → {"rule_set": "my-ontology", "enabled": false}
```

### Phase 2 — Inference

#### `POST /datalog/infer/{rule_set}`

Materialize derived triples (semi-naive evaluation).

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology
# → {"derived": 42}
```

#### `POST /datalog/infer/{rule_set}/stats`

Infer with detailed per-stratum statistics.

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology/stats
# → {"derived": 42, "iterations": 3, "eliminated_rules": 0, "parallel_groups": 2, …}
```

#### `POST /datalog/infer/{rule_set}/agg`

Aggregate-aware inference (Datalog^agg).

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology/agg
# → {"derived": 12}
```

#### `POST /datalog/infer/{rule_set}/wfs`

Well-Founded Semantics inference (three-valued).

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology/wfs
# → {"derived": 8}
```

#### `POST /datalog/infer/{rule_set}/demand`

Demand-transformed (goal-directed) inference. Body: JSON.

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology/demand \
  -H "Content-Type: application/json" \
  -d '{"demands": [{"predicate": "ancestor", "bound": [0]}]}'
# → {"derived": 12, "iterations": 2, "demand_predicates": ["ancestor_bf"]}
```

#### `POST /datalog/infer/{rule_set}/lattice`

Lattice-based inference (Datalog^L). Body: JSON.

```bash
curl -X POST http://localhost:7878/datalog/infer/my-ontology/lattice \
  -H "Content-Type: application/json" \
  -d '{"lattice": "min"}'
# → {"derived": 5}
```

### Phase 3 — Query & constraints

#### `POST /datalog/query/{rule_set}`

Goal-directed query via magic sets. Body: Datalog goal text.

```bash
curl -X POST http://localhost:7878/datalog/query/my-ontology \
  -H "Content-Type: text/x-datalog" \
  -d 'ancestor(ex:alice, ?y).'
# → {"derived": 5, "iterations": 2, "matching": [{"y": "http://example.org/bob"}, …]}
```

#### `GET /datalog/constraints`

Check all constraint rules; return violations.

```bash
curl http://localhost:7878/datalog/constraints
# → [{"rule": "no_self_parent", "violated": false}, …]
```

#### `GET /datalog/constraints/{rule_set}`

Check constraints for a specific rule set.

```bash
curl http://localhost:7878/datalog/constraints/my-ontology
```

### Phase 4 — Admin & monitoring

#### `GET /datalog/stats/cache`

Rule plan cache statistics.

```bash
curl http://localhost:7878/datalog/stats/cache
# → {"size": 12, "hits": 340, "misses": 8, …}
```

#### `GET /datalog/stats/tabling`

Tabling/memoization cache statistics.

```bash
curl http://localhost:7878/datalog/stats/tabling
# → {"entries": 100, "hit_rate": 0.82, …}
```

#### `GET /datalog/lattices`

List registered lattice types.

```bash
curl http://localhost:7878/datalog/lattices
# → [{"name": "min", "join_fn": "LEAST", "bottom": "Infinity"}, …]
```

#### `POST /datalog/lattices`

Register a new lattice type. Body: JSON.

```bash
curl -X POST http://localhost:7878/datalog/lattices \
  -H "Content-Type: application/json" \
  -d '{"name": "my_min", "join_fn": "my_schema.my_min", "bottom": "9999"}'
# → {"created": "my_min"}
```

#### `GET /datalog/views`

List all Datalog materialized views.

```bash
curl http://localhost:7878/datalog/views
# → [{"name": "ancestor_view", "goal": "ancestor(?x, ?y).", …}, …]
```

#### `POST /datalog/views`

Create a Datalog materialized view. Body: JSON.

```bash
curl -X POST http://localhost:7878/datalog/views \
  -H "Content-Type: application/json" \
  -d '{"name": "ancestor_view", "goal": "ancestor(?x, ?y).", "rule_set": "my-ontology"}'
# → {"created": "ancestor_view"}
```

#### `DELETE /datalog/views/{name}`

Drop a Datalog materialized view.

```bash
curl -X DELETE http://localhost:7878/datalog/views/ancestor_view
# → {"dropped": "ancestor_view"}
```

### Error codes

| HTTP Status | `error` field | Trigger |
|---|---|---|
| `400` | `datalog_parse_error` | Malformed Datalog rule text |
| `400` | `datalog_goal_error` | Invalid goal pattern |
| `400` | `invalid_request` | Missing body, wrong content-type, non-numeric `rule_id` |
| `404` | `rule_set_not_found` | Infer/drop on a nonexistent rule set |
| `401` | — | Missing or invalid `Authorization` token |
| `503` | `service_unavailable` | Connection pool exhausted |

All error responses include a `trace_id` field for log correlation.

