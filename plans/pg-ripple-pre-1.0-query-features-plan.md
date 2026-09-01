# pg-ripple Pre-1.0 Query Interface Completion Plan

**Status:** Accepted and integrated roadmap amendment  
**Baseline:** `main` after commit `f3b66c8bbee6c35a326bf9ae11269dd6ac30ce88`  
**Plan date:** 2026-08-26  
**Target releases:** v0.134.0, v0.135.0, and post-GA v1.1.0  
**Applies to:** PostgreSQL extension, `pg_ripple_http`, SQL migrations, CI, documentation, release evidence, and the v1 stable API manifest

**Roadmap pages:** [v0.134.0](../roadmap/v0.134.0.md), [v0.135.0](../roadmap/v0.135.0.md), and [v1.1.0](../roadmap/v1.1.0.md)

---

## 1. Executive decision

pg-ripple should add only two tightly scoped query-interface milestones before v1.0.0:

1. **v0.134.0 — true HTTP streaming, backpressure, timeout, and cancellation**, folded into the existing performance and scale qualification release.
2. **v0.135.0 — parameterized SPARQL and SPARQL use of the existing prefix registry**, folded into the existing API and compatibility freeze release.

A stored/prepared-query registry should be delivered in **v1.1.0**, after the v1 API is frozen and released.

This plan does **not** create another pre-GA version. It expands v0.134.0 and v0.135.0 while leaving v0.136.0–v0.143.0 focused on audit, qualification, and promotion.

The additions qualify for pre-1.0 inclusion because they close existing core-interface gaps:

- `pg_ripple_http` advertises streaming paths, but the current implementations either buffer the complete PostgreSQL result through `client.query()` or emit only start/done SSE events.
- The SQL extension already exposes cursor-oriented query functions, so completing the HTTP path is finishing an existing contract rather than introducing a new product family.
- Applications currently have no typed initial-binding API and must construct SPARQL text themselves.
- `_pg_ripple.prefixes`, `register_prefix()`, and `prefixes()` already exist, but the registry is currently used for export only; SPARQL queries must still declare every prefix inline.

The pre-GA scope remains intentionally narrow:

- No WebSocket or gRPC protocol.
- No new query language.
- No parameterized SPARQL Update in v1.0.
- No multi-row binding tables in v1.0.
- No stored-query catalog before v1.0.
- No automatic HTTP endpoint generation.
- No change to strict standards behavior unless the caller explicitly opts into registered prefixes.

---

## 2. Repository baseline

### 2.1 Existing streaming pieces

The extension already exposes:

```sql
pg_ripple.sparql_cursor(query text)
pg_ripple.sparql_cursor_turtle(query text)
pg_ripple.sparql_cursor_jsonld(query text)
```

The intended contract is bounded Rust-side memory through page-oriented fetching.

The HTTP service currently has two incomplete streaming paths:

1. `POST /sparql/stream` creates a response channel but calls `tokio_postgres::Client::query()`. That API returns all rows before the service starts iterating over them, so the PostgreSQL result is buffered before HTTP delivery.
2. `pg_ripple_http/src/stream.rs` creates an SSE response, emits a start event and a done event, but does not execute or emit the SPARQL result rows.

The two paths should be replaced by one execution and encoding pipeline.

### 2.2 Existing prefix registry

The repository already creates:

```sql
CREATE TABLE _pg_ripple.prefixes (
    prefix    text primary key,
    expansion text not null
);
```

and exposes:

```sql
pg_ripple.register_prefix(prefix text, expansion text)
pg_ripple.prefixes()
```

Current documentation explicitly states that the registry is not used by SPARQL queries. The v0.135.0 work therefore hardens and extends the existing registry; it must not introduce a parallel registry.

### 2.3 Missing parameter API

The current public SQL surface accepts a query string but no typed initial bindings:

```sql
pg_ripple.sparql(query text)
pg_ripple.sparql_construct(query text)
pg_ripple.sparql_describe(query text, strategy text)
pg_ripple.sparql_cursor(query text)
```

No `sparql_bind` or equivalent typed overload exists.

---

## 3. Roadmap changes at a glance

| Version | Existing theme | Revised theme | Incremental effort | Revised release gate |
|---|---|---|---:|---|
| **v0.134.0** | Performance and scale qualification | **Performance, scale, and true streaming qualification** | **3–5 person-weeks** | Large results are delivered with bounded memory; slow clients apply backpressure; disconnects and deadlines cancel PostgreSQL work |
| **v0.135.0** | API, schema, GUC, and compatibility freeze | **Safe application query API and compatibility freeze** | **3–5 person-weeks** | Typed bindings cannot alter query syntax; registered prefixes are deterministic and opt-in; the new surface is included in the v1 manifest |
| **v1.1.0** | Existing broad ecosystem proposal | **Prepared query registry and client conveniences** | **4–6 person-weeks** | Stored read queries have ownership, typed parameter schemas, invalidation, and controlled execution |

Estimated combined pre-GA addition: **6–9 person-weeks**, allowing for overlap in HTTP integration, documentation, tests, and stable-manifest work.

### Dependency order

```text
v0.134.0 streaming foundation
        ↓
v0.135.0 parameterized queries use the same streaming pipeline
        ↓
v0.136.0 external audit covers both additions
        ↓
v0.137.0–v0.143.0 fail-closed qualification sequence exercises both additions
        ↓
v1.0.0 promotes the exact qualified candidate
```

---

# 4. v0.134.0 — Performance, scale, and true streaming qualification

