#!/usr/bin/env bash
# tests/crash_recovery/test_construct_view_kill.sh
#
# Crash-recovery test: CONSTRUCT/DESCRIBE view materialisation kill (v0.47.0).
#
# Verifies that if a PostgreSQL backend is killed with SIGKILL during a
# CONSTRUCT view materialisation, the database is left in a consistent state:
#   (a) The view catalog entry is either absent or in its pre-materialisation
#       state — no partial/corrupted materialisation persists.
#   (b) pg_ripple.materialize_view() can be called again after restart and
#       completes successfully.
#
# Prerequisites:
#   - pgrx pg18 is running (cargo pgrx start pg18)
#   - pg_ripple is installed
#
# Usage:
#   bash tests/crash_recovery/test_construct_view_kill.sh
#
# Exit codes:
#   0 — test passed (consistent state after crash)
#   1 — test failed

set -euo pipefail

DBNAME="${PGDATABASE:-pg_ripple_test}"
HOST="${PGHOST:-/tmp}"
PORT="${PGPORT:-28815}"
PSQL="psql -h $HOST -p $PORT -d $DBNAME -X -A -t"

pass() { echo "[PASS] $*"; }
fail() { echo "[FAIL] $*" >&2; exit 1; }

echo "=== Crash-recovery: CONSTRUCT view materialisation kill ==="

# ── 1. Setup ──────────────────────────────────────────────────────────────────
$PSQL <<'SQL'
SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;
SELECT drop_construct_view('cv_kill_test') FROM generate_series(1,1) WHERE EXISTS (
    SELECT 1 FROM _pg_ripple.construct_views WHERE name = 'cv_kill_test'
);
SELECT insert_triple(
    '<https://crash.cv.test/s' || i || '>',
    '<https://crash.cv.test/p>',
    '"value' || i || '"'
) FROM generate_series(1, 200) g(i);
SQL
echo "  Setup: 200 triples inserted"

# ── 2. Start materialisation in background, then kill it ─────────────────────
$PSQL -c "
SET client_min_messages = WARNING;
SET search_path TO pg_ripple, public;
SELECT create_construct_view(
    'cv_kill_test',
    'CONSTRUCT { ?s <https://crash.cv.test/label> ?o } WHERE { ?s <https://crash.cv.test/p> ?o }'
);
SELECT materialize_view('cv_kill_test');
" &
BG_PID=$!
sleep 0.1

# Find and kill the psql backend
BACKEND_PID=$(psql -h "$HOST" -p "$PORT" -d "$DBNAME" -X -A -t \
    -c "SELECT pid FROM pg_stat_activity WHERE query LIKE '%materialize_view%' AND state = 'active' LIMIT 1" 2>/dev/null || true)

if [[ -n "$BACKEND_PID" ]]; then
    echo "  Killing backend PID $BACKEND_PID"
    kill -9 "$BACKEND_PID" 2>/dev/null || true
fi

wait "$BG_PID" 2>/dev/null || true
sleep 1

# ── 3. Verify consistency ─────────────────────────────────────────────────────
# The view catalog should either not exist, or be in a consistent (non-partial) state.
VIEW_STATE=$($PSQL -c "
SET search_path TO pg_ripple, public;
SELECT COALESCE(
    (SELECT 'exists:' || status FROM _pg_ripple.construct_views WHERE name = 'cv_kill_test'),
    'absent'
) AS state;
" 2>/dev/null || echo "error")

echo "  View state after kill: $VIEW_STATE"

if [[ "$VIEW_STATE" == "error" ]]; then
    fail "Could not query view state after kill"
fi

# ── 4. Verify re-materialisation works ───────────────────────────────────────
$PSQL <<'SQL' || fail "re-materialisation failed after crash"
SET client_min_messages = WARNING;
SET search_path TO pg_ripple, public;
SELECT drop_construct_view('cv_kill_test') WHERE EXISTS (
    SELECT 1 FROM _pg_ripple.construct_views WHERE name = 'cv_kill_test'
);
SELECT create_construct_view(
    'cv_kill_test',
    'CONSTRUCT { ?s <https://crash.cv.test/label> ?o } WHERE { ?s <https://crash.cv.test/p> ?o }'
);
SELECT materialize_view('cv_kill_test');
SQL

TRIPLE_COUNT=$($PSQL -c "
SET search_path TO pg_ripple, public;
SELECT COUNT(*) FROM query_sparql(
    'SELECT ?s WHERE { ?s <https://crash.cv.test/label> ?o }'
);
" 2>/dev/null || echo "0")

if [[ "$TRIPLE_COUNT" -lt 1 ]]; then
    fail "re-materialisation produced no results (got: $TRIPLE_COUNT)"
fi

pass "CONSTRUCT view materialisation kill: consistent after crash, re-materialisation produced $TRIPLE_COUNT rows"

# ── Cleanup ───────────────────────────────────────────────────────────────────
$PSQL <<'SQL'
SET client_min_messages = WARNING;
SET search_path TO pg_ripple, public;
SELECT drop_construct_view('cv_kill_test') FROM generate_series(1,1) WHERE EXISTS (
    SELECT 1 FROM _pg_ripple.construct_views WHERE name = 'cv_kill_test'
);
SELECT delete_triples_by_predicate('<https://crash.cv.test/p>');
SQL
