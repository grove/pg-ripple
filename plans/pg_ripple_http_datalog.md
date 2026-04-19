# Implementation Plan: Datalog API for pg_ripple_http

## Overview

Extend the `pg_ripple_http` service with a REST API for Datalog rule management, inference, and querying. The Datalog API complements the existing SPARQL endpoint by exposing pg_ripple's 27 Datalog-related SQL functions over HTTP, enabling external applications to manage rule sets, trigger inference, run goal-directed queries, and monitor the reasoning engine without a PostgreSQL driver.

## Motivation

- **Microservice integration**: Applications using REST/JSON cannot call `pg_ripple.load_rules()` or `pg_ripple.infer()` directly. The SPARQL endpoint only covers query/update; Datalog rule management has no HTTP surface.
- **Separation of concerns**: Datalog endpoints deserve their own `/datalog` namespace — different authentication scopes, rate limits, and content types from SPARQL.
- **Tooling**: A REST API enables web-based rule editors, monitoring dashboards, and CI/CD pipelines that load/test Datalog rules as part of deployment.

## Design Principles

1. **Thin HTTP layer**: Each endpoint is a straightforward axum handler that maps HTTP parameters to a single `pg_ripple.*` SQL function call via the existing `deadpool_postgres` pool. No Datalog parsing happens in the HTTP service.
2. **JSON-first**: All responses are `application/json`. Request bodies use `text/x-datalog` for rule text and `application/json` for structured inputs.
3. **Reuse existing infrastructure**: Authentication (`check_auth`), rate limiting (governor), CORS, metrics recording, and error redaction are shared with the SPARQL endpoints.
4. **RESTful resource modeling**: Rule sets are the primary resource (`/datalog/rules/{rule_set}`), inference and queries are action endpoints.

---

## Endpoints

### Phase 1: Rule Management (CRUD)

| Method | Path | SQL function | Description |
|--------|------|-------------|-------------|
| `POST` | `/datalog/rules/{rule_set}` | `pg_ripple.load_rules($body, $rule_set)` | Load rules from Datalog text body |
| `POST` | `/datalog/rules/{rule_set}/builtin` | `pg_ripple.load_rules_builtin($rule_set)` | Load a built-in rule set (`rdfs`, `owl-rl`) |
| `GET` | `/datalog/rules` | `pg_ripple.list_rules()` | List all rule sets and their rules |
| `DELETE` | `/datalog/rules/{rule_set}` | `pg_ripple.drop_rules($rule_set)` | Delete all rules in a rule set |
| `POST` | `/datalog/rules/{rule_set}/add` | `pg_ripple.add_rule($rule_set, $body)` | Add a single rule to an existing set |
| `DELETE` | `/datalog/rules/{rule_set}/{rule_id}` | `pg_ripple.remove_rule($rule_id)` | Remove a single rule by ID (triggers DRed) |
| `PUT` | `/datalog/rules/{rule_set}/enable` | `pg_ripple.enable_rule_set($rule_set)` | Activate a rule set |
| `PUT` | `/datalog/rules/{rule_set}/disable` | `pg_ripple.disable_rule_set($rule_set)` | Deactivate a rule set |

#### Request/Response Examples

**Load rules:**
```http
POST /datalog/rules/my-ontology HTTP/1.1
Content-Type: text/x-datalog

ancestor(?x, ?y) :- parent(?x, ?y).
ancestor(?x, ?z) :- parent(?x, ?y), ancestor(?y, ?z).
```
```json
{ "rule_set": "my-ontology", "rules_loaded": 2 }
```

**List rules:**
```http
GET /datalog/rules HTTP/1.1
Accept: application/json
```
```json
[
  {
    "id": 1,
    "rule_set": "my-ontology",
    "rule_text": "ancestor(?x, ?y) :- parent(?x, ?y).",
    "stratum": 0,
    "is_recursive": false,
    "active": true
  }
]
```

**Load built-in:**
```http
POST /datalog/rules/rdfs/builtin HTTP/1.1
```
```json
{ "rule_set": "rdfs", "rules_loaded": 13 }
```

