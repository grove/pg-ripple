# HTTP API Reference

`pg_ripple_http` is a standalone Rust binary that exposes the full pg_ripple feature set over HTTP. It supports the W3C SPARQL 1.1 Protocol, Server-Sent Events streaming, Datalog REST operations, PageRank, probabilistic reasoning, Arrow Flight bulk export, and administrative endpoints.

Start the service:

```bash
PG_RIPPLE_HTTP_PG_URL=postgresql://localhost/mydb \
PG_RIPPLE_HTTP_PORT=7878 \
PG_RIPPLE_HTTP_AUTH_TOKEN=mysecret \
pg_ripple_http
```

---

## Authentication

All endpoints that mutate state require a Bearer token when `PG_RIPPLE_HTTP_AUTH_TOKEN` is set.

```
Authorization: Bearer <token>
```

Read-only endpoints (`GET /health`, `GET /metrics`, `GET /openapi.yaml`) do not require authentication.

---

## Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://localhost/postgres` | PostgreSQL connection URL |
| `PG_RIPPLE_HTTP_PORT` | `7878` | TCP listening port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `16` | PostgreSQL connection pool size |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | *(none)* | Bearer token; set to enable auth |
| `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` | *(falls back to auth token)* | Optional separate token for mutating Datalog/rule endpoints |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `100` | Per-IP rate limit (req/s; 0 = unlimited) |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `''` | Comma-separated allowed CORS origins; `*` enables permissive CORS |
| `PG_RIPPLE_HTTP_MAX_BODY_BYTES` | `10485760` | Max request body size (10 MiB) |
| `PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK` | *(unset)* | Set to `1` to skip extension version check |
| `PG_RIPPLE_HTTP_STRICT_COMPAT` | *(unset)* | Set to `1` to fail startup on incompatible extension versions |
| `PG_RIPPLE_HTTP_METRICS_TOKEN` | *(none)* | Optional bearer token for `/metrics` |
| `PG_RIPPLE_HTTP_AUTH_REALM` | `pg_ripple` | `WWW-Authenticate` realm for 401 responses |
| `PG_RIPPLE_HTTP_REPLICA_DSN` | *(none)* | Optional read-replica PostgreSQL DSN |
| `PG_RIPPLE_HTTP_TRUST_PROXY` | *(none)* | Trusted upstream proxy IP/CIDR list for forwarded headers |
| `PG_RIPPLE_HTTP_CA_BUNDLE` | *(system roots)* | Extra CA bundle for outbound HTTPS clients |
| `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` | *(none)* | Optional pinned TLS certificate fingerprints |
| `PG_RIPPLE_HTTP_SHUTDOWN_TIMEOUT_SECS` | `30` | Graceful shutdown timeout |
| `ARROW_FLIGHT_SECRET` | *(none)* | HMAC secret for Arrow Flight tickets |
| `ARROW_UNSIGNED_TICKETS_ALLOWED` | `false` | Allow unsigned Arrow tickets for local development |
| `ARROW_NONCE_CACHE_MAX` | `10000` | Replay-protection nonce cache size |

---

## Complete Endpoint Reference

