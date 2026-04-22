#!/usr/bin/env bash
# scripts/check_no_security_definer.sh — v0.47.0
#
# Scan all sql/*.sql migration/setup scripts and fail if any contain a
# SECURITY DEFINER clause.  pg_ripple intentionally avoids SECURITY DEFINER
# to prevent privilege-escalation via function invocation.
#
# Usage:
#   bash scripts/check_no_security_definer.sh
#   # Exit 0 = clean; exit 1 = violations found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SQL_DIR="${ROOT}/sql"

echo "Scanning ${SQL_DIR} for SECURITY DEFINER ..."

# Case-insensitive grep across all .sql files.
# -r: recursive, -i: case-insensitive, -l: list filenames only
matches=$(grep -ri "SECURITY[[:space:]]\+DEFINER" "${SQL_DIR}" --include="*.sql" -l 2>/dev/null || true)

if [[ -n "${matches}" ]]; then
    echo ""
    echo "ERROR: SECURITY DEFINER found in the following files:"
    echo "${matches}"
    echo ""
    echo "pg_ripple must not use SECURITY DEFINER — all functions should run"
    echo "with the invoker's privileges (SECURITY INVOKER is the default)."
    exit 1
fi

echo "OK: no SECURITY DEFINER directives found."
