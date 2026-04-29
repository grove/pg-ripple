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

The endpoint streams Arrow IPC record batches as they are produced. Memory usage is bounded by
the batch size (configurable via `pg_ripple_http.arrow_batch_size`). For very large result sets,
the client should use streaming reads rather than buffering the entire response.

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `pg_ripple.arrow_flight_secret` | — | HMAC secret for ticket signing (required) |
| `pg_ripple.arrow_unsigned_tickets_allowed` | `off` | Allow unsigned tickets (development only) |
| `pg_ripple_http.arrow_batch_size` | `1000` | Rows per Arrow IPC batch |

## Status

Arrow Flight bulk export is **experimental** in v0.70.0. The HMAC-SHA256 signing,
expiry and nonce checking are fully implemented (v0.67.0 FLIGHT-SEC-02).
Streaming validation and multi-chunk behavior are targeted for v0.71.0 (FLIGHT-STREAM-01).

See also: [HTTP API](http-api.md), [Architecture](architecture.md).
