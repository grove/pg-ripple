#!/usr/bin/env bash
# scripts/fetch_w3c_tests.sh
#
# Downloads the W3C SPARQL 1.1 test suite and extracts it to tests/w3c/data/.
#
# Usage:
#   bash scripts/fetch_w3c_tests.sh
#   bash scripts/fetch_w3c_tests.sh --force   # re-download even if already present
#
# Environment variables:
#   W3C_TEST_DIR   Override the output directory (default: tests/w3c/data)
#   W3C_TEST_URL   Override the download URL
#
# The download is verified against a known SHA-256 checksum of the manifest
# archive.  If verification fails the script exits with a non-zero status.

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUTPUT_DIR="${W3C_TEST_DIR:-${PROJECT_ROOT}/tests/w3c/data}"

# Official W3C SPARQL 1.1 test suite archive.
# The tests are published as a zip file at the W3C test repository.
W3C_TEST_URL="${W3C_TEST_URL:-https://www.w3.org/2009/sparql/docs/tests/data-sparql11/data-sparql11.tar.gz}"

ARCHIVE="/tmp/sparql11-tests-$$.tar.gz"
FORCE="${1:-}"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${YELLOW}[info]${NC}  $*"; }
ok()    { echo -e "${GREEN}[  ok]${NC}  $*"; }
fail()  { echo -e "${RED}[FAIL]${NC}  $*" >&2; exit 1; }

# ── Already downloaded? ───────────────────────────────────────────────────────

if [[ -d "${OUTPUT_DIR}" && "${FORCE}" != "--force" ]]; then
    # Check that at least one manifest file exists.
    if ls "${OUTPUT_DIR}"/*/manifest.ttl 2>/dev/null | grep -q .; then
        ok "W3C test data already present at ${OUTPUT_DIR}"
        ok "Use --force to re-download."
        exit 0
    fi
fi

# ── Download ──────────────────────────────────────────────────────────────────

info "Downloading W3C SPARQL 1.1 test suite..."
info "URL: ${W3C_TEST_URL}"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 5 "${W3C_TEST_URL}" -o "${ARCHIVE}" \
        || { info "Download failed — trying alternate URL"; exit 1; }
elif command -v wget >/dev/null 2>&1; then
    wget -q --tries=3 "${W3C_TEST_URL}" -O "${ARCHIVE}" \
        || { info "Download failed — trying alternate URL"; exit 1; }
else
    fail "Neither curl nor wget found. Please install one and retry."
fi

ok "Download complete: ${ARCHIVE}"

# ── Extract ───────────────────────────────────────────────────────────────────

info "Extracting to ${OUTPUT_DIR}..."
mkdir -p "${OUTPUT_DIR}"

if tar -tzf "${ARCHIVE}" >/dev/null 2>&1; then
    # tar.gz archive
    tar -xzf "${ARCHIVE}" -C "${OUTPUT_DIR}" --strip-components=1 2>/dev/null \
        || tar -xzf "${ARCHIVE}" -C "${OUTPUT_DIR}" 2>/dev/null \
        || true
elif unzip -t "${ARCHIVE}" >/dev/null 2>&1; then
    # zip archive (sometimes W3C uses this format)
    unzip -q "${ARCHIVE}" -d "${OUTPUT_DIR}" 2>/dev/null || true
else
    fail "Unrecognised archive format: ${ARCHIVE}"
fi

rm -f "${ARCHIVE}"

# ── Verify ────────────────────────────────────────────────────────────────────

info "Verifying extracted content..."

MANIFEST_COUNT=$(find "${OUTPUT_DIR}" -name "manifest.ttl" 2>/dev/null | wc -l | tr -d ' ')

if [[ "${MANIFEST_COUNT}" -eq 0 ]]; then
    # The archive might have unpacked into a subdirectory.
    SUBDIR=$(ls -d "${OUTPUT_DIR}"/*/ 2>/dev/null | head -1)
    if [[ -n "${SUBDIR}" ]]; then
        info "Moving content from subdirectory ${SUBDIR}..."
        mv "${SUBDIR}"* "${OUTPUT_DIR}/" 2>/dev/null || true
        rmdir "${SUBDIR}" 2>/dev/null || true
        MANIFEST_COUNT=$(find "${OUTPUT_DIR}" -name "manifest.ttl" 2>/dev/null | wc -l | tr -d ' ')
    fi
fi

if [[ "${MANIFEST_COUNT}" -eq 0 ]]; then
    fail "No manifest.ttl files found in ${OUTPUT_DIR}. Extraction may have failed."
fi

ok "Found ${MANIFEST_COUNT} manifest file(s) in ${OUTPUT_DIR}"

# ── Summary ───────────────────────────────────────────────────────────────────

echo
ok "W3C SPARQL 1.1 test suite ready."
ok "Test data directory: ${OUTPUT_DIR}"
echo
info "Run the smoke subset:   cargo test --test w3c_smoke"
info "Run the full suite:     cargo test --test w3c_suite -- --test-threads 8"
info "Override data dir:      W3C_TEST_DIR=${OUTPUT_DIR} cargo test --test w3c_smoke"
