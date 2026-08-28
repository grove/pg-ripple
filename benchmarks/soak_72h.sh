#!/usr/bin/env bash
# v0.132.0 H18-04: bounded soak harness. Set SOAK_HOURS=72 for the release gate.
set -euo pipefail

SOAK_HOURS="${SOAK_HOURS:-1}"
INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-300}"
OUT="${SOAK_OUTPUT:-results/soak/$(date -u +%Y%m%dT%H%M%SZ).jsonl}"
command -v psql >/dev/null 2>&1 || { echo "psql is required" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
deadline=$(( $(date +%s) + SOAK_HOURS * 3600 ))

while (( $(date +%s) < deadline )); do
    started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    metrics=$(psql -X -At -c "SELECT json_build_object('started_at', '$started', 'triples', (SELECT COALESCE(sum(triple_count), 0) FROM _pg_ripple.predicates), 'unmerged_delta_rows', ((pg_ripple.stats() ->> 'unmerged_delta_rows')::bigint), 'merge_worker_pid', ((pg_ripple.stats() ->> 'merge_worker_pid')::integer))")
    printf '%s\n' "$metrics" >> "$OUT"
    sleep "$INTERVAL_SECONDS"
done

echo "Soak metrics written to $OUT"
