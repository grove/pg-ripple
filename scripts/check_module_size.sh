#!/usr/bin/env bash
# Enforce the production Rust module size limit.
# Files over 800 LOC are reported for follow-up; files over 1200 LOC fail.
set -euo pipefail

src_root="${1:-src}"
warn_limit=800
limit=1200
warnings=0
failed=0

while IFS= read -r -d '' file; do
    lines=$(wc -l < "$file")
    if (( lines > limit )); then
        echo "FAIL  $lines LOC  ${file#./} (limit: $limit)"
        failed=1
    elif (( lines > warn_limit )); then
        echo "WARN  $lines LOC  ${file#./} (review threshold: $warn_limit)"
        warnings=$((warnings + 1))
    fi
done < <(find "$src_root" -name '*.rs' \
    -not -path '*/target/*' -not -path '*/fuzz/*' -not -path '*/tests/*' \
    -print0 | sort -z)

if (( failed )); then
    exit 1
fi
echo "Module size check passed. All production Rust files are <= $limit LOC."
if (( warnings )); then
    echo "${warnings} file(s) exceed the $warn_limit LOC review threshold."
fi