## 4.1 Objective

Complete the query-delivery path so large SPARQL results are sent incrementally from PostgreSQL to the client with:

- Bounded memory.
- Natural network backpressure.
- Explicit query deadlines.
- PostgreSQL cancellation on timeout or client disconnect.
- Standards-valid output.
- Clean connection-pool recovery.
- Release-grade observability.

This work is part of v0.134.0 because streaming behavior must be benchmarked and memory-qualified alongside the existing performance program.

## 4.2 Supported scope

### Query forms

| Query form | v0.134 streaming status |
|---|---|
| `SELECT` | Required |
| `ASK` | Required, one result |
| `CONSTRUCT` | Required as N-Triples |
| `DESCRIBE` | Required as N-Triples |
| SPARQL Update | Not a result-streaming operation; existing buffered response remains |

### Streaming formats

| Format | Media type | v0.134 behavior |
|---|---|---|
| SPARQL Results JSON | `application/sparql-results+json` | Valid streaming JSON envelope |
| CSV | `text/csv` | Header followed by escaped rows |
| TSV | `text/tab-separated-values` | Header followed by escaped rows |
| N-Triples | `application/n-triples` | One canonical triple per line |
| NDJSON extension | `application/x-ndjson` | Optional explicit extension; never mislabeled as SPARQL Results JSON |
| SPARQL Results XML | `application/sparql-results+xml` | May remain buffered in v0.134, with existing row/byte limits |
| Turtle | `text/turtle` | May remain buffered; callers needing true graph streaming use N-Triples |
| JSON-LD | `application/ld+json` | Remains buffered because document compaction/framing is document-oriented |

The standard `/sparql` endpoint should use the streaming pipeline whenever the negotiated format is streaming-safe. `POST /sparql/stream` remains as an explicit compatibility alias and must call the same implementation.

## 4.3 Architecture

### 4.3.1 One execution pipeline

Create:

```text
pg_ripple_http/src/streaming/
├── mod.rs
├── execution.rs
├── cancel.rs
├── limits.rs
├── metrics.rs
└── encoder/
    ├── mod.rs
    ├── sparql_json.rs
    ├── csv.rs
    ├── tsv.rs
    ├── ntriples.rs
    └── ndjson.rs
```

Remove duplicated behavior from:

- `pg_ripple_http/src/stream.rs`
- `pg_ripple_http/src/routing/sparql_handlers.rs`

The route handlers should only:

1. Authenticate and authorize.
2. Parse request/query parameters.
3. Validate the query form and output format.
4. Select the primary or replica pool.
5. Construct a `StreamingQuery`.
6. Return the body stream.

### 4.3.2 Streaming PostgreSQL rows

Replace all streaming-path uses of:

```rust
client.query(sql, params).await
```

with:

```rust
client.query_raw(sql, params).await
```

or the typed equivalent returning a `RowStream`.

The response body must poll the PostgreSQL row stream directly. Do not place a detached producer task ahead of an unbounded or large channel. A small encoder buffer is allowed only to coalesce output up to a fixed maximum, such as 64 KiB.

The response stream owns:

- The checked-out database connection.
- The PostgreSQL `RowStream`.
- The output encoder state.
- A PostgreSQL cancel token.
- A deadline.
- Request metrics.
- The transaction/session cleanup state.

The connection is returned to the pool only after:

- Normal completion and transaction cleanup.
- Query cancellation and cleanup.
- A database error and cleanup.

### 4.3.3 Database transaction and timeout handling

Each stream runs on a dedicated checked-out connection.

Before executing the result query:

```sql
BEGIN READ ONLY;
SET LOCAL statement_timeout = '<bounded value>';
SET LOCAL idle_in_transaction_session_timeout = '<bounded value>';
```

For requests routed to the primary, read-only query forms must still use a read-only transaction.

At normal completion:

```sql
COMMIT;
```

After timeout, disconnect, encoding failure, or database error:

1. Send PostgreSQL cancel.
2. Await or bound the cancellation attempt.
3. Issue `ROLLBACK`.
4. Discard the pooled connection if session state cannot be proven clean.

Configuration:

```text
PG_RIPPLE_HTTP_QUERY_TIMEOUT_MS          default 300000
PG_RIPPLE_HTTP_QUERY_TIMEOUT_MAX_MS      default 900000
PG_RIPPLE_HTTP_STREAM_CHUNK_BYTES        default 65536
PG_RIPPLE_HTTP_STREAM_MAX_ROW_BYTES      default 1048576
PG_RIPPLE_HTTP_STREAM_IDLE_TIMEOUT_MS    default 60000
```

A caller-supplied timeout may lower the configured timeout but may not exceed the configured maximum.

### 4.3.4 Cancellation

Implement a `CancellationGuard`:

- Obtain `tokio_postgres::CancelToken` before starting the query.
- Mark the guard complete only after the final encoder footer and transaction completion.
- On body-stream drop, enqueue cancellation to a small dedicated cancellation executor.
- Use `CancelToken::cancel_query(...)`.
- Record whether cancellation was requested due to:
  - `client_disconnect`
  - `deadline`
  - `idle_timeout`
  - `server_shutdown`
  - `encoder_error`
- If cancellation fails, close/discard the connection and increment a failure metric.

A disconnect test must prove that the query disappears from `pg_stat_activity` within a bounded interval.

### 4.3.5 Typed binding-row representation

Do not keep the current HTTP heuristic that guesses term type from a decoded string.

Add a shared internal representation:

```rust
struct ResultBindingRow {
    values: BTreeMap<String, Option<RdfBindingValue>>,
}

enum RdfBindingValue {
    Iri(String),
    BlankNode(String),
    Literal {
        lexical: String,
        datatype: Option<String>,
        language: Option<String>,
    },
    Triple(Box<QuotedTripleBinding>),
}
```

The extension’s decode path already has access to dictionary kind, datatype, and language. Add an internal streaming function that returns one typed binding object per result row.

Suggested internal SQL surface:

```sql
_pg_ripple.sparql_stream_metadata(query text) returns jsonb
_pg_ripple.sparql_stream_bindings(query text) returns setof jsonb
_pg_ripple.sparql_stream_triples(query text) returns setof text
```

These functions are internal implementation APIs and are excluded from the stable public manifest.

`_pg_ripple.sparql_stream_metadata()` returns:

```json
{
  "form": "select",
  "variables": ["person", "name"]
}
```

This ensures correct headers for empty result sets.

`_pg_ripple.sparql_stream_bindings()` returns each row using the SPARQL Results JSON binding-value shape, for example:

```json
{
  "person": {
    "type": "uri",
    "value": "https://example.org/alice"
  },
  "name": {
    "type": "literal",
    "value": "Alice",
    "xml:lang": "en"
  }
}
```

The same representation should be used by buffered and streaming HTTP formatters so they cannot disagree about RDF term types.

## 4.4 Output encoder requirements

### SPARQL Results JSON

Emit:

```text
{"head":{"vars":["person","name"]},"results":{"bindings":[
```

Then zero or more comma-separated binding objects, followed by:

```text
]}}
```

Requirements:

- Empty results still include the projected variable list.
- No trailing comma.
- All strings use `serde_json`, never manual escaping.
- A complete successful response is valid JSON.
- An execution error after headers have been emitted terminates the body and is recorded server-side; it must not append an invalid pseudo-error object to the standards document.

### CSV and TSV

- Emit headers from query metadata.
- Use RFC-compatible CSV quoting.
- TSV values use SPARQL TSV term syntax, not display strings.
- Newlines, tabs, quotes, backslashes, Unicode, language tags, and typed literals must round-trip.

### N-Triples

- Use the existing RDF serializer rather than hand-built string formatting.
- Emit one complete line per triple.
- Support RDF-star only when the selected serializer and advertised media type support it; otherwise reject with a clear format error.

### NDJSON

If retained:

- Advertise `application/x-ndjson`.
- Emit one complete typed binding row per line.
- Permit a final structured error record because NDJSON is a pg-ripple extension.
- Document that NDJSON is not the W3C SPARQL Results JSON format.

## 4.5 HTTP behavior

### Standard endpoint

Existing GET and POST `/sparql` behavior remains wire-compatible. Streaming is an implementation improvement for safe formats.

### Explicit stream endpoint

`POST /sparql/stream`:

- Requires `application/sparql-query` or the existing accepted request form.
- Honors `Accept`.
- Returns `406 Not Acceptable` for formats that are not streamable in v0.134.
- Does not invent a misleading content type.
- Uses the same authentication, replica routing, limits, trace propagation, and error model as `/sparql`.

### Headers

Add:

```text
X-Pg-Ripple-Streaming: true
X-Pg-Ripple-Query-Id: <opaque UUID>
```

Do not manually set `Transfer-Encoding`; let the HTTP server select chunked transfer or HTTP/2 framing.

Optional response trailers may be investigated, but they are not required for v0.134 acceptance.

## 4.6 Observability

Add Prometheus metrics:

```text
pg_ripple_http_stream_requests_total{form,format,status}
pg_ripple_http_stream_active{form,format}
pg_ripple_http_stream_rows_total{form}
pg_ripple_http_stream_bytes_total{format}
pg_ripple_http_stream_duration_seconds{form,format}
pg_ripple_http_stream_first_byte_seconds{form,format}
pg_ripple_http_stream_cancellations_total{reason}
pg_ripple_http_stream_cancel_failures_total
pg_ripple_http_stream_db_errors_total{class}
pg_ripple_http_stream_encoder_errors_total{format}
pg_ripple_http_stream_connection_discards_total{reason}
```

The extension’s `streaming_metrics()` counters must be wired to actual row and page emission rather than remaining at zero due to unused increment helpers.

## 4.7 Tests

### Encoder unit tests

Cover:

- Empty results.
- One row.
- Multiple rows.
- All RDF term types.
- Quotes, commas, tabs, backslashes, newlines, Unicode, and control characters.
- Typed and language-tagged literals.
- Empty and unbound variables.
- Error before first row.
- Error after one or more rows.

### HTTP integration tests

Add `pg_ripple_http/tests/streaming.rs`:

1. **First-byte test:** a deliberately slow query sends its first result before the full query finishes.
2. **Bounded-memory test:** stream at least one million rows while asserting RSS remains within a fixed envelope.
3. **Slow-client test:** throttle reads and assert bounded buffering.
4. **Disconnect test:** stop reading, drop the body, and verify PostgreSQL cancellation through `pg_stat_activity`.
5. **Deadline test:** a query exceeding the configured timeout is canceled.
6. **Pool-reuse test:** the next request receives a clean connection.
7. **Empty-result test:** JSON/CSV/TSV headers are correct.
8. **Format validity:** parse the complete JSON response and round-trip CSV/TSV/N-Triples.
9. **Replica test:** `replica=ok` uses the replica pool and cancellation still works.
10. **Shutdown test:** SIGTERM cancels or drains streams according to the configured shutdown policy.