`Read` and `Write` mean the endpoint calls the HTTP companion's read or write
authentication check when `PG_RIPPLE_HTTP_AUTH_TOKEN` is configured. `None` means
the endpoint is intentionally unauthenticated.

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/sparql` | Read | SPARQL 1.1 query via URL parameters |
| `POST` | `/sparql` | Query-dependent | SPARQL 1.1 query/update via request body; updates require write auth |
| `POST` | `/sparql/stream` | Read | Streaming SPARQL SELECT (SSE) |
| `POST` | `/rag` | Read | RAG retrieval / NL-to-SPARQL |
| `GET` | `/health` | None | Liveness probe |
| `GET` | `/ready` | None | Kubernetes readiness probe |
| `GET` | `/health/ready` | None | Deep extension readiness probe |
| `GET` | `/metrics` | None or metrics token | Prometheus metrics |
| `GET` | `/metrics/extension` | None | Extension-internal Prometheus metrics |
| `GET` | `/void` | None | VoID dataset description |
| `GET` | `/service` | None | SPARQL service description |
| `GET` | `/openapi.yaml` | None | OpenAPI 3.1 specification |
| `GET` | `/explorer` | Read | Web-based SPARQL explorer UI |
| `GET` | `/admin/bench-history` | Write | Recent benchmark history from `_pg_ripple.bench_history` |
| `GET` | `/admin/diagnostic-snapshot` | Write | Diagnostic bundle with schema, GUC, version, and metrics data |
| `POST` | `/flight/do_get` | Write | Arrow Flight bulk export |
| `GET` | `/subscribe/{subscription_id}` | Read | Live SPARQL subscription SSE stream |
| `GET` | `/datalog/rules` | Read | List Datalog rule sets |
| `POST/DELETE` | `/datalog/rules/{rule_set}` | Write | Load or drop a rule set |
| `POST` | `/datalog/rules/{rule_set}/builtin` | Write | Load built-in rules for a rule set |
| `POST` | `/datalog/rules/{rule_set}/add` | Write | Add a rule to a rule set |
| `PUT` | `/datalog/rules/{rule_set}/enable` | Write | Enable a rule set |
| `PUT` | `/datalog/rules/{rule_set}/disable` | Write | Disable a rule set |
| `DELETE` | `/datalog/rules/{rule_set}/{rule_id}` | Write | Delete a specific rule |
| `POST` | `/datalog/infer/{rule_set}` | Write | Run forward-chaining inference |
| `POST` | `/datalog/infer/{rule_set}/stats` | Write | Run inference and return stats |
| `POST` | `/datalog/infer/{rule_set}/agg` | Write | Run inference with aggregation |
| `POST` | `/datalog/infer/{rule_set}/wfs` | Write | Run well-founded semantics inference |
| `POST` | `/datalog/infer/{rule_set}/demand` | Write | Goal-directed demand inference |
| `POST` | `/datalog/infer/{rule_set}/lattice` | Write | Lattice-based inference |
| `POST` | `/datalog/query/{rule_set}` | Read | Goal-directed Datalog query |
| `GET` | `/datalog/constraints` | Read | Check all constraint rules |
| `GET` | `/datalog/constraints/{rule_set}` | Read | Check constraints for one rule set |
| `GET` | `/datalog/stats/cache` | Read | Rule plan cache statistics |
| `GET` | `/datalog/stats/tabling` | Read | Tabling cache statistics |
| `GET/POST` | `/datalog/lattices` | Read/Write | List or create lattice structures |
| `GET/POST` | `/datalog/views` | Read/Write | List or create Datalog-backed views |
| `DELETE` | `/datalog/views/{name}` | Write | Drop a Datalog-backed view |
| `POST` | `/pagerank/run` | Write | Start PageRank computation |
| `GET` | `/pagerank/status` | Read | PageRank computation status |
| `GET` | `/pagerank/results` | Read | Retrieve PageRank scores |
| `GET` | `/pagerank/export` | Read | Export PageRank scores |
| `GET` | `/pagerank/explain/{node_iri}` | Read | Explain PageRank score for a node |
| `GET` | `/pagerank/queue-stats` | Read | PageRank queue statistics |
| `POST` | `/pagerank/vacuum-dirty` | Write | Vacuum stale PageRank rows |
| `POST` | `/centrality/run` | Write | Compute centrality metrics |
| `GET` | `/centrality/results` | Read | Retrieve centrality results |
| `POST` | `/pagerank/find-duplicates` | Read | Find near-duplicate nodes |
| `POST` | `/confidence/load` | Write | Load triples with confidence scores |
| `GET` | `/confidence/shacl-score` | Read | Get SHACL soft-validation scores |
| `GET` | `/confidence/shacl-report` | Read | Get scored SHACL validation report |
| `POST` | `/confidence/vacuum` | Write | Vacuum stale confidence rows |
| `POST` | `/confidence/update` | Write | Run Bayesian confidence update |
| `POST` | `/confidence/bulk-update` | Write | Bulk confidence update |
| `POST` | `/explain` | Read | Natural-language explanation of a query or result |
| `GET` | `/explain` | Read | Explanation endpoint metadata/query form |
| `POST` | `/hypothetical` | Write | What-if reasoning against hypothetical facts |
| `GET` | `/rule-conflicts/{ruleset}` | Read | Detect rule conflicts in a rule set |
| `GET` | `/rule-libraries` | Read | List rule libraries |
| `GET` | `/rule-libraries/{name}/stream` | Read | Stream a published rule library |
| `POST` | `/rule-libraries/{name}/subscribe` | Write | Subscribe to a remote rule library stream |
| `POST` | `/rules/draft` | Write | Draft rules from natural-language guidance |
| `POST` | `/rules/validate` | Write | Validate a drafted rule |
| `GET` | `/rules/{id}/explain` | Read | Explain a rule by ID |
| `GET/POST` | `/temporal/mark` | Read/Write | List or mark temporal predicates |
| `POST` | `/temporal/point_in_time` | Write | Set point-in-time temporal context |
| `GET` | `/temporal/facts` | Read | List temporal facts |
| `GET` | `/temporal/graphs/{iri}/snapshot` | Read | Materialize a point-in-time graph snapshot |
| `GET` | `/temporal/graphs/{iri}/diff` | Read | Diff a named graph between two timestamps |
| `POST` | `/pprl/bloom_encode` | Write | Privacy-preserving Bloom encoding |
| `POST` | `/pprl/dice_similarity` | Read | Dice similarity over encoded values |
| `POST` | `/dp/noisy_count` | Read | Differential privacy noisy count |
| `POST` | `/dp/noisy_histogram` | Read | Differential privacy noisy histogram |
| `GET` | `/dp/budget/{dataset}/{principal}` | Read | Privacy budget status |
| `POST` | `/entity-resolution/resolve` | Write | Run entity resolution |
| `POST` | `/entity-resolution/evaluate` | Read | Evaluate entity-resolution output |
| `POST` | `/entity-resolution/monitoring/enable` | Write | Enable entity-resolution monitoring |
| `POST` | `/entity-resolution/monitoring/disable` | Write | Disable entity-resolution monitoring |
| `GET` | `/proof-tree/{subject}/{predicate}/{object}` | Read | Explain derivation/proof tree for a triple |
| `GET/POST` | `/tenants` | Read/Write | List or create tenants |
| `GET/DELETE` | `/tenants/{name}` | Read/Write | Get or delete a tenant |
| `GET/POST` | `/tenants/{name}/quota` | Read/Write | Get or update tenant quota |
| `GET` | `/federation/{endpoint}/auth-status` | Write | Per-endpoint federation credential status |
| `POST` | `/json-mapping/{name}/writeback` | Write | Synchronous JSON mapping relational writeback |
| `GET` | `/json-mapping/{name}/writeback/status` | Read | JSON mapping writeback queue status |
| `GET` | `/json-mapping/{name}/writeback/config` | Read | Validated JSON mapping writeback configuration |

---

## SPARQL Endpoints

### `GET /sparql`

W3C SPARQL 1.1 Protocol query endpoint.

**Query Parameters:**

| Parameter | Required | Description |
|---|---|---|
| `query` | Yes (query) | URL-encoded SPARQL SELECT/CONSTRUCT/ASK/DESCRIBE query |
| `update` | Yes (update) | URL-encoded SPARQL UPDATE |
| `default-graph-uri` | No | Default graph URI |
| `named-graph-uri` | No | Named graph URI (repeatable) |

**Accept header** controls the result format:

| Accept | Format |
|---|---|
| `application/sparql-results+json` | SPARQL JSON (default) |
| `application/sparql-results+xml` | SPARQL XML |
| `text/turtle` | Turtle (for CONSTRUCT/DESCRIBE) |
| `application/n-triples` | N-Triples (for CONSTRUCT/DESCRIBE) |
| `application/ld+json` | JSON-LD (for CONSTRUCT/DESCRIBE) |

**Example:**

```bash
curl -G "http://localhost:7878/sparql" \
  --data-urlencode 'query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10' \
  -H "Accept: application/sparql-results+json"
