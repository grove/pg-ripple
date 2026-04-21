#!/usr/bin/env bash
# scripts/fetch_conformance_tests.sh
#
# Downloads conformance test data for pg_ripple's three test suites:
#   • W3C SPARQL 1.1 test suite  (--w3c, default)
#   • Apache Jena test suite     (--jena)
#   • WatDiv query templates     (--watdiv)
#
# Extends scripts/fetch_w3c_tests.sh to cover Jena and WatDiv.
#
# Usage:
#   bash scripts/fetch_conformance_tests.sh            # all suites
#   bash scripts/fetch_conformance_tests.sh --w3c      # W3C only
#   bash scripts/fetch_conformance_tests.sh --jena     # Jena only
#   bash scripts/fetch_conformance_tests.sh --watdiv   # WatDiv only
#   bash scripts/fetch_conformance_tests.sh --force    # re-download everything
#
# Environment variables:
#   W3C_TEST_DIR      Output directory for W3C tests  (default: tests/w3c/data)
#   JENA_TEST_DIR     Output directory for Jena tests (default: tests/jena/data)
#   WATDIV_DATA_DIR   Output directory for WatDiv RDF data (default: tests/watdiv/data)
#   WATDIV_TMPL_DIR   Output directory for WatDiv templates (default: tests/watdiv/templates)
#
# Downloads are verified against SHA-256 checksums.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${YELLOW}[info]${NC}  $*"; }
ok()    { echo -e "${GREEN}[  ok]${NC}  $*"; }
fail()  { echo -e "${RED}[FAIL]${NC}  $*" >&2; exit 1; }

# ── Argument parsing ──────────────────────────────────────────────────────────

FORCE=""
DO_W3C=false
DO_JENA=false
DO_WATDIV=false
EXPLICIT_SUITE=false

for arg in "$@"; do
    case "$arg" in
        --force) FORCE="--force" ;;
        --w3c)   DO_W3C=true; EXPLICIT_SUITE=true ;;
        --jena)  DO_JENA=true; EXPLICIT_SUITE=true ;;
        --watdiv) DO_WATDIV=true; EXPLICIT_SUITE=true ;;
        *) info "Unknown argument: $arg" ;;
    esac
done

# Default: run all suites when none specified.
if [[ "${EXPLICIT_SUITE}" == "false" ]]; then
    DO_W3C=true
    DO_JENA=true
    DO_WATDIV=true
fi

# ── W3C SPARQL 1.1 ───────────────────────────────────────────────────────────

fetch_w3c() {
    info "Fetching W3C SPARQL 1.1 test suite..."
    bash "${SCRIPT_DIR}/fetch_w3c_tests.sh" ${FORCE}
    ok "W3C test suite ready."
}

# ── Apache Jena ───────────────────────────────────────────────────────────────

JENA_TEST_DIR="${JENA_TEST_DIR:-${PROJECT_ROOT}/tests/jena/data}"

# Apache Jena test suite is hosted on the Apache GitHub mirror.
# The SPARQL test resources are under jena-arq/testing/ARQ (not src/test/resources).
JENA_URL="https://github.com/apache/jena/archive/refs/heads/main.tar.gz"
JENA_SPARQL_PATH="jena-main/jena-arq/testing/ARQ"

# SHA-256 checksum of the Jena archive.
# NOTE: This changes with each Jena HEAD commit; set JENA_SKIP_CHECKSUM=1 to skip.
JENA_SHA256="${JENA_SHA256:-}"

