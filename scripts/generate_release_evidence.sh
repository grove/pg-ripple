#!/usr/bin/env bash
# scripts/generate_release_evidence.sh
#
# v0.64.0 TRUTH-09: Generate a release evidence dashboard artifact.
#
# Produces:
#   target/release-evidence/<version>/summary.json
#   target/release-evidence/<version>/summary.md
#
# The artifact contains:
#   - migration chain result
#   - GitHub Actions pinning result
#   - SECURITY DEFINER lint result
#   - API drift lint result
#   - docs/roadmap evidence result
#   - feature-status smoke test (if PostgreSQL is available)
#
# Usage:
#   bash scripts/generate_release_evidence.sh <version>
#
# Example:
#   bash scripts/generate_release_evidence.sh 0.64.0

set -euo pipefail

VERSION="${1:-unknown}"
OUT_DIR="target/release-evidence/${VERSION}"
mkdir -p "$OUT_DIR"

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
RESULTS=()

run_check() {
    local name="$1"
    local cmd="$2"
    local result
    if result=$(eval "$cmd" 2>&1); then
        echo "  PASS: $name"
        RESULTS+=("{\"check\":\"$name\",\"status\":\"pass\",\"detail\":\"\"}")
    else
        echo "  FAIL: $name"
        # Truncate detail to 200 chars for JSON.
        local detail
        detail=$(echo "$result" | head -5 | tr -d '"' | tr '\n' ' ' | cut -c1-200)
        RESULTS+=("{\"check\":\"$name\",\"status\":\"fail\",\"detail\":\"$detail\"}")
    fi
}

echo "=== pg_ripple release evidence: v${VERSION} ==="
echo "Timestamp: $TIMESTAMP"
echo ""
echo "Running checks..."

run_check "github_actions_pinning" "bash scripts/check_github_actions_pinned.sh"
run_check "security_definer_lint" "bash scripts/check_no_security_definer.sh"
run_check "api_drift_check" "bash scripts/check_api_drift.sh"
run_check "roadmap_evidence_check" "bash scripts/check_roadmap_evidence.sh"
run_check "migration_headers_lint" "bash scripts/check_migration_headers.sh"

# Check if Cargo.toml version matches pg_ripple.control.
CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | grep -oP '"\K[^"]+')
CONTROL_VER=$(grep '^default_version' pg_ripple.control | grep -oP "'\K[^']+")
if [[ "$CARGO_VER" == "$CONTROL_VER" ]]; then
    echo "  PASS: version_sync (Cargo.toml=$CARGO_VER, control=$CONTROL_VER)"
    RESULTS+=("{\"check\":\"version_sync\",\"status\":\"pass\",\"detail\":\"$CARGO_VER\"}")
else
    echo "  FAIL: version_sync (Cargo.toml=$CARGO_VER != control=$CONTROL_VER)"
    RESULTS+=("{\"check\":\"version_sync\",\"status\":\"fail\",\"detail\":\"Cargo.toml=$CARGO_VER control=$CONTROL_VER\"}")
fi

# Check that the migration script for this version exists.
PREV_VER=""
if [[ "$VERSION" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    MAJOR="${BASH_REMATCH[1]}"
    MINOR="${BASH_REMATCH[2]}"
    PATCH="${BASH_REMATCH[3]}"
    if [[ $MINOR -gt 0 ]]; then
        PREV_MINOR=$((MINOR - 1))
        PREV_VER="${MAJOR}.${PREV_MINOR}.${PATCH}"
    fi
fi
if [[ -n "$PREV_VER" ]] && [[ -f "sql/pg_ripple--${PREV_VER}--${VERSION}.sql" ]]; then
    echo "  PASS: migration_script (sql/pg_ripple--${PREV_VER}--${VERSION}.sql exists)"
    RESULTS+=("{\"check\":\"migration_script\",\"status\":\"pass\",\"detail\":\"${PREV_VER} -> ${VERSION}\"}")
elif [[ -n "$PREV_VER" ]]; then
    echo "  FAIL: migration_script (sql/pg_ripple--${PREV_VER}--${VERSION}.sql missing)"
    RESULTS+=("{\"check\":\"migration_script\",\"status\":\"fail\",\"detail\":\"${PREV_VER} -> ${VERSION} migration missing\"}")
fi

# Check changelog entry exists for this version.
if grep -q "## \[${VERSION}\]" CHANGELOG.md; then
    echo "  PASS: changelog_entry (v${VERSION} found)"
    RESULTS+=("{\"check\":\"changelog_entry\",\"status\":\"pass\",\"detail\":\"\"}")
else
    echo "  FAIL: changelog_entry (v${VERSION} not found in CHANGELOG.md)"
    RESULTS+=("{\"check\":\"changelog_entry\",\"status\":\"fail\",\"detail\":\"\"}")
fi

# Build JSON summary.
RESULTS_JSON=$(printf '%s\n' "${RESULTS[@]}" | paste -sd ',' | sed 's/^/[/' | sed 's/$/]/')
PASS_COUNT=$(printf '%s\n' "${RESULTS[@]}" | grep -c '"pass"' || echo 0)
FAIL_COUNT=$(printf '%s\n' "${RESULTS[@]}" | grep -c '"fail"' || echo 0)
OVERALL=$([ "$FAIL_COUNT" -eq 0 ] && echo "pass" || echo "fail")

cat > "$OUT_DIR/summary.json" <<EOF
{
  "version": "$VERSION",
  "timestamp": "$TIMESTAMP",
  "overall": "$OVERALL",
  "pass_count": $PASS_COUNT,
  "fail_count": $FAIL_COUNT,
  "checks": $RESULTS_JSON
}
EOF

# Build Markdown summary.
cat > "$OUT_DIR/summary.md" <<EOF
# Release Evidence: v${VERSION}

Generated: ${TIMESTAMP}  
Overall: **${OVERALL}** (${PASS_COUNT} pass, ${FAIL_COUNT} fail)

## Checks

| Check | Status |
|-------|--------|
EOF

for result in "${RESULTS[@]}"; do
    check=$(echo "$result" | grep -oP '"check":"\K[^"]+')
    status=$(echo "$result" | grep -oP '"status":"\K[^"]+')
    icon=$([ "$status" = "pass" ] && echo "✅" || echo "❌")
    echo "| $check | $icon $status |" >> "$OUT_DIR/summary.md"
done

echo "" >> "$OUT_DIR/summary.md"
echo "See \`summary.json\` for full detail." >> "$OUT_DIR/summary.md"

echo ""
echo "Evidence dashboard written to: $OUT_DIR/"
echo "  summary.json"
echo "  summary.md"
echo ""
echo "Overall: $OVERALL ($PASS_COUNT pass, $FAIL_COUNT fail)"
