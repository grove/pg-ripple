#!/usr/bin/env bash
# CI-safe v0.134.0 streaming qualification validation.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROADMAP="$ROOT/roadmap/v0.134.0.md"
STREAM="$ROOT/pg_ripple_http/src/stream.rs"
ENCODER="$ROOT/pg_ripple_http/src/streaming"

test -f "$ROOT/sql/pg_ripple--0.133.0--0.134.0.sql"
grep -q '^-- Migration 0\.133\.0 → 0\.134\.0:' "$ROOT/sql/pg_ripple--0.133.0--0.134.0.sql"
test -f "$STREAM"
test -f "$ENCODER/encoder.rs"
test -f "$ENCODER/terms.rs"
test -f "$ENCODER/coalesce.rs"
test -f "$ROOT/pg_ripple_http/tests/streaming.rs"
test -x "$ROOT/tests/http_integration/stream_format.sh"
test -f "$ROOT/tests/pg_regress/sql/v0134_streaming.sql"
grep -q 'query_raw' "$STREAM"
grep -q 'RowStream' "$STREAM"
grep -q 'Body::from_stream' "$STREAM"
! grep -q 'tokio::sync::mpsc' "$STREAM"
! grep -q 'Transfer-Encoding' "$STREAM"
grep -q 'sparql_stream_metadata' "$ROOT/src/sparql_api.rs"
grep -q 'sparql_stream_bindings' "$ROOT/src/sparql_api.rs"
grep -q 'sparql_stream_triples' "$ROOT/src/sparql_api.rs"
grep -REq 'X-Pg-Ripple-Streaming|x-pg-ripple-streaming' "$ROOT/pg_ripple_http/src"
grep -q 'PG_RIPPLE_HTTP_STREAM_IDLE_TIMEOUT_MS' "$ROOT/docs/src/reference/http-api.md"
grep -q 'PG_RIPPLE_HTTP_STREAM_CHUNK_BYTES' "$ROOT/docs/src/reference/http-api.md"
grep -q 'PG_RIPPLE_HTTP_STREAM_MAX_ROW_BYTES' "$ROOT/docs/src/reference/http-api.md"
grep -q '^\*\*Status:\*\* Released$' "$ROADMAP"
grep -q 'http-stream-format' "$ROADMAP"
grep -q 'http-stream-first-byte' "$ROADMAP"
grep -q 'http-stream-memory' "$ROADMAP"
grep -q 'http-stream-slow-client' "$ROADMAP"
grep -q 'http-stream-disconnect-cancel' "$ROADMAP"
grep -q 'http-stream-timeout' "$ROADMAP"
grep -q 'http-stream-pool-reuse' "$ROADMAP"
grep -q 'benchmark-streaming' "$ROADMAP"
grep -q '0\.134\.0' "$ROOT/README.md"
grep -q '0\.134\.x' "$ROOT/docs/src/operations/compatibility.md"

echo 'PASS: v0.134.0 streaming implementation and qualification contract are present'
