#!/usr/bin/env bash
# FLIGHT-STREAM-01 (v0.71.0): Arrow Flight large-export integration test.
#
# Verifies that pg_ripple_http /flight/do_get:
#   1. Returns Transfer-Encoding: chunked (streaming response, not buffered).
#   2. Produces a valid Arrow IPC stream with the correct row count.
#
# Prerequisites:
#   - pg_ripple installed in PostgreSQL and accessible at PG_URL.
#   - pg_ripple_http running at HTTP_URL with ARROW_UNSIGNED_TICKETS_ALLOWED=true.
#   - python3 with pyarrow installed (for IPC validation).
#   - bc, curl, jq available.
#
# Usage:
#   PG_URL=postgres://localhost/postgres \
#   HTTP_URL=http://localhost:7878 \
#   bash tests/http_integration/arrow_export_large.sh
#
# Environment variables:
#   PG_URL              PostgreSQL connection string (default: postgres://localhost/postgres)
#   HTTP_URL            pg_ripple_http base URL     (default: http://localhost:7878)
#   TRIPLE_COUNT        Number of triples to insert  (default: 10000000)
#   EXPECTED_MAX_RSS_MB Maximum allowed RSS in MiB   (default: 512)
#   GRAPH_IRI           Named graph IRI to use       (default: https://flight-test.example/large)
#
# Exit codes:
#   0 — all assertions passed
#   1 — an assertion failed or prerequisite is missing

set -euo pipefail

PG_URL="${PG_URL:-postgres://localhost/postgres}"
HTTP_URL="${HTTP_URL:-http://localhost:7878}"
TRIPLE_COUNT="${TRIPLE_COUNT:-10000000}"
EXPECTED_MAX_RSS_MB="${EXPECTED_MAX_RSS_MB:-512}"
GRAPH_IRI="${GRAPH_IRI:-https://flight-test.example/large}"

echo "=== Arrow Flight large-export integration test ==="
echo "PG_URL          : $PG_URL"
echo "HTTP_URL        : $HTTP_URL"
echo "TRIPLE_COUNT    : $TRIPLE_COUNT"
echo "MAX RSS (MiB)   : $EXPECTED_MAX_RSS_MB"
echo "GRAPH_IRI       : $GRAPH_IRI"
echo

# ── Prerequisites ─────────────────────────────────────────────────────────────

for cmd in curl jq psql python3; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "ERROR: required command '$cmd' not found" >&2
        exit 1
    fi
done

if ! python3 -c "import pyarrow" 2>/dev/null; then
    echo "ERROR: python3 module 'pyarrow' not installed — run: pip install pyarrow" >&2
    exit 1
fi

# ── Step 1: Populate the named graph ─────────────────────────────────────────

echo "Step 1: Inserting $TRIPLE_COUNT triples into <$GRAPH_IRI>..."

psql "$PG_URL" -v ON_ERROR_STOP=1 <<SQL
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;
SELECT create_graph('$GRAPH_IRI');
SQL

# Insert triples in batches of 100,000 to avoid huge single transactions.
BATCH=100000
inserted=0
while [ "$inserted" -lt "$TRIPLE_COUNT" ]; do
    remaining=$(( TRIPLE_COUNT - inserted ))
    batch_size=$(( remaining < BATCH ? remaining : BATCH ))

    psql "$PG_URL" -v ON_ERROR_STOP=1 -c "
        SET search_path TO pg_ripple, public;
        SELECT sparql_update(format(
            'INSERT DATA { GRAPH <%s> { %s } }',
            '$GRAPH_IRI',
            string_agg(
                '<https://flight-test.example/s/' || i || '> <https://schema.org/index> \"' || i || '\" .',
                ' '
            )
        ))
        FROM generate_series($inserted + 1, $inserted + $batch_size) AS i;
    " -q

    inserted=$(( inserted + batch_size ))
    echo "  ... $inserted / $TRIPLE_COUNT triples inserted"
done

# ── Step 2: Get the graph ID and create a signed ticket ───────────────────────

echo "Step 2: Creating Arrow Flight ticket..."

