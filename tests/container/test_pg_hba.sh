#!/usr/bin/env bash
# tests/container/test_pg_hba.sh — v0.128.1 emergency containment
# (production database authentication).
#
# Verifies that the pg_ripple production Docker image does NOT accept
# passwordless remote PostgreSQL connections by default, and that normal
# SCRAM authentication and local Unix-socket access still work as documented.
#
# Background: docker/00-pg_hba.sh used to add unconditional `trust` rules for
# all external TCP connections to every built image, including production
# pulls — any reachable client got an unauthenticated superuser connection.
# It is now inert unless PG_RIPPLE_DEV_TRUST_AUTH=1 is set (see that script).
#
# Usage:
#   bash tests/container/test_pg_hba.sh              # builds the image locally
#   IMAGE_REF=<already-built-ref> bash tests/container/test_pg_hba.sh
#     # e.g. the digest a release workflow just pushed — skips the local build
#
# Exit codes:
#   0 — all checks passed
#   1 — one or more checks failed (or the image is not secure by default)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_TAG="${IMAGE_REF:-pg_ripple:hba-test}"
CONTAINER_NAME="pg_ripple_hba_test_$$"
HOST_PORT="15432"
PG_PASSWORD="hba-test-password"
FAILURES=0

cleanup() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if [ -n "${IMAGE_REF:-}" ]; then
  echo "==> Using already-built image ${IMAGE_TAG} (skipping local build)"
else
  echo "==> Building production image (docker/00-pg_hba.sh must stay inert by default)..."
  docker build --tag "${IMAGE_TAG}" "${REPO_ROOT}"
fi

echo "==> Starting container with SCRAM password, no PG_RIPPLE_DEV_TRUST_AUTH..."
docker run --rm -d \
  --name "${CONTAINER_NAME}" \
  -p "${HOST_PORT}:5432" \
  -e POSTGRES_PASSWORD="${PG_PASSWORD}" \
  "${IMAGE_TAG}" >/dev/null

echo -n "==> Waiting for PostgreSQL to accept connections..."
READY=0
for _ in $(seq 1 60); do
  if docker exec "${CONTAINER_NAME}" pg_isready -U postgres >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 1
done
echo ""
if [ "${READY}" -ne 1 ]; then
  echo "ERROR: PostgreSQL never became ready" >&2
  docker logs "${CONTAINER_NAME}" >&2 || true
  exit 1
fi

check() {
  local description="$1"
  shift
  echo -n "  ${description} ... "
  if "$@"; then
    echo "OK"
  else
    echo "FAILED"
    FAILURES=$((FAILURES + 1))
  fi
}

# 1. Passwordless remote TCP auth must fail (this is the regression this
#    patch fixes — it used to succeed unconditionally).
passwordless_tcp_fails() {
  ! PGPASSWORD="" psql "host=127.0.0.1 port=${HOST_PORT} user=postgres dbname=postgres sslmode=disable connect_timeout=5" \
    -c "SELECT 1" >/dev/null 2>&1
}
check "passwordless remote TCP auth fails" passwordless_tcp_fails

# 2. SCRAM auth with the configured password must still succeed.
scram_auth_succeeds() {
  PGPASSWORD="${PG_PASSWORD}" psql "host=127.0.0.1 port=${HOST_PORT} user=postgres dbname=postgres sslmode=disable connect_timeout=5" \
    -c "SELECT 1" >/dev/null 2>&1
}
check "SCRAM auth with configured password succeeds" scram_auth_succeeds

# 3. Local Unix-socket access from inside the container (the documented
#    policy: the entrypoint and administrators connect locally without a
#    password prompt, matching the upstream postgres:18 image default).
local_socket_succeeds() {
  docker exec -u postgres "${CONTAINER_NAME}" psql -U postgres -c "SELECT 1" >/dev/null 2>&1
}
check "local Unix-socket access succeeds" local_socket_succeeds

echo ""
if [ "${FAILURES}" -eq 0 ]; then
  echo "==> All pg_hba container auth tests PASSED."
  exit 0
else
  echo "==> ${FAILURES} pg_hba container auth test(s) FAILED."
  exit 1
fi
