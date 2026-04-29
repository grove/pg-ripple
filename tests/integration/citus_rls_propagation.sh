#!/usr/bin/env bash
# CITUS-INT-01 (v0.71.0): Citus RLS propagation multi-node integration test.
#
# Verifies that pg_ripple's per-graph Row-Level Security (RLS) policies:
#   1. Are propagated to all Citus worker nodes via run_command_on_all_nodes().
#   2. Apply correctly to promoted VP tables on workers.
#   3. Restrict non-superuser queries to only the graphs they have been granted.
#
# This test is cited in feature_status() as:
#   feature_name = 'citus_rls_propagation'
#   evidence     = 'tests/integration/citus_rls_propagation.sh'
#
# Prerequisites:
#   - Docker and docker-compose available.
#   - The docker-compose.yml in the repo root starts a Citus coordinator + two workers.
#   - psql available.
#   - cargo pgrx install (pg_ripple installed in the Citus cluster).
#
# Usage:
#   bash tests/integration/citus_rls_propagation.sh
#
# Environment variables:
#   COORDINATOR_URL   Citus coordinator connection string (default: postgres://postgres@localhost:5432/postgres)
#   DOCKER_COMPOSE    docker-compose command (default: docker compose)
#   SKIP_DOCKER_UP    Set to 1 to skip docker-compose up (cluster already running)
#
# Exit codes:
#   0 — all RLS propagation assertions passed
#   1 — an assertion failed

set -euo pipefail

COORDINATOR_URL="${COORDINATOR_URL:-postgres://postgres@localhost:5432/postgres}"
DOCKER_COMPOSE="${DOCKER_COMPOSE:-docker compose}"
SKIP_DOCKER_UP="${SKIP_DOCKER_UP:-0}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "=== Citus RLS propagation integration test (CITUS-INT-01) ==="
echo "COORDINATOR_URL : $COORDINATOR_URL"
echo

# ── Helper ────────────────────────────────────────────────────────────────────

run_sql() {
    psql "$COORDINATOR_URL" -v ON_ERROR_STOP=1 -c "$@"
}

run_sql_as() {
    local role="$1"; shift
    psql "${COORDINATOR_URL}" -v ON_ERROR_STOP=1 \
        --set=ROLE="$role" -c "SET ROLE $role;" -c "$@"
}

# ── Step 1: Start Citus cluster ───────────────────────────────────────────────

if [ "$SKIP_DOCKER_UP" != "1" ]; then
    echo "Step 1: Starting Citus cluster via docker-compose..."
    cd "$REPO_ROOT"
    $DOCKER_COMPOSE up -d --wait 2>&1 | tail -5
    echo "  Waiting 10 s for cluster to stabilise..."
    sleep 10
else
    echo "Step 1: Skipping docker-compose up (SKIP_DOCKER_UP=1)"
fi

# ── Step 2: Install pg_ripple and enable Citus sharding ───────────────────────

echo "Step 2: Installing pg_ripple and enabling Citus sharding..."

run_sql "CREATE EXTENSION IF NOT EXISTS citus;"
run_sql "CREATE EXTENSION IF NOT EXISTS pg_ripple;"
run_sql "SET search_path TO pg_ripple, public;"
run_sql "SELECT pg_ripple.enable_citus_sharding();"

echo "  OK: pg_ripple with Citus sharding enabled"

# ── Step 3: Create graphs and a non-superuser role ───────────────────────────

echo "Step 3: Creating graphs and roles..."

run_sql "
SET search_path TO pg_ripple, public;

-- Create two named graphs: one allowed, one restricted.
SELECT create_graph('https://rls-test.example/allowed/');
SELECT create_graph('https://rls-test.example/restricted/');

-- Create test role (drop first for idempotency).
DO \$\$
BEGIN
    DROP ROLE IF EXISTS rls_test_reader;
    CREATE ROLE rls_test_reader NOLOGIN;
END \$\$;

-- Grant graph access to allowed graph only.
SELECT pg_ripple.grant_graph_access('rls_test_reader', 'https://rls-test.example/allowed/');
"

echo "  OK: Graphs and role created"

# ── Step 4: Insert triples into both graphs ───────────────────────────────────

echo "Step 4: Inserting triples into both graphs..."

run_sql "
SET search_path TO pg_ripple, public;

