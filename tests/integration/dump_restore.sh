#!/usr/bin/env bash
# tests/integration/dump_restore.sh
# v0.60.0 [6.14]: pg_dump / pg_restore round-trip CI integration test.
#
# Populates a pg_ripple instance with triples, inferred facts, and SHACL
# validation results; dumps it with pg_dump; restores to a fresh instance;
# and asserts that all triple counts, query results, and validation hashes
# match exactly.
#
# This script is a CI-ready wrapper around tests/pg_dump_restore.sh that
# sets sensible defaults for the pgrx test environment.
#
# Usage (requires cargo pgrx start pg18):
#   bash tests/integration/dump_restore.sh
#
# Environment:
#   PGHOST   — default: /tmp
#   PGPORT   — default: 28818
#   PGUSER   — default: current user

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Delegate to the full pg_dump_restore.sh script.
exec bash "${ROOT}/tests/pg_dump_restore.sh" "$@"
