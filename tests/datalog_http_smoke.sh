#!/usr/bin/env bash
# tests/datalog_http_smoke.sh — curl-based smoke test for the Datalog HTTP API
#
# Prerequisites:
#   - pg_ripple_http running at $BASE_URL (default: http://localhost:7878)
#   - pg_ripple v0.39.0+ installed in the target PostgreSQL instance
#
# Usage:
#   BASE_URL=http://localhost:7878 bash tests/datalog_http_smoke.sh
#   # With auth token:
#   BASE_URL=http://localhost:7878 AUTH_TOKEN=mytoken bash tests/datalog_http_smoke.sh

set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:7878}"
AUTH_TOKEN="${AUTH_TOKEN:-}"
RULE_SET="smoke_test_$$"
PASS=0
FAIL=0

# ─── Helpers ──────────────────────────────────────────────────────────────────

auth_header() {
    if [[ -n "$AUTH_TOKEN" ]]; then
        echo "-H" "Authorization: Bearer $AUTH_TOKEN"
    fi
}

check() {
    local label="$1"
    local expected_status="$2"
    local actual_status="$3"
    local body="$4"

    if [[ "$actual_status" == "$expected_status" ]]; then
        echo "  PASS  $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL  $label  (expected HTTP $expected_status, got $actual_status)"
        echo "        body: $body"
        FAIL=$((FAIL + 1))
    fi
}

curl_json() {
    # Returns "STATUS\nBODY" separated by a newline
    local method="$1"; shift
    local url="$1"; shift
    curl -s -X "$method" \
        -H "Accept: application/json" \
        -w "\n%{http_code}" \
        $(auth_header) \
        "$@" \
        "$url"
}

# ─── Phase 1: Rule management ─────────────────────────────────────────────────

echo "=== Phase 1: Rule management ==="

RESPONSE=$(curl_json POST "$BASE_URL/datalog/rules/$RULE_SET" \
    -H "Content-Type: text/x-datalog" \
    -d "ancestor(?x, ?y) :- parent(?x, ?y).
ancestor(?x, ?z) :- parent(?x, ?y), ancestor(?y, ?z).")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/rules/$RULE_SET" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/rules")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/rules" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json POST "$BASE_URL/datalog/rules/$RULE_SET/add" \
    -H "Content-Type: text/x-datalog" \
    -d "sibling(?x, ?y) :- parent(?p, ?x), parent(?p, ?y).")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/rules/$RULE_SET/add" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json PUT "$BASE_URL/datalog/rules/$RULE_SET/enable")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "PUT /datalog/rules/$RULE_SET/enable" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json PUT "$BASE_URL/datalog/rules/$RULE_SET/disable")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "PUT /datalog/rules/$RULE_SET/disable" "200" "$STATUS" "$BODY"

# ─── Phase 2: Inference ───────────────────────────────────────────────────────

echo "=== Phase 2: Inference ==="

RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/$RULE_SET" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET/stats")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/$RULE_SET/stats" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET/agg")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/$RULE_SET/agg" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET/wfs")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/$RULE_SET/wfs" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET/demand" \
    -H "Content-Type: application/json" \
    -d '{"demands": [{"predicate": "ancestor", "bound": [0]}]}')
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/$RULE_SET/demand" "200" "$STATUS" "$BODY"

# ─── Phase 3: Query & constraints ─────────────────────────────────────────────

echo "=== Phase 3: Query & constraints ==="

RESPONSE=$(curl_json POST "$BASE_URL/datalog/query/$RULE_SET" \
    -H "Content-Type: text/x-datalog" \
    -d "ancestor(?x, ?y).")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/query/$RULE_SET" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/constraints")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/constraints" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/constraints/$RULE_SET")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/constraints/$RULE_SET" "200" "$STATUS" "$BODY"

# ─── Phase 4: Admin & monitoring ──────────────────────────────────────────────

echo "=== Phase 4: Admin & monitoring ==="

RESPONSE=$(curl_json GET "$BASE_URL/datalog/stats/cache")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/stats/cache" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/stats/tabling")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/stats/tabling" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/lattices")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/lattices" "200" "$STATUS" "$BODY"

RESPONSE=$(curl_json GET "$BASE_URL/datalog/views")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "GET /datalog/views" "200" "$STATUS" "$BODY"

# ─── Error paths ──────────────────────────────────────────────────────────────

echo "=== Error paths ==="

# Missing body → 400
RESPONSE=$(curl_json POST "$BASE_URL/datalog/rules/err_test" \
    -H "Content-Type: text/x-datalog" \
    -d "")
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/rules with empty body → 400" "400" "$STATUS" ""

# Invalid JSON body for demand → 400
RESPONSE=$(curl_json POST "$BASE_URL/datalog/infer/$RULE_SET/demand" \
    -H "Content-Type: application/json" \
    -d "not-json")
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "POST /datalog/infer/demand with bad JSON → 400" "400" "$STATUS" ""

# Invalid rule_id → 400
RESPONSE=$(curl_json DELETE "$BASE_URL/datalog/rules/$RULE_SET/not-a-number")
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "DELETE /datalog/rules/:set/not-a-number → 400" "400" "$STATUS" ""

# ─── Cleanup ──────────────────────────────────────────────────────────────────

RESPONSE=$(curl_json DELETE "$BASE_URL/datalog/rules/$RULE_SET")
BODY=$(echo "$RESPONSE" | head -n -1)
STATUS=$(echo "$RESPONSE" | tail -n 1)
check "DELETE /datalog/rules/$RULE_SET (cleanup)" "200" "$STATUS" "$BODY"

# ─── Metrics check ────────────────────────────────────────────────────────────

echo "=== Metrics ==="
METRICS=$(curl -s $(auth_header) "$BASE_URL/metrics")
if echo "$METRICS" | grep -q "pg_ripple_http_datalog_queries_total"; then
    echo "  PASS  /metrics includes pg_ripple_http_datalog_queries_total"
    PASS=$((PASS + 1))
else
    echo "  FAIL  /metrics missing pg_ripple_http_datalog_queries_total"
    FAIL=$((FAIL + 1))
fi

# ─── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "Results: $PASS passed, $FAIL failed"
[[ $FAIL -eq 0 ]]
