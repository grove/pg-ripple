#!/usr/bin/env bash
# tests/stress/promotion_race.sh — v0.47.0
#
# Stress-tests the VP table promotion race condition:
# fire 50 concurrent sessions each inserting triples at the rare-predicate
# promotion threshold and verify that:
#   1. No duplicate SIDs are assigned across workers.
#   2. No errors are emitted (deadlock, unique violation, etc.).
#   3. All inserted triples are retrievable after the storm.
#
# Requires:
#   - A running pgrx-managed PostgreSQL 18 instance (cargo pgrx start pg18).
#   - pg_ripple extension installed in the target database.
#   - psql on PATH.
#
# Usage:
#   bash tests/stress/promotion_race.sh [DBNAME] [PGHOST] [PGPORT]

set -euo pipefail

DBNAME="${1:-pg_ripple}"
PGHOST="${2:-/tmp}"
PGPORT="${3:-28818}"
WORKERS=50
PREDICATE="https://stress.test/promotionRace/p1"
TRIPLES_PER_WORKER=25

echo "=== promotion_race stress test ==="
echo "  database : ${DBNAME}"
echo "  host     : ${PGHOST}"
echo "  port     : ${PGPORT}"
echo "  workers  : ${WORKERS}"
echo ""

# ── Helper: run SQL via psql ──────────────────────────────────────────────────

psql_run() {
    psql -h "${PGHOST}" -p "${PGPORT}" -d "${DBNAME}" -v ON_ERROR_STOP=1 -c "$1" -t -A 2>&1
}

# ── Setup ─────────────────────────────────────────────────────────────────────

echo "Setting up extension..."
psql_run "CREATE EXTENSION IF NOT EXISTS pg_ripple;" || true

# Lower the promotion threshold so we cross it quickly.
psql_run "SET pg_ripple.vp_promotion_threshold = 10;"

# ── Fire concurrent workers ───────────────────────────────────────────────────

echo "Starting ${WORKERS} concurrent workers..."

pids=()
for i in $(seq 1 "${WORKERS}"); do
    (
        for j in $(seq 1 "${TRIPLES_PER_WORKER}"); do
            subject="<https://stress.test/promotionRace/s_${i}_${j}>"
            predicate="<${PREDICATE}>"
            object="\"value_${i}_${j}\""
            psql -h "${PGHOST}" -p "${PGPORT}" -d "${DBNAME}" \
                 -v ON_ERROR_STOP=1 -q \
                 -c "SELECT pg_ripple.insert_triple('${subject}', '${predicate}', '${object}');" \
                 2>&1
        done
    ) &
    pids+=($!)
done

# Wait for all workers and collect exit codes.
failed=0
for pid in "${pids[@]}"; do
    if ! wait "${pid}"; then
        failed=$((failed + 1))
    fi
done

if [[ "${failed}" -gt 0 ]]; then
    echo "FAIL: ${failed} worker(s) exited with errors."
    exit 1
fi

echo "All workers completed without errors."

# ── Verify: count inserted triples ───────────────────────────────────────────

expected=$((WORKERS * TRIPLES_PER_WORKER))
actual=$(psql_run "SELECT pg_ripple.sparql_select('SELECT (COUNT(*) AS ?c) WHERE { ?s <${PREDICATE}> ?o }') AS r;" | grep -oP '"c":"\K[0-9]+' || echo 0)

echo "Expected: ${expected} triples for predicate, got: ${actual}"

if [[ "${actual}" -lt "${expected}" ]]; then
    echo "FAIL: Missing triples — possible SID collision or lost insert."
    exit 1
fi

# ── Verify: no duplicate SIDs via dictionary ─────────────────────────────────

dup_count=$(psql_run \
    "SELECT COUNT(*) FROM (
         SELECT sid FROM _pg_ripple.dictionary
         GROUP BY sid HAVING COUNT(*) > 1
     ) dups;" | tr -d ' ')

if [[ "${dup_count}" -gt 0 ]]; then
    echo "FAIL: ${dup_count} duplicate SID(s) found in dictionary table."
    exit 1
fi

echo "OK: No duplicate SIDs found."

# ── Cleanup ───────────────────────────────────────────────────────────────────

psql_run "SELECT pg_ripple.drop_triples_by_graph(NULL);" || true
echo "=== promotion_race: PASS ==="
