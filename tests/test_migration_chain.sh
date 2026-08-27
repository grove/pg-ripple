#!/usr/bin/env bash
# Apply every migration discovered by scripts/migration_graph.py to a clean DB.
set -euo pipefail

PGRX_HOST="${PGRX_HOST:-${HOME}/.pgrx}"
PGRX_PORT="${PGRX_PORT:-28818}"
PGRX_USER="${PGRX_USER:-${USER}}"
PSQL=(psql -h "${PGRX_HOST}" -p "${PGRX_PORT}" -U "${PGRX_USER}")
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GRAPH="${ROOT}/target/migration-graph.json"
TEST_DB="pg_ripple_migration_chain_$$"

info() { printf '[info] %s\n' "$*"; }
ok() { printf '[  ok] %s\n' "$*"; }
fail() { printf '[FAIL] %s\n' "$*" >&2; exit 1; }
run_sql() { "${PSQL[@]}" -d "${TEST_DB}" --no-psqlrc --tuples-only --no-align --quiet "$@"; }
assert_true() {
    local label="$1" sql="$2"
    [[ "$(run_sql -c "SELECT CASE WHEN (${sql}) THEN 'yes' ELSE 'no' END;")" == yes ]] || fail "${label}"
    ok "${label}"
}
assert_column() {
    assert_true "column $1.$2.$3 exists" "EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema='$1' AND table_name='$2' AND column_name='$3')"
}
assert_table() {
    assert_true "table $1.$2 exists" "to_regclass('$1.$2') IS NOT NULL"
}

cleanup() { "${PSQL[@]}" -d postgres --quiet -c "DROP DATABASE IF EXISTS \"${TEST_DB}\";" >/dev/null 2>&1 || true; }
trap cleanup EXIT

python3 "${ROOT}/scripts/migration_graph.py" --output "${GRAPH}"
MIGRATION_LIST="$(python3 - "${GRAPH}" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    graph = json.load(stream)
print(graph["base"])
for migration in graph["migrations"]:
    print(migration["file"])
PY
)"
MIGRATION_TARGETS="$(python3 - "${GRAPH}" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    graph = json.load(stream)
for migration in graph["migrations"]:
    print(migration["to"])
PY
)"
TARGET_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["target_version"])' "${GRAPH}")"
BASE_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["base_version"])' "${GRAPH}")"

"${PSQL[@]}" -d postgres --quiet -c "SELECT 1;" >/dev/null || fail "cannot connect to PostgreSQL at ${PGRX_HOST}:${PGRX_PORT}"
"${PSQL[@]}" -d postgres --quiet -c "CREATE DATABASE \"${TEST_DB}\";" >/dev/null

EXTENSION_AVAILABLE="$("${PSQL[@]}" -d postgres --no-psqlrc --tuples-only --no-align --quiet \
    -c "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_ripple');")"
PG_SHARE="$(pg_config --sharedir 2>/dev/null || true)"
BASE_INSTALL="${PG_SHARE}/extension/pg_ripple--${BASE_VERSION}.sql"
if [[ "${EXTENSION_AVAILABLE}" == t && -f "${BASE_INSTALL}" ]]; then
    info "installed pg_ripple files detected; testing CREATE EXTENSION and ALTER EXTENSION"
    if ! run_sql -c "CREATE EXTENSION pg_ripple VERSION '${BASE_VERSION}';" >/dev/null; then
        fail "pg_ripple is available but CREATE EXTENSION baseline failed; refusing raw-SQL fallback"
    fi
    while IFS= read -r version; do
        info "updating extension to ${version}"
        if ! run_sql -c "ALTER EXTENSION pg_ripple UPDATE TO '${version}';" >/dev/null; then
            fail "ALTER EXTENSION UPDATE to ${version} failed"
        fi
    done <<< "${MIGRATION_TARGETS}"
    assert_true "pg_extension extversion is ${TARGET_VERSION}" \
        "(SELECT extversion FROM pg_extension WHERE extname='pg_ripple') = '${TARGET_VERSION}'"
    ok "real extension upgrade path applied"
else
    if [[ "${EXTENSION_AVAILABLE}" == t ]]; then
        info "installed pg_ripple has no ${BASE_VERSION} installation script; using raw migration SQL"
    else
        info "pg_ripple is not installed; using raw migration SQL fallback"
    fi
    if [[ "${PG_RIPPLE_REQUIRE_EXTENSION:-0}" == 1 ]]; then
        fail "pg_ripple is not installed; set up the extension or unset PG_RIPPLE_REQUIRE_EXTENSION for raw-SQL checks"
    fi
    while IFS= read -r migration; do
        info "applying ${migration}"
        run_sql -f "${ROOT}/sql/${migration}" >/dev/null
    done <<< "${MIGRATION_LIST}"
    ok "all migration SQL files applied"
fi

assert_table _pg_ripple dictionary
assert_table _pg_ripple json_mappings
assert_table _pg_ripple json_writeback_queue
run_sql -c "INSERT INTO _pg_ripple.dictionary (hash_hi, hash_lo, value, kind) VALUES (1, 2, 'https://example.org/migration-chain', 0) ON CONFLICT DO NOTHING;" >/dev/null
assert_true "representative dictionary row survives" "EXISTS (SELECT 1 FROM _pg_ripple.dictionary WHERE value='https://example.org/migration-chain')"
assert_true "current user can read dictionary" "has_table_privilege(current_user, '_pg_ripple.dictionary', 'SELECT')"
assert_true "current user can write json mappings" "has_table_privilege(current_user, '_pg_ripple.json_mappings', 'INSERT')"

for column in writeback_table writeback_schema writeback_key_columns writeback_conflict_policy writeback_enabled; do
    assert_column _pg_ripple json_mappings "${column}"
done
assert_true "writeback enqueue trigger function exists" "to_regprocedure('_pg_ripple.json_writeback_enqueue_fn()') IS NOT NULL"

# The Rust function exists only when this chain is run against an installed build.
if [[ "$(run_sql -c "SELECT to_regprocedure('pg_ripple.enable_json_writeback(text)') IS NOT NULL;")" == t ]]; then
    run_sql -c "CREATE TABLE public.migration_writeback_smoke (id integer PRIMARY KEY); INSERT INTO _pg_ripple.json_mappings (name, context) VALUES ('migration_chain_smoke', '{}'::jsonb) ON CONFLICT (name) DO NOTHING; SELECT pg_ripple.configure_json_writeback('migration_chain_smoke', 'public', 'migration_writeback_smoke', ARRAY['id'], 'replace'); SELECT pg_ripple.enable_json_writeback('migration_chain_smoke');" >/dev/null
    ok "enable_json_writeback smoke test"
else
    info "Rust extension not installed; skipped enable_json_writeback smoke call"
fi

ok "migration chain ${TARGET_VERSION} passed"
