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

validate_source() {
    local suite="$1"
    shift
    python3 "${SCRIPT_DIR}/validate_conformance_sources.py" \
        --suite "$suite" "$@"
}

# ── Argument parsing ──────────────────────────────────────────────────────────

FORCE=""
DO_W3C=false
DO_JENA=false
DO_WATDIV=false
DO_OWL2RL=false
DO_BSBM=false
EXPLICIT_SUITE=false

for arg in "$@"; do
    case "$arg" in
        --force) FORCE="--force" ;;
        --w3c)   DO_W3C=true; EXPLICIT_SUITE=true ;;
        --jena)  DO_JENA=true; EXPLICIT_SUITE=true ;;
        --watdiv) DO_WATDIV=true; EXPLICIT_SUITE=true ;;
        --owl2rl) DO_OWL2RL=true; EXPLICIT_SUITE=true ;;
        --bsbm)  DO_BSBM=true; EXPLICIT_SUITE=true ;;
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

# Pin the fixture snapshot consumed by the harness.
JENA_URL="https://raw.githubusercontent.com/apache/jena/790b3dc08fccb6be1ea2868b97bfcbae8f113062/jena-arq/testing/ARQ/testing-2026-05.zip"

# SHA-256 checksum of the Jena archive.
# Set JENA_SKIP_CHECKSUM=1 to skip verification when testing a new snapshot.
JENA_SHA256="cfe989a8429ca57e6a737ccc094c761ec57737ef06ed7a749257a3ad7d0f7f3e"

fetch_jena() {
    if [[ -d "${JENA_TEST_DIR}" && "${FORCE}" != "--force" ]]; then
        if validate_source jena --directory "${JENA_TEST_DIR}"; then
            ok "Jena test data already present at ${JENA_TEST_DIR}"
            ok "Use --force to re-download."
            return 0
        fi
    fi

    info "Downloading Apache Jena test suite from GitHub..."
    info "URL: ${JENA_URL}"
    info "This will extract the SPARQL test resources (~4 MB)."

    local archive="/tmp/jena-tests-$$.zip"
    local extract_dir="/tmp/jena-tests-extract-$$"
    trap "rm -rf '${extract_dir}' '${archive}'" EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 --retry-delay 5 "${JENA_URL}" -o "${archive}" \
            || fail "Download failed."
    elif command -v wget >/dev/null 2>&1; then
        wget -q --tries=3 --wait=5 "${JENA_URL}" -O "${archive}" \
            || fail "Download failed."
    else
        fail "Neither curl nor wget is available. Please install one."
    fi

    validate_source jena --archive "${archive}"

    if ! command -v unzip >/dev/null 2>&1; then
        fail "unzip is not available."
    fi

    info "Extracting SPARQL test resources..."
    mkdir -p "${JENA_TEST_DIR}" "${extract_dir}"
    unzip -q "${archive}" -d "${extract_dir}"
    mkdir -p "${JENA_TEST_DIR}/ARQ"
    cp -R "${extract_dir}/testing/ARQ/." "${JENA_TEST_DIR}/ARQ/"

    # Create sub-suite directories expected by the test harness.
    for suite in sparql-query sparql-update sparql-syntax algebra; do
        local src_dir="${JENA_TEST_DIR}/${suite}"
        if [[ ! -d "${src_dir}" ]]; then
            # Try alternative layout (Jena uses various directory structures).
            local alt="${JENA_TEST_DIR}/SPARQL/${suite}"
            if [[ -d "${alt}" ]]; then
                ln -sfn "${alt}" "${src_dir}"
            fi
        fi
    done

    validate_source jena --directory "${JENA_TEST_DIR}"
    ok "Jena test data extracted to ${JENA_TEST_DIR}"
}

# ── WatDiv ────────────────────────────────────────────────────────────────────

WATDIV_DATA_DIR="${WATDIV_DATA_DIR:-${PROJECT_ROOT}/tests/watdiv/data}"
WATDIV_TMPL_DIR="${WATDIV_TMPL_DIR:-${PROJECT_ROOT}/tests/watdiv/templates}"

# WatDiv query templates are in a GitHub repository.
WATDIV_TMPL_URL="https://api.github.com/repos/dsg-uwaterloo/watdiv/tarball/482524d0e35423ae1ab7ad1bcfda5bd3c8f76308"

# SHA-256 of the WatDiv template archive.
WATDIV_TMPL_SHA256="ce1707e2ceec57ae8f2d18e0d1e0b8905db31a2c11c6e2768b1eb16ec023f031"

# WatDiv data generation: requires the watdiv binary or Docker image.
# If WATDIV_BINARY is set, use it; otherwise try Docker.
WATDIV_BINARY="${WATDIV_BINARY:-}"
WATDIV_SCALE="${WATDIV_SCALE:-10000000}"   # 10M triples (default for CI)