```

---

### `POST /sparql`

Accepts the query either as an `application/x-www-form-urlencoded` body (form post) or as a raw `application/sparql-query` / `application/sparql-update` body.

**Form body parameters:** same as `GET /sparql` query parameters.

**Example (SELECT):**

```bash
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-query" \
  -d 'SELECT ?label WHERE { <https://example.org/Alice> rdfs:label ?label }'
```

**Example (UPDATE, requires auth):**

```bash
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -H "Authorization: Bearer $TOKEN" \
  -d 'INSERT DATA { <:Alice> rdfs:label "Alice" }'
```

---

### `POST /sparql/stream`

Streaming SPARQL SELECT using Server-Sent Events. Results are delivered as SSE `data:` events, one result row per event, as the query executes.

**Request:** Same as `POST /sparql`.

**Response Content-Type:** `text/event-stream`

**Event format:**

```
event: row
data: {"?name": "\"Alice\"^^xsd:string", "?age": "\"30\"^^xsd:integer"}

event: done
data: {"total_rows": 42}
```

**Example:**

```bash
curl -X POST http://localhost:7878/sparql/stream \
  -H "Content-Type: application/sparql-query" \
  -H "Accept: text/event-stream" \
  -d 'SELECT ?s ?p ?o WHERE { ?s ?p ?o }' \
  --no-buffer