### Phase 2: Inference

| Method | Path | SQL function | Description |
|--------|------|-------------|-------------|
| `POST` | `/datalog/infer/{rule_set}` | `pg_ripple.infer($rule_set)` | Materialize derived triples (simple) |
| `POST` | `/datalog/infer/{rule_set}/stats` | `pg_ripple.infer_with_stats($rule_set)` | Infer with detailed statistics |
| `POST` | `/datalog/infer/{rule_set}/agg` | `pg_ripple.infer_agg($rule_set)` | Aggregate-aware inference |
| `POST` | `/datalog/infer/{rule_set}/wfs` | `pg_ripple.infer_wfs($rule_set)` | Well-Founded Semantics inference |
| `POST` | `/datalog/infer/{rule_set}/demand` | `pg_ripple.infer_demand($rule_set, $demands)` | Demand-transformed inference |
| `POST` | `/datalog/infer/{rule_set}/lattice` | `pg_ripple.infer_lattice($rule_set, $lattice)` | Lattice-based inference |

#### Request/Response Examples

**Simple inference:**
```http
POST /datalog/infer/my-ontology HTTP/1.1
```
```json
{ "derived": 42 }
```

**Inference with stats:**
```http
POST /datalog/infer/my-ontology/stats HTTP/1.1
```
```json
{
  "derived": 42,
  "iterations": 3,
  "eliminated_rules": 0,
  "parallel_groups": 2,
  "max_concurrent": 4
}
```

**Demand-transformed inference:**
```http
POST /datalog/infer/my-ontology/demand HTTP/1.1
Content-Type: application/json

{
  "demands": [
    { "predicate": "ancestor", "bound": [0] }
  ]
}
```
```json
{
  "derived": 12,
  "iterations": 2,
  "demand_predicates": ["ancestor_bf"]
}
```

### Phase 3: Query & Goals

| Method | Path | SQL function | Description |
|--------|------|-------------|-------------|
| `POST` | `/datalog/query/{rule_set}` | `pg_ripple.infer_goal($rule_set, $goal)` | Goal-directed query via magic sets |
| `GET` | `/datalog/constraints` | `pg_ripple.check_constraints()` | Check constraint rules, return violations |
| `GET` | `/datalog/constraints/{rule_set}` | `pg_ripple.check_constraints($rule_set)` | Check constraints for a specific rule set |

#### Request/Response Examples

**Goal-directed query:**
```http
POST /datalog/query/my-ontology HTTP/1.1
Content-Type: text/x-datalog

ancestor(ex:alice, ?y).
```
```json
{
  "derived": 5,
  "iterations": 2,
  "matching": [
    { "y": "http://example.org/bob" },
    { "y": "http://example.org/carol" }
  ]
}
```

**Constraint check:**
```http
GET /datalog/constraints HTTP/1.1
```
```json
[
  { "rule": "no_self_parent", "violated": false },
  { "rule": "type_disjointness", "violated": true }
]
```

### Phase 4: Administration & Monitoring

| Method | Path | SQL function | Description |
|--------|------|-------------|-------------|
| `GET` | `/datalog/stats/cache` | `pg_ripple.rule_plan_cache_stats()` | Rule plan cache statistics |
| `GET` | `/datalog/stats/tabling` | `pg_ripple.tabling_stats()` | Tabling/memoisation cache statistics |
| `GET` | `/datalog/lattices` | `pg_ripple.list_lattices()` | List registered lattice types |
| `POST` | `/datalog/lattices` | `pg_ripple.create_lattice(...)` | Register a new lattice type |
| `GET` | `/datalog/views` | `pg_ripple.list_datalog_views()` | List all Datalog views |
| `POST` | `/datalog/views` | `pg_ripple.create_datalog_view(...)` | Create a Datalog view |
| `DELETE` | `/datalog/views/{name}` | `pg_ripple.drop_datalog_view($name)` | Drop a Datalog view |

---

## Implementation Details

### File Structure

