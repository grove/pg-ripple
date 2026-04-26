#!/usr/bin/env bash
# tests/integration/v1_readiness/run_all.sh
# pg_ripple v0.58.0 — Run all v1 readiness integration tests
#
# Usage: bash tests/integration/v1_readiness/run_all.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASSED=0
FAILED=0
RESULTS=()

run_test() {
  local name="$1"
  local script="$SCRIPT_DIR/$2"
  echo ""
  echo "────────────────────────────────────────────"
  if bash "$script"; then
    PASSED=$(( PASSED + 1 ))
    RESULTS+=("PASS: $name")
  else
    FAILED=$(( FAILED + 1 ))
    RESULTS+=("FAIL: $name")
  fi
}

echo "════════════════════════════════════════════"
echo "  pg_ripple v0.58.0 — v1 readiness suite"
echo "════════════════════════════════════════════"

run_test "crash_recovery"        "crash_recovery.sh"
run_test "concurrent_writes"     "concurrent_writes.sh"
run_test "upgrade_chain"         "upgrade_chain.sh"
run_test "regress_mismatch_audit" "regress_mismatch_audit.sh"

echo ""
echo "════════════════════════════════════════════"
echo "  Results: $PASSED passed, $FAILED failed"
echo "════════════════════════════════════════════"
for result in "${RESULTS[@]}"; do
  echo "  $result"
done

if [ "$FAILED" -gt 0 ]; then
  exit 1
fi