```

---

## RAG Endpoint

### `POST /rag`

Execute a hybrid SPARQL + vector-similarity RAG retrieval query. The endpoint embeds the question, finds the nearest RDF entities by cosine similarity, and returns structured results with a plain-text context string suitable for use as an LLM prompt.

**Content-Type:** `application/json`

**Request body:**

```json
{
  "question": "what treats headaches?",
  "sparql_filter": "?entity a <https://pharma.example/Drug>",
  "k": 5,
  "model": "text-embedding-3-small",
  "output_format": "jsonb"
}
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `question` | string | **yes** | — | Natural-language question |
| `sparql_filter` | string | no | `null` | SPARQL WHERE fragment to filter candidates |
| `k` | integer | no | `5` | Number of nearest neighbors |
| `model` | string | no | *(GUC)* | Override `pg_ripple.embedding_model` |
| `output_format` | string | no | `"jsonb"` | `"jsonb"` or `"jsonld"` |

**Response (200 OK):**

```json
{
  "results": [
    {
      "entity_iri": "https://pharma.example/aspirin",
      "label": "aspirin",
      "context_json": {
        "label": "aspirin",
        "types": ["Drug", "NSAID"],
        "properties": [{"predicate": "treats", "object": "headache"}],
        "contextText": "aspirin. Type: NSAID, Drug."
      },
      "distance": 0.12
    }
  ],
  "context": "aspirin. Type: NSAID, Drug.\n\nibuprofen. Type: Drug."
}
```

The `context` field is a concatenated plain-text summary ready for use as an LLM system prompt.

---

## JSON Mapping Writeback

### `POST /json-mapping/{name}/writeback`

Synchronously write one RDF subject back to the relational table configured for
the named JSON mapping. This calls `pg_ripple.writeback_json_row(name,
subject_iri)` and requires write auth when authentication is enabled.

**Content-Type:** `application/json`

```json
{
  "subject_iri": "https://example.com/contacts/c001"
}
```

**Response (200 OK):**

```json
{
  "rows_affected": 1
}
```

**Error mapping:**

| Status | Error | Cause |
|---|---|---|
| `422` | `writeback_target_not_configured` | The mapping has no writeback table or key columns (`PT0550`) |
| `409` | `writeback_conflict` | Conflict policy is `error` and a conflicting row exists (`PT0551`) |
| `422` | `writeback_key_column_missing` | A configured key-column predicate has no value for this subject (`PT0552`) |

### `GET /json-mapping/{name}/writeback/status`

Return queue depth, error count, and last processed timestamp for one mapping.
This is a filtered HTTP view over `pg_ripple.json_writeback_status()` and
requires read auth when authentication is enabled.