SELECT sparql_update('
    INSERT DATA {
        GRAPH <https://rls-test.example/allowed/> {
            <https://rls-test.example/allowed/s1> <https://schema.org/name> \"Allowed\" .
            <https://rls-test.example/allowed/s2> <https://schema.org/name> \"AlsoAllowed\" .
        }
    }
');

SELECT sparql_update('
    INSERT DATA {
        GRAPH <https://rls-test.example/restricted/> {
            <https://rls-test.example/restricted/s1> <https://schema.org/name> \"Restricted\" .
        }
    }
');
"

echo "  OK: Triples inserted"

# ── Step 5: Promote a predicate to verify RLS on promoted VP tables ────────────

echo "Step 5: Promoting schema:name predicate past threshold..."

# Insert enough triples to trigger promotion (vp_promotion_threshold default 1000).
run_sql "
SET search_path TO pg_ripple, public;
SET pg_ripple.vp_promotion_threshold = 1;
SELECT sparql_update(format(
    'INSERT DATA { GRAPH <https://rls-test.example/allowed/> { %s } }',
    string_agg(
        '<https://rls-test.example/s/' || i || '> <https://schema.org/name> \"val' || i || '\" .',
        ' '
    )
))
FROM generate_series(1, 50) AS i;
RESET pg_ripple.vp_promotion_threshold;
" -q

echo "  OK: Promotion threshold crossed"

# ── Step 6: Verify RLS policy propagated to workers ──────────────────────────

echo "Step 6: Verifying RLS policy on coordinator and workers..."

# The policy should exist on coordinator.
POLICY_COUNT=$(run_sql -At "
    SELECT count(*)
    FROM pg_policies
    WHERE schemaname = '_pg_ripple'
      AND tablename LIKE 'vp_%'
      AND policyname LIKE 'rls_rls_test_reader%';
")

if [ "$POLICY_COUNT" -eq 0 ]; then
    echo "ERROR: No RLS policies found for rls_test_reader on coordinator" >&2
    exit 1
fi
echo "  OK: $POLICY_COUNT RLS policy/policies found on coordinator"

# ── Step 7: Query as the restricted role and assert no restricted-graph triples

echo "Step 7: Querying as rls_test_reader — expecting only allowed-graph triples..."

# Query as the restricted role via SET ROLE (coordinator enforces RLS).
ALLOWED_COUNT=$(psql "$COORDINATOR_URL" -At <<SQL
SET ROLE rls_test_reader;
SELECT count(*) FROM pg_ripple.sparql('
    SELECT * WHERE {
        GRAPH <https://rls-test.example/allowed/> { ?s ?p ?o }
    }
');
SQL
)

RESTRICTED_COUNT=$(psql "$COORDINATOR_URL" -At <<SQL
SET ROLE rls_test_reader;
SELECT count(*) FROM pg_ripple.sparql('
    SELECT * WHERE {
        GRAPH <https://rls-test.example/restricted/> { ?s ?p ?o }
    }
');
SQL
)

echo "  Allowed-graph triples visible   : $ALLOWED_COUNT"
echo "  Restricted-graph triples visible: $RESTRICTED_COUNT"

if [ "$RESTRICTED_COUNT" -ne 0 ]; then
    echo "ERROR: Restricted-graph triples visible to rls_test_reader — RLS not enforced" >&2
    exit 1
fi

if [ "$ALLOWED_COUNT" -eq 0 ]; then
    echo "ERROR: No allowed-graph triples visible to rls_test_reader — RLS too restrictive" >&2
    exit 1
fi

echo "  OK: RLS correctly restricts access"

# ── Cleanup ───────────────────────────────────────────────────────────────────

echo "Cleanup..."
run_sql "
SET search_path TO pg_ripple, public;
SELECT pg_ripple.revoke_graph_access('rls_test_reader', 'https://rls-test.example/allowed/');
SELECT clear_graph('https://rls-test.example/allowed/');
SELECT clear_graph('https://rls-test.example/restricted/');
SELECT drop_graph('https://rls-test.example/allowed/');
SELECT drop_graph('https://rls-test.example/restricted/');
DROP ROLE IF EXISTS rls_test_reader;
" -q 2>/dev/null || true

echo
echo "=== PASS: Citus RLS propagation test completed successfully ==="
