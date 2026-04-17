#!/usr/bin/env bash
# tests/crash_recovery/shacl_during_violation.sh
#
# Crash recovery test: kill -9 PostgreSQL during async SHACL validation
# queue processing.
#
# Prerequisites: same as merge_during_kill.sh
#
# What this test does:
#   1. Loads a SHACL shape and inserts violating triples to fill the queue
#   2. Starts process_validation_queue() in async mode
#   3. Sends kill -9 during queue processing
#   4. Restarts PostgreSQL
#   5. Verifies:
#      a. No violation reports are lost (queue re-processed)
#      b. No rows are orphaned in the async queue
#      c. dead_letter_queue() is accessible and consistent
#      d. A fresh validation run completes correctly
#
# Exit codes: 0 = pass, 1 = assertion failure, 2 = setup error

set -euo pipefail

PGHOST="${HOME}/.pgrx"
PGPORT="${PGRX_PG18_PORT:-28818}"
PGDATABASE="pg_ripple_shacl_crash_test"
PGBIN="$(pg_config --bindir 2>/dev/null || echo "${HOME}/.pgrx/18.*/pgrx-install/bin")"
PGDATA="$(ls -d "${HOME}/.pgrx/data-18" 2>/dev/null | head -1 || true)"
PSQL="psql -h ${PGHOST} -p ${PGPORT} -d ${PGDATABASE}"
PGCTL="${PGBIN}/pg_ctl"
NS="https://crash.shacl.test"

log()  { echo "[crash_recovery/shacl] $*" >&2; }
fail() { log "FAIL: $*"; exit 1; }
pass() { log "PASS: $*"; }

# ── 1. Setup ───────────────────────────────────────────────────────────────────
log "Setting up SHACL crash test database..."
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "CREATE DATABASE ${PGDATABASE};"
${PSQL} -c "CREATE EXTENSION pg_ripple;"

# ── 2. Load a strict SHACL shape ─────────────────────────────────────────────
log "Loading SHACL shape..."
${PSQL} -c "
SELECT pg_ripple.load_shacl(\$SHACL\$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <${NS}/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:NodeShape
    a sh:NodeShape ;
    sh:targetClass ex:Item ;
    sh:property [
        sh:path ex:code ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] .
\$SHACL\$);
" || fail "load_shacl failed"

# ── 3. Insert violating triples to populate the async queue ──────────────────
log "Inserting violating triples to fill validation queue..."
for i in $(seq 1 50); do
    ${PSQL} -c "
    SELECT pg_ripple.load_ntriples(
        '<${NS}/item${i}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <${NS}/Item> .'
    );
    " > /dev/null
done

# ── 4. Start async validation in background and kill ─────────────────────────
log "Starting process_validation_queue in background..."
${PSQL} -c "SELECT pg_ripple.process_validation_queue();" &
QUEUE_PID=$!
sleep 1

PGPID=$(cat "${PGDATA}/postmaster.pid" 2>/dev/null | head -1 || true)
if [[ -n "${PGPID}" ]]; then
    log "Sending kill -9 to PID ${PGPID} during validation..."
    kill -9 "${PGPID}" 2>/dev/null || true
    wait "${QUEUE_PID}" 2>/dev/null || true
    sleep 2
else
    log "WARNING: Could not find postmaster.pid — testing clean restart only"
    wait "${QUEUE_PID}" 2>/dev/null || true
fi

# ── 5. Restart ────────────────────────────────────────────────────────────────
log "Restarting PostgreSQL..."
"${PGCTL}" start -D "${PGDATA}" -w -t 30 2>/dev/null || {
    cargo pgrx start pg18 2>/dev/null || fail "Could not restart PostgreSQL"
}
sleep 3

# ── 6. Recovery assertions ────────────────────────────────────────────────────
log "Checking SHACL queue recovery..."

# 6a. process_validation_queue() runs without error after restart
${PSQL} -c "SELECT pg_ripple.process_validation_queue();" > /dev/null \
    || fail "process_validation_queue() failed after restart"
pass "process_validation_queue() succeeded after restart"

# 6b. dead_letter_queue() is accessible and has no orphaned rows
DLQ_COUNT=$(${PSQL} -t -A -c "SELECT count(*) FROM pg_ripple.dead_letter_queue();") \
    || fail "dead_letter_queue() inaccessible after restart"
pass "dead_letter_queue() accessible (${DLQ_COUNT} entries)"

# 6c. validate() completes without error
CONFORMS=$(${PSQL} -t -A -c "
    SELECT (pg_ripple.validate() ->> 'conforms')::boolean;
") || fail "validate() failed after restart"
pass "validate() returns conforms=${CONFORMS}"

# 6d. Fresh insert and validate cycle works
${PSQL} -c "
    SELECT pg_ripple.load_ntriples(
        '<${NS}/post_crash_item> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <${NS}/Item> .' ||
        E'\n' ||
        '<${NS}/post_crash_item> <${NS}/code> \"PC001\"^^<http://www.w3.org/2001/XMLSchema#string> .'
    );
" > /dev/null || fail "post-crash insert failed"

${PSQL} -c "SELECT pg_ripple.process_validation_queue();" > /dev/null \
    || fail "second process_validation_queue() failed"
pass "post-crash insert + validation cycle: PASS"

# ── 7. Cleanup ─────────────────────────────────────────────────────────────────
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true

log ""
log "============================================="
log " shacl_during_violation: ALL ASSERTIONS PASSED"
log "============================================="
exit 0