```
pg_ripple_http/src/
├── main.rs           # Existing — add Datalog route registration
├── metrics.rs        # Existing — add datalog_queries counter
├── datalog.rs        # NEW — all Datalog endpoint handlers
└── common.rs         # NEW — extract shared helpers (check_auth, redacted_error, pool access)
```

### Step 1: Extract Common Helpers → `common.rs`

Move these from `main.rs` into a shared module:

- `AppState` struct (already used by SPARQL; Datalog handlers need it too)
- `check_auth()` — authentication check
- `redacted_error()` — error response builder
- `env_or()` — config helper

This avoids duplication and keeps both SPARQL and Datalog handlers thin.

### Step 2: Create `datalog.rs`

```rust
//! Datalog REST API handlers for pg_ripple_http.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::common::{check_auth, redacted_error, AppState};

// ─── Rule management ─────────────────────────────────────────────────────────

/// POST /datalog/rules/{rule_set}
/// Body: Datalog rule text (text/x-datalog or text/plain)
pub async fn load_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
    body: Body,
) -> Response { /* ... */ }

/// POST /datalog/rules/{rule_set}/builtin
pub async fn load_builtin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
) -> Response { /* ... */ }

/// GET /datalog/rules
pub async fn list_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response { /* ... */ }

/// DELETE /datalog/rules/{rule_set}
pub async fn drop_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rule_set): Path<String>,
) -> Response { /* ... */ }

// ... (remaining handlers follow the same pattern)
```

Each handler:
1. Calls `check_auth()`.
2. Acquires a connection from `state.pool`.
3. Calls the corresponding `pg_ripple.*` SQL function via `client.query()` / `client.execute()` with parameterized queries.
4. Maps the result to a JSON response.
5. Records metrics via `state.metrics.record_query()`.

### Step 3: Register Routes in `main.rs`

```rust
mod datalog;
mod common;

// In main(), after building the SPARQL routes:
let app = Router::new()
    // Existing SPARQL routes
    .route("/sparql", get(sparql_get).post(sparql_post))
    .route("/rag", axum::routing::post(rag_post))
    .route("/health", get(health))
    .route("/metrics", get(metrics_endpoint))
    // Datalog rule management
    .route("/datalog/rules", get(datalog::list_rules))
    .route("/datalog/rules/:rule_set",
        post(datalog::load_rules).delete(datalog::drop_rules))
    .route("/datalog/rules/:rule_set/builtin",
        post(datalog::load_builtin))
    .route("/datalog/rules/:rule_set/add",
        post(datalog::add_rule))
    .route("/datalog/rules/:rule_set/:rule_id",
        delete(datalog::remove_rule))
    .route("/datalog/rules/:rule_set/enable",
        put(datalog::enable_rule_set))
    .route("/datalog/rules/:rule_set/disable",
        put(datalog::disable_rule_set))
    // Datalog inference
    .route("/datalog/infer/:rule_set",
        post(datalog::infer))
    .route("/datalog/infer/:rule_set/stats",
        post(datalog::infer_with_stats))
    .route("/datalog/infer/:rule_set/agg",
        post(datalog::infer_agg))
    .route("/datalog/infer/:rule_set/wfs",
        post(datalog::infer_wfs))
    .route("/datalog/infer/:rule_set/demand",
        post(datalog::infer_demand))
    .route("/datalog/infer/:rule_set/lattice",
        post(datalog::infer_lattice))
    // Datalog queries
    .route("/datalog/query/:rule_set",
        post(datalog::query_goal))
    .route("/datalog/constraints",
        get(datalog::check_constraints_all))
    .route("/datalog/constraints/:rule_set",
        get(datalog::check_constraints))
    // Datalog admin
    .route("/datalog/stats/cache",
        get(datalog::cache_stats))
    .route("/datalog/stats/tabling",
        get(datalog::tabling_stats))
    .route("/datalog/lattices",
        get(datalog::list_lattices).post(datalog::create_lattice))
    .route("/datalog/views",
        get(datalog::list_views).post(datalog::create_view))
    .route("/datalog/views/:name",
        delete(datalog::drop_view))
    .layer(cors)
    .with_state(state);
```

### Step 4: Extend Metrics

