#!/usr/bin/env bash
# scripts/check_github_actions_pinned.sh
#
# v0.64.0 TRUTH-03: CI lint — reject any GitHub Actions workflow that references
# a third-party action with a mutable ref (tag, branch name, or 'stable').
#
# A mutable ref looks like:
#   uses: actions/checkout@v6
#   uses: dtolnay/rust-toolchain@stable
#   uses: docker/build-push-action@main
#
# A pinned (immutable) ref must be a full 40-character commit SHA:
#   uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd  # v6
#
# Usage:
#   bash scripts/check_github_actions_pinned.sh
#
# Exit code 0 = all actions pinned; non-zero = mutable refs found.

set -euo pipefail

WORKFLOW_DIR=".github/workflows"
FAILURES=0

if [[ ! -d "$WORKFLOW_DIR" ]]; then
    echo "ERROR: $WORKFLOW_DIR directory not found" >&2
    exit 1
fi

# Regex for a mutable action reference.
# Mutable patterns: @vN, @vN.N, @vN.N.N, @stable, @main, @master, @latest,
# @<branch-name>.  Immutable = exactly 40 hex chars after @.
MUTABLE_PATTERN='uses:[[:space:]]+[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+@([^#[:space:]]+)'
SHA_PATTERN='^[0-9a-fA-F]{40}$'

while IFS= read -r -d '' workflow; do
    rel="${workflow#./}"
    lineno=0
    while IFS= read -r line; do
        lineno=$((lineno + 1))
        # Skip comment-only lines
        if [[ "$line" =~ ^[[:space:]]*# ]]; then
            continue
        fi
        if [[ "$line" =~ uses:[[:space:]]+([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@([^#[:space:]]+) ]]; then
            action="${BASH_REMATCH[1]}"
            ref="${BASH_REMATCH[2]}"
            if ! [[ "$ref" =~ ^[0-9a-fA-F]{40}$ ]]; then
                echo "FAIL: $rel:$lineno — $action@$ref is not pinned to a 40-char SHA"
                FAILURES=$((FAILURES + 1))
            fi
        fi
    done < "$workflow"
done < <(find "$WORKFLOW_DIR" -name "*.yml" -print0)

if [[ $FAILURES -gt 0 ]]; then
    echo ""
    echo "Found $FAILURES mutable GitHub Actions ref(s)."
    echo ""
    echo "To fix: replace each mutable ref with the full commit SHA for that tag."
    echo "Example:"
    echo "  uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd  # v6"
    echo ""
    echo "To find the SHA for a tag:"
    echo "  gh api repos/actions/checkout/git/refs/tags/v6 --jq '.object.sha'"
    exit 1
fi

echo "OK: all GitHub Actions refs are pinned to immutable commit SHAs."
exit 0
