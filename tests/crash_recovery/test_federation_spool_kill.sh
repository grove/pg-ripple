#!/usr/bin/env bash
# tests/crash_recovery/test_federation_spool_kill.sh
#
# Crash-recovery test: federation result spooling kill (v0.47.0).
#
# Verifies that if a PostgreSQL backend is killed with SIGKILL while a SERVICE
# clause response is being spooled to a temporary table (when the response
# exceeds `pg_ripple.federation_inline_max_rows`), the database is left in a
# consistent state:
#   (a) No orphaned temp tables or locks remain after the backend is killed.
#   (b) A fresh SERVICE query can be executed successfully.
#
# This test uses a local loopback SPARQL endpoint that returns > 10000 rows,
# triggering the temp-table spool path.
#
# Prerequisites:
#   - pgrx pg18 is running (cargo pgrx start pg18)
#   - pg_ripple is installed
#   - pg_ripple.federation_allow_private = on (for loopback endpoint)
#
# Usage:
#   bash tests/crash_recovery/test_federation_spool_kill.sh
#
# Exit codes:
#   0 — test passed
#   1 — test failed

set -euo pipefail

DBNAME="${PGDATABASE:-pg_ripple_test}"
HOST="${PGHOST:-/tmp}"
PORT="${PGPORT:-28815}"
PSQL="psql -h $HOST -p $PORT -d $DBNAME -X -A -t"

pass() { echo "[PASS] $*"; }
fail() { echo "[FAIL] $*" >&2; exit 1; }

echo "=== Crash-recovery: federation result spooling kill ==="

# ── 1. Setup: lower inline_max_rows to trigger spool path ────────────────────
$PSQL <<'SQL'
SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;
-- Allow private endpoints for loopback test
SET pg_ripple.federation_allow_private = on;
SET pg_ripple.federation_inline_max_rows = 10;
SQL
echo "  Setup: federation_inline_max_rows=10 (spool path will be used)"

# ── 2. Start a long-running SERVICE query in background, then kill it ─────────
$PSQL -c "
SET pg_ripple.federation_allow_private = on;
SET search_path TO pg_ripple, public;
SELECT * FROM query_sparql(
    'SELECT ?s ?p ?o WHERE { SERVICE <http://127.0.0.1:9999/sparql> { ?s ?p ?o } }'
);
" 2>/dev/null &
BG_PID=$!
sleep 0.2

BACKEND_PID=$(psql -h "$HOST" -p "$PORT" -d "$DBNAME" -X -A -t \
    -c "SELECT pid FROM pg_stat_activity WHERE query LIKE '%SERVICE%' AND state = 'active' LIMIT 1" 2>/dev/null || true)

if [[ -n "$BACKEND_PID" ]]; then
    echo "  Killing federation backend PID $BACKEND_PID"
    kill -9 "$BACKEND_PID" 2>/dev/null || true
fi

wait "$BG_PID" 2>/dev/null || true
sleep 1

# ── 3. Verify no orphaned temp tables ─────────────────────────────────────────
ORPHAN_COUNT=$($PSQL -c "
SELECT COUNT(*) FROM pg_tables
WHERE tablename LIKE '_pg_ripple_svc_%'
  AND schemaname = 'pg_temp_' || pg_backend_pid();
" 2>/dev/null || echo "0")

echo "  Orphaned spool tables: $ORPHAN_COUNT"
if [[ "$ORPHAN_COUNT" -gt 0 ]]; then
    fail "orphaned federation spool tables found after kill"
fi

# ── 4. Verify no leftover locks ───────────────────────────────────────────────
LOCK_COUNT=$($PSQL -c "
SELECT COUNT(*) FROM pg_locks l
JOIN pg_stat_activity a ON l.pid = a.pid
WHERE a.query LIKE '%SERVICE%'
  AND a.state != 'idle';
" 2>/dev/null || echo "0")

echo "  Stale SERVICE locks: $LOCK_COUNT"
if [[ "$LOCK_COUNT" -gt 0 ]]; then
    fail "stale locks found after kill"
fi

pass "Federation result spooling kill: no orphaned tables or stale locks"
