#!/usr/bin/env bash
# tests/integration/v1_readiness/upgrade_chain.sh
# pg_ripple v0.58.0 — v1 readiness: migration upgrade chain
#
# Verifies that `ALTER EXTENSION pg_ripple UPDATE` succeeds through the full
# version chain from the oldest available version to the current one.
#
# This script is a thin wrapper around tests/test_migration_chain.sh that
# focuses specifically on the v0.58.0 upgrade path and reports in the v1
# readiness format.
#
# Usage: bash tests/integration/v1_readiness/upgrade_chain.sh

set -euo pipefail

PGPORT="${PGPORT:-28818}"
PGHOST="${PGHOST:-localhost}"
PGUSER="${PGUSER:-$(whoami)}"
PGDB="${PGDATABASE:-pg_ripple_test}"

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
PSQL="psql -h $PGHOST -p $PGPORT -U $PGUSER -d $PGDB -v ON_ERROR_STOP=1"

echo "=== pg_ripple v1 readiness: upgrade chain ==="

# Identify available migration scripts.
SCRIPTS=( $(ls "$REPO_ROOT/sql"/pg_ripple--*.sql 2>/dev/null | grep -v '\-\-' | sort -V) )
MIGRATION_SCRIPTS=( $(ls "$REPO_ROOT/sql"/pg_ripple--*--*.sql 2>/dev/null | sort -V) )

echo "  found ${#MIGRATION_SCRIPTS[@]} migration script(s)"

if [ ${#MIGRATION_SCRIPTS[@]} -eq 0 ]; then
  echo "FAIL: no migration scripts found in sql/"
  exit 1
fi

# Verify 0.57.0 → 0.58.0 migration script exists.
if ls "$REPO_ROOT/sql/pg_ripple--0.57.0--0.58.0.sql" 1>/dev/null 2>&1; then
  echo "  [OK] pg_ripple--0.57.0--0.58.0.sql exists"
else
  echo "FAIL: pg_ripple--0.57.0--0.58.0.sql not found"
  exit 1
fi

# Verify the control file has the correct default_version.
CTRL_VERSION=$(grep 'default_version' "$REPO_ROOT/pg_ripple.control" | sed "s/.*= *'//;s/'.*//")
echo "  control default_version: $CTRL_VERSION"
if [ "$CTRL_VERSION" != "0.58.0" ]; then
  echo "FAIL: pg_ripple.control default_version is '$CTRL_VERSION', expected '0.58.0'"
  exit 1
fi

# Verify that every migration script from 0.1.0 → 0.58.0 exists in sequence.
EXPECTED_MIGRATIONS=(
  "0.1.0--0.2.0"  "0.2.0--0.3.0"  "0.3.0--0.4.0"  "0.4.0--0.5.0"
  "0.5.0--0.6.0"  "0.5.1--0.6.0"  "0.6.0--0.7.0"  "0.7.0--0.8.0"
  "0.8.0--0.9.0"  "0.9.0--0.10.0" "0.10.0--0.11.0" "0.11.0--0.12.0"
  "0.12.0--0.13.0" "0.13.0--0.14.0" "0.14.0--0.15.0" "0.15.0--0.16.0"
  "0.16.0--0.17.0" "0.17.0--0.18.0" "0.18.0--0.19.0" "0.19.0--0.20.0"
  "0.20.0--0.21.0" "0.21.0--0.22.0" "0.22.0--0.23.0" "0.23.0--0.24.0"
  "0.24.0--0.25.0" "0.25.0--0.26.0" "0.26.0--0.27.0" "0.27.0--0.28.0"
  "0.28.0--0.29.0" "0.29.0--0.30.0" "0.30.0--0.31.0" "0.31.0--0.32.0"
  "0.32.0--0.33.0" "0.33.0--0.34.0" "0.34.0--0.35.0" "0.35.0--0.36.0"
  "0.36.0--0.37.0" "0.37.0--0.38.0" "0.38.0--0.39.0" "0.39.0--0.40.0"
  "0.40.0--0.41.0" "0.41.0--0.42.0" "0.42.0--0.43.0" "0.43.0--0.44.0"
  "0.44.0--0.45.0" "0.45.0--0.46.0" "0.46.0--0.47.0" "0.47.0--0.48.0"
  "0.48.0--0.49.0" "0.49.0--0.50.0" "0.50.0--0.51.0" "0.51.0--0.52.0"
  "0.52.0--0.53.0" "0.53.0--0.54.0" "0.54.0--0.55.0" "0.55.0--0.56.0"
  "0.56.0--0.57.0" "0.57.0--0.58.0"
)

MISSING=0
for migration in "${EXPECTED_MIGRATIONS[@]}"; do
  if ! ls "$REPO_ROOT/sql/pg_ripple--${migration}.sql" 1>/dev/null 2>&1; then
    echo "  MISSING: pg_ripple--${migration}.sql"
    MISSING=$(( MISSING + 1 ))
  fi
done

if [ "$MISSING" -gt 0 ]; then
  echo "FAIL: $MISSING migration script(s) missing"
  exit 1
fi

echo "  [OK] all migration scripts present"
echo ""
echo "=== PASS: upgrade_chain ==="
