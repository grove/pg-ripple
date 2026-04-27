#!/usr/bin/env bash
# tests/concurrent/merge_cutover_chaos.sh
#
# v0.60.0 J7-1: HTAP merge-cutover chaos test.
#
# Hammers the VP view with continuous SELECT queries while the merge worker
# churns.  Asserts zero "relation does not exist" errors over a 60-second run.
#
# Prerequisites:
#   - cargo pgrx start pg18 must be running
#   - The pg_ripple extension must be installed
#   - Run as the current user (not root)
#
# Usage:
#   cargo pgrx start pg18
#   bash tests/concurrent/merge_cutover_chaos.sh
#
# Environment:
#   PGHOST       — socket directory (default: /tmp)
#   PGPORT       — port (default: 28818 for pgrx test instance)
#   PGUSER       — user (default: current user)
#   CHAOS_SECS   — test window in seconds (default: 60)
#   INSERT_BATCH — triples per merge cycle (default: 500)

set -euo pipefail

PGHOST="${PGHOST:-/tmp}"
PGPORT="${PGPORT:-28818}"
PGUSER="${PGUSER:-$(whoami)}"
CHAOS_SECS="${CHAOS_SECS:-60}"
INSERT_BATCH="${INSERT_BATCH:-500}"
TEST_DB="pg_ripple_merge_chaos_test"

cleanup() {
    psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
        -c "DROP DATABASE IF EXISTS \"$TEST_DB\";" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "=== merge_cutover_chaos.sh — ${CHAOS_SECS}s window ==="

# Create test database.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "DROP DATABASE IF EXISTS \"$TEST_DB\";" >/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "CREATE DATABASE \"$TEST_DB\";" >/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "CREATE EXTENSION pg_ripple;" >/dev/null

# Enable HTAP mode and seed initial data.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" <<'SQL' >/dev/null
SET pg_ripple.htap_enabled = on;
SELECT pg_ripple.load_turtle($$
  @prefix ex: <https://chaos.test/> .
  ex:s1 ex:pred ex:o1 .
  ex:s2 ex:pred ex:o2 .
  ex:s3 ex:pred ex:o3 .
$$);
SQL

ERROR_FILE=$(mktemp)
READER_PID=""
WRITER_PID=""

# Background reader: continuous SELECT queries against the VP view.
# Records "relation does not exist" errors to ERROR_FILE.
(
    END_TIME=$(( $(date +%s) + CHAOS_SECS ))
    ITER=0
    while [[ $(date +%s) -lt $END_TIME ]]; do
        RESULT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
            -c "SELECT pg_ripple.sparql('SELECT ?s ?o WHERE { ?s <https://chaos.test/pred> ?o }');" \
            2>&1 || true)
        if echo "$RESULT" | grep -qi "relation.*does not exist"; then
            echo "$RESULT" >> "$ERROR_FILE"
            echo "ERROR at iteration $ITER: relation does not exist"
        fi
        ITER=$(( ITER + 1 ))
    done
) &
READER_PID=$!

# Background writer: repeatedly inserts batches and triggers manual merges.
(
    END_TIME=$(( $(date +%s) + CHAOS_SECS ))
    SEQ=100
    while [[ $(date +%s) -lt $END_TIME ]]; do
        # Build a batch of INSERT triples via SPARQL UPDATE.
        INSERTS=""
        for (( i=0; i<INSERT_BATCH; i++ )); do
            INSERTS="${INSERTS} <https://chaos.test/s${SEQ}> <https://chaos.test/pred> <https://chaos.test/o${SEQ}> ."
            SEQ=$(( SEQ + 1 ))
        done
        psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
            -c "SELECT pg_ripple.sparql_update('INSERT DATA { ${INSERTS} }');" \
            >/dev/null 2>&1 || true
        # Trigger a merge cycle.
        psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
            -c "SELECT pg_ripple.merge_all();" \
            >/dev/null 2>&1 || true
    done
) &
WRITER_PID=$!

echo "Reader PID: $READER_PID  Writer PID: $WRITER_PID"
echo "Running for ${CHAOS_SECS} seconds..."

wait "$READER_PID" || true
wait "$WRITER_PID" || true

# Assert no errors.
if [[ -s "$ERROR_FILE" ]]; then
    echo ""
    echo "FAIL: 'relation does not exist' errors detected during merge-cutover chaos:"
    cat "$ERROR_FILE"
    rm -f "$ERROR_FILE"
    exit 1
fi

rm -f "$ERROR_FILE"
echo ""
echo "PASS: zero 'relation does not exist' errors over ${CHAOS_SECS}s merge-cutover chaos run."