Add a `datalog_queries` counter to the `Metrics` struct to differentiate SPARQL vs. Datalog traffic:

```rust
pub struct Metrics {
    queries: AtomicU64,          // SPARQL queries
    datalog_queries: AtomicU64,  // Datalog queries
    errors: AtomicU64,
    total_duration_us: AtomicU64,
}
```

Expose as a separate line in the `/metrics` output:

```
pg_ripple_http_sparql_queries_total 1234
pg_ripple_http_datalog_queries_total 56
pg_ripple_http_errors_total 3
pg_ripple_http_query_duration_seconds_total 12.345
```

### Step 5: Update README

Add a `## Datalog API` section to `pg_ripple_http/README.md` documenting every endpoint with curl examples, content types, and error codes.

### Step 6: Update `docker-compose.yml`

No changes needed — the same binary serves both SPARQL and Datalog endpoints on the same port.

---

## SQL Mapping Reference

Complete mapping from HTTP endpoints to parameterized SQL calls:

| Handler | SQL | Parameters |
|---------|-----|------------|
| `load_rules` | `SELECT pg_ripple.load_rules($1, $2)` | `(body_text, rule_set)` |
| `load_builtin` | `SELECT pg_ripple.load_rules_builtin($1)` | `(rule_set)` |
| `list_rules` | `SELECT pg_ripple.list_rules()` | — |
| `drop_rules` | `SELECT pg_ripple.drop_rules($1)` | `(rule_set)` |
| `add_rule` | `SELECT pg_ripple.add_rule($1, $2)` | `(rule_set, body_text)` |
| `remove_rule` | `SELECT pg_ripple.remove_rule($1)` | `(rule_id::i64)` |
| `enable_rule_set` | `SELECT pg_ripple.enable_rule_set($1)` | `(rule_set)` |
| `disable_rule_set` | `SELECT pg_ripple.disable_rule_set($1)` | `(rule_set)` |
| `infer` | `SELECT pg_ripple.infer($1)` | `(rule_set)` |
| `infer_with_stats` | `SELECT pg_ripple.infer_with_stats($1)` | `(rule_set)` |
| `infer_agg` | `SELECT pg_ripple.infer_agg($1)` | `(rule_set)` |
| `infer_wfs` | `SELECT pg_ripple.infer_wfs($1)` | `(rule_set)` |
| `infer_demand` | `SELECT pg_ripple.infer_demand($1, $2::jsonb)` | `(rule_set, demands_json)` |
| `infer_lattice` | `SELECT pg_ripple.infer_lattice($1, $2)` | `(rule_set, lattice_name)` |
| `query_goal` | `SELECT pg_ripple.infer_goal($1, $2)` | `(rule_set, goal_text)` |
| `check_constraints` | `SELECT pg_ripple.check_constraints($1)` | `(rule_set)` or `(NULL)` |
| `cache_stats` | `SELECT * FROM pg_ripple.rule_plan_cache_stats()` | — |
| `tabling_stats` | `SELECT * FROM pg_ripple.tabling_stats()` | — |
| `list_lattices` | `SELECT pg_ripple.list_lattices()` | — |
| `create_lattice` | `SELECT pg_ripple.create_lattice($1, $2, $3)` | `(name, join_fn, bottom)` |
| `list_views` | `SELECT pg_ripple.list_datalog_views()` | — |
| `create_view` | `SELECT pg_ripple.create_datalog_view($1, $2, $3, $4, $5, $6)` | `(name, rules, goal, rule_set, schedule, decode)` |
| `drop_view` | `SELECT pg_ripple.drop_datalog_view($1)` | `(name)` |

**All queries use `$1`, `$2`, … parameterized placeholders** — never string concatenation.

---

## Content Types

| Direction | MIME type | Usage |
|-----------|----------|-------|
| Request body (rules) | `text/x-datalog` or `text/plain` | Datalog rule text for `load_rules`, `add_rule`, `query_goal` |
| Request body (structured) | `application/json` | Demand specs, lattice definitions, view configs |
| Response | `application/json` | All Datalog responses |

