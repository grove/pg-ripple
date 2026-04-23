#!/usr/bin/env bash
# scripts/check_migration_headers.sh
# v0.51.0: Verify that every SQL migration script has a proper header comment.
#
# Each `sql/pg_ripple--X.Y.Z--A.B.C.sql` file must start with a comment line:
#   -- Migration X.Y.Z → A.B.C: <description>
#
# Usage: bash scripts/check_migration_headers.sh
# Exit code: 0 = all headers present, 1 = missing or malformed header.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SQL_DIR="$REPO_ROOT/sql"

FAIL=0

for f in "$SQL_DIR"/pg_ripple--*--*.sql; do
    basename="$(basename "$f")"
    # Extract FROM and TO versions from the filename.
    from_ver="$(echo "$basename" | sed -E 's/pg_ripple--([^-]+)--([^.]+)\.sql/\1/')"
    to_ver="$(echo "$basename" | sed -E 's/pg_ripple--([^-]+)--([^.]+)\.sql/\2/')"

    # The first non-empty line of the file must be a comment starting with
    # "-- Migration " and containing the from and to versions.
    first_comment="$(grep -m1 '.' "$f" | head -1)"
    if ! echo "$first_comment" | grep -q "-- Migration ${from_ver}"; then
        echo "MISSING HEADER: $basename"
        echo "  Expected first line: -- Migration ${from_ver} → ${to_ver}: <description>"
        echo "  Got: $first_comment"
        FAIL=1
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "ERROR: Some migration scripts are missing proper header comments."
    exit 1
fi

echo "OK: all migration scripts have proper headers."
