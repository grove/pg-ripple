#!/usr/bin/env bash
# scripts/check_no_string_format_in_sql.sh
# v0.51.0: Lint for unsafe dynamic SQL patterns in Rust source.
#
# Searches for `format!("...{...}...SQL...` patterns that could indicate
# table-name injection.  The pattern "format!(..." near "SELECT|INSERT|UPDATE|DELETE|CREATE"
# is flagged; callers should always look up OIDs in _pg_ripple.predicates
# and use $N bind parameters for values.
#
# Usage: bash scripts/check_no_string_format_in_sql.sh
# Exit code: 0 = clean, 1 = suspicious patterns found.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_DIR="$REPO_ROOT/src"

# Pattern: format! that contains SQL DML keywords and a `{}` placeholder.
# We allow format! for table name construction when it uses a numeric pred_id
# (safe: pred_id is always i64, never user-supplied text).
SUSPICIOUS=0

while IFS= read -r match; do
    # Allow patterns that only interpolate integer pred_id or graph_id.
    if echo "$match" | grep -qE 'vp_\{pred_id\}|vp_\{p_id\}|vp_\{pred\}|vp_\{id\}|g = \{graph_id\}|g = \{g_id\}'; then
        continue
    fi
    echo "SUSPICIOUS: $match"
    SUSPICIOUS=1
done < <(grep -rn 'format!.*\{.*\}.*\(SELECT\|INSERT\|UPDATE\|DELETE\|CREATE\)' "$SRC_DIR" || true)

if [[ $SUSPICIOUS -ne 0 ]]; then
    echo ""
    echo "ERROR: Found potentially unsafe dynamic SQL construction."
    echo "Use \$N bind parameters for user-supplied values."
    echo "For table names, always look up OID in _pg_ripple.predicates."
    exit 1
fi

echo "OK: no suspicious dynamic SQL patterns found."
