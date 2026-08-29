#!/usr/bin/env bash
# v0.134.0: standards-valid HTTP streaming format smoke test.
#
# Requires a running pg_ripple_http instance and a database containing at least
# one triple. Set PG_RIPPLE_HTTP_URL and PG_RIPPLE_HTTP_AUTH_TOKEN as needed.

set -euo pipefail

BASE_URL="${PG_RIPPLE_HTTP_URL:-http://127.0.0.1:7878}"
TIMEOUT="${HTTP_TIMEOUT:-15}"
QUERY='SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }'
GRAPH='CONSTRUCT { ?s <https://v0134.test/p> ?o } WHERE { ?s <https://v0134.test/p> ?o }'
AUTH_ARGS=()
if [[ -n "${PG_RIPPLE_HTTP_AUTH_TOKEN:-}" ]]; then
    AUTH_ARGS=(-H "Authorization: Bearer ${PG_RIPPLE_HTTP_AUTH_TOKEN}")
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsS --max-time "$TIMEOUT" "${BASE_URL}/health" >/dev/null

curl -fsS --max-time "$TIMEOUT" "${BASE_URL}/sparql/stream" \
    "${AUTH_ARGS[@]}" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=${QUERY}" \
    -D "$TMP_DIR/headers" -o "$TMP_DIR/body"
grep -qi '^content-type: application/sparql-results+json' "$TMP_DIR/headers"
grep -qi '^x-pg-ripple-streaming: true' "$TMP_DIR/headers"
python3 - "$TMP_DIR/body" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    result = json.load(stream)
assert result["head"]["vars"] == ["s", "o"]
assert result["results"]["bindings"][0]["s"]["type"] == "uri"
assert result["results"]["bindings"][0]["o"]["xml:lang"] == "en"
PY

curl -fsS --max-time "$TIMEOUT" -G "${BASE_URL}/sparql" \
    "${AUTH_ARGS[@]}" \
    -H 'Accept: text/csv' \
    --data-urlencode "query=${QUERY}" \
    -o "$TMP_DIR/csv"
grep -q '^s,o$' "$TMP_DIR/csv"

curl -fsS --max-time "$TIMEOUT" -G "${BASE_URL}/sparql" \
    "${AUTH_ARGS[@]}" \
    -H 'Accept: application/n-triples' \
    --data-urlencode "query=${GRAPH}" \
    -o "$TMP_DIR/ntriples"
grep -qF -- '<https://v0134.test/s> <https://v0134.test/p> "hello"@en .' "$TMP_DIR/ntriples"

echo "PASS: HTTP streaming JSON, CSV, and N-Triples formats are valid"
