#!/usr/bin/env bash
# scripts/check_unsafe_safety.sh
# Advisory: check that all unsafe blocks have // SAFETY: comments.
#
# The AGENTS.md convention requires a // SAFETY: comment above every unsafe {}
# block explaining the invariant that makes the code sound.  This script flags
# violations so they can be fixed incrementally.
#
# Exit code: 0 always (advisory only, non-blocking in CI).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FAIL=0
CHECKED=0
MISSING=0

while IFS= read -r -d '' file; do
    prev=""
    lineno=0
    while IFS= read -r line; do
        lineno=$((lineno+1))
        if echo "$line" | grep -qP '^\s*unsafe\s*\{' && \
           ! echo "$prev" | grep -qP '//\s*SAFETY:'; then
            echo "MISSING SAFETY: $file:$lineno: $line"
            MISSING=$((MISSING+1))
            FAIL=1
        fi
        CHECKED=$((CHECKED+1))
        prev="$line"
    done < "$file"
done < <(find "$REPO_ROOT/src" -name "*.rs" -print0)

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "Advisory: $MISSING unsafe block(s) lack // SAFETY: comments."
    echo "Add a // SAFETY: comment above each unsafe {} block explaining the invariant."
fi
echo "check_unsafe_safety done (checked $CHECKED lines; $MISSING blocks missing SAFETY)."
exit 0  # advisory only, non-blocking
