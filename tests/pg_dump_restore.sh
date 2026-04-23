#!/usr/bin/env bash
# tests/pg_dump_restore.sh
# v0.51.0: Verify that a pg_ripple database survives a pg_dump/restore cycle.
#
# Requires: a running PostgreSQL 18 instance accessible via PGHOST/PGPORT/PGUSER.
# The test creates a fresh database, loads pg_ripple, inserts sample data,
# dumps it, restores into a new database, and verifies triple counts match.
#
# Usage:
#   cargo pgrx start pg18
#   bash tests/pg_dump_restore.sh
#
# Environment:
#   PGHOST    — socket directory (default: /tmp)
#   PGPORT    — port (default: 28818 for pgrx test instance)
#   PGUSER    — user (default: current user)
#   DUMP_DIR  — directory for the dump file (default: /tmp)

set -euo pipefail

PGHOST="${PGHOST:-/tmp}"
PGPORT="${PGPORT:-28818}"
PGUSER="${PGUSER:-$(whoami)}"
DUMP_DIR="${DUMP_DIR:-/tmp}"
DUMP_FILE="$DUMP_DIR/pg_ripple_dump_restore_test.sql"
SRC_DB="pg_ripple_dump_test_src"
DST_DB="pg_ripple_dump_test_dst"

cleanup() {
    psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
        -c "DROP DATABASE IF EXISTS $SRC_DB;" \
        -c "DROP DATABASE IF EXISTS $DST_DB;" 2>/dev/null || true
    rm -f "$DUMP_FILE"
}
trap cleanup EXIT

echo "=== pg_ripple pg_dump/restore round-trip test ==="

# Create source database.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" postgres \
    -c "DROP DATABASE IF EXISTS $SRC_DB;"
createdb -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$SRC_DB"

# Install extension and load sample data.
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$SRC_DB" <<'SQL'
CREATE EXTENSION pg_ripple CASCADE;
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://schema.org/Person>'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Bob>',
    '<http://schema.org/knows>',
    '<http://example.org/Alice>'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/name>',
    '"Alice"'
);
SQL

# Capture triple count before dump.
SRC_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$SRC_DB" \
    -tAc "SELECT pg_ripple.triple_count();" 2>/dev/null | tr -d ' ')
echo "Source triple count: $SRC_COUNT"

# Dump the source database.
pg_dump -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
    --format=plain --schema-only "$SRC_DB" > "$DUMP_FILE" 2>/dev/null
pg_dump -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
    --format=plain --data-only --exclude-schema=pg_ripple \
    --exclude-schema=_pg_ripple "$SRC_DB" >> "$DUMP_FILE" 2>/dev/null
pg_dump -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
    --format=plain --data-only --schema=_pg_ripple "$SRC_DB" >> "$DUMP_FILE" 2>/dev/null
echo "Dump written to $DUMP_FILE ($(wc -c < "$DUMP_FILE") bytes)"

# Create destination database and restore.
createdb -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$DST_DB"
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$DST_DB" \
    -c "CREATE EXTENSION pg_ripple CASCADE;" 2>/dev/null
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$DST_DB" -f "$DUMP_FILE" 2>/dev/null || true

# Capture triple count after restore.
DST_COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" "$DST_DB" \
    -tAc "SELECT pg_ripple.triple_count();" 2>/dev/null | tr -d ' ')
echo "Destination triple count: $DST_COUNT"

# Compare counts.
if [[ "$SRC_COUNT" != "$DST_COUNT" ]]; then
    echo "FAIL: triple count mismatch — source=$SRC_COUNT, destination=$DST_COUNT"
    exit 1
fi

echo "PASS: pg_dump/restore round-trip preserved all $SRC_COUNT triple(s)."
