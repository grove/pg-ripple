#!/usr/bin/env bash
# CI-safe v0.136.0 audit-readiness qualification.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

test -f "$ROOT/audit/engagement-brief.md"
test -f "$ROOT/audit/v0.136.0-findings.md"
test -f "$ROOT/roadmap/v0.136.0.md"
test -f "$ROOT/sql/pg_ripple--0.135.0--0.136.0.sql"
grep -q '^# v0.136.0 external security audit engagement brief$' "$ROOT/audit/engagement-brief.md"
grep -q '^# v0.136.0 audit findings ledger$' "$ROOT/audit/v0.136.0-findings.md"
grep -q 'Status: awaiting the independent external audit\.' "$ROOT/audit/v0.136.0-findings.md"
grep -q '^-- Migration 0.135.0 -> 0.136.0:' "$ROOT/sql/pg_ripple--0.135.0--0.136.0.sql"
grep -q '"release": "0.136.0"' "$ROOT/api/stable-v1.json"
grep -q '0.136.0' "$ROOT/pg_ripple_http/openapi.yaml"
grep -q '0.136.0' "$ROOT/pg_ripple_http/src/routing/mod.rs"
grep -q 'HTTP_COMPANION_MIN_VERSION.*0.135.0' "$ROOT/src/compat.rs"

for check in \
    scripts/check_no_string_format_in_sql.sh \
    scripts/check_security_definer_search_path.sh \
    scripts/check_http_routes.py \
    scripts/check_v0134_streaming.sh \
    scripts/migration_graph.py; do
    test -f "$ROOT/$check"
done

test -f "$ROOT/tests/pg_regress/sql/v0136_audit_readiness.sql"
test -f "$ROOT/tests/pg_regress/expected/v0136_audit_readiness.out"
test -f "$ROOT/fuzz/fuzz_targets/sparql_bindings_json.rs"
test -f "$ROOT/fuzz/fuzz_targets/sparql_prefix_prologue.rs"

echo 'PASS: v0.136.0 audit-readiness contract is present; external audit remains pending'