### Extension tests

- Cursor page counters increase.
- Row counters increase.
- Large result iteration does not collect all rows.
- Metadata returns variables for an empty result.
- Typed term descriptors preserve datatype and language.

### Performance gates

Add to v0.134 benchmark evidence:

- Time to first byte.
- Rows per second.
- Peak HTTP RSS.
- Peak PostgreSQL backend memory where measurable.
- Cancellation latency.
- Behavior at 1, 10, 100, and 500 concurrent streams.
- Slow-client performance.
- JSON versus CSV versus TSV versus N-Triples throughput.

## 4.8 Files expected to change

```text
pg_ripple_http/Cargo.toml
pg_ripple_http/src/stream.rs                 remove or reduce to compatibility wrapper
pg_ripple_http/src/routing/sparql_handlers.rs
pg_ripple_http/src/spi_bridge.rs
pg_ripple_http/src/common.rs
pg_ripple_http/src/metrics.rs
pg_ripple_http/src/streaming/*
src/sparql_api.rs
src/sparql/cursor.rs
src/sparql/decode.rs
src/stats.rs
tests/pg_regress/sql/v0134_streaming.sql
tests/pg_regress/expected/v0134_streaming.out
pg_ripple_http/tests/streaming.rs
docs/src/reference/http-api.md
docs/src/user-guide/sql-reference/sparql-query.md
docs/src/operations/tuning.md
docs/src/reference/limits.md
roadmap/v0.134.0.md
roadmap/v0.134.0-full.md
```

## 4.9 Required CI jobs

```text
http-stream-format
http-stream-first-byte
http-stream-disconnect-cancel
http-stream-timeout
http-stream-slow-client
http-stream-memory
http-stream-pool-reuse
regress-v0134-streaming
benchmark-streaming
```

## 4.10 Exit criteria

v0.134.0 may ship only when:

- No streaming-safe format buffers the full result in `pg_ripple_http`.
- A one-million-row result stays within the accepted memory envelope.
- A disconnected client cancels its PostgreSQL query.
- Timeouts cancel both local and replica-routed queries.
- The next pooled request sees no leaked transaction or session setting.
- JSON, CSV, TSV, and N-Triples outputs pass independent parsers.
- Current raw streaming benchmarks are included in the release evidence.
- Existing v0.134 scale gates also pass.

---

# 5. v0.135.0 — Safe application query API and compatibility freeze

## 5.1 Objective

Add a typed, non-interpolating SPARQL binding API and integrate the existing prefix registry into SPARQL through an explicit deterministic mode. Freeze both capabilities as part of the v1 application contract.

## 5.2 Public SQL API

Add overloads rather than a parallel family of nearly identical functions:

```sql
pg_ripple.sparql(
    query text,
    bindings jsonb
) returns setof jsonb
```

```sql
pg_ripple.sparql_construct(
    query text,
    bindings jsonb
) returns setof jsonb
```

```sql
pg_ripple.sparql_describe(
    query text,
    bindings jsonb,
    strategy text default 'cbd'
) returns setof jsonb
```

```sql
pg_ripple.sparql_cursor(
    query text,
    bindings jsonb
) returns setof jsonb
```

The existing one-argument functions remain unchanged and delegate to the new path with an empty binding map.

Do **not** add parameterized SPARQL Update before v1.0.0. A call attempting to use bindings with an update operation must fail explicitly.

## 5.3 Binding JSON contract

Use the W3C SPARQL Results JSON term representation as the input vocabulary.

Example:

```json
{
  "email": {
    "type": "literal",
    "value": "alice@example.com"
  },
  "class": {
    "type": "uri",
    "value": "https://schema.org/Person"
  },
  "minimumAge": {
    "type": "literal",
    "value": "18",
    "datatype": "http://www.w3.org/2001/XMLSchema#integer"
  },
  "label": {
    "type": "literal",
    "value": "Hei",
    "xml:lang": "nb"
  }
}
```

### Validation rules

- The top-level value must be a JSON object.
- Variable keys may be `name` or `?name`; they are normalized to `name`.
- Two keys that normalize to the same name are an error.
- Variable names must satisfy the SPARQL variable grammar.
- Maximum bindings per request: 64 by default.
- Supported `type` values in v1.0:
  - `uri`
  - `literal`
- `bnode` input is rejected before v1.0 because blank-node identifiers are scoped and not portable between clients.
- RDF-star `triple` input is deferred.
- `literal` may specify either `datatype` or `xml:lang`, never both.
- A language tag must be normalized and validated.
- A datatype must be an absolute IRI.
- JSON `null` is rejected; callers omit an unbound variable.
- Unknown object fields are rejected by default.
- A binding for a variable absent from the parsed query is an error by default.
- An option to permit unused bindings is not part of the v1 stable surface.

### Limits

Add:

```text
pg_ripple.sparql_max_initial_bindings       default 64
pg_ripple.sparql_max_binding_value_bytes    default 1048576
```

The HTTP request body remains subject to the sidecar body-size limit.

## 5.4 Algebra and execution design

### 5.4.1 Never perform textual substitution

The implementation must not use:

- `str::replace`
- regex replacement
- string concatenation into the query body
- manual quoting of IRIs or literals

Bindings are applied after parsing.

### 5.4.2 Initial solution mapping

Create:

```text
src/sparql/bindings.rs
```

Responsibilities:

