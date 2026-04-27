#!/usr/bin/env bash
# tests/concurrent/promotion_race.sh
#
# v0.60.0 F7-2: Rare-predicate promotion concurrency test.
#
# Drives two parallel psql sessions across the vp_promotion_threshold and
# asserts that exactly one VP table is created (no double-promotion race).
#
# Prerequisites:
#   - cargo pgrx start pg18 must be running
#   - The pg_ripple extension must be installed
#   - Run as the current user (not root)
#
# Usage:
#   cargo pgrx start pg18
#   bash tests/concurrent/promotion_race.sh
#
# Environment:
#   PGHOST   — socket directory (default: /tmp)
#   PGPORT   — port (default: 28818 for pgrx test instance)
#   PGUSER   — user (default: current user)

set -euo pipefail

PGHOST="${PGHOST:-/tmp}"
PGPORT="${PGPORT:-28818}"
PGUSER="${PGUSER:-$(whoami)}"
TEST_DB="pg_ripple_promotion_race_test"

cleanup() {
    psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
        -c "DROP DATABASE IF EXISTS \"$TEST_DB\";" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "=== promotion_race.sh ==="

# Create test database.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "DROP DATABASE IF EXISTS \"$TEST_DB\";" >/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "CREATE DATABASE \"$TEST_DB\";" >/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "CREATE EXTENSION pg_ripple;" >/dev/null

# Set a very low promotion threshold (10 triples) so we can cross it easily.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "ALTER SYSTEM SET pg_ripple.vp_promotion_threshold = 10;" >/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "SELECT pg_reload_conf();" >/dev/null

PREDICATE="https://promo.test/concurrentProp"

# Pre-insert 8 triples (just below threshold of 10) in the rare-predicate table.
for (( i=1; i<=8; i++ )); do
    psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
        -c "SELECT pg_ripple.sparql_update('INSERT DATA { <https://promo.test/s${i}> <${PREDICATE}> <https://promo.test/o${i}> }');" \
        >/dev/null 2>&1
done

# Verify we are still below the threshold (stored in vp_rare).
RARE_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT count(*) FROM _pg_ripple.vp_rare;" 2>/dev/null || echo "0")
echo "Pre-race rare count: $RARE_COUNT"

# Spawn two sessions that each insert 2 more triples concurrently —
# together they push the count from 8 to 12, crossing the threshold.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "SELECT pg_ripple.sparql_update('INSERT DATA { <https://promo.test/s9> <${PREDICATE}> <https://promo.test/o9> . <https://promo.test/s10> <${PREDICATE}> <https://promo.test/o10> }');" \
    >/dev/null 2>&1 &
PID1=$!

psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "SELECT pg_ripple.sparql_update('INSERT DATA { <https://promo.test/s11> <${PREDICATE}> <https://promo.test/o11> . <https://promo.test/s12> <${PREDICATE}> <https://promo.test/o12> }');" \
    >/dev/null 2>&1 &
PID2=$!

wait "$PID1" || true
wait "$PID2" || true

# Give any async promotion a moment to settle.
sleep 1

# Count VP tables for this predicate IRI.
PRED_ID=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT id FROM _pg_ripple.dictionary WHERE iri = '${PREDICATE}';" 2>/dev/null || echo "")

VP_TABLE_COUNT=0
if [[ -n "$PRED_ID" ]]; then
    VP_TABLE_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
        -tAc "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
              WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_${PRED_ID}' AND c.relkind IN ('r','v');" \
        2>/dev/null || echo "0")
fi

echo "VP table/view count for predicate $PREDICATE (id=$PRED_ID): $VP_TABLE_COUNT"

if [[ "$VP_TABLE_COUNT" -ne 1 ]]; then
    echo "FAIL: expected exactly 1 VP table/view for promoted predicate, got $VP_TABLE_COUNT"
    exit 1
fi

echo "PASS: exactly one VP table created for concurrently-promoted predicate."
