# Arrow Flight Reference

pg_ripple exposes an [Apache Arrow Flight](https://arrow.apache.org/docs/format/Flight.html)
bulk-export endpoint via the `pg_ripple_http` companion service.

## Endpoint

```
GET /flight/do_get
```

Streams Arrow IPC record batches from VP tables (or a SPARQL SELECT query result) directly to
the client using the Apache Arrow Flight protocol.

## Authentication

Tickets are HMAC-SHA256 signed with an expiry timestamp and a random nonce to prevent replay
attacks. The secret key is configured via `pg_ripple.arrow_flight_secret`. Unsigned tickets are
rejected unless `pg_ripple.arrow_unsigned_tickets_allowed = on` (disabled by default).

## Ticket Format

A valid Arrow Flight ticket is a JSON object:

```json
{
  "query": "SELECT * FROM pg_ripple.sparql_select($1)",
  "exp": 1735689600,
  "nonce": "a1b2c3d4e5f6",
  "sig": "HMAC-SHA256 hex signature"
}
```

The `sig` field is computed over `query + exp + nonce` using the configured secret.

## Streaming Behavior

The endpoint uses `Transfer-Encoding: chunked` HTTP streaming (via `axum::body::Body::from_stream`)
so that clients can begin decoding Arrow IPC record batches before the full export completes.
Response bytes are sent in 64 KiB chunks as the IPC buffer is produced.

**Memory bound**: The Arrow IPC buffer for the entire export is built in memory before streaming
begins. For very large result sets the RSS of `pg_ripple_http` scales with result-set size
(approximately 32 bytes per row in the IPC buffer plus ~200 bytes per row in PostgreSQL client
memory). The recommended upper bound for a single export call is **10 million rows** (RSS ≲ 512 MB
on a host with 1 GB available to the HTTP companion). For larger exports, partition by named graph
or predicate and call the endpoint in batches.

Clients should use streaming reads (e.g., chunked IPC reader) rather than buffering the full
response body.

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `pg_ripple.arrow_flight_secret` | — | HMAC secret for ticket signing (required) |
| `pg_ripple.arrow_unsigned_tickets_allowed` | `off` | Allow unsigned tickets (development only) |
| `ARROW_BATCH_SIZE` env var | `1000` | Rows per Arrow IPC record batch |

## Response Headers

| Header | Description |
|--------|-------------|
| `Content-Type` | `application/vnd.apache.arrow.stream` |
| `X-Arrow-Rows` | Total number of triples exported |
| `X-Arrow-Batches` | Number of Arrow IPC record batches sent |
| `Transfer-Encoding` | `chunked` — response is streamed, not buffered |

## Status

Arrow Flight bulk export is **experimental** in v0.71.0. The HMAC-SHA256 signing,
expiry and nonce checking are fully implemented (v0.67.0 FLIGHT-SEC-02). Chunked HTTP
streaming via `Body::from_stream` is confirmed and validated (v0.71.0 FLIGHT-STREAM-01).

See also: [HTTP API](http-api.md), [Architecture](architecture.md), [Compatibility Matrix](../operations/compatibility.md).
