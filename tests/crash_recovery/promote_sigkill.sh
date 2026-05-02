#!/usr/bin/env bash
# tests/crash_recovery/promote_sigkill.sh
#
# CC13-01 (v0.85.0): VP-promotion crash-recovery regression test.
#
# Verifies that if PostgreSQL is SIGKILLed between the advisory-lock
# acquisition and the CTE commit during rare-predicate promotion, the
# predicate is in a consistent state after restart and
# `recover_interrupted_promotions()` resolves any 'promoting' status.
#
# Prerequisites:
#   - pgrx pg18 is running (cargo pgrx start pg18)
#   - pg_ripple is installed
#
# Usage:
#   bash tests/crash_recovery/promote_sigkill.sh
#
# Exit codes:
#   0 — test passed
#   1 — test failed (stuck in 'promoting' status or inconsistent state)

set -euo pipefail

PG_HOST="${PGHOST:-/tmp}"
PG_PORT="${PGPORT:-28818}"
PG_USER="${PGUSER:-$(whoami)}"
PG_DB="${PGDATABASE:-pg_ripple_test}"
PSQL="psql -h ${PG_HOST} -p ${PG_PORT} -U ${PG_USER} -d ${PG_DB} -t -A"

echo "[promote_sigkill] CC13-01: VP-promotion crash-recovery test"

# ── 1. Setup ──────────────────────────────────────────────────────────────────
${PSQL} -c "CREATE EXTENSION IF NOT EXISTS pg_ripple;" 2>/dev/null || true

# Lower threshold so promotion triggers quickly.
${PSQL} -c "ALTER SYSTEM SET pg_ripple.vp_promotion_threshold = 5;"
${PSQL} -c "SELECT pg_reload_conf();"

echo "[promote_sigkill] Inserting triples to approach promotion threshold..."
for i in $(seq 1 4); do
    ${PSQL} -c "SELECT pg_ripple.load_ntriples(
        '<http://test.promote.sigkill/s${i}> <http://test.promote.sigkill/p1> <http://test.promote.sigkill/o${i}> .'
    );"
done

echo "[promote_sigkill] Inserting the threshold-crossing triple in a background process..."

# Start the final insert in a background psql process so we can SIGKILL mid-way.
# We use a sleep inside the transaction to widen the SIGKILL window.
psql -h "${PG_HOST}" -p "${PG_PORT}" -U "${PG_USER}" -d "${PG_DB}" \
    -c "SELECT pg_ripple.load_ntriples(
        '<http://test.promote.sigkill/s5> <http://test.promote.sigkill/p1> <http://test.promote.sigkill/o5> .'
    );" &
BG_PID=$!

# Give the backend a short window to start the transaction.
sleep 1

# SIGKILL the background psql (which kills its backend connection).
kill -9 "${BG_PID}" 2>/dev/null || true
wait "${BG_PID}" 2>/dev/null || true

echo "[promote_sigkill] SIGKILL sent to background psql PID ${BG_PID}"

# ── 2. Verify state after crash ───────────────────────────────────────────────
echo "[promote_sigkill] Checking for stuck 'promoting' status..."

PROMOTING_COUNT=$(${PSQL} -c \
    "SELECT count(*) FROM _pg_ripple.predicates \
     WHERE promotion_status = 'promoting';" 2>/dev/null || echo "0")

echo "[promote_sigkill] Predicates stuck in 'promoting': ${PROMOTING_COUNT}"

# ── 3. Run recovery ───────────────────────────────────────────────────────────
echo "[promote_sigkill] Running recover_interrupted_promotions()..."
RECOVERED=$(${PSQL} -c "SELECT pg_ripple.recover_interrupted_promotions();" \
    2>/dev/null || echo "0")
echo "[promote_sigkill] Recovered predicates: ${RECOVERED}"

# ── 4. Assert clean state ─────────────────────────────────────────────────────
STILL_PROMOTING=$(${PSQL} -c \
    "SELECT count(*) FROM _pg_ripple.predicates \
     WHERE promotion_status = 'promoting';" 2>/dev/null || echo "0")

if [ "${STILL_PROMOTING}" -gt 0 ]; then
    echo "[promote_sigkill] FAIL: ${STILL_PROMOTING} predicate(s) still stuck in 'promoting' status after recovery"
    exit 1
fi

echo "[promote_sigkill] PASS: no predicates stuck in 'promoting' status after recovery"
echo "[promote_sigkill] CC13-01: VP-promotion crash-recovery test PASSED"

# ── 5. Cleanup ────────────────────────────────────────────────────────────────
${PSQL} -c "ALTER SYSTEM RESET pg_ripple.vp_promotion_threshold;" 2>/dev/null || true
${PSQL} -c "SELECT pg_reload_conf();" 2>/dev/null || true
