#!/usr/bin/env bash
# scripts/check_docs_coverage.sh — CI job that verifies documentation
# covers all pg_extern functions.
#
# Diffs exported function signatures in src/lib.rs against the SQL
# Function Reference and fails when a function has no corresponding
# docs/ mention.
#
# Usage: bash scripts/check_docs_coverage.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC_FILE="$PROJECT_DIR/src/lib.rs"
DOCS_DIR="$PROJECT_DIR/docs/src"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

# Extract all pg_extern function names from src/lib.rs
# Looks for: fn function_name(
extract_functions() {
    grep -A1 '#\[pg_extern' "$SRC_FILE" \
        | grep -oP 'fn\s+\K[a-z_][a-z0-9_]*(?=\s*\()' \
        | sort -u
}

# Check if a function is mentioned in docs
check_function_in_docs() {
    local func_name="$1"
    # Search for the function name in docs (as function call or reference)
    grep -rlq "$func_name" "$DOCS_DIR" 2>/dev/null
}

echo "Checking documentation coverage for pg_extern functions..."
echo "============================================================"

MISSING=0
TOTAL=0
FOUND=0
MISSING_LIST=""

while IFS= read -r func; do
    ((TOTAL++))
    if check_function_in_docs "$func"; then
        ((FOUND++))
    else
        ((MISSING++))
        MISSING_LIST+="  - $func"$'\n'
    fi
done < <(extract_functions)

echo ""
echo "Total functions: $TOTAL"
echo "Documented: $FOUND"
echo "Missing: $MISSING"

if [[ $MISSING -gt 0 ]]; then
    echo ""
    echo -e "${RED}The following functions are not mentioned in docs/:${NC}"
    echo "$MISSING_LIST"
    echo ""
    echo "Add documentation for these functions or mention them in the"
    echo "SQL Function Reference (docs/src/reference/sql-functions.md)."
    exit 1
else
    echo ""
    echo -e "${GREEN}All pg_extern functions are documented.${NC}"
    exit 0
fi
