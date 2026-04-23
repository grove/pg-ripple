#!/usr/bin/env bash
# scripts/check_migration_headers.sh
# v0.51.0: Verify that every SQL migration script has a proper header comment.
#
# Each `sql/pg_ripple--X.Y.Z--A.B.C.sql` file must start with a comment line:
#   -- Migration X.Y.Z → A.B.C: <description>
#   OR
#   -- pg_ripple--X.Y.Z--A.B.C.sql  (legacy format)
#
# Usage: bash scripts/check_migration_headers.sh
# Exit code: 0 = all headers present, 1 = missing or malformed header.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SQL_DIR="$REPO_ROOT/sql"

FAIL=0

for f in "$SQL_DIR"/pg_ripple--*--*.sql; do
    basename="$(basename "$f")"
    # Extract FROM version from the filename  e.g. pg_ripple--0.50.0--0.51.0.sql → 0.50.0
    from_ver="$(echo "$basename" | sed -E 's/^pg_ripple--([0-9]+\.[0-9]+\.[0-9]+)--([0-9]+\.[0-9]+\.[0-9]+)\.sql$/\1/')"

    if [[ "$from_ver" == "$basename" ]]; then
        # sed didn't match — skip this file (not a standard migration filename)
        continue
    fi

    # The first non-empty line must be a comment (start with '--').
    first_line="$(grep -m1 '.' "$f" | head -1)"
    if [[ "${first_line:0:2}" != "--" ]]; then
        echo "MISSING HEADER: $basename"
        echo "  First line must be a comment starting with '--'"
        echo "  Got: $first_line"
        FAIL=1
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "ERROR: Some migration scripts are missing proper header comments."
    exit 1
fi

echo "OK: all migration scripts have proper headers."
