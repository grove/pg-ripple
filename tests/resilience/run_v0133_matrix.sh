#!/usr/bin/env bash
# Canonical v0.133.0 resilience qualification entry point.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$SCRIPT_DIR/fault_matrix.sh" "$@"
