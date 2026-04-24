#!/usr/bin/env bash
# tests/concurrent/parallel_insert.sh
#
# v0.55.0 J-3: Concurrent write test — N parallel psql sessions each insert
# a disjoint set of triples and verify that no data is lost or duplicated.
#
# Prerequisites:
#   - cargo pgrx start pg18 must be running
#   - The pg_ripple extension must be installed
#   - Run as the current user (not root)
#
# What this test does:
#   1. Creates a fresh test database with pg_ripple
#   2. Spawns WORKERS parallel psql sessions; each inserts TRIPLES_PER_WORKER triples
#   3. Waits for all sessions to complete
#   4. Asserts:
#      a. Total triple count = WORKERS × TRIPLES_PER_WORKER (no lost writes)
#      b. No duplicate dictionary entries for the inserted IRIs
#      c. SPARQL SELECT retrieves at least WORKERS × TRIPLES_PER_WORKER rows
#
# Environment variables (optional overrides):
#   WORKERS                — parallel sessions (default: 4)
#   TRIPLES_PER_WORKER     — triples per session (default: 500)
#   PGRX_PG18_PORT         — PostgreSQL port (default: 28818)
#
# Exit codes:
#   0 — all assertions passed
#   1 — an assertion failed
#   2 — setup/teardown error

set -euo pipefail

WORKERS="${WORKERS:-4}"
TRIPLES_PER_WORKER="${TRIPLES_PER_WORKER:-500}"
PGHOST="${HOME}/.pgrx"
PGPORT="${PGRX_PG18_PORT:-28818}"
PGDATABASE="pg_ripple_parallel_test"
PSQL_BASE="psql -h ${PGHOST} -p ${PGPORT} -d ${PGDATABASE}"

log()  { echo "[concurrent/parallel_insert] $*" >&2; }
fail() { log "FAIL: $*"; exit 1; }
step() { log "--- $* ---"; }

# ── Cleanup ────────────────────────────────────────────────────────────────────
cleanup() {
    log "cleanup: dropping test database"
    psql -h "${PGHOST}" -p "${PGPORT}" -d postgres -q \
        -c "DROP DATABASE IF EXISTS ${PGDATABASE};" 2>/dev/null || true
    # Kill any lingering worker psql processes
    jobs -p | xargs -r kill 2>/dev/null || true
    wait 2>/dev/null || true
}
trap cleanup EXIT

# ── Setup ──────────────────────────────────────────────────────────────────────
step "Creating test database"
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres -q \
    -c "DROP DATABASE IF EXISTS ${PGDATABASE};"
psql -h "${PGHOST}" -p "${PGPORT}" -d postgres -q \
    -c "CREATE DATABASE ${PGDATABASE};"
${PSQL_BASE} -q -c "CREATE EXTENSION pg_ripple;"

EXPECTED_TOTAL=$(( WORKERS * TRIPLES_PER_WORKER ))
log "workers=${WORKERS} triples_per_worker=${TRIPLES_PER_WORKER} expected_total=${EXPECTED_TOTAL}"

# ── Parallel insert ────────────────────────────────────────────────────────────
step "Launching ${WORKERS} parallel insert sessions"

declare -a PIDS=()
for W in $(seq 1 "${WORKERS}"); do
    # Each worker inserts a disjoint set: subject = /W{worker}/T{0..N-1}
    OFFSET=$(( (W - 1) * TRIPLES_PER_WORKER ))
    ${PSQL_BASE} -q <<SQL &
DO \$\$
DECLARE
    i    INT;
    base INT := ${OFFSET};
BEGIN
    FOR i IN 0..$((TRIPLES_PER_WORKER - 1)) LOOP
        PERFORM pg_ripple.load_turtle(format(
            '<https://parallel.test/S%s> <https://parallel.test/value> "%s" .',
            base + i, base + i
        ), false);
    END LOOP;
END \$\$;
SQL
    PIDS+=($!)
    log "  launched worker ${W} (PID ${PIDS[-1]})"
done

# Wait for all workers; collect exit codes
FAILED=0
for PID in "${PIDS[@]}"; do
    if ! wait "${PID}"; then
        log "worker PID ${PID} exited with error"
        FAILED=$(( FAILED + 1 ))
    fi
done
[[ ${FAILED} -eq 0 ]] || fail "${FAILED} worker(s) failed during parallel insert"

# ── Assertions ─────────────────────────────────────────────────────────────────
step "Assertion A: total triple count = ${EXPECTED_TOTAL}"
ACTUAL=$(${PSQL_BASE} -t -A -q <<'SQL'
SELECT count(*) FROM pg_ripple.sparql(
  'SELECT ?s ?o WHERE { ?s <https://parallel.test/value> ?o . }'
);
SQL
)
[[ "${ACTUAL}" =~ ^[0-9]+$ ]] || fail "SPARQL returned non-numeric result: '${ACTUAL}'"
[[ "${ACTUAL}" -eq "${EXPECTED_TOTAL}" ]] \
    || fail "triple count mismatch: expected ${EXPECTED_TOTAL}, got ${ACTUAL}"
log "  total triples: ${ACTUAL} — OK"

step "Assertion B: no duplicate dictionary entries"
DUPS=$(${PSQL_BASE} -t -A -q <<'SQL'
SELECT count(*) FROM (
    SELECT value, count(*) c
    FROM _pg_ripple.dictionary
    WHERE value LIKE 'https://parallel.test/%'
    GROUP BY value
    HAVING count(*) > 1
) sub;
SQL
)
[[ "${DUPS}" -eq 0 ]] \
    || fail "duplicate dictionary entries found: ${DUPS} duplicated values"
log "  duplicate dictionary entries: 0 — OK"

step "Assertion C: SPARQL SELECT retrieves all rows"
SPARQL_ROWS=$(${PSQL_BASE} -t -A -q <<'SQL'
SELECT count(*) FROM pg_ripple.sparql(
  'SELECT ?s ?o WHERE { ?s <https://parallel.test/value> ?o . }'
);
SQL
)
[[ "${SPARQL_ROWS}" -ge "${EXPECTED_TOTAL}" ]] \
    || fail "SPARQL returned only ${SPARQL_ROWS} rows, expected >= ${EXPECTED_TOTAL}"
log "  SPARQL rows: ${SPARQL_ROWS} — OK"

log "=== parallel_insert concurrent write test PASSED ==="
exit 0
