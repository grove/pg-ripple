#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
SOURCE="${2:-.}"

if [[ ! "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 VERSION [ARTIFACT_DIR]" >&2
    exit 2
fi
if [[ ! -d "${SOURCE}" ]]; then
    echo "artifact directory not found: ${SOURCE}" >&2
    exit 1
fi

ROOT="results/conformance"
DEST="${ROOT}/${VERSION}"
mkdir -p "${DEST}"
cp -R "${SOURCE}"/. "${DEST}"/
rm -f "${ROOT}/latest"
ln -s "${VERSION}" "${ROOT}/latest"
echo "published conformance artifacts to ${DEST}"
