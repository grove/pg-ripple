#!/usr/bin/env bash
# scripts/check_no_security_definer.sh — v0.84.0
#
# Scan all sql/*.sql migration/setup scripts and Rust source files and fail if any
# SECURITY DEFINER clause is found without an inline SECURITY-JUSTIFY comment.
#
# S13-01 (v0.84.0): The script now requires a `-- SECURITY-JUSTIFY:` marker on
# the SECURITY DEFINER line (or within ±2 lines) for every occurrence. This turns
# the allowlist-based check into a documentation-enforcement gate.
#
# pg_ripple uses SECURITY DEFINER in exactly two places:
#   1. _pg_ripple.ddl_guard_vp_tables()  — DDL event trigger that blocks
#      accidental VP table drops; requires elevated privilege to inspect
#      pg_event_trigger_dropped_objects().
#
# Any SECURITY DEFINER without a SECURITY-JUSTIFY comment is a violation.
#
# Usage:
#   bash scripts/check_no_security_definer.sh
#   # Exit 0 = clean; exit 1 = violations found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SQL_DIR="${ROOT}/sql"
SRC_DIR="${ROOT}/src"

echo "Scanning ${SQL_DIR} and ${SRC_DIR} for SECURITY DEFINER ..."

VIOLATIONS=0

# Check SQL files
check_file() {
    local file="$1"
    # Get line numbers of SECURITY DEFINER hits.
    while IFS= read -r lineno; do
        # Extract ±2 lines of context around the hit.
        context=$(awk "NR>=$((lineno - 2)) && NR<=$((lineno + 2))" "$file")
        if echo "$context" | grep -qi "SECURITY-JUSTIFY"; then
            echo "OK (justified) in $file:$lineno"
        else
            echo "VIOLATION in $file:$lineno — SECURITY DEFINER without SECURITY-JUSTIFY comment:"
            awk "NR==$lineno" "$file"
            echo "  Fix: Add '-- SECURITY-JUSTIFY: <reason>' on or near the SECURITY DEFINER line."
            VIOLATIONS=$(( VIOLATIONS + 1 ))
        fi
    done < <(grep -in "SECURITY[[:space:]]\+DEFINER" "$file" | cut -d: -f1)
}

# Scan SQL files
sql_matches=$(grep -ril "SECURITY[[:space:]]\+DEFINER" "${SQL_DIR}" --include="*.sql" 2>/dev/null || true)
if [[ -n "${sql_matches}" ]]; then
    while IFS= read -r file; do
        check_file "$file"
    done <<< "$sql_matches"
fi

# Scan Rust source files  
rs_matches=$(grep -ril "SECURITY[[:space:]]\+DEFINER" "${SRC_DIR}" --include="*.rs" 2>/dev/null || true)
if [[ -n "${rs_matches}" ]]; then
    while IFS= read -r file; do
        check_file "$file"
    done <<< "$rs_matches"
fi

if [[ -z "${sql_matches}" ]] && [[ -z "${rs_matches}" ]]; then
    echo "OK: no SECURITY DEFINER directives found."
    exit 0
fi

if [[ $VIOLATIONS -gt 0 ]]; then
    echo ""
    echo "ERROR: $VIOLATIONS SECURITY DEFINER usage(s) lack a SECURITY-JUSTIFY comment."
    echo "Add '-- SECURITY-JUSTIFY: <reason>' immediately after each SECURITY DEFINER."
    exit 1
fi

echo "OK: all SECURITY DEFINER occurrences have SECURITY-JUSTIFY annotations."
