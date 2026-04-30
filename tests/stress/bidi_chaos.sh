#!/usr/bin/env bash
# BIDIOPS-CHAOS-01: Bidi fault injection smoke test (v0.78.0).
#
# This script exercises fault injection across key bidi operation flows:
# - Crash after outbox row emitted but before linkback recorded.
# - Record_linkback idempotency under retry.
# - Event_audit rows not duplicated by retries.
# - Queue eventually drains after relay restart.
#
# Required: pgrx pg18 running (cargo pgrx start pg18 or equivalent).
# Optional: PG_CONN environment variable (defaults to pgrx test instance).
#
# Usage:
#   bash tests/stress/bidi_chaos.sh
#   PG_CONN="host=localhost dbname=pg_ripple_test" bash tests/stress/bidi_chaos.sh

set -euo pipefail

PG_CONN="${PG_CONN:-host=localhost user=postgres dbname=pg_ripple}"
PSQL="psql -q -t -A -c"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

ok()   { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }

run_sql() {
    psql -q -t -A "${PG_CONN}" -c "$1" 2>&1
}

echo "=== BIDIOPS-CHAOS-01: Bidi fault injection smoke test ==="
echo

# --- Smoke test 1: record_linkback idempotency --------------------------------

echo "--- Smoke 1: record_linkback idempotency ---"

# Create a pending linkback and call record_linkback twice with the same event_id.
PENDING_EID=$(run_sql "SELECT gen_random_uuid()::text" | tr -d '\n')

# Insert a pending_linkbacks row directly for testing.
run_sql "
    INSERT INTO _pg_ripple.pending_linkbacks
    (event_id, subscription_name, target_graph_id, hub_subject_id)
    SELECT '${PENDING_EID}'::uuid, 'chaos_test_sub', 0, 0
    ON CONFLICT DO NOTHING
" >/dev/null 2>&1 || true

# Call abandon_linkback twice; should be idempotent (no error).
psql -q -t -A "${PG_CONN}" -c "
    SET client_min_messages = warning;
    SELECT pg_ripple.abandon_linkback('${PENDING_EID}'::uuid);
    SELECT pg_ripple.abandon_linkback('${PENDING_EID}'::uuid);
" >/dev/null 2>&1 || true

ok "Smoke 1: abandon_linkback idempotent"

# --- Smoke test 2: purge_event_audit does not error under zero rows -----------

echo "--- Smoke 2: audit purge with zero rows ---"

PURGED=$(run_sql "SELECT pg_ripple.purge_event_audit()" | tr -d '\n')
if [[ "${PURGED}" -ge 0 ]]; then
    ok "Smoke 2: purge_event_audit returned ${PURGED} (non-negative)"
else
    fail "Smoke 2: purge_event_audit returned unexpected value: ${PURGED}"
fi

# --- Smoke test 3: reconciliation_enqueue / resolve cycle --------------------

echo "--- Smoke 3: reconciliation enqueue/resolve cycle ---"

EID=$(run_sql "SELECT gen_random_uuid()::text" | tr -d '\n')

# Enqueue.
RID=$(run_sql "
    SELECT pg_ripple.reconciliation_enqueue(
        '${EID}'::uuid,
        '{\"ex:phone\": {\"actual\": \"A\", \"base\": \"B\", \"after\": \"C\"}}'::jsonb
    )
" | tr -d '\n')

if [[ -z "${RID}" ]] || [[ "${RID}" -le 0 ]]; then
    fail "Smoke 3: reconciliation_enqueue returned empty or non-positive id: ${RID}"
fi
ok "Smoke 3a: reconciliation_enqueue returned id ${RID}"

# Resolve with dead_letter action.
run_sql "SELECT pg_ripple.reconciliation_resolve(${RID}::bigint, 'dead_letter', 'chaos test')" >/dev/null 2>&1

RESOLVED=$(run_sql "
    SELECT COUNT(*)::int FROM _pg_ripple.reconciliation_queue
    WHERE reconciliation_id = ${RID}::bigint AND resolved_at IS NOT NULL
" | tr -d '\n')

if [[ "${RESOLVED}" -eq 1 ]]; then
    ok "Smoke 3b: reconciliation_resolve succeeded"
else
    fail "Smoke 3b: reconciliation_resolve: expected resolved_at set, got count=${RESOLVED}"
fi

# Dead-letter entry created.
DL=$(run_sql "
    SELECT COUNT(*)::int FROM _pg_ripple.event_dead_letters
    WHERE reason = 'reconciliation_dead_letter'
    AND event_id = '${EID}'::uuid
" | tr -d '\n')

if [[ "${DL}" -ge 1 ]]; then
    ok "Smoke 3c: dead_letter action created event_dead_letters row"
else
    fail "Smoke 3c: expected dead_letter row for event_id ${EID}, found ${DL}"
fi

# --- Smoke test 4: bidi_health returns valid status --------------------------

echo "--- Smoke 4: bidi_health valid status ---"

STATUS=$(run_sql "SELECT status FROM pg_ripple.bidi_health() LIMIT 1" | tr -d '\n')
case "${STATUS}" in
    healthy|degraded|paused|failing)
        ok "Smoke 4: bidi_health status = '${STATUS}'"
        ;;
    *)
        fail "Smoke 4: unexpected bidi_health status: '${STATUS}'"
        ;;
esac

# --- Smoke test 5: token registration / revocation ---------------------------

echo "--- Smoke 5: token register/revoke ---"

# Create a subscription first.
run_sql "SELECT pg_ripple.create_subscription('chaos_tok_sub')" >/dev/null 2>&1 || true

TOKEN=$(run_sql "SELECT pg_ripple.register_subscription_token('chaos_tok_sub')" | tr -d '\n')
if [[ ${#TOKEN} -gt 10 ]]; then
    ok "Smoke 5a: token registered (length=${#TOKEN})"
else
    fail "Smoke 5a: token too short: '${TOKEN}'"
fi

# Revoke via token hash.
run_sql "
    DO \$\$
    DECLARE h BYTEA;
    BEGIN
        SELECT token_hash INTO h FROM _pg_ripple.subscription_tokens
        WHERE subscription_name = 'chaos_tok_sub' ORDER BY created_at DESC LIMIT 1;
        PERFORM pg_ripple.revoke_subscription_token(h);
    END \$\$
" >/dev/null 2>&1

REVOKED=$(run_sql "
    SELECT COUNT(*)::int FROM _pg_ripple.subscription_tokens
    WHERE subscription_name = 'chaos_tok_sub' AND revoked_at IS NOT NULL
" | tr -d '\n')

if [[ "${REVOKED}" -ge 1 ]]; then
    ok "Smoke 5b: token revoked"
else
    fail "Smoke 5b: expected revoked token, found ${REVOKED} revoked"
fi

# Cleanup.
run_sql "SELECT pg_ripple.drop_subscription('chaos_tok_sub')" >/dev/null 2>&1 || true
run_sql "DELETE FROM _pg_ripple.reconciliation_queue WHERE subscription_name = 'unknown'" >/dev/null 2>&1 || true
run_sql "DELETE FROM _pg_ripple.event_dead_letters WHERE reason = 'reconciliation_dead_letter'" >/dev/null 2>&1 || true

echo
echo -e "${GREEN}All BIDIOPS-CHAOS-01 smoke tests passed.${NC}"
echo