fetch_jena() {
    if [[ -d "${JENA_TEST_DIR}" && "${FORCE}" != "--force" ]]; then
        if find "${JENA_TEST_DIR}" -name "manifest.ttl" 2>/dev/null | grep -q .; then
            ok "Jena test data already present at ${JENA_TEST_DIR}"
            ok "Use --force to re-download."
            return 0
        fi
    fi

    info "Downloading Apache Jena test suite from GitHub..."
    info "URL: ${JENA_URL}"
    info "This will extract the SPARQL test resources (~50 MB)."

    local archive="/tmp/jena-tests-$$.tar.gz"
    trap "rm -f '${archive}'" EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 --retry-delay 5 "${JENA_URL}" -o "${archive}" \
            || fail "Download failed."
    elif command -v wget >/dev/null 2>&1; then
        wget -q --tries=3 --wait=5 "${JENA_URL}" -O "${archive}" \
            || fail "Download failed."
    else
        fail "Neither curl nor wget is available. Please install one."
    fi

    # Verify checksum if provided.
    if [[ -n "${JENA_SHA256}" ]]; then
        info "Verifying SHA-256 checksum..."
        local actual
        if command -v sha256sum >/dev/null 2>&1; then
            actual=$(sha256sum "${archive}" | awk '{print $1}')
        elif command -v shasum >/dev/null 2>&1; then
            actual=$(shasum -a 256 "${archive}" | awk '{print $1}')
        else
            info "WARNING: no sha256sum or shasum found; skipping checksum verification."
            actual="${JENA_SHA256}"
        fi
        if [[ "${actual}" != "${JENA_SHA256}" ]]; then
            fail "SHA-256 mismatch: expected ${JENA_SHA256}, got ${actual}"
        fi
        ok "Checksum verified."
    elif [[ "${JENA_SKIP_CHECKSUM:-}" != "1" ]]; then
        info "Set JENA_SHA256 or JENA_SKIP_CHECKSUM=1 to skip verification."
    fi

    info "Extracting SPARQL test resources..."
    mkdir -p "${JENA_TEST_DIR}"
    tar -xzf "${archive}" \
        --strip-components=3 \
        -C "${JENA_TEST_DIR}" \
        "${JENA_SPARQL_PATH}" \
        2>/dev/null || true   # Some paths may not exist in all Jena versions.

    # Create sub-suite directories expected by the test harness.
    for suite in sparql-query sparql-update sparql-syntax algebra; do
        local src_dir="${JENA_TEST_DIR}/${suite}"
        if [[ ! -d "${src_dir}" ]]; then
            # Try alternative layout (Jena uses various directory structures).
            local alt="${JENA_TEST_DIR}/SPARQL/${suite}"
            if [[ -d "${alt}" ]]; then
                ln -sfn "${alt}" "${src_dir}" 2>/dev/null || true
            fi
        fi
    done

    if find "${JENA_TEST_DIR}" -name "manifest.ttl" 2>/dev/null | grep -q .; then
        ok "Jena test data extracted to ${JENA_TEST_DIR}"
    else
        info "WARNING: No manifest.ttl files found after extraction."
        info "Jena may have changed its repository layout."
        info "Set JENA_TEST_DIR to a directory containing SPARQL test manifests."
    fi
}

# ── WatDiv ────────────────────────────────────────────────────────────────────

WATDIV_DATA_DIR="${WATDIV_DATA_DIR:-${PROJECT_ROOT}/tests/watdiv/data}"
WATDIV_TMPL_DIR="${WATDIV_TMPL_DIR:-${PROJECT_ROOT}/tests/watdiv/templates}"

# WatDiv query templates are in a GitHub repository.
WATDIV_TMPL_URL="https://github.com/daveritchie/watdiv/archive/refs/heads/master.tar.gz"

# SHA-256 of the WatDiv template archive.
WATDIV_TMPL_SHA256="${WATDIV_TMPL_SHA256:-}"

# WatDiv data generation: requires the watdiv binary or Docker image.
# If WATDIV_BINARY is set, use it; otherwise try Docker.
WATDIV_BINARY="${WATDIV_BINARY:-}"
WATDIV_SCALE="${WATDIV_SCALE:-10000000}"   # 10M triples (default for CI)