1. Parse and validate the JSON binding map.
2. Convert terms to an internal `InitialBinding`.
3. Verify variables against parsed query scope.
4. Add a one-row binding relation to the query algebra.
5. Preserve the binding in projection even when it does not occur in a triple pattern.
6. Combine correctly with query-local `VALUES`, `OPTIONAL`, `UNION`, filters, subqueries, aggregation, and property paths.

The preferred semantic implementation is a one-row `VALUES`/table algebra node joined at the root query pattern. Construct it programmatically from parsed terms.

### 5.4.3 SQL parameters

Extend query compilation to produce:

```rust
struct CompiledSparql {
    sql: String,
    parameters: Vec<SqlParameter>,
    parameter_types: Vec<PgOid>,
    metadata: QueryMetadata,
    cache_key: PlanCacheKey,
}
```

Initial bound RDF terms are encoded through the dictionary and supplied as typed SQL parameters. They must not be embedded into generated SQL.

### 5.4.4 Plan cache behavior

The plan cache key includes:

- Normalized query.
- Query form.
- Ordered bound-variable names.
- Bound-term categories and PostgreSQL parameter types.
- Prefix mode.
- Prefix-registry generation when registered-prefix mode is active.
- Existing schema generation and relevant GUC keys.

The key does **not** include binding values.

Acceptance test:

- Execute the same query with 100 different bound values.
- Assert one compilation miss followed by cache hits, subject to existing cache policy.
- Assert results remain value-correct.

## 5.5 Prefix registry integration

### 5.5.1 Preserve strict behavior

Existing standard behavior remains the default:

```text
pg_ripple.sparql_prefix_mode = 'strict'
```

In strict mode, every prefix used by a query must be declared in that query.

Add opt-in mode:

```text
pg_ripple.sparql_prefix_mode = 'registered'
```

In registered mode:

- Query-local declarations always win.
- Missing declarations may be supplied from `_pg_ripple.prefixes`.
- Unknown prefixes remain parse errors.
- Prefix changes invalidate affected cached plans.
- The final expanded query/algebra is available through `explain_sparql`.

The standard HTTP `/sparql` endpoint remains strict unless the authenticated caller explicitly requests registered mode through the pg-ripple JSON binding endpoint described below.

### 5.5.2 Harden the existing registry

Extend `_pg_ripple.prefixes`:

```sql
ALTER TABLE _pg_ripple.prefixes
    ADD COLUMN owner_oid oid not null default current_user::regrole::oid,
    ADD COLUMN created_at timestamptz not null default now(),
    ADD COLUMN updated_at timestamptz not null default now();
```

Add singleton state:

```sql
CREATE TABLE _pg_ripple.prefix_registry_state (
    singleton boolean primary key default true check (singleton),
    generation bigint not null default 1
);
```

Every successful register, replace, or drop increments `generation`.

Public API:

```sql
pg_ripple.register_prefix(prefix text, expansion text) returns void
pg_ripple.drop_prefix(prefix text) returns boolean
pg_ripple.prefixes() returns table(prefix text, expansion text)
pg_ripple.prefix_registry_generation() returns bigint
```

Existing `register_prefix()` and `prefixes()` signatures remain compatible.

### 5.5.3 Validation and privileges

At registration time:

- Validate the prefix label against the SPARQL/Turtle prefix grammar.
- Reject empty labels unless the project explicitly supports a default prefix.
- Require expansion to be an absolute IRI.
- Reject whitespace, control characters, angle-bracket terminators, and malformed percent escapes.
- Normalize only where standards allow; do not silently rewrite semantically distinct IRIs.

Prefix changes affect database-wide query semantics. Therefore:

- Revoke direct INSERT/UPDATE/DELETE on `_pg_ripple.prefixes`.
- Permit registry mutation only to the database owner, superuser, or the designated pg-ripple administrator role.
- Allow all authorized query users to read resolved prefixes.
- Keep the functions `SECURITY INVOKER` unless a narrowly reviewed definer function is necessary.
- If a definer function is used, pin `search_path` and fully qualify all objects.

### 5.5.4 Query prologue processing

Create:

```text
src/sparql/prologue.rs
```

Algorithm:

1. Scan only the formal SPARQL prologue.
2. Collect query-local `PREFIX` declarations and optional `BASE`.
3. Load registered prefixes only when mode is `registered`.
4. Filter out names already declared locally.
5. Serialize validated missing declarations in deterministic lexical order.
6. Prepend them to the query before the normal parser.
7. Record the effective prefix map in query metadata and `explain_sparql`.

Do not search-and-replace QName-looking text. Strings, comments, IRIs, and expressions must never be modified.

### 5.5.5 Cache invalidation

When registered-prefix mode is used, include the prefix-registry generation in the plan-cache key.

Changing a prefix must:

- Increment generation transactionally.
- Make existing cached plans unreachable.
- Not invalidate strict-mode plans.
- Be visible only after commit.
- Roll back generation changes if the transaction rolls back.

## 5.6 HTTP API

Add:

```text
POST /sparql/bindings
Content-Type: application/json
```

Request:

```json
{
  "query": "SELECT ?person WHERE { ?person schema:email ?email }",
  "bindings": {
    "email": {
      "type": "literal",
      "value": "alice@example.com"
    }
  },
  "prefix_mode": "registered",
  "replica": "ok",
  "timeout_ms": 30000
}
```

Rules:

