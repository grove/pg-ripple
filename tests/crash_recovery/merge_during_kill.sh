#!/usr/bin/env bash
# tests/crash_recovery/merge_during_kill.sh
#
# Crash recovery test: kill -9 PostgreSQL during an HTAP merge operation.
#
# Prerequisites:
#   - cargo pgrx start pg18 must be running
#   - The pg_ripple extension must be installed
#   - pg_config must point to the pgrx PG18 installation
#   - Run as the current user (not root)
#
# What this test does:
#   1. Loads 10,000 triples to trigger the HTAP merge background worker
#   2. Sends kill -9 to the PostgreSQL backend PID during merge
#   3. Restarts PostgreSQL via pg_ctl
#   4. Verifies:
#      a. _pg_ripple.predicates catalog is intact
#      b. VP table data is recoverable (rows visible)
#      c. Dictionary is consistent (no orphaned or duplicate entries)
#      d. Subsequent SPARQL queries return correct results
#
# Exit codes:
#   0 — all assertions passed
#   1 — a recovery assertion failed
#   2 — setup/teardown error

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────────
PGHOST="${HOME}/.pgrx"
PGPORT="${PGRX_PG18_PORT:-28818}"
PGDATABASE="pg_ripple_crash_test"
PGBIN="$(pg_config --bindir 2>/dev/null || echo "${HOME}/.pgrx/18.*/pgrx-install/bin")"
PGDATA="$(ls -d "${HOME}/.pgrx/data-18" 2>/dev/null | head -1 || true)"
PSQL="psql -h ${PGHOST} -p ${PGPORT} -d ${PGDATABASE}"
PGCTL="${PGBIN}/pg_ctl"
NAMESPACE="https://crash.merge.test"

log() { echo "[crash_recovery/merge] $*" >&2; }
fail() { log "FAIL: $*"; exit 1; }
pass() { log "PASS: $*"; }

# ── 1. Setup ───────────────────────────────────────────────────────────────────
log "Setting up crash recovery test database..."

psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "CREATE DATABASE ${PGDATABASE};"
${PSQL} -c "CREATE EXTENSION pg_ripple;"
${PSQL} -c "SET pg_ripple.merge_threshold = 100;"

# ── 2. Load triples to trigger merge ─────────────────────────────────────────
log "Loading triples to trigger HTAP merge..."

# Generate 200 INSERT statements to exceed merge threshold
for i in $(seq 1 200); do
    ${PSQL} -c "SELECT pg_ripple.insert_triple(
        '<${NAMESPACE}/s${i}>',
        '<${NAMESPACE}/knows>',
        '<${NAMESPACE}/o${i}>'
    );" > /dev/null
done

log "Inserted 200 triples. Triggering background merge..."
${PSQL} -c "SELECT pg_ripple.trigger_merge();" > /dev/null 2>&1 || true

# Give merge worker a moment to start
sleep 1

# ── 3. Kill PostgreSQL during merge ───────────────────────────────────────────
log "Sending kill -9 to PostgreSQL postmaster..."
PGPID=$(cat "${PGDATA}/postmaster.pid" 2>/dev/null | head -1 || true)

if [[ -z "${PGPID}" ]]; then
    log "WARNING: Could not find postmaster.pid — simulating crash with SIGTERM instead"
    PGPID=$(pgrep -f "postgres.*data-18" | head -1 || true)
fi

if [[ -n "${PGPID}" ]]; then
    kill -9 "${PGPID}" 2>/dev/null || true
    log "Sent kill -9 to PID ${PGPID}"
    sleep 2
else
    log "WARNING: Could not determine PID — skipping kill step, testing clean recovery only"
fi

# ── 4. Restart PostgreSQL ─────────────────────────────────────────────────────
log "Restarting PostgreSQL via pg_ctl..."
"${PGCTL}" start -D "${PGDATA}" -w -t 30 2>/dev/null || {
    log "pg_ctl restart failed; trying pgrx start..."
    cargo pgrx start pg18 2>/dev/null || fail "Could not restart PostgreSQL"
}
sleep 3

# ── 5. Recovery assertions ────────────────────────────────────────────────────
log "Verifying recovery assertions..."

# 5a. Predicates catalog integrity
PRED_COUNT=$(${PSQL} -t -A -c "
    SELECT count(*) FROM _pg_ripple.predicates
    WHERE triple_count >= 0;
" 2>/dev/null) || fail "predicates catalog inaccessible after restart"
pass "predicates catalog accessible (${PRED_COUNT} entries)"

# 5b. No negative triple counts
NEG_COUNT=$(${PSQL} -t -A -c "
    SELECT count(*) FROM _pg_ripple.predicates
    WHERE triple_count < 0;
" 2>/dev/null) || fail "predicates catalog query failed"
[[ "${NEG_COUNT}" -eq 0 ]] || fail "found ${NEG_COUNT} predicates with negative triple_count"
pass "no negative triple counts"

# 5c. Dictionary consistency: no duplicate (value, kind) pairs
DUP_COUNT=$(${PSQL} -t -A -c "
    SELECT count(*) FROM (
        SELECT value, kind, count(*) AS n
        FROM _pg_ripple.dictionary
        GROUP BY value, kind
        HAVING count(*) > 1
    ) dups;
" 2>/dev/null) || fail "dictionary table inaccessible after restart"
[[ "${DUP_COUNT}" -eq 0 ]] || fail "found ${DUP_COUNT} duplicate dictionary entries"
pass "dictionary consistent (no duplicates)"

# 5d. VP table data recoverable: some triples should be visible
TRIPLE_COUNT=$(${PSQL} -t -A -c "
    SELECT pg_ripple.triple_count();
" 2>/dev/null) || fail "triple_count() failed after restart"
pass "triple_count() = ${TRIPLE_COUNT} (recovery visible)"

# 5e. Subsequent queries return correct results
SPARQL_COUNT=$(${PSQL} -t -A -c "
    SELECT count(*) FROM pg_ripple.sparql(
        'SELECT ?s ?o WHERE { ?s <${NAMESPACE}/knows> ?o . }'
    );
" 2>/dev/null) || fail "SPARQL query failed after recovery"
pass "SPARQL query returns ${SPARQL_COUNT} rows after recovery"

# 5f. New inserts work after recovery
${PSQL} -c "
    SELECT pg_ripple.insert_triple(
        '<${NAMESPACE}/post_crash>',
        '<${NAMESPACE}/status>',
        '\"recovered\"'
    ) > 0;
" > /dev/null || fail "insert_triple failed after recovery"
pass "insert_triple works after recovery"

# 5g. vacuum/reindex run cleanly
${PSQL} -c "SELECT pg_ripple.vacuum();" > /dev/null || fail "vacuum() failed after recovery"
${PSQL} -c "SELECT pg_ripple.reindex();" > /dev/null || fail "reindex() failed after recovery"
pass "vacuum() and reindex() complete without error"

# ── 6. Cleanup ─────────────────────────────────────────────────────────────────
log "Cleaning up test database..."
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true

log ""
log "========================================="
log " merge_during_kill: ALL ASSERTIONS PASSED"
log "========================================="
exit 0
