#!/usr/bin/env bash
# tests/integration/v1_readiness/regress_mismatch_audit.sh
# pg_ripple v0.58.0 — v1 readiness: pg_regress mismatch audit
#
# Runs the pg_regress suite and reports any test mismatches or failures
# in a structured format.  Exits 0 only if all regression tests pass.
#
# Usage: bash tests/integration/v1_readiness/regress_mismatch_audit.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"

echo "=== pg_ripple v1 readiness: pg_regress mismatch audit ==="
echo "  repo: $REPO_ROOT"

cd "$REPO_ROOT"

# Run the pg_regress suite.
echo "[1/2] Running pg_regress suite..."
if cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on" 2>&1; then
  echo "  [OK] pg_regress suite passed"
else
  REGRESS_EXIT=$?
  echo ""
  echo "[2/2] Regression diff (if any):"
  if [ -f tests/pg_regress/regression.diffs ]; then
    cat tests/pg_regress/regression.diffs
  fi
  echo ""
  echo "FAIL: pg_regress suite exited with code $REGRESS_EXIT"
  exit $REGRESS_EXIT
fi

# Check for leftover diffs file (would indicate partial failures).
if [ -f tests/pg_regress/regression.diffs ] && [ -s tests/pg_regress/regression.diffs ]; then
  echo "FAIL: regression.diffs is non-empty after reported pass"
  cat tests/pg_regress/regression.diffs
  exit 1
fi

echo ""
echo "=== PASS: regress_mismatch_audit ==="