GRAPH_ID=$(psql "$PG_URL" -At -c "
    SELECT id FROM _pg_ripple.dictionary WHERE value = '$GRAPH_IRI' LIMIT 1;
")

if [ -z "$GRAPH_ID" ]; then
    echo "ERROR: graph IRI not found in dictionary" >&2
    exit 1
fi

NOW_SECS=$(date +%s)
EXP_SECS=$(( NOW_SECS + 600 ))

TICKET=$(jq -nc \
    --arg graph_iri "$GRAPH_IRI" \
    --argjson graph_id "$GRAPH_ID" \
    --argjson exp "$EXP_SECS" \
    --argjson iat "$NOW_SECS" \
    '{type:"arrow_flight_v2",aud:"pg_ripple_http",graph_iri:$graph_iri,graph_id:$graph_id,exp:$exp,iat:$iat,sig:"unsigned"}')

echo "  Ticket: $TICKET"

# ── Step 3: Call /flight/do_get and capture response headers ──────────────────

echo "Step 3: Calling $HTTP_URL/flight/do_get..."

TMPFILE=$(mktemp /tmp/arrow_ipc_XXXXXX.ipc)
HEADERFILE=$(mktemp /tmp/arrow_headers_XXXXXX.txt)
trap 'rm -f "$TMPFILE" "$HEADERFILE"' EXIT

HTTP_STATUS=$(curl -s -w "%{http_code}" \
    -X POST "$HTTP_URL/flight/do_get" \
    -H "Content-Type: application/json" \
    -d "$TICKET" \
    -D "$HEADERFILE" \
    -o "$TMPFILE")

if [ "$HTTP_STATUS" != "200" ]; then
    echo "ERROR: HTTP status $HTTP_STATUS (expected 200)" >&2
    cat "$HEADERFILE" >&2
    exit 1
fi

# ── Step 4: Verify Transfer-Encoding: chunked ─────────────────────────────────

echo "Step 4: Verifying Transfer-Encoding: chunked..."

if ! grep -qi "transfer-encoding: chunked" "$HEADERFILE"; then
    echo "ERROR: Response does not use chunked transfer encoding" >&2
    echo "Response headers:" >&2
    cat "$HEADERFILE" >&2
    exit 1
fi
echo "  OK: Transfer-Encoding: chunked confirmed"

# ── Step 5: Validate Arrow IPC and row count ──────────────────────────────────

echo "Step 5: Validating Arrow IPC stream and row count..."

ARROW_ROWS=$(python3 - "$TMPFILE" "$TRIPLE_COUNT" <<'PYEOF'
import sys, pyarrow as pa

ipc_path = sys.argv[1]
expected_rows = int(sys.argv[2])

with open(ipc_path, "rb") as f:
    reader = pa.ipc.open_stream(f)
    total = sum(batch.num_rows for batch in reader)

print(total)
assert total == expected_rows, f"Row count mismatch: got {total}, expected {expected_rows}"
print(f"PASS: Arrow IPC stream has {total} rows (expected {expected_rows})", file=sys.stderr)
PYEOF
)

echo "  OK: Arrow IPC row count = $ARROW_ROWS"

# ── Step 6: Check RSS of pg_ripple_http process ───────────────────────────────

HTTP_PID=$(pgrep -f "pg_ripple_http" 2>/dev/null | head -1 || true)
if [ -n "$HTTP_PID" ]; then
    echo "Step 6: Checking RSS of pg_ripple_http (PID $HTTP_PID)..."
    # ps -o rss outputs RSS in KiB on Linux/macOS
    RSS_KB=$(ps -o rss= -p "$HTTP_PID" 2>/dev/null || echo "0")
    RSS_MB=$(( RSS_KB / 1024 ))
    echo "  RSS = ${RSS_MB} MiB (limit ${EXPECTED_MAX_RSS_MB} MiB)"
    if [ "$RSS_MB" -gt "$EXPECTED_MAX_RSS_MB" ]; then
        echo "ERROR: RSS ${RSS_MB} MiB exceeds limit ${EXPECTED_MAX_RSS_MB} MiB" >&2
        exit 1
    fi
    echo "  OK: RSS within bound"
else
    echo "Step 6: pg_ripple_http PID not found — skipping RSS check"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────

echo "Cleanup: removing test graph..."
psql "$PG_URL" -q -c "
    SET search_path TO pg_ripple, public;
    SELECT clear_graph('$GRAPH_IRI');
    SELECT drop_graph('$GRAPH_IRI');
" 2>/dev/null || true

echo
echo "=== PASS: Arrow Flight large-export test completed successfully ==="
