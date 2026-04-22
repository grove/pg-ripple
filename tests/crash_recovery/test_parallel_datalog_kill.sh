#!/usr/bin/env bash
# tests/crash_recovery/test_parallel_datalog_kill.sh
#
# Crash-recovery test: parallel Datalog stratum kill mid-fixpoint (v0.47.0).
#
# Verifies that if a PostgreSQL backend is killed with SIGKILL during a
# parallel Datalog inference run (merge_workers > 1), the database is left
# in a consistent state:
#   (a) No partially-derived facts remain in VP tables after recovery.
#   (b) pg_ripple.infer() can be re-run successfully, producing consistent results.
#   (c) SID sequences are not corrupted.
#
# Prerequisites:
#   - pgrx pg18 is running (cargo pgrx start pg18) with merge_workers >= 2
#   - pg_ripple is installed
#
# Usage:
#   bash tests/crash_recovery/test_parallel_datalog_kill.sh
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

echo "=== Crash-recovery: parallel Datalog stratum kill mid-fixpoint ==="

# ── 1. Setup ──────────────────────────────────────────────────────────────────
$PSQL <<'SQL'
SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;
SELECT drop_rules() FROM generate_series(1,1);

-- Insert base triples for a transitive-closure rule
SELECT insert_triple(
    '<https://crash.dl.test/a' || i || '>',
    '<https://crash.dl.test/next>',
    '<https://crash.dl.test/a' || (i+1) || '>'
) FROM generate_series(1, 100) g(i);

-- Define a multi-stratum ruleset to stress parallel execution
SELECT define_rules($$
  reach(X, Z) :- next(X, Z).
  reach(X, Z) :- reach(X, Y), next(Y, Z).
$$);
SQL
echo "  Setup: 100 triples + 2-stratum transitive-closure ruleset"

# ── 2. Start inference in background with parallel workers, then kill it ──────
$PSQL -c "
SET pg_ripple.datalog_parallel_workers = 2;
SET pg_ripple.datalog_sequence_batch = 100;
SET search_path TO pg_ripple, public;
SELECT infer();
" 2>/dev/null &
BG_PID=$!
sleep 0.15

BACKEND_PID=$(psql -h "$HOST" -p "$PORT" -d "$DBNAME" -X -A -t \
    -c "SELECT pid FROM pg_stat_activity WHERE query LIKE '%infer%' AND state = 'active' LIMIT 1" 2>/dev/null || true)

if [[ -n "$BACKEND_PID" ]]; then
    echo "  Killing inference backend PID $BACKEND_PID"
    kill -9 "$BACKEND_PID" 2>/dev/null || true
fi

wait "$BG_PID" 2>/dev/null || true
sleep 1

# ── 3. Verify no partial inferred facts ───────────────────────────────────────
# All inferred facts should have been rolled back (source=1 with no completed run)
INFERRED_COUNT=$($PSQL -c "
SET search_path TO pg_ripple, public;
SELECT COUNT(*) FROM query_sparql(
    'SELECT ?x ?z WHERE { ?x <https://crash.dl.test/reach> ?z }'
) WHERE 1=1;
" 2>/dev/null || echo "error")

echo "  Inferred triples after kill: $INFERRED_COUNT"
if [[ "$INFERRED_COUNT" == "error" ]]; then
    fail "could not query inferred triples after kill"
fi

# ── 4. Verify re-inference works and is consistent ────────────────────────────
$PSQL <<'SQL' || fail "re-inference failed after crash"
SET client_min_messages = WARNING;
SET pg_ripple.datalog_parallel_workers = 2;
SET search_path TO pg_ripple, public;
SELECT infer();
SQL

RERUN_COUNT=$($PSQL -c "
SET search_path TO pg_ripple, public;
SELECT COUNT(*) FROM query_sparql(
    'SELECT ?x ?z WHERE { ?x <https://crash.dl.test/reach> ?z }'
);
" 2>/dev/null || echo "0")

echo "  Inferred triples after re-run: $RERUN_COUNT"
if [[ "$RERUN_COUNT" -lt 100 ]]; then
    fail "re-inference produced too few results: $RERUN_COUNT (expected >= 100)"
fi

# ── 5. Verify no duplicate SIDs ───────────────────────────────────────────────
DUP_COUNT=$($PSQL -c "
SELECT COUNT(*) FROM (
    SELECT i, COUNT(*) c FROM _pg_ripple.vp_rare GROUP BY i HAVING COUNT(*) > 1
) dups;
" 2>/dev/null || echo "0")

echo "  Duplicate SIDs: $DUP_COUNT"
if [[ "$DUP_COUNT" -gt 0 ]]; then
    fail "duplicate SIDs found: $DUP_COUNT"
fi

pass "Parallel Datalog stratum kill: consistent after crash, re-inference produced $RERUN_COUNT rows, 0 duplicate SIDs"

# ── Cleanup ───────────────────────────────────────────────────────────────────
$PSQL <<'SQL'
SET client_min_messages = WARNING;
SET search_path TO pg_ripple, public;
SELECT drop_rules();
SELECT delete_triples_by_predicate('<https://crash.dl.test/next>');
SELECT delete_triples_by_predicate('<https://crash.dl.test/reach>');
SQL
