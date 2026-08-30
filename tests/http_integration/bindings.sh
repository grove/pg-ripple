#!/usr/bin/env bash
# v0.135.0: typed binding HTTP route smoke test.
#
# Requires a running pg_ripple_http instance and the v0.134 stream fixture.

set -euo pipefail

BASE_URL="${PG_RIPPLE_HTTP_URL:-http://127.0.0.1:7878}"
TIMEOUT="${HTTP_TIMEOUT:-15}"
AUTH_ARGS=()
if [[ -n "${PG_RIPPLE_HTTP_AUTH_TOKEN:-}" ]]; then
    AUTH_ARGS=(-H "Authorization: Bearer ${PG_RIPPLE_HTTP_AUTH_TOKEN}")
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

REQUEST='{"query":"SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }","bindings":{"o":{"type":"literal","value":"hello","xml:lang":"en"}}}'
curl -fsS --max-time "$TIMEOUT" "${BASE_URL}/sparql/bindings" \
    "${AUTH_ARGS[@]}" \
    -H 'Content-Type: application/json' \
    -H 'Accept: application/sparql-results+json' \
    --data "$REQUEST" -o "$TMP_DIR/json"
python3 - "$TMP_DIR/json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    result = json.load(stream)
bindings = result["results"]["bindings"]
assert len(bindings) == 1
assert bindings[0]["o"]["xml:lang"] == "en"
PY

for accept in text/csv text/tab-separated-values; do
    output="$TMP_DIR/${accept##*/}"
    curl -fsS --max-time "$TIMEOUT" "${BASE_URL}/sparql/bindings" \
        "${AUTH_ARGS[@]}" \
        -H 'Content-Type: application/json' \
        -H "Accept: ${accept}" \
        --data "$REQUEST" -o "$output"
    if [[ "$accept" == "text/csv" ]]; then
        head -1 "$output" | grep -q '^s,o$'
    else
        head -1 "$output" | grep -qF $'?s\t?o'
    fi
done

GRAPH_REQUEST='{"query":"CONSTRUCT { ?s <https://v0134.test/p> ?o } WHERE { ?s <https://v0134.test/p> ?o }","bindings":{"o":{"type":"literal","value":"hello","xml:lang":"en"}}}'
curl -fsS --max-time "$TIMEOUT" "${BASE_URL}/sparql/bindings" \
    "${AUTH_ARGS[@]}" \
    -H 'Content-Type: application/json' \
    -H 'Accept: application/n-triples' \
    --data "$GRAPH_REQUEST" -o "$TMP_DIR/ntriples"
grep -qF -- '<https://v0134.test/s> <https://v0134.test/p> "hello"@en .' "$TMP_DIR/ntriples"

echo "PASS: HTTP typed bindings JSON, CSV, TSV, and N-Triples formats are valid"
