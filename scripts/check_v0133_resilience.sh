#!/usr/bin/env bash
# CI-safe v0.133.0 resilience qualification validation.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

bash "$ROOT/tests/resilience/run_v0133_matrix.sh" --validate

while IFS= read -r -d '' script; do
    bash -n "$script"
done < <(find "$ROOT/tests" -type f -name '*.sh' -print0)

python3 "$ROOT/scripts/check_docs_links.py"
python3 "$ROOT/scripts/check_docs_summary.py"

echo "PASS: v0.133.0 resilience scripts and operations documentation validated"