**Response (200 OK):**

```json
{
  "mapping_name": "contacts",
  "pending": 0,
  "errors": 0,
  "last_error": null,
  "last_processed_at": null
}
```

### `GET /json-mapping/{name}/writeback/config`

Return the validated relational target and writeback health for one mapping.
The response mirrors `pg_ripple.writeback_inspect()` and requires read auth
when authentication is enabled.

**Response (200 OK):**

```json
{
  "target_schema": "public",
  "target_table": "contacts",
  "key_columns": ["contact_id"],
  "conflict_policy": "replace",
  "writeback_enabled": false,
  "trigger_count": 0,
  "queue_depth": 0
}
```

---

## Arrow Flight Bulk Export

### `POST /flight/do_get`

Bulk-export SPARQL query results as an [Apache Arrow IPC](https://arrow.apache.org/docs/format/Columnar.html) record stream. Suitable for high-throughput extract pipelines (DuckDB, pandas, Spark).

**Content-Type:** `application/json`

**Request body:**

```json
{
  "ticket": "eyJhbGciOiJIUzI1NiJ9...",
  "query": "SELECT ?s ?p ?o WHERE { ?s ?p ?o }"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `ticket` | string | **yes** | HMAC-signed JWT ticket issued by `pg_ripple.arrow_flight_ticket(query)` |
| `query` | string | no | Inline query (only when `pg_ripple.arrow_unsigned_tickets_allowed = on`) |

**Response:** `application/vnd.apache.arrow.stream` — Arrow IPC stream.

**Example (Python):**

```python
import requests, pyarrow as pa

# 1. Get a signed ticket from PostgreSQL
ticket = pg.execute("SELECT pg_ripple.arrow_flight_ticket('SELECT ?s ?p ?o WHERE { ?s ?p ?o }')")

# 2. Stream the Arrow IPC export
r = requests.post("http://localhost:7878/flight/do_get",
    json={"ticket": ticket},
    headers={"Authorization": f"Bearer {token}"},
    stream=True)

reader = pa.ipc.open_stream(r.raw)
table = reader.read_all()
print(table.to_pandas())
```

---

## Streaming Subscriptions

### `GET /subscribe/{subscription_id}`

Subscribe to a live SPARQL result stream via Server-Sent Events. The `subscription_id` is returned by `pg_ripple.subscribe(query)`.

**Response Content-Type:** `text/event-stream`

**Event types:**

| Event | Payload | Description |
|---|---|---|
| `row` | JSON object | New result row |
| `retract` | JSON object | Retracted row (triple deleted) |
| `heartbeat` | `{}` | Keep-alive every 30 s |
| `error` | `{"message": "..."}` | Subscription error |

**Example:**

```bash
# Create subscription in PostgreSQL
psql -c "SELECT pg_ripple.subscribe('SELECT ?s ?p ?o WHERE { ?s ?p ?o }')"
-- Returns: sub_abc123

# Connect SSE stream
curl -N "http://localhost:7878/subscribe/sub_abc123" \
  -H "Accept: text/event-stream"
```

---

## Datalog REST API

### `GET /datalog/rules`

List all registered Datalog rule sets.

**Response (200 OK):**

```json
[
  {"rule_set": "rdfs_closure", "rule_count": 13, "enabled": true},
  {"rule_set": "custom_rules", "rule_count": 5, "enabled": true}
]
```

---

### `POST /datalog/rules/{rule_set}/add`

Add a Datalog rule to a rule set.

**Content-Type:** `application/json`

```json
{
  "rule": "ancestor(?x, ?z) :- ancestor(?x, ?y), parent(?y, ?z).",
  "enabled": true
}
```

**Response (200 OK):**

```json
{"rule_id": "rule_42", "rule_set": "custom_rules"}
```

---

### `POST /datalog/infer/{rule_set}`

Run forward-chaining inference for a rule set.

**Content-Type:** `application/json`

```json
{
  "graph": "https://example.org/graph1",
  "max_iterations": 100,
  "dry_run": false
}
```

**Response (200 OK):**

```json
{
  "derived_count": 847,
  "iterations": 3,
  "elapsed_ms": 42,
  "stratification_order": ["base_rules", "closure_rules"]
}
```

---

### `POST /datalog/infer/{rule_set}/wfs`

Run well-founded semantics inference. Returns three-valued results (true / false / undefined) for recursive rules with negation.

---

### `POST /datalog/infer/{rule_set}/demand`

Goal-directed magic-sets inference. Only derives facts relevant to a specific query goal.

**Content-Type:** `application/json`

```json
{
  "goal": "ancestor(<:Alice>, ?z)",
  "graph": "https://example.org/graph1"
}
```

---

### `POST /datalog/query/{rule_set}`

Run a single Datalog goal query against a materialized rule set without modifying the store.

**Content-Type:** `application/json`

```json
{
  "goal": "ancestor(<:Alice>, ?z)",
  "limit": 100
}
```

**Response (200 OK):**

```json
{
  "results": [
    {"?z": "<https://example.org/Bob>"},
    {"?z": "<https://example.org/Carol>"}
  ],
  "total": 2
}
```

---

### `GET /datalog/constraints`

Check all constraint rules across all rule sets.

**Response (200 OK):**

```json
{
  "violations": [
    {
      "rule_set": "integrity_rules",
      "constraint": "uniqueness_violation",
      "violating_bindings": [{"?x": "<:entity1>"}]
    }
  ],
  "total_violations": 1
}
```

---

### `GET /datalog/stats/cache`

Returns rule plan cache hit/miss statistics.

---

### `GET /datalog/stats/tabling`

Returns tabling cache occupancy and eviction statistics.

---

## PageRank API

### `POST /pagerank/run`

Start a PageRank computation job.

**Content-Type:** `application/json`

```json
{
  "graph": "https://example.org/graph1",
  "edge_predicates": ["schema:knows", "schema:worksFor"],
  "damping": 0.85,
  "max_iterations": 100,
  "convergence_delta": 0.0001
}
```

**Response (200 OK):**

```json
{
  "job_id": "pr_20250503_001",
  "status": "running",
  "started_at": "2025-05-03T12:00:00Z"
}
```

---

### `GET /pagerank/status`

Poll the status of the most recent PageRank job.

**Response:**

```json
{
  "job_id": "pr_20250503_001",
  "status": "completed",
  "iterations": 47,
  "elapsed_ms": 1204,
  "node_count": 50000,
  "edge_count": 250000
}
```

---

### `GET /pagerank/export`

Export PageRank scores as JSON or CSV.

**Query parameters:**

| Parameter | Default | Description |
|---|---|---|
| `format` | `json` | `json` or `csv` |
| `limit` | `1000` | Maximum nodes |
| `offset` | `0` | Pagination offset |
| `graph` | *(all)* | Filter by named graph |

**Response (200 OK, JSON):**

```json
{
  "scores": [
    {"iri": "https://example.org/Alice", "score": 0.0423, "rank": 1},
    {"iri": "https://example.org/Bob", "score": 0.0381, "rank": 2}
  ],
  "total": 50000
}
```

---

### `GET /pagerank/explain/{node_iri}`

Explain the PageRank score for a specific node, showing top contributing in-edges.

**Path parameter:** `node_iri` — URL-encoded IRI.

**Response:**

```json
{
  "iri": "https://example.org/Alice",
  "score": 0.0423,
  "rank": 1,
  "top_contributors": [
    {"from_iri": "https://example.org/Bob", "edge_weight": 0.012},
    {"from_iri": "https://example.org/Carol", "edge_weight": 0.009}
  ]
}
```

---

### `POST /centrality/run`

Compute centrality metrics (degree, betweenness, Katz).

**Content-Type:** `application/json`

```json
{
  "graph": "https://example.org/graph1",
  "metrics": ["degree", "katz"],
  "katz_alpha": 0.01
}
```

---

### `POST /pagerank/find-duplicates`

Find near-duplicate nodes by PageRank score similarity and graph structure.

---

## Confidence / Probabilistic API

### `POST /confidence/load`

Load triples with associated confidence scores.

**Content-Type:** `application/json`

```json
{
  "triples": [
    {
      "subject": "https://example.org/Alice",
      "predicate": "rdf:type",
      "object": "schema:Person",
      "confidence": 0.95,
      "graph": "https://example.org/g1"
    }
  ]
}
```

**Response (200 OK):**

```json
{"inserted": 1, "updated": 0}
```

---

### `GET /confidence/shacl-score`

Get SHACL soft-validation confidence scores for shapes.

**Query parameters:**

| Parameter | Default | Description |
|---|---|---|
| `shape_iri` | *(all)* | Filter by specific shape |
| `min_score` | `0.0` | Minimum score threshold |

---

### `GET /confidence/shacl-report`

Get a full SHACL validation report with confidence scores per constraint.

---

### `POST /confidence/vacuum`

Vacuum stale confidence score rows from `_pg_ripple.confidence`.

---

## Administrative Endpoints

### `GET /health`

Returns `{"status": "ok"}` when the service is running and the database connection pool is healthy.

**Response:** `200 OK` — `{"status": "ok"}`
**Response:** `503 Service Unavailable` — database unreachable.

---

### `GET /health/ready`

Kubernetes-compatible readiness probe. Checks that the PostgreSQL connection pool has at least one available connection.

---

### `GET /metrics`

Prometheus-format metrics. No authentication required.

**Key metrics:**

| Metric | Type | Description |
|---|---|---|
| `pg_ripple_http_requests_total` | Counter | Total HTTP requests by method, path, status |
| `pg_ripple_http_request_duration_seconds` | Histogram | Request latency |
| `pg_ripple_sparql_queries_total` | Counter | SPARQL queries by type |
| `pg_ripple_sparql_query_duration_seconds` | Histogram | SPARQL query latency |
| `pg_ripple_federation_calls_total` | Counter | SERVICE clause calls by endpoint |
| `pg_ripple_datalog_infer_duration_seconds` | Histogram | Inference run latency |

### `GET /metrics/extension`

Extension-internal Prometheus metrics sourced directly from the pg_ripple extension within PostgreSQL.

---

### `GET /void`

Returns a VoID (Vocabulary of Interlinked Datasets) dataset description, summarizing the RDF store contents: triple counts, predicate frequencies, and class distributions.

**Accept:** `text/turtle` or `application/ld+json`

---

### `GET /service`

Returns a SPARQL 1.1 Service Description document describing the endpoint's capabilities.

**Accept:** `text/turtle` or `application/ld+json`

---

### `GET /openapi.yaml`

Returns the full OpenAPI 3.1 specification for the `pg_ripple_http` API.

---

### `GET /explorer`

Web-based SPARQL explorer UI — an embedded query editor with syntax highlighting, result table, and endpoint configuration.

---

## Error Responses

All error responses use a consistent JSON envelope:

```json
{
  "error": "PT440",
  "message": "query exceeds maximum algebra depth (256)",
  "hint": "Simplify the query or raise pg_ripple.sparql_max_algebra_depth"
}
```

| HTTP Status | Meaning |
|---|---|
| 400 | Invalid request body or SPARQL syntax error |
| 401 | Missing or invalid Bearer token |
| 403 | Operation not permitted for this role |
| 404 | Resource not found |
| 422 | Semantic error (SHACL violation, constraint error) |
| 429 | Rate limit exceeded |
| 500 | Internal error (check PostgreSQL logs) |
| 503 | Database connection unavailable |

---

## Security

- Bearer token authentication controls mutating endpoints
- SSRF protection: federation SERVICE endpoints are checked against `federation_endpoint_policy`
- SQL injection prevention: all queries use parameterized PostgreSQL SPI
- Arrow Flight tickets are HMAC-SHA256 signed; set `pg_ripple.arrow_flight_secret`
- Rate limiting: per-IP, configurable via `PG_RIPPLE_HTTP_RATE_LIMIT`
- HTTPS termination should be handled by a reverse proxy (nginx, Caddy, Traefik)
- Never expose `PG_RIPPLE_HTTP_AUTH_TOKEN` in URLs or logs
