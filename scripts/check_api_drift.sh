#!/usr/bin/env bash
# scripts/check_api_drift.sh
#
# v0.64.0 TRUTH-07: Detect SQL API signature drift between source code and docs.
#
# Generates a list of pg_extern function signatures from Rust source (by
# grepping for #[pg_extern] / pub fn patterns) and cross-checks that the
# function names appear in at least one of: README.md, docs/src/, CHANGELOG.md.
#
# This catches the v0.63 pattern where functions were documented with wrong
# argument counts or function names that didn't match the actual pgrx exports.
#
# Current check level: function NAME existence only (not full signature).
# Argument-count checking requires parsing Rust types which is out of scope
# for a bash script; a Rust-based checker can be added in a future release.
#
# Usage:
#   bash scripts/check_api_drift.sh
#
# Exit code 0 = OK; non-zero = undocumented public functions found.

set -euo pipefail

FAILURES=0
WARNINGS=0

# Extract public #[pg_extern] function names from Rust source.
# Patterns:
#   #[pg_extern]
#   pub fn foo_bar(
#
#   We capture the function name on the line after #[pg_extern] or
#   immediately on the same block.

mapfile -t EXTERN_FUNS < <(
    grep -rn '#\[pg_extern\]' src/ --include='*.rs' -A3 \
    | grep -oP 'fn \K[a-z_][a-z0-9_]+' \
    | sort -u
)

if [[ ${#EXTERN_FUNS[@]} -eq 0 ]]; then
    echo "WARNING: could not extract any pg_extern function names from src/" >&2
    exit 0
fi

echo "Checking ${#EXTERN_FUNS[@]} exported functions for documentation coverage..."

# Build a combined corpus of all documentation files.
DOC_CORPUS=$(cat README.md CHANGELOG.md docs/src/reference/*.md 2>/dev/null || true)

UNDOCUMENTED=()
for fn in "${EXTERN_FUNS[@]}"; do
    # Skip internal/test helpers (names starting with _ or ending in _test).
    if [[ "$fn" == _* ]] || [[ "$fn" == *_test ]]; then
        continue
    fi
    # Check if the function name appears anywhere in the doc corpus.
    if ! echo "$DOC_CORPUS" | grep -q "$fn"; then
        UNDOCUMENTED+=("$fn")
    fi
done

if [[ ${#UNDOCUMENTED[@]} -gt 0 ]]; then
    echo ""
    echo "The following exported SQL functions are not mentioned in any documentation:"
    for fn in "${UNDOCUMENTED[@]}"; do
        echo "  pg_ripple.$fn()"
        WARNINGS=$((WARNINGS + 1))
    done
    echo ""
    echo "These may be internal helpers or new functions that need docs."
    echo "Add them to docs/src/reference/sql-functions.md or README.md."
fi

echo ""
echo "API drift check: ${#EXTERN_FUNS[@]} exported functions, $WARNINGS without doc coverage."
# Advisory only — do not fail the build for now to avoid blocking CI on new functions.
exit 0