fetch_watdiv_templates() {
    if [[ -d "${WATDIV_TMPL_DIR}" && "${FORCE}" != "--force" ]]; then
        if validate_source watdiv --directory "${WATDIV_TMPL_DIR}"; then
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

    validate_source watdiv --archive "${archive}"

    info "Extracting WatDiv templates..."
    local extract_dir="/tmp/watdiv-tmpl-extract-$$"
    mkdir -p "${WATDIV_TMPL_DIR}" "${extract_dir}"
    tar -xzf "${archive}" -C "${extract_dir}" 2>/dev/null \
        || fail "Could not extract the WatDiv archive"
    local query_dir
    query_dir=$(find "${extract_dir}" -type d -path '*/data-model/queries' -print -quit)
    if [[ -z "${query_dir}" ]]; then
        fail "Could not locate WatDiv query templates in the extracted archive"
    fi
    find "${query_dir}" -type f \( -name '*.sparql' -o -name '*.rq' \) \
        -exec cp {} "${WATDIV_TMPL_DIR}/" \;
    rm -rf "${extract_dir}"

    # Organise into sub-directories if not already.
    for class in star chain snowflake complex; do
        mkdir -p "${WATDIV_TMPL_DIR}/${class}"
    done
    # Move templates by prefix: S→star, C→chain, F→snowflake, B/L→complex.
    shopt -s nullglob
    for f in "${WATDIV_TMPL_DIR}"/*.sparql "${WATDIV_TMPL_DIR}"/*.rq; do
        base="$(basename "$f")"
        case "${base}" in
            S*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/star/" ;;
            C*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/chain/" ;;
            F*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/snowflake/" ;;
            B*.*|L*.*)  mv -n "$f" "${WATDIV_TMPL_DIR}/complex/" ;;
        esac
    done
    shopt -u nullglob

    validate_source watdiv --directory "${WATDIV_TMPL_DIR}"
    ok "WatDiv templates extracted to ${WATDIV_TMPL_DIR}"
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

# ── OWL 2 RL test suite (v0.46.0) ────────────────────────────────────────────

OWL2RL_DIR="${OWL2RL_TEST_DIR:-${PROJECT_ROOT}/tests/owl2rl/data}"

fetch_owl2rl() {
    info "Fetching W3C OWL 2 RL test manifests..."

    if [[ -d "${OWL2RL_DIR}" && "${FORCE}" != "--force" ]]; then
        if python3 "${SCRIPT_DIR}/validate_conformance_sources.py" \
            --suite owl2_rl --directory "${OWL2RL_DIR}"; then
            ok "OWL 2 RL test data already present at ${OWL2RL_DIR}"
            return 0
        fi
    fi
    fail "OWL 2 RL corpus is missing; provide the pinned corpus under ${OWL2RL_DIR}"
}

# ── BSBM data (v0.46.0) ───────────────────────────────────────────────────────

BSBM_DATA_DIR="${BSBM_DATA_DIR:-${PROJECT_ROOT}/benchmarks/bsbm/data}"

fetch_bsbm() {
    info "Fetching BSBM 1M-triple product dataset..."

    if [[ -d "${BSBM_DATA_DIR}" && "${FORCE}" != "--force" ]]; then
        if find "${BSBM_DATA_DIR}" -name "*.nt" -o -name "*.ttl" 2>/dev/null | grep -q .; then
            ok "BSBM data already present at ${BSBM_DATA_DIR}"
            return 0
        fi
    fi

    mkdir -p "${BSBM_DATA_DIR}"

    # BSBM requires the Java-based data generator.  We check for it and skip
    # gracefully if it's not available.
    if ! command -v java >/dev/null 2>&1; then
        info "WARNING: Java not found — BSBM data generation requires Java."
        info "Install Java and the BSBM tools from http://wbsg.informatik.uni-mannheim.de/bizer/berlinsparqlbenchmark/"
        info "BSBM regression tests will skip gracefully without data."
        return 0
    fi

    # Check for the BSBM generator jar.
    local BSBM_JAR="${BSBM_JAR:-}"
    if [[ -z "${BSBM_JAR}" ]]; then
        info "WARNING: BSBM_JAR not set — set it to the path of the BSBM generator jar."
        info "Download from http://wbsg.informatik.uni-mannheim.de/bizer/berlinsparqlbenchmark/"
        info "BSBM regression tests will skip gracefully without data."
        return 0
    fi

    info "Generating BSBM 1M-triple dataset (scale factor 1000)..."
    java -jar "${BSBM_JAR}" -pc 1000 -dir "${BSBM_DATA_DIR}" \
        || { info "WARNING: BSBM data generation failed."; return 0; }

    ok "BSBM 1M-triple dataset generated at ${BSBM_DATA_DIR}"
}

# ── Main ──────────────────────────────────────────────────────────────────────

[[ "${DO_W3C}" == "true" ]]    && fetch_w3c
[[ "${DO_JENA}" == "true" ]]   && fetch_jena
[[ "${DO_WATDIV}" == "true" ]] && fetch_watdiv
[[ "${DO_OWL2RL}" == "true" ]] && fetch_owl2rl
[[ "${DO_BSBM}" == "true" ]]   && fetch_bsbm

ok "Conformance test data fetch complete."
info "Run the test suites with:"
info "  cargo test --test w3c_suite"
info "  cargo test --test jena_suite"
info "  cargo test --test watdiv_suite"
info "  cargo test --test owl2rl_suite"
info "  cargo test --test datalog_convergence_suite"
