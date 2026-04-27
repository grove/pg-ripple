#!/usr/bin/env bash
# scripts/check_no_security_definer.sh — v0.60.0
#
# Scan all sql/*.sql migration/setup scripts and fail if any SECURITY DEFINER
# clause is found outside the known, intentional allowlist.
#
# pg_ripple uses SECURITY DEFINER in exactly two places:
#   1. _pg_ripple.ddl_guard_vp_tables()  — DDL event trigger that blocks
#      accidental VP table drops; requires elevated privilege to inspect
#      pg_event_trigger_dropped_objects().
#
# Any other SECURITY DEFINER is a privilege-escalation risk and must be
# reviewed before landing.
#
# Usage:
#   bash scripts/check_no_security_definer.sh
#   # Exit 0 = clean; exit 1 = violations found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SQL_DIR="${ROOT}/sql"

# Allowlisted function names whose SECURITY DEFINER is intentional.
# Each entry is matched against the line that precedes the SECURITY DEFINER
# keyword (or the nearby function declaration).
ALLOWLIST=(
    "ddl_guard_vp_tables"
)

echo "Scanning ${SQL_DIR} for SECURITY DEFINER ..."

# Collect files that contain SECURITY DEFINER (case-insensitive).
matches=$(grep -ri "SECURITY[[:space:]]\+DEFINER" "${SQL_DIR}" --include="*.sql" -l 2>/dev/null || true)

if [[ -z "${matches}" ]]; then
    echo "OK: no SECURITY DEFINER directives found."
    exit 0
fi

# For each matching file, verify every SECURITY DEFINER occurrence is in the
# allowlist.  Extract a small context window around each hit and check whether
# any allowlisted function name appears nearby (within ±10 lines).
VIOLATIONS=0
while IFS= read -r file; do
    # Get line numbers of SECURITY DEFINER hits.
    while IFS= read -r lineno; do
        # Extract ±10 lines of context around the hit.
        context=$(awk "NR>=$((lineno - 10)) && NR<=$((lineno + 10))" "$file")
        allowed=0
        for fn_name in "${ALLOWLIST[@]}"; do
            if echo "$context" | grep -qi "$fn_name"; then
                allowed=1
                break
            fi
        done
        if [[ $allowed -eq 0 ]]; then
            echo "VIOLATION in $file:$lineno — SECURITY DEFINER outside allowlist:"
            awk "NR==$lineno" "$file"
            VIOLATIONS=$(( VIOLATIONS + 1 ))
        else
            echo "OK (allowlisted) in $file:$lineno"
        fi
    done < <(grep -in "SECURITY[[:space:]]\+DEFINER" "$file" | cut -d: -f1)
done <<< "$matches"

if [[ $VIOLATIONS -gt 0 ]]; then
    echo ""
    echo "ERROR: $VIOLATIONS unapproved SECURITY DEFINER usage(s) found."
    echo "Add to the allowlist in scripts/check_no_security_definer.sh only"
    echo "after security review by grove/pg-ripple-maintainers."
    exit 1
fi

echo "OK: all SECURITY DEFINER usage is in the allowlist."
