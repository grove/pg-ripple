#!/usr/bin/env bash
# tests/integration/v1_readiness/concurrent_writes.sh
# pg_ripple v0.58.0 — v1 readiness: concurrent write correctness
#
# Launches N parallel psql sessions that all insert into the same triple store
# and verifies the final triple count and absence of duplicates.
#
# Usage: bash tests/integration/v1_readiness/concurrent_writes.sh [N_WORKERS]

set -euo pipefail

N_WORKERS="${1:-4}"
TRIPLES_PER_WORKER=250

PGPORT="${PGPORT:-28818}"
PGHOST="${PGHOST:-localhost}"
PGUSER="${PGUSER:-$(whoami)}"
PGDB="${PGDATABASE:-pg_ripple_test}"

PSQL="psql -h $PGHOST -p $PGPORT -U $PGUSER -d $PGDB -v ON_ERROR_STOP=1"

echo "=== pg_ripple v1 readiness: concurrent writes ==="
echo "  workers=$N_WORKERS triples_per_worker=$TRIPLES_PER_WORKER"

# Clean up test data from previous runs.
$PSQL -c "
  DELETE FROM _pg_ripple.vp_rare WHERE p = (
    SELECT id FROM _pg_ripple.dictionary WHERE value = 'urn:concurrent_writes:pred'
  );
" 2>/dev/null || true

EXPECTED=$(( N_WORKERS * TRIPLES_PER_WORKER ))

# Launch N parallel writers.
PIDS=()
for i in $(seq 1 $N_WORKERS); do
  OFFSET=$(( (i - 1) * TRIPLES_PER_WORKER ))
  $PSQL -c "
  DO \$\$
  BEGIN
    PERFORM pg_ripple.insert_triple(
      '<urn:concurrent_writes:s$i:' || j::text || '>',
      '<urn:concurrent_writes:pred>',
      '<urn:concurrent_writes:o' || ($OFFSET + j)::text || '>'
    )
    FROM generate_series(1, $TRIPLES_PER_WORKER) j;
  END \$\$;
  " &
  PIDS+=($!)
done

# Wait for all writers.
FAILED=0
for pid in "${PIDS[@]}"; do
  if ! wait "$pid"; then
    echo "  WARNING: worker $pid exited with error"
    FAILED=$(( FAILED + 1 ))
  fi
done

if [ "$FAILED" -gt "0" ]; then
  echo "FAIL: $FAILED worker(s) failed during concurrent write"
  exit 1
fi

# Verify total count.
ACTUAL=$($PSQL -t -c "
  SELECT count(*) FROM pg_ripple.find_triples(NULL, '<urn:concurrent_writes:pred>', NULL)
" | tr -d ' ')

echo "  expected=$EXPECTED actual=$ACTUAL"

if [ "$ACTUAL" -ne "$EXPECTED" ]; then
  echo "FAIL: expected $EXPECTED triples but got $ACTUAL"
  exit 1
fi

# Verify no duplicate statement IDs.
DUPES=$($PSQL -t -c "
  SELECT count(*) FROM (
    SELECT i, count(*) AS cnt
    FROM _pg_ripple.vp_rare
    GROUP BY i HAVING count(*) > 1
  ) dups
" | tr -d ' ')

if [ "$DUPES" -gt "0" ]; then
  echo "FAIL: $DUPES duplicate statement IDs found in vp_rare"
  exit 1
fi

echo ""
echo "=== PASS: concurrent_writes ==="
