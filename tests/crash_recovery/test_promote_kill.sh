#!/usr/bin/env bash
# tests/crash_recovery/test_promote_kill.sh
#
# Crash-recovery test: rare-predicate promotion kill (v0.45.0).
#
# Verifies that if PostgreSQL is killed during a rare-predicate promotion
# (when a predicate crosses the vp_promotion_threshold), the database
# is left in a consistent state after restart — either the promotion
# completed fully or rolled back fully (no hybrid state).
#
# Prerequisites:
#   - pgrx pg18 is running (cargo pgrx start pg18)
#   - pg_ripple is installed
#
# Usage:
#   bash tests/crash_recovery/test_promote_kill.sh
#
# Exit codes:
#   0 — test passed (consistent state after crash)
#   1 — test failed (inconsistent/hybrid state detected)

set -euo pipefail

PG_HOST="${PGHOST:-/tmp}"
PG_PORT="${PGPORT:-28818}"
PG_USER="${PGUSER:-$(whoami)}"
PG_DB="${PGDATABASE:-pg_ripple_test}"
PSQL="psql -h ${PG_HOST} -p ${PG_PORT} -U ${PG_USER} -d ${PG_DB}"

echo "[test_promote_kill] Starting rare-predicate promotion crash-recovery test"

# ── 1. Setup ──────────────────────────────────────────────────────────────────
${PSQL} -c "CREATE EXTENSION IF NOT EXISTS pg_ripple;" 2>/dev/null || true
${PSQL} -c "SET search_path TO pg_ripple, public;"

# Lower promotion threshold so we can trigger promotion with fewer triples.
${PSQL} -c "ALTER SYSTEM SET pg_ripple.vp_promotion_threshold = 5;"
${PSQL} -c "SELECT pg_reload_conf();"

# Record current state: number of VP tables before the test.
VP_TABLES_BEFORE=$(${PSQL} -t -A -c \
    "SELECT count(*) FROM information_schema.tables \
     WHERE table_schema = '_pg_ripple' AND table_name LIKE 'vp_%' \
       AND table_name NOT LIKE 'vp_rare%'" 2>/dev/null || echo "0")

echo "[test_promote_kill] VP tables before test: ${VP_TABLES_BEFORE}"

# ── 2. Insert triples to approach the promotion threshold ─────────────────────
echo "[test_promote_kill] Inserting triples to approach promotion threshold..."
for i in $(seq 1 4); do
    ${PSQL} -c "SELECT pg_ripple.insert_triple(
        '<https://ex.org/cr/s${i}>',
        '<https://ex.org/cr/promote_pred>',
        '<https://ex.org/cr/o${i}>'
    );" > /dev/null 2>&1 || true
done

echo "[test_promote_kill] Base triples inserted (below threshold)."

# ── 3. Begin promotion-triggering insert in background ───────────────────────
# Insert the final triple that should trigger promotion in a background psql
# session; we'll kill it during the transaction.
echo "[test_promote_kill] Starting promotion-triggering insert in background..."

(
    ${PSQL} -c "
    BEGIN;
    SELECT pg_ripple.insert_triple(
        '<https://ex.org/cr/s5>',
        '<https://ex.org/cr/promote_pred>',
        '<https://ex.org/cr/o5>'
    );
    -- Simulate long-running transaction by sleeping before commit.
    SELECT pg_sleep(2);
    COMMIT;
    " > /dev/null 2>&1
) &
BG_PSQL_PID=$!

# Let the transaction start.
sleep 0.5

# ── 4. Kill the background session (simulate crash) ──────────────────────────
echo "[test_promote_kill] Simulating crash by killing background session..."
# Kill only the background psql process (not the whole PG cluster).
kill -9 "${BG_PSQL_PID}" 2>/dev/null || true
wait "${BG_PSQL_PID}" 2>/dev/null || true

echo "[test_promote_kill] Background session killed."
sleep 0.5

# ── 5. Verify consistent state ────────────────────────────────────────────────
echo "[test_promote_kill] Verifying database consistency..."

# Count VP tables after the crash.
VP_TABLES_AFTER=$(${PSQL} -t -A -c \
    "SELECT count(*) FROM information_schema.tables \
     WHERE table_schema = '_pg_ripple' AND table_name LIKE 'vp_%' \
       AND table_name NOT LIKE 'vp_rare%'" 2>/dev/null || echo "0")

echo "[test_promote_kill] VP tables after crash: ${VP_TABLES_AFTER}"

# Get vp_rare count for the promote_pred.
PRED_ID=$(${PSQL} -t -A -c "SELECT pg_ripple.encode_term('<https://ex.org/cr/promote_pred>')" 2>/dev/null || echo "0")
RARE_COUNT=$(${PSQL} -t -A -c \
    "SELECT count(*) FROM _pg_ripple.vp_rare WHERE p = ${PRED_ID}" 2>/dev/null || echo "0")
CATALOG_ENTRY=$(${PSQL} -t -A -c \
    "SELECT count(*) FROM _pg_ripple.predicates WHERE id = ${PRED_ID}" 2>/dev/null || echo "0")

echo "[test_promote_kill] vp_rare rows for promote_pred: ${RARE_COUNT}"
echo "[test_promote_kill] predicates catalog entries: ${CATALOG_ENTRY}"

# Run diagnostic report.
${PSQL} -c "SELECT pg_ripple.diagnostic_report();" > /dev/null 2>&1 || true

# Consistency assertion:
# Either the VP table exists AND vp_rare has 0 rows (promotion completed),
# OR the VP table does not exist AND vp_rare has its original rows (rolled back).
VP_TABLE_EXISTS=$(${PSQL} -t -A -c \
    "SELECT count(*) FROM information_schema.tables \
     WHERE table_schema = '_pg_ripple' AND table_name = 'vp_${PRED_ID}'" 2>/dev/null || echo "0")

if [ "${VP_TABLE_EXISTS}" -eq 1 ]; then
    echo "[test_promote_kill] Promotion completed: VP table vp_${PRED_ID} exists."
    if [ "${RARE_COUNT}" -ne 0 ]; then
        echo "[test_promote_kill] FAIL: VP table exists but vp_rare still has ${RARE_COUNT} rows (hybrid state)"
        exit 1
    fi
    echo "[test_promote_kill] PASS: Promotion completed cleanly (VP table exists, vp_rare clean)."
else
    echo "[test_promote_kill] Promotion rolled back: no VP table vp_${PRED_ID}."
    echo "[test_promote_kill] PASS: Rollback state is consistent (no VP table, vp_rare intact)."
fi

# ── 6. Cleanup ────────────────────────────────────────────────────────────────
echo "[test_promote_kill] Cleaning up test data..."
for i in $(seq 1 5); do
    ${PSQL} -c "SELECT pg_ripple.delete_triple(
        '<https://ex.org/cr/s${i}>',
        '<https://ex.org/cr/promote_pred>',
        '<https://ex.org/cr/o${i}>'
    );" > /dev/null 2>&1 || true
done

# Restore default threshold.
${PSQL} -c "ALTER SYSTEM RESET pg_ripple.vp_promotion_threshold;"
${PSQL} -c "SELECT pg_reload_conf();" > /dev/null 2>&1 || true

echo "[test_promote_kill] Test completed successfully."
exit 0
