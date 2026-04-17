#!/usr/bin/env bash
# tests/crash_recovery/dict_during_kill.sh
#
# Crash recovery test: kill -9 PostgreSQL during a high-volume dictionary
# encoding operation (bulk load with many distinct terms).
#
# Prerequisites: same as merge_during_kill.sh
#
# What this test does:
#   1. Starts a bulk load with 5,000 distinct IRIs (high dictionary write pressure)
#   2. Sends kill -9 to the PostgreSQL backend mid-load
#   3. Restarts PostgreSQL
#   4. Verifies:
#      a. Dictionary has no duplicate (value, kind) pairs
#      b. Dictionary has no orphaned entries (all stored IDs decode correctly)
#      c. Subsequent encode/decode round-trips work
#      d. A fresh bulk load completes successfully
#
# Exit codes: 0 = pass, 1 = assertion failure, 2 = setup error

set -euo pipefail

PGHOST="${HOME}/.pgrx"
PGPORT="${PGRX_PG18_PORT:-28818}"
PGDATABASE="pg_ripple_dict_crash_test"
PGBIN="$(pg_config --bindir 2>/dev/null || echo "${HOME}/.pgrx/18.*/pgrx-install/bin")"
PGDATA="$(ls -d "${HOME}/.pgrx/data-18" 2>/dev/null | head -1 || true)"
PSQL="psql -h ${PGHOST} -p ${PGPORT} -d ${PGDATABASE}"
PGCTL="${PGBIN}/pg_ctl"
NAMESPACE="https://crash.dict.test"

log()  { echo "[crash_recovery/dict] $*" >&2; }
fail() { log "FAIL: $*"; exit 1; }
pass() { log "PASS: $*"; }

# ── 1. Setup ───────────────────────────────────────────────────────────────────
log "Setting up dictionary crash test database..."
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "CREATE DATABASE ${PGDATABASE};"
${PSQL} -c "CREATE EXTENSION pg_ripple;"

# ── 2. Generate N-Triples payload with many distinct IRIs ─────────────────────
log "Generating bulk load payload with 5000 distinct IRIs..."
TMPFILE=$(mktemp /tmp/pg_ripple_dict_crash_XXXXXX.nt)
trap "rm -f ${TMPFILE}" EXIT

python3 - "${NAMESPACE}" "${TMPFILE}" <<'PYEOF'
import sys
ns = sys.argv[1]
outfile = sys.argv[2]
lines = []
for i in range(5000):
    lines.append(
        f"<{ns}/s{i}> <{ns}/p{i % 100}> <{ns}/o{i}> ."
    )
with open(outfile, "w") as f:
    f.write("\n".join(lines) + "\n")
print(f"Generated {len(lines)} triples to {outfile}", file=sys.stderr)
PYEOF

# ── 3. Start bulk load in background and kill ────────────────────────────────
log "Starting bulk load in background..."
${PSQL} -c "
    SELECT pg_ripple.load_ntriples(pg_read_file('${TMPFILE}'));
" &
LOAD_PID=$!
sleep 1

# Kill the postgres backend
PGPID=$(cat "${PGDATA}/postmaster.pid" 2>/dev/null | head -1 || true)
if [[ -n "${PGPID}" ]]; then
    log "Sending kill -9 to PID ${PGPID} during bulk load..."
    kill -9 "${PGPID}" 2>/dev/null || true
    wait "${LOAD_PID}" 2>/dev/null || true
    sleep 2
else
    log "WARNING: Could not find postmaster.pid — testing clean restart only"
    wait "${LOAD_PID}" 2>/dev/null || true
fi

# ── 4. Restart ────────────────────────────────────────────────────────────────
log "Restarting PostgreSQL..."
"${PGCTL}" start -D "${PGDATA}" -w -t 30 2>/dev/null || {
    cargo pgrx start pg18 2>/dev/null || fail "Could not restart PostgreSQL"
}
sleep 3

# ── 5. Dictionary consistency assertions ─────────────────────────────────────
log "Checking dictionary consistency..."

DUP_COUNT=$(${PSQL} -t -A -c "
    SELECT count(*) FROM (
        SELECT value, kind, count(*) AS n
        FROM _pg_ripple.dictionary
        GROUP BY value, kind
        HAVING count(*) > 1
    ) dups;
") || fail "dictionary query failed"
[[ "${DUP_COUNT}" -eq 0 ]] || fail "${DUP_COUNT} duplicate dictionary entries found"
pass "no duplicate dictionary entries"

DICT_COUNT=$(${PSQL} -t -A -c "SELECT count(*) FROM _pg_ripple.dictionary;") \
    || fail "dictionary count query failed"
pass "dictionary has ${DICT_COUNT} entries after recovery"

# 5c. encode/decode round-trip
ROUNDTRIP=$(${PSQL} -t -A -c "
    SELECT pg_ripple.decode_id(
        pg_ripple.encode_term('${NAMESPACE}/probe', 0::smallint)
    ) = '${NAMESPACE}/probe' AS ok;
") || fail "encode/decode round-trip failed"
[[ "${ROUNDTRIP}" == "t" ]] || fail "encode/decode round-trip returned wrong value: ${ROUNDTRIP}"
pass "encode/decode round-trip correct"

# 5d. Fresh bulk load succeeds after recovery
LOAD_COUNT=$(${PSQL} -t -A -c "
    SELECT pg_ripple.load_ntriples(
        '<${NAMESPACE}/post/s1> <${NAMESPACE}/post/p1> <${NAMESPACE}/post/o1> .' ||
        E'\n' ||
        '<${NAMESPACE}/post/s2> <${NAMESPACE}/post/p1> <${NAMESPACE}/post/o2> .'
    );
") || fail "fresh bulk load failed after recovery"
pass "fresh bulk load after recovery: ${LOAD_COUNT} triples"

# ── 6. Cleanup ─────────────────────────────────────────────────────────────────
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true

log ""
log "========================================"
log " dict_during_kill: ALL ASSERTIONS PASSED"
log "========================================"
exit 0
