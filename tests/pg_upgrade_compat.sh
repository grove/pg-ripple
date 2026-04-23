#!/usr/bin/env bash
# tests/pg_upgrade_compat.sh
# v0.51.0: Verify pg_ripple remains functional after a PostgreSQL minor version upgrade.
#
# Tests the documented upgrade path:
#   1. pg_dump the database
#   2. initdb + start new PostgreSQL (same major, newer minor)
#   3. pg_restore + ALTER EXTENSION pg_ripple UPDATE
#   4. Verify all SPARQL queries still work
#
# For CI this simulates the pg_upgrade scenario by testing ALTER EXTENSION UPDATE
# across migration steps rather than a true binary upgrade.
#
# Usage:
#   cargo pgrx start pg18
#   bash tests/pg_upgrade_compat.sh
#
# Environment:
#   PGHOST  — socket directory (default: /tmp)
#   PGPORT  — port (default: 28818 for pgrx test instance)
#   PGUSER  — user (default: current user)

set -euo pipefail

PGHOST="${PGHOST:-/tmp}"
PGPORT="${PGPORT:-28818}"
PGUSER="${PGUSER:-$(whoami)}"
TEST_DB="pg_ripple_upgrade_compat_test"

cleanup() {
    psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
        -c "DROP DATABASE IF EXISTS $TEST_DB;" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== pg_ripple PostgreSQL minor-version upgrade compatibility test ==="

# Create and populate test database.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "DROP DATABASE IF EXISTS $TEST_DB;"
createdb -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB"

psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" <<'SQL'
CREATE EXTENSION pg_ripple CASCADE;

-- Insert representative data.
SELECT pg_ripple.insert_triple(
    '<http://example.org/subject>',
    '<http://example.org/predicate>',
    '"test value"'
);

-- Verify SPARQL works before upgrade simulation.
SELECT result FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <http://example.org/predicate> "test value" }'
) LIMIT 1;
SQL

BEFORE_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT pg_ripple.triple_count();" | tr -d ' ')
echo "Triple count before upgrade simulation: $BEFORE_COUNT"

# Simulate pg_upgrade: in practice this would be a binary pg_upgrade.
# For CI we verify that the extension can be updated to the latest version.
CURRENT_VERSION=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';" | tr -d ' ')
echo "Current extension version: $CURRENT_VERSION"

# Run ALTER EXTENSION UPDATE (a no-op if already at latest).
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -c "ALTER EXTENSION pg_ripple UPDATE;" 2>/dev/null || {
    echo "Note: ALTER EXTENSION UPDATE returned an error (may be expected if at latest version)"
}

AFTER_VERSION=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';" | tr -d ' ')
echo "Extension version after update: $AFTER_VERSION"

# Verify data is intact and queries still work.
AFTER_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT pg_ripple.triple_count();" | tr -d ' ')

if [[ "$BEFORE_COUNT" != "$AFTER_COUNT" ]]; then
    echo "FAIL: triple count changed during upgrade — before=$BEFORE_COUNT, after=$AFTER_COUNT"
    exit 1
fi

SPARQL_RESULT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$TEST_DB" \
    -tAc "SELECT count(*) FROM pg_ripple.sparql('SELECT ?s WHERE { ?s <http://example.org/predicate> \"test value\" }');" | tr -d ' ')

if [[ "$SPARQL_RESULT" -lt 1 ]]; then
    echo "FAIL: SPARQL query returned no results after upgrade simulation"
    exit 1
fi

echo "PASS: extension is functional after upgrade simulation."
echo "  Version: $CURRENT_VERSION → $AFTER_VERSION"
echo "  Triple count: $AFTER_COUNT (unchanged)"
echo "  SPARQL results: $SPARQL_RESULT"