- Only `SELECT`, `ASK`, `CONSTRUCT`, and `DESCRIBE`.
- `prefix_mode` is `strict` or `registered`; default `strict`.
- `replica` follows existing read-replica rules.
- `timeout_ms` may only lower the configured maximum.
- `Accept` selects the output format.
- The response uses the v0.134 streaming pipeline for streaming-safe formats.
- Authentication and rate limiting are identical to `/sparql`.
- The route is listed as a read endpoint in the central authorization registry and OpenAPI document.

Do not add bindings to URL query parameters or headers.

## 5.7 Error-code allocation

Reserve **PT0570–PT0579**:

| Code | Meaning |
|---|---|
| `PT0570` | Bindings must be a JSON object |
| `PT0571` | Invalid or duplicate variable name |
| `PT0572` | Unsupported RDF term type |
| `PT0573` | Invalid literal datatype/language combination |
| `PT0574` | Binding references a variable not in the query |
| `PT0575` | Blank-node binding is unsupported |
| `PT0576` | Invalid prefix label |
| `PT0577` | Invalid prefix expansion IRI |
| `PT0578` | Prefix mutation is not authorized |
| `PT0579` | Parameterized SPARQL Update is unsupported in v1 |

Before implementation, run the repository PT-code collision checker and reassign this range if any code has since been allocated.

## 5.8 Tests

### Binding parser unit tests

- URI.
- Plain literal.
- Typed literal.
- Language literal.
- Unicode.
- Quotes and escape sequences.
- Invalid JSON shape.
- Missing `type` or `value`.
- Both datatype and language.
- Invalid variable names.
- Duplicate normalized variable names.
- Unknown fields.
- JSON null.
- Oversized value.
- Too many bindings.
- Unsupported blank node and RDF-star triple.

### Algebra semantics tests

Test bound variables with:

- Subject, predicate, and object positions.
- `FILTER`.
- `OPTIONAL`.
- `UNION`.
- `MINUS`.
- `VALUES`.
- `BIND`.
- Subqueries.
- Aggregation and `HAVING`.
- Property paths.
- Named graphs.
- `SERVICE`, subject to federation policy.
- Empty results.
- ASK.
- CONSTRUCT.
- DESCRIBE.
- Projection of a bound variable.

### Security tests

- Literal containing quote, brace, semicolon, comment marker, and SPARQL keywords.
- IRI containing characters requiring validation.
- Attempted query termination inside a binding.
- Attempted SQL text inside a binding.
- Prefix containing whitespace or syntax characters.
- Expansion containing `>` or control characters.
- Unauthorized registry update.
- Direct internal-table DML denied.

### Plan-cache tests

- Same query and binding shape, different values: cache reuse.
- Different bound-variable set: distinct plan.
- Strict versus registered prefix mode: distinct plan.
- Prefix registry update: registered plan invalidated.
- Rolled-back prefix update: generation and cache behavior unchanged.

### HTTP tests

- Binding request streamed as JSON, CSV, and TSV.
- Construct request streamed as N-Triples.
- Strict mode rejects undeclared prefix.
- Registered mode accepts the existing registered prefix.
- Local declaration overrides the registry.
- Timeout and disconnect cancellation from v0.134 still work.
- Read-replica routing works.
- Invalid binding returns structured PT error before stream headers.

### Fuzzing

Add:

```text
fuzz/fuzz_targets/sparql_bindings_json.rs
fuzz/fuzz_targets/sparql_prefix_prologue.rs
```

Properties:

- Never panic.
- Never produce malformed SQL.
- Never interpret binding contents as query syntax.
- Prefix expansion changes only the prologue.
- Locally declared prefixes are never overwritten.

## 5.9 Migration

Create:

```text
sql/pg_ripple--0.134.0--0.135.0.sql
```

The migration must:

1. Preserve existing prefixes.
2. Add ownership/timestamp columns.
3. Create registry state and seed generation.
4. Revoke direct DML privileges.
5. Create or replace prefix functions.
6. Add binding overload SQL definitions where generated SQL does not cover them.
7. Upgrade from an installation containing existing prefix rows.
8. Pass fresh-install versus upgrade schema fingerprint comparison.

No stored-query catalog is created in v0.135.0.

## 5.10 Stable API freeze integration

Add to `api/stable-v1.json`:

- New SQL overloads.
- `drop_prefix()`.
- `prefix_registry_generation()`.
- `pg_ripple.sparql_prefix_mode`.
- Binding-limit GUCs.
- `POST /sparql/bindings`.
- Binding request and error schemas.
- PT0570–PT0579.
- The strict-default prefix policy.
- The explicit statement that parameterized updates, blank-node bindings, RDF-star bindings, and multi-row binding sets are not part of v1.

The internal `_pg_ripple.sparql_stream_*` functions are excluded.

## 5.11 Files expected to change

```text
src/sparql_api.rs
src/sparql/mod.rs
src/sparql/parse.rs
src/sparql/plan.rs
src/sparql/sqlgen.rs
src/sparql/bindings.rs
src/sparql/prologue.rs
src/sparql/plan_cache.rs
src/dict_api.rs
src/schema/tables.rs
src/gucs/sparql.rs
src/gucs/registration/*
pg_ripple_http/src/routing/mod.rs
pg_ripple_http/src/routing/sparql_handlers.rs
pg_ripple_http/src/streaming/*
pg_ripple_http/src/metrics.rs
pg_ripple_http/openapi.yaml or generated OpenAPI source
sql/pg_ripple--0.134.0--0.135.0.sql
tests/pg_regress/sql/v0135_sparql_bindings.sql
tests/pg_regress/expected/v0135_sparql_bindings.out
tests/pg_regress/sql/v0135_prefix_mode.sql
tests/pg_regress/expected/v0135_prefix_mode.out
pg_ripple_http/tests/sparql_bindings.rs
fuzz/fuzz_targets/sparql_bindings_json.rs
fuzz/fuzz_targets/sparql_prefix_prologue.rs
docs/src/user-guide/sql-reference/sparql-query.md
docs/src/user-guide/sql-reference/prefix.md
docs/src/reference/http-api.md
docs/src/reference/error-codes.md
docs/src/reference/api-stability.md
roadmap/v0.135.0.md
roadmap/v0.135.0-full.md
api/stable-v1.json
```

