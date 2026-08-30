#!/usr/bin/env bash
# pg_ripple_http/tests/process_smoke.sh — v0.128.1 emergency containment.
#
# Starts the built pg_ripple_http binary against a real PostgreSQL instance
# with pg_ripple installed, polls /health and /ready, then sends SIGTERM and
# verifies a clean exit. This is the "does it actually start and serve
# traffic" gate that a compile-only check cannot catch — Axum panics at
# router-construction time happen only when the binary actually runs.
#
# Requires: a reachable PostgreSQL instance with `CREATE EXTENSION pg_ripple;`
# already applied (point PG_RIPPLE_HTTP_PG_URL at it — e.g. a running release
# image, or a `cargo pgrx start` instance in CI).
#
# Usage:
#   PG_RIPPLE_HTTP_PG_URL="postgresql://postgres@localhost:5432/postgres" \
#     bash pg_ripple_http/tests/process_smoke.sh [path-to-binary]
#
# Exit codes:
#   0 — health/ready responded and the process shut down cleanly on SIGTERM
#   1 — binary failed to start, ports never came up, or shutdown was unclean

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${1:-${REPO_ROOT}/target/debug/pg_ripple_http}"
PORT="${PG_RIPPLE_HTTP_PORT:-18787}"
PG_URL="${PG_RIPPLE_HTTP_PG_URL:-postgresql://postgres@localhost:5432/postgres}"

if [ ! -x "${BIN}" ]; then
  echo "ERROR: pg_ripple_http binary not found or not executable at ${BIN}" >&2
  echo "Build it first: cargo build -p pg_ripple_http" >&2
  exit 1
fi

echo "==> Starting pg_ripple_http (port ${PORT}) against ${PG_URL}"
PG_RIPPLE_HTTP_PORT="${PORT}" PG_RIPPLE_HTTP_PG_URL="${PG_URL}" "${BIN}" &
PID=$!

cleanup() {
  if kill -0 "${PID}" 2>/dev/null; then
    kill -9 "${PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "==> Waiting for /health to respond..."
UP=0
for _ in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    UP=1
    break
  fi
  if ! kill -0 "${PID}" 2>/dev/null; then
    echo "ERROR: pg_ripple_http exited before it started serving requests" >&2
    wait "${PID}" || true
    exit 1
  fi
  sleep 0.5
done

if [ "${UP}" -ne 1 ]; then
  echo "ERROR: /health never responded within 15s" >&2
  exit 1
fi
echo "OK: /health responded"

echo "==> Checking /ready..."
READY_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/ready")
if [ "${READY_STATUS}" != "200" ] && [ "${READY_STATUS}" != "503" ]; then
  echo "ERROR: /ready returned unexpected status ${READY_STATUS}" >&2
  exit 1
fi
echo "OK: /ready responded (${READY_STATUS})"

if [ "${PG_RIPPLE_HTTP_STREAM_FORMAT_SMOKE:-0}" = "1" ]; then
  PG_RIPPLE_HTTP_URL="http://127.0.0.1:${PORT}" \
    bash "${REPO_ROOT}/tests/http_integration/stream_format.sh"
fi

if [ "${PG_RIPPLE_HTTP_BINDINGS_SMOKE:-0}" = "1" ]; then
  PG_RIPPLE_HTTP_URL="http://127.0.0.1:${PORT}" \
    bash "${REPO_ROOT}/tests/http_integration/bindings.sh"
fi

echo "==> Sending SIGTERM and waiting for clean exit..."
kill -TERM "${PID}"
trap - EXIT

if wait "${PID}"; then
  echo "OK: process exited cleanly after SIGTERM"
else
  STATUS=$?
  echo "ERROR: process exited with status ${STATUS} after SIGTERM" >&2
  exit 1
fi

echo "==> pg_ripple_http process smoke test PASSED."