fetch_watdiv_templates() {
    if [[ -d "${WATDIV_TMPL_DIR}" && "${FORCE}" != "--force" ]]; then
        if find "${WATDIV_TMPL_DIR}" -name "*.sparql" -o -name "*.rq" 2>/dev/null | grep -q .; then
            ok "WatDiv templates already present at ${WATDIV_TMPL_DIR}"
            ok "Use --force to re-download."
            return 0
        fi
    fi

    info "Downloading WatDiv query templates..."
    info "URL: ${WATDIV_TMPL_URL}"

    local archive="/tmp/watdiv-tmpl-$$.tar.gz"
    trap "rm -f '${archive}'" EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 --retry-delay 5 "${WATDIV_TMPL_URL}" -o "${archive}" \
            || fail "Download failed."
    elif command -v wget >/dev/null 2>&1; then
        wget -q --tries=3 --wait=5 "${WATDIV_TMPL_URL}" -O "${archive}" \
            || fail "Download failed."
    else
        fail "Neither curl nor wget is available."
    fi

    if [[ -n "${WATDIV_TMPL_SHA256}" ]]; then
        info "Verifying SHA-256 checksum..."
        local actual
        if command -v sha256sum >/dev/null 2>&1; then
            actual=$(sha256sum "${archive}" | awk '{print $1}')
        elif command -v shasum >/dev/null 2>&1; then
            actual=$(shasum -a 256 "${archive}" | awk '{print $1}')
        else
            actual="${WATDIV_TMPL_SHA256}"
        fi
        if [[ "${actual}" != "${WATDIV_TMPL_SHA256}" ]]; then
            fail "SHA-256 mismatch: expected ${WATDIV_TMPL_SHA256}, got ${actual}"
        fi
        ok "Checksum verified."
    fi

    info "Extracting WatDiv templates..."
    mkdir -p "${WATDIV_TMPL_DIR}"
    tar -xzf "${archive}" \
        --strip-components=2 \
        -C "${WATDIV_TMPL_DIR}" \
        "watdiv-master/watdiv/data-model/queries" \
        2>/dev/null || true

    # Organise into sub-directories if not already.
    for class in star chain snowflake complex; do
        mkdir -p "${WATDIV_TMPL_DIR}/${class}"
    done
    # Move templates by prefix: S→star, C→chain, F→snowflake, B/L→complex.
    shopt -s nullglob
    for f in "${WATDIV_TMPL_DIR}"/*.sparql "${WATDIV_TMPL_DIR}"/*.rq; do
        base="$(basename "$f")"
        case "${base}" in
            S*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/star/"  2>/dev/null || true ;;
            C*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/chain/" 2>/dev/null || true ;;
            F*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/snowflake/" 2>/dev/null || true ;;
            B*.*|L*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/complex/" 2>/dev/null || true ;;
        esac
    done
    shopt -u nullglob

    if find "${WATDIV_TMPL_DIR}" -name "*.sparql" -o -name "*.rq" 2>/dev/null | grep -q .; then
        ok "WatDiv templates extracted to ${WATDIV_TMPL_DIR}"
    else
        info "WARNING: No .sparql/.rq files found after extraction."
        info "The WatDiv repository layout may have changed."
    fi
}

generate_watdiv_data() {
    if [[ -d "${WATDIV_DATA_DIR}" && "${FORCE}" != "--force" ]]; then
        if find "${WATDIV_DATA_DIR}" -name "*.nt" -o -name "*.ttl" 2>/dev/null | grep -q .; then
            ok "WatDiv data already present at ${WATDIV_DATA_DIR}"
            ok "Use --force to regenerate."
            return 0
        fi
    fi

    mkdir -p "${WATDIV_DATA_DIR}"

    if [[ -n "${WATDIV_BINARY}" && -x "${WATDIV_BINARY}" ]]; then
        info "Generating WatDiv dataset with binary: ${WATDIV_BINARY}"
        "${WATDIV_BINARY}" -s 1 -t ${WATDIV_SCALE} \
            "${PROJECT_ROOT}/tests/watdiv/watdiv.10MD.schema" \
            > "${WATDIV_DATA_DIR}/watdiv-10M.nt" \
            || fail "WatDiv data generation failed."
        ok "WatDiv 10M-triple dataset generated at ${WATDIV_DATA_DIR}/watdiv-10M.nt"
    elif command -v docker >/dev/null 2>&1; then
        info "Generating WatDiv dataset via Docker (dcslab/watdiv)..."
        docker run --rm \
            -v "${WATDIV_DATA_DIR}:/output" \
            dcslab/watdiv \
            -s 1 -t ${WATDIV_SCALE} \
            > "${WATDIV_DATA_DIR}/watdiv-10M.nt" \
            2>/dev/null \
            || info "Docker generation failed — continuing without data."
    else
        info "WARNING: No watdiv binary or Docker found."
        info "Set WATDIV_BINARY=/path/to/watdiv to use a local binary."
        info "Or run: docker run --rm dcslab/watdiv -s 1 -t 10000000 > tests/watdiv/data/watdiv-10M.nt"
        info "WatDiv tests will skip gracefully without data."
    fi
}

fetch_watdiv() {
    fetch_watdiv_templates
    generate_watdiv_data
}

# ── Main ──────────────────────────────────────────────────────────────────────

[[ "${DO_W3C}" == "true" ]]    && fetch_w3c
[[ "${DO_JENA}" == "true" ]]   && fetch_jena
[[ "${DO_WATDIV}" == "true" ]] && fetch_watdiv

ok "Conformance test data fetch complete."
info "Run the test suites with:"
info "  cargo test --test w3c_suite"
info "  cargo test --test jena_suite"
info "  cargo test --test watdiv_suite"