---

## Error Handling

Reuse the existing `redacted_error()` pattern. Category strings for Datalog:

| HTTP Status | Category | Trigger |
|-------------|----------|---------|
| `400` | `datalog_parse_error` | Malformed rule text (parser error from extension) |
| `400` | `datalog_goal_error` | Invalid goal pattern |
| `400` | `invalid_request` | Missing body, wrong content-type, invalid rule_id |
| `404` | `rule_set_not_found` | `drop_rules` / `infer` on a nonexistent rule set |
| `503` | `service_unavailable` | Connection pool exhausted |

---

## Security Considerations

1. **Authentication**: All `/datalog/*` endpoints go through `check_auth()`, same as SPARQL.
2. **Write protection**: Rule-modifying endpoints (`POST`, `PUT`, `DELETE` under `/datalog/rules`) could optionally be gated behind a separate `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` env var, allowing read-only access for inference/query while restricting rule management.
3. **SQL injection**: All SQL calls use parameterized queries via `tokio_postgres` — never string interpolation of user input.
4. **Request size limits**: Rule text bodies are limited to 10 MB (same as SPARQL), enforced via `axum::body::to_bytes(body, 10 * 1024 * 1024)`.
5. **Rate limiting**: The existing governor layer applies to all routes including `/datalog/*`.

---

## Testing Strategy

### Unit / Integration Tests

1. **Handler tests**: Use `axum::test::TestClient` (from `axum-test` crate) to exercise each endpoint against a mock or real PostgreSQL with pg_ripple.
2. **Round-trip tests**: Load rules → infer → query goal → verify results → drop rules — all via HTTP.
3. **Error paths**: Malformed Datalog, nonexistent rule sets, oversized bodies, missing auth.

### Manual Smoke Tests

```bash
# Load rules
curl -X POST http://localhost:7878/datalog/rules/test \
  -H "Content-Type: text/x-datalog" \
  -d 'ancestor(?x, ?y) :- parent(?x, ?y).
ancestor(?x, ?z) :- parent(?x, ?y), ancestor(?y, ?z).'

# List rules
curl http://localhost:7878/datalog/rules | jq .

# Infer
curl -X POST http://localhost:7878/datalog/infer/test

# Goal query
curl -X POST http://localhost:7878/datalog/query/test \
  -H "Content-Type: text/x-datalog" \
  -d 'ancestor(ex:alice, ?y).'

# Check constraints
curl http://localhost:7878/datalog/constraints

# Cleanup
curl -X DELETE http://localhost:7878/datalog/rules/test
```

---

## Phased Delivery

| Phase | Scope | Endpoints | Complexity |
|-------|-------|-----------|------------|
| **1** | Rule CRUD | 8 endpoints | Low — direct SQL passthrough |
| **2** | Inference | 6 endpoints | Low — single SQL call each, JSON passthrough |
| **3** | Query & Constraints | 3 endpoints | Low–Medium — goal text parsing, result formatting |
| **4** | Admin & Monitoring | 7 endpoints | Low — read-only SQL calls |

All four phases can be delivered in a single release since each handler is a thin wrapper (~30–50 lines) around an existing SQL function. Total new Rust code: ~800–1000 lines in `datalog.rs`, ~100 lines in `common.rs`, ~50 lines of route registration and metrics changes.

---

## Open Questions

1. **Separate auth token for Datalog writes?** Adding `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` would allow public read (query/infer) while restricting rule management. Worth implementing from day one or defer?
2. **Streaming large inference results?** `infer_goal` returns JSONB from the extension. For very large result sets, consider streaming rows with `SELECT result FROM pg_ripple.sparql(...)` — but this changes the interface. Defer unless needed.
3. **WebSocket support for long-running inference?** Some `infer` calls on large datasets take seconds to minutes. A WebSocket or SSE endpoint for progress would be useful but adds significant complexity. Defer to a later release.
4. **OpenAPI spec?** Generate an OpenAPI 3.1 spec for the full API (SPARQL + Datalog + RAG). Could use `utoipa` crate for auto-generation from handler annotations.