## 5.12 Required CI jobs

```text
regress-v0135-bindings
regress-v0135-prefix-mode
http-bindings
bindings-plan-cache
prefix-cache-invalidation
bindings-security-negative
bindings-fuzz-smoke
prefix-fuzz-smoke
api-stability-manifest
migration-0134-to-0135
fresh-vs-upgrade-0135
```

## 5.13 Exit criteria

v0.135.0 may ship only when:

- No binding value is inserted into query or SQL text.
- Binding semantics pass the positive and negative matrix.
- Query plans are reusable across values.
- Strict mode remains the default.
- Registered prefix mode is deterministic, permission-controlled, and cache-safe.
- Local query declarations override registered prefixes.
- Prefix changes are transactional.
- The HTTP binding route uses v0.134 streaming and cancellation.
- Parameterized updates and unsupported RDF term types fail explicitly.
- All new public APIs are frozen in `api/stable-v1.json`.
- No additional public query API is accepted after RC0 except release-blocker remediation.

---

# 6. v1.1.0 — Prepared query registry and client conveniences

## 6.1 Decision

Stored/prepared queries are deliberately excluded from pre-1.0. They introduce ownership, grants, replacement, invalidation, migration, and lifecycle semantics that should build on the proven v1 parameterized-query contract.

The existing broad v1.1.0 proposal should be split. Its first core release should be small:

> **v1.1.0 — Prepared SPARQL query registry and client conveniences**

Jupyter, LangChain, LlamaIndex, dbt, Kafka, and full Cypher/GQL should be separate packages or later roadmap items.

## 6.2 Proposed API

```sql
pg_ripple.prepare_sparql(
    name text,
    query text,
    parameter_schema jsonb,
    prefix_mode text default 'strict',
    replace boolean default false
) returns jsonb
```

```sql
pg_ripple.execute_sparql(
    name text,
    bindings jsonb
) returns setof jsonb
```

```sql
pg_ripple.drop_prepared_sparql(name text) returns boolean
```

```sql
pg_ripple.list_prepared_sparql() returns table(...)
```

Initial scope:

- Read query forms only.
- Owner plus explicit execute grants.
- Typed parameter schema validated at prepare time.
- Query parsed and validated at prepare time.
- Prefix policy pinned when prepared.
- Plan recompiled on schema, prefix, or extension generation change.
- No automatic route generation.
- No scheduling or workflow language.
- No stored credentials.

## 6.3 Catalog sketch

```sql
CREATE TABLE _pg_ripple.prepared_sparql (
    id bigint generated always as identity primary key,
    name text not null,
    owner_oid oid not null,
    query_text text not null,
    query_form text not null,
    parameter_schema jsonb not null,
    prefix_mode text not null,
    prefix_generation bigint,
    schema_generation bigint not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique(owner_oid, name)
);
```

Execution must use v0.135 bindings and v0.134 streaming; it must not implement another execution engine.

---

# 7. Cross-version work breakdown

## 7.1 Suggested issue sequence

### v0.134.0

| Issue | Scope |
|---|---|
| `STREAM-01` | Define typed result-row and query-metadata contract |
| `STREAM-02` | Add internal streaming SQL functions |
| `STREAM-03` | Replace `client.query()` with direct `RowStream` execution |
| `STREAM-04` | Implement JSON/CSV/TSV/N-Triples encoders |
| `STREAM-05` | Implement cancellation guard and deadline handling |
| `STREAM-06` | Unify `/sparql` and `/sparql/stream` execution |
| `STREAM-07` | Add metrics and tracing |
| `STREAM-08` | Add memory, slow-client, disconnect, timeout, and format tests |
| `STREAM-09` | Add benchmark and release-evidence integration |
| `STREAM-10` | Update docs and remove misleading streaming claims |

### v0.135.0

| Issue | Scope |
|---|---|
| `BIND-01` | Freeze binding JSON schema and PT error range |
| `BIND-02` | Implement binding parser and RDF term validation |
| `BIND-03` | Add initial solution mapping to SPARQL algebra |
| `BIND-04` | Parameterize generated SQL and update plan cache keys |
| `BIND-05` | Add SQL function overloads |
| `BIND-06` | Add `/sparql/bindings` HTTP route |
| `PREFIX-01` | Harden existing prefix catalog and privileges |
| `PREFIX-02` | Add transactional prefix generation |
| `PREFIX-03` | Implement strict/registered prologue processing |
| `PREFIX-04` | Add prefix-aware cache invalidation |
| `BIND-07` | Add regression, security, HTTP, and fuzz tests |
| `API-01` | Add the completed surface to `api/stable-v1.json` |
| `DOC-01` | Update SQL, HTTP, prefix, limits, security, and error docs |

## 7.2 Parallelization

A three-engineer allocation:

