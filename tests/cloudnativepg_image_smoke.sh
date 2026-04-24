#!/usr/bin/env bash
# tests/cloudnativepg_image_smoke.sh — v0.54.0
#
# Smoke test for the pg_ripple CloudNativePG extension image.
# Verifies that:
#   1. The Docker image builds successfully from docker/Dockerfile.cnpg
#   2. The compiled pg_ripple.so is present at the expected path
#   3. The SQL migration files are present at the expected path
#   4. The pgvector.so is present at the expected path
#
# Does NOT require a running Kubernetes cluster or CloudNativePG operator.
# Runs locally with Docker available.
#
# Usage:
#   bash tests/cloudnativepg_image_smoke.sh
#
# Exit codes:
#   0 — all checks passed
#   1 — one or more checks failed

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_TAG="pg_ripple:smoke-test-cnpg"
EXTENSION_FILES_DIR="/var/lib/postgresql/extension-files"

echo "==> Building CloudNativePG extension image..."
docker build \
    --file "${REPO_ROOT}/docker/Dockerfile.cnpg" \
    --tag "${IMAGE_TAG}" \
    "${REPO_ROOT}"

echo "==> Verifying extension files in image..."

check_file() {
    local path="$1"
    echo -n "  Checking ${path} ... "
    if docker run --rm --entrypoint sh "${IMAGE_TAG}" \
            -c "test -f '${path}'" 2>/dev/null; then
        echo "OK"
    else
        echo "MISSING"
        FAILURES=$((FAILURES + 1))
    fi
}

FAILURES=0

check_file "${EXTENSION_FILES_DIR}/lib/pg_ripple.so"
check_file "${EXTENSION_FILES_DIR}/ext/pg_ripple.control"
check_file "${EXTENSION_FILES_DIR}/lib/vector.so"
check_file "${EXTENSION_FILES_DIR}/ext/vector.control"

# Verify at least one migration SQL file exists
echo -n "  Checking for pg_ripple SQL files ... "
SQL_COUNT=$(docker run --rm --entrypoint sh "${IMAGE_TAG}" \
    -c "ls ${EXTENSION_FILES_DIR}/ext/pg_ripple--*.sql 2>/dev/null | wc -l" 2>/dev/null || echo "0")
if [ "${SQL_COUNT}" -gt 0 ]; then
    echo "OK (${SQL_COUNT} files)"
else
    echo "MISSING"
    FAILURES=$((FAILURES + 1))
fi

echo ""
if [ "${FAILURES}" -eq 0 ]; then
    echo "==> All CloudNativePG image smoke tests PASSED."
    docker rmi "${IMAGE_TAG}" 2>/dev/null || true
    exit 0
else
    echo "==> ${FAILURES} CloudNativePG image smoke test(s) FAILED."
    docker rmi "${IMAGE_TAG}" 2>/dev/null || true
    exit 1
fi
