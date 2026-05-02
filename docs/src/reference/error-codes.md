# Error Code Registry

> **A13-03 (v0.86.0)**: this page is the authoritative registry for all `PT` error codes used in pg_ripple. Every production error path **must** reference a code from this list. CI enforces `PT` codes on production error paths.

pg_ripple uses structured error codes in the range **PT001–PT799** (extension) and **PT400–PT503** (HTTP companion). Error messages follow PostgreSQL conventions: lowercase first word, no trailing period.

See also: [Error Message Catalog](error-catalog.md) for the full list of error messages by subsystem.

---

## Code Ranges

| Range | Subsystem |
|---|---|
| PT001–PT099 | Dictionary encoding |
| PT100–PT199 | VP storage |
| PT200–PT299 | SPARQL query engine |
| PT300–PT399 | Datalog inference |
| PT400–PT499 | Input validation / HTTP |
| PT500–PT599 | Internal execution |
| PT600–PT699 | SHACL validation |
| PT700–PT799 | External services (LLM, federation) |

---

## HTTP Companion PT Codes

| Code | HTTP Status | Meaning | Source |
|---|---|---|---|
| PT400 | 400 | Missing or malformed query parameter | `routing/sparql_handlers.rs` |
| PT400_SPARQL_PARSE | 400 | SPARQL syntax error — parse failed | `routing/sparql_handlers.rs`, `spi_bridge.rs` |
| PT401 | 401 | Unauthorized — missing or invalid Bearer token | `common.rs` |
| PT403 | 403 | Forbidden — path outside allowed directory | `bulk_load.rs` |
| PT404 | 413 | Request body exceeds maximum allowed size | `routing/sparql_handlers.rs` |
| PT413 | 413 | Arrow Flight export result is too large | `arrow_encode.rs` |
| PT503 | 503 | Database connection unavailable | `common.rs`, `stream.rs` |

---

## Extension PT Codes (selected)

| Code | Message | Subsystem |
|---|---|---|
| PT001 | dictionary encode failed: hash collision detected | Dictionary |
| PT002 | dictionary decode failed: id not found | Dictionary |
| PT003 | invalid term kind: expected 0/1/2 | Dictionary |
| PT008 | malformed IRI: `<detail>` | Dictionary |
| PT400 | SPARQL parse error: `<detail>` | SPARQL |
| PT403 | file path outside allowed directory | Bulk load |
| PT501 | deprecated GUC: use `<replacement>` | Storage GUC |
| PT512 | strict_dictionary: unknown dictionary id | Dictionary |
| PT600 | SHACL constraint violation: `<detail>` | SHACL |
| PT700 | LLM endpoint unreachable or returned HTTP error | LLM |

---

## CI Enforcement

The CI job `check-pt-codes` (added in v0.86.0) scans all `pgrx::error!` and `tracing::error!` call sites in production code to verify that each one references a PT code in either:

- The error message body (e.g., `"... (PT400)"`), or  
- The error code argument (e.g., `json_error("PT400", ..., StatusCode::BAD_REQUEST)`).

Internal-only errors that use `"internal: <description> — please report"` format are exempted.

To add a new error code, update this file **first**, then reference the code in the implementation.