| Engineer | v0.134 focus | v0.135 focus |
|---|---|---|
| A | Extension typed row/cursor path | Binding algebra and SQL parameters |
| B | HTTP RowStream, encoders, cancellation | HTTP binding route and prefix integration |
| C | Tests, benchmarks, metrics, docs | Migration, cache tests, fuzzing, API manifest |

Cross-review requirements:

- The engineer implementing cancellation may not be the sole reviewer of pool cleanup.
- The engineer implementing binding validation may not be the sole reviewer of injection tests.
- Prefix privilege changes require security review.
- Stable-manifest additions require maintainer approval.

---

# 8. Release-gate matrix

| Capability | v0.134 | v0.135 | v0.136 audit | v0.137 soak |
|---|---:|---:|---:|---:|
| Streaming-safe formats do not buffer full results | Required | Regression | Reviewed | Exercised |
| Slow-client backpressure | Required | Regression | Reviewed | Exercised |
| Client disconnect cancels PostgreSQL | Required | Regression | Reviewed | Exercised |
| Deadline cancellation | Required | Regression | Reviewed | Exercised |
| Pool/session cleanup | Required | Regression | Reviewed | Exercised |
| Typed parameter parsing | — | Required | Reviewed | Exercised |
| No query/SQL interpolation | — | Required | Reviewed | Exercised |
| Plan reuse across values | — | Required | Reviewed | Measured |
| Strict prefix mode default | — | Required | Reviewed | Exercised |
| Prefix permissions and transactional generation | — | Required | Reviewed | Exercised |
| Prefix cache invalidation | — | Required | Reviewed | Exercised |
| Stable API manifest | — | Required | Compared | Frozen |

---

# 9. Adopted ROADMAP entries

## 9.1 v0.134.0 overview row

```markdown
| [v0.134.0](../roadmap/v0.134.0.md) | Performance, scale, and true streaming qualification | Planned | Performance release | Large results use bounded memory and backpressure; disconnects and deadlines cancel PostgreSQL work; current raw evidence passes |
```

## 9.2 v0.135.0 overview row

```markdown
| [v0.135.0](../roadmap/v0.135.0.md) | Safe application query API and compatibility freeze | Planned | RC0 | Typed bindings cannot alter syntax; registered prefixes are opt-in, governed, transactional, and cache-safe; the v1 manifest is frozen |
```

## 9.3 v1.1.0 replacement overview row

```markdown
| [v1.1.0](../roadmap/v1.1.0.md) | **Prepared SPARQL query registry and client conveniences** — build on the v1 typed-binding contract with `prepare_sparql()`, `execute_sparql()`, `drop_prepared_sparql()`, and `list_prepared_sparql()` for read query forms; validate typed parameter schemas and query forms at prepare time; enforce owner and explicit execute privileges; pin strict/registered prefix policy; invalidate compiled plans on schema, prefix, or extension generation changes; execute through the v1 streaming pipeline; exclude automatic HTTP route generation, scheduling, stored credentials, and SPARQL Update; move Jupyter, LangChain/LlamaIndex, dbt, Kafka, Cypher/GQL, and other ecosystem work to separate packages or later roadmap versions | Planned | Large | [Full details](../roadmap/v1.1.0-full.md) |
```

---

# 10. Definition of done

The feature program is complete only when all of the following are true:

## v0.134.0

- [ ] Standard streaming formats are syntactically valid.
- [ ] Streaming paths never call a buffering PostgreSQL query API.
- [ ] Peak HTTP memory is bounded as row count increases.
- [ ] Slow clients do not cause unbounded buffering.
- [ ] Disconnect and timeout cancel PostgreSQL work.
- [ ] Connections return to the pool with no open transaction or leaked setting.
- [ ] Empty results include correct metadata.
- [ ] Metrics report real rows, bytes, pages, duration, and cancellation.
- [ ] Streaming benchmarks and raw evidence are attached to the release.

## v0.135.0

- [ ] Typed URI and literal bindings work across the supported query forms.
- [ ] Binding contents cannot alter SPARQL or SQL syntax.
- [ ] Plan cache reuse is demonstrated across values.
- [ ] Unsupported binding forms fail with stable PT errors.
- [ ] Strict prefix behavior remains the default.
- [ ] Registered prefixes are opt-in, deterministic, validated, and permission-controlled.
- [ ] Query-local declarations override registry values.
- [ ] Prefix changes invalidate registered-mode plans transactionally.
- [ ] HTTP bindings use the streaming/cancellation path.
- [ ] Fresh installs and upgrades are equivalent.
- [ ] Every public addition appears in `api/stable-v1.json`.

## v0.136.0 through v0.143.0

- [ ] External audit includes streaming cancellation, binding parsing, generated SQL parameters, prefix privilege boundaries, and cache invalidation.
- [ ] The v0.143.0 candidate workload includes large streams, slow clients, deliberate disconnects, varying bound values, prefix changes, and concurrent registry reads.
- [ ] No new Critical or High finding remains.
- [ ] v1.0.0 promotes the exact qualified artifacts.

---

# 11. Final scope statement

The pre-1.0 additions are deliberately limited to:

1. **True result streaming and cancellation in v0.134.0.**
2. **Typed SPARQL bindings and SPARQL integration with the existing prefix registry in v0.135.0.**

Stored/prepared queries move to **v1.1.0**.

This yields a defensible v1 application contract without reopening feature sprawl:

> pg-ripple can safely execute application-supplied values, can stream large standards-valid results without buffering them, can stop work when clients disappear, and can optionally use a governed database prefix registry while preserving strict SPARQL behavior by default.
