#!/usr/bin/env bash
# tests/pg_dump_restore.sh
# v0.51.0: Verify that a pg_ripple database survives a pg_dump/restore cycle.
#
# Requires: a running PostgreSQL 18 instance accessible via PGHOST/PGPORT/PGUSER.
# The test creates fresh databases, loads pg_ripple, inserts sample data,
# performs a strict custom-format dump/restore, and verifies counts and health.
#
# Usage:
#   cargo pgrx start pg18
#   bash tests/pg_dump_restore.sh
#
# Environment:
#   PGHOST    — socket directory (default: /tmp)
#   PGPORT    — port (default: 28818 for pgrx test instance)
#   PGUSER    — user (default: current user)
#   DUMP_DIR  — directory for the dump file (default: a private temp directory)

set -euo pipefail

PGHOST="${PGHOST:-/tmp}"
PGPORT="${PGPORT:-28818}"
PGUSER="${PGUSER:-$(whoami)}"
RUN_ID="${PG_RIPPLE_RESILIENCE_RUN_ID:-$$}"
[[ "$RUN_ID" =~ ^[a-zA-Z0-9_]+$ ]] || {
    echo "FAIL: PG_RIPPLE_RESILIENCE_RUN_ID must contain only letters, digits, and underscores" >&2
    exit 1
}
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-logical.XXXXXX")"
DUMP_DIR="${DUMP_DIR:-$WORKDIR}"
[[ -d "$DUMP_DIR" ]] || { echo "FAIL: DUMP_DIR does not exist: $DUMP_DIR" >&2; exit 1; }
DUMP_FILE="$DUMP_DIR/pg_ripple_dump_restore_${RUN_ID}.dump"
SRC_DB="pg_ripple_dump_test_src_${RUN_ID}"
DST_DB="pg_ripple_dump_test_dst_${RUN_ID}"
PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER")
DB_CREATED=0
RESTORE_CREATED=0

cleanup_status=0
cleanup() {
    if ! "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"$SRC_DB\";" >/dev/null ||
        ! "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"$DST_DB\";" >/dev/null; then
        echo "CLEANUP: failed to drop logical-restore test databases" >&2
        cleanup_status=1
    fi
    if ! rm -f -- "$DUMP_FILE"; then
        echo "CLEANUP: failed to remove $DUMP_FILE" >&2
        cleanup_status=1
    fi
    if ! rm -rf -- "$WORKDIR"; then
        echo "CLEANUP: failed to remove $WORKDIR" >&2
        cleanup_status=1
    fi
}
finish() {
    local status=$?
    trap - EXIT
    cleanup
    if (( status == 0 && cleanup_status != 0 )); then status=1; fi
    exit "$status"
}
trap finish EXIT

echo "=== pg_ripple pg_dump/restore round-trip test ==="

# Create source database.
"${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"$SRC_DB\";" >/dev/null
"${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"$DST_DB\";" >/dev/null
"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"$SRC_DB\";" >/dev/null
DB_CREATED=1

# Install extension and load sample data.
"${PSQL[@]}" -d "$SRC_DB" <<'SQL'
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
SRC_COUNT=$("${PSQL[@]}" -d "$SRC_DB" \
    -tAc "SELECT pg_ripple.triple_count();" 2>/dev/null | tr -d ' ')
echo "Source triple count: $SRC_COUNT"

# Dump the complete database in one standard, restorable archive.
pg_dump -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
    --format=custom --file="$DUMP_FILE" "$SRC_DB"
echo "Dump written to $DUMP_FILE ($(wc -c < "$DUMP_FILE") bytes)"

# Create destination database and restore.
"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"$DST_DB\";" >/dev/null
RESTORE_CREATED=1
pg_restore --exit-on-error --no-owner -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
    -d "$DST_DB" "$DUMP_FILE"

# Capture triple count after restore.
DST_COUNT=$("${PSQL[@]}" -d "$DST_DB" \
    -tAc "SELECT pg_ripple.triple_count();" 2>/dev/null | tr -d ' ')
echo "Destination triple count: $DST_COUNT"

# Compare counts.
if [[ "$SRC_COUNT" != "$DST_COUNT" ]]; then
    echo "FAIL: triple count mismatch — source=$SRC_COUNT, destination=$DST_COUNT"
    exit 1
fi

"${PSQL[@]}" -d "$DST_DB" -Atqc "SELECT pg_ripple.health() IS NOT NULL" | grep -Fxq t || {
    echo "FAIL: restored database failed pg_ripple.health()" >&2
    exit 1
}

echo "PASS: pg_dump/restore round-trip preserved all $SRC_COUNT triple(s)."
