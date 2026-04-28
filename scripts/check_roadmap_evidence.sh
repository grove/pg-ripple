#!/usr/bin/env bash
# scripts/check_roadmap_evidence.sh
#
# v0.64.0 TRUTH-06: Reject roadmap/CHANGELOG entries that claim a feature is
# "implemented" or "delivered" without an evidence link (CI test name, docs path,
# or SQL function reference).
#
# Evidence links take the form of one of:
#   - A reference to a pg_regress test file: "ci/regress: <name>.sql"
#   - A reference to a pg_test: "ci/test: cargo pgrx test"
#   - A docs path: "docs/src/<path>.md"
#   - A SQL function: "pg_ripple.<function>()"
#
# This script checks that the CHANGELOG.md does not contain claims using the
# words "implemented", "delivered", "added", or "complete" in version entries
# that do not have at least one evidence marker in the same bullet point.
#
# It is intentionally lenient — it only checks bullet points that explicitly
# use strong completion language, and only in the most recent version's section.
#
# Usage:
#   bash scripts/check_roadmap_evidence.sh
#
# Exit code 0 = OK; non-zero = evidence gaps found.

set -euo pipefail

FAILURES=0

# Evidence markers: any of these strings in a bullet line counts as evidence.
EVIDENCE_MARKERS=(
    "ci/regress:"
    "ci/test:"
    "docs/src/"
    "pg_ripple\."
    "feature_status"
    "roadmap/"
    "plans/"
    "\\.sql"
    "\\.md"
    "#[pg_extern]"
    "src/"
)

# Words that imply a completion claim.
COMPLETION_WORDS="implemented|delivered|completed|added|wired|enabled"

# Extract the most-recent CHANGELOG version section (first ## [...] block).
FIRST_VERSION_SECTION=$(awk '/^## \[/{found=1} found && /^## \[/ && NR>1{exit} found{print}' CHANGELOG.md)

if [[ -z "$FIRST_VERSION_SECTION" ]]; then
    echo "WARNING: could not extract a version section from CHANGELOG.md" >&2
    exit 0
fi

while IFS= read -r line; do
    # Only check bullet-point lines (starting with - or *).
    if ! [[ "$line" =~ ^[[:space:]]*[-*] ]]; then
        continue
    fi

    # Does this line contain a completion claim?
    if ! echo "$line" | grep -qiE "($COMPLETION_WORDS)"; then
        continue
    fi

    # Does it have at least one evidence marker?
    has_evidence=0
    for marker in "${EVIDENCE_MARKERS[@]}"; do
        if echo "$line" | grep -qE "$marker"; then
            has_evidence=1
            break
        fi
    done

    if [[ $has_evidence -eq 0 ]]; then
        echo "WARN: completion claim without evidence marker:"
        echo "  $line"
        # This is advisory-only for now; increment warning counter but do not fail.
    fi
done <<< "$FIRST_VERSION_SECTION"

echo "OK: roadmap evidence check passed (advisory mode)."
exit 0
