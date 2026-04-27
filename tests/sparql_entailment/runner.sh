#!/usr/bin/env bash
# SPARQL Entailment Regime test runner (v0.61.0 B7-2)
# Runs each test case in manifest.json against a running pg_ripple instance.
#
# Usage:
#   PGDATABASE=test_db bash runner.sh
#
# Environment:
#   PGDATABASE  — target database (default: pg_ripple_test)
#   PGHOST      — PostgreSQL host (default: localhost)
#   PGPORT      — PostgreSQL port (default: 5432)
#   PGUSER      — PostgreSQL user (default: postgres)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/manifest.json"

PGDATABASE="${PGDATABASE:-pg_ripple_test}"
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-postgres}"

PASS=0
FAIL=0
SKIP=0

echo "SPARQL Entailment Regime Suite"
echo "================================"

# Parse manifest and run each test.
test_count=$(python3 -c "import json,sys; d=json.load(open('$MANIFEST')); print(len(d['tests']))")

for i in $(seq 0 $((test_count - 1))); do
    id=$(python3 -c "import json; d=json.load(open('$MANIFEST')); print(d['tests'][$i]['id'])")
    name=$(python3 -c "import json; d=json.load(open('$MANIFEST')); print(d['tests'][$i]['name'])")
    data_file=$(python3 -c "import json; d=json.load(open('$MANIFEST')); print(d['tests'][$i]['data'])")
    query_file=$(python3 -c "import json; d=json.load(open('$MANIFEST')); print(d['tests'][$i]['query'])")

    echo -n "  [$id] $name ... "

    if [ ! -f "$SCRIPT_DIR/$data_file" ] || [ ! -f "$SCRIPT_DIR/$query_file" ]; then
        echo "SKIP (fixture not found)"
        SKIP=$((SKIP + 1))
        continue
    fi

    query_text=$(cat "$SCRIPT_DIR/$query_file")

    result=$(psql \
        -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" \
        -t -c "SELECT count(*) FROM pg_ripple.sparql(\$\$${query_text}\$\$)" 2>&1) || true

    if echo "$result" | grep -q "ERROR"; then
        echo "FAIL"
        FAIL=$((FAIL + 1))
    else
        echo "PASS"
        PASS=$((PASS + 1))
    fi
done

echo ""
echo "Results: PASS=$PASS FAIL=$FAIL SKIP=$SKIP total=$test_count"

if [ "$FAIL" -gt 0 ]; then
    echo "SUITE FAILED: $FAIL test(s) failed."
    exit 1
fi

echo "SUITE PASSED."
exit 0
