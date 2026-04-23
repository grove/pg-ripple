#!/usr/bin/env bash
# scripts/check_pt_codes.sh
# v0.51.0: Verify that all PT error codes in Rust source have documentation.
#
# PT codes are pg_ripple-specific error codes of the form PT\d{3} used in
# pgrx::error!() calls.  This script:
#   1. Extracts all PT codes from src/**/*.rs
#   2. Checks that each code is mentioned in docs/src/ or CHANGELOG.md
#
# Usage: bash scripts/check_pt_codes.sh
# Exit code: 0 = all codes documented, 1 = undocumented codes found.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_DIR="$REPO_ROOT/src"
DOCS_DIR="$REPO_ROOT/docs/src"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"

# Extract unique PT codes from Rust source.
mapfile -t PT_CODES < <(
    grep -rho 'PT[0-9]\{3\}' "$SRC_DIR" 2>/dev/null | sort -u
)

if [[ ${#PT_CODES[@]} -eq 0 ]]; then
    echo "OK: no PT error codes found in source."
    exit 0
fi

FAIL=0

for code in "${PT_CODES[@]}"; do
    # Check if the code appears anywhere in docs or CHANGELOG.
    if ! grep -rq "$code" "$DOCS_DIR" 2>/dev/null && \
       ! grep -q "$code" "$CHANGELOG" 2>/dev/null; then
        echo "UNDOCUMENTED: $code — referenced in src/ but not in docs/ or CHANGELOG.md"
        FAIL=1
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "ERROR: Some PT error codes lack documentation."
    echo "Add each code to docs/src/ or CHANGELOG.md with a description."
    exit 1
fi

echo "OK: all ${#PT_CODES[@]} PT code(s) are documented."
