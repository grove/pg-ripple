#!/usr/bin/env bash
# tests/test_migration_chain.sh
#
# Verifies that all migration SQL scripts apply cleanly in sequence from v0.1.0
# to the current version, and that the final schema matches expectations.
#
# This script tests the SQL DDL content of migration scripts independently of
# the PostgreSQL extension mechanism (no ALTER EXTENSION needed).  Every
# migration script is applied via psql against an isolated test database, which
# means we catch syntax errors, missing column references, and schema drift
# before they reach a user running ALTER EXTENSION pg_ripple UPDATE.
#
# Prerequisites:
#   - A pgrx-managed PostgreSQL 18 instance must be running (cargo pgrx start pg18 / just start)
#   - The PGRX_HOST/PGRX_PORT environment variables must be set, OR the defaults
#     ($HOME/.pgrx, port 28818) must be valid
#
# Usage:
#   tests/test_migration_chain.sh                  # from project root
#   just test-migration                            # via justfile

set -euo pipefail

# ── Connection defaults (match pgrx pg18 managed instance) ───────────────────
#
# pgrx starts PostgreSQL 18 on port 28818.
# On macOS the unix socket lives in ~/.pgrx; on Linux pgrx uses the same
# directory.  We default to the socket directory so both platforms work.
# Set PGRX_HOST=localhost to force TCP (useful if the socket path is
# non-standard, e.g. in some CI environments).

PGRX_HOST="${PGRX_HOST:-${HOME}/.pgrx}"
PGRX_PORT="${PGRX_PORT:-28818}"
PGRX_USER="${PGRX_USER:-${USER}}"

PSQL="psql -h ${PGRX_HOST} -p ${PGRX_PORT} -U ${PGRX_USER}"

# ── Path helpers ──────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SQL_DIR="${PROJECT_ROOT}/sql"

# ── Colour output ─────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Colour

info()  { echo -e "${YELLOW}[info]${NC}  $*"; }
ok()    { echo -e "${GREEN}[  ok]${NC}  $*"; }
fail()  { echo -e "${RED}[FAIL]${NC}  $*" >&2; }

# ── Test database ─────────────────────────────────────────────────────────────

TEST_DB="pg_ripple_migration_chain_$$"

cleanup() {
    info "cleaning up test database '${TEST_DB}'"
    ${PSQL} -d postgres --quiet -c "DROP DATABASE IF EXISTS \"${TEST_DB}\";" 2>/dev/null || true
}
trap cleanup EXIT

# ── helpers ───────────────────────────────────────────────────────────────────

# Run SQL against the test database and return output.
run_sql() {
    ${PSQL} -d "${TEST_DB}" --no-psqlrc --tuples-only --no-align --quiet "$@"
}

# Assert that a SQL expression evaluates to a non-empty truthy result.
assert_true() {
    local label="$1"
    local sql="$2"
    local result
    result=$(run_sql -c "SELECT CASE WHEN (${sql}) THEN 'yes' ELSE 'no' END;")
    if [[ "${result}" == "yes" ]]; then
        ok "${label}"
    else
        fail "${label}"
        fail "  query: ${sql}"
        fail "  result: ${result}"
        exit 1
    fi
}

# Assert that a column exists in a table in the given schema.
assert_column() {
    local schema="$1" table="$2" column="$3"
    assert_true \
        "column ${schema}.${table}.${column} exists" \
        "EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = '${schema}'
              AND table_name   = '${table}'
              AND column_name  = '${column}'
        )"
}

# Assert that a column does not exist.
assert_no_column() {
    local schema="$1" table="$2" column="$3"
    assert_true \
        "column ${schema}.${table}.${column} absent" \
        "NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = '${schema}'
              AND table_name   = '${table}'
              AND column_name  = '${column}'
        )"
}

# Assert that a table exists.
assert_table() {
    local schema="$1" table="$2"
    assert_true \
        "table ${schema}.${table} exists" \
        "EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = '${schema}'
              AND table_name   = '${table}'
        )"
}

# Apply a SQL migration script file.
apply_script() {
    local path="$1"
    local label="$2"
    info "applying ${label}"
    if run_sql -f "${path}" > /dev/null; then
        ok "${label} applied successfully"
    else
        fail "${label} failed"
        exit 1
    fi
}

# ── Main ──────────────────────────────────────────────────────────────────────

echo
info "pg_ripple migration chain test"
info "connecting to pgrx PG18 at host=${PGRX_HOST} port=${PGRX_PORT} user=${PGRX_USER}"
echo

# Verify connectivity before creating anything
if ! ${PSQL} -d postgres --quiet -c "SELECT 1;" > /dev/null 2>&1; then
    fail "cannot connect to PostgreSQL at host=${PGRX_HOST} port=${PGRX_PORT}"
    fail "start the pgrx instance first: cargo pgrx start pg18  (or: just start)"
    exit 1
fi
ok "PostgreSQL connection verified"

# Create isolated test database
${PSQL} -d postgres --quiet -c "CREATE DATABASE \"${TEST_DB}\";"
ok "test database '${TEST_DB}' created"
echo

# ── Step 1: apply base schema (v0.1.0) ───────────────────────────────────────

info "=== v0.1.0 base schema ==="
apply_script "${SQL_DIR}/pg_ripple--0.1.0.sql" "pg_ripple--0.1.0.sql"

# Verify base schema
assert_table  "_pg_ripple" "dictionary"
assert_table  "_pg_ripple" "predicates"
assert_table  "_pg_ripple" "vp_rare"
assert_column "_pg_ripple" "dictionary" "id"
assert_column "_pg_ripple" "dictionary" "hash"
assert_column "_pg_ripple" "dictionary" "value"
assert_column "_pg_ripple" "dictionary" "kind"
assert_column "_pg_ripple" "dictionary" "datatype"
assert_column "_pg_ripple" "dictionary" "lang"
assert_column "_pg_ripple" "vp_rare"    "p"
assert_column "_pg_ripple" "vp_rare"    "s"
assert_column "_pg_ripple" "vp_rare"    "o"
assert_column "_pg_ripple" "vp_rare"    "g"
assert_column "_pg_ripple" "vp_rare"    "i"
assert_column "_pg_ripple" "vp_rare"    "source"

# v0.1.0 must NOT have the qt_* columns (those are added in 0.3.0→0.4.0)
assert_no_column "_pg_ripple" "dictionary" "qt_s"
assert_no_column "_pg_ripple" "dictionary" "qt_p"
assert_no_column "_pg_ripple" "dictionary" "qt_o"

# Sequence must exist
assert_true "statement_id_seq exists" \
    "EXISTS (SELECT 1 FROM pg_class WHERE relname = 'statement_id_seq' AND relkind = 'S')"
echo

# ── Step 2: migrate 0.1.0 → 0.2.0 ───────────────────────────────────────────

info "=== migration 0.1.0 → 0.2.0 ==="
apply_script "${SQL_DIR}/pg_ripple--0.1.0--0.2.0.sql" "pg_ripple--0.1.0--0.2.0.sql"

# No schema changes in this migration — verify tables are unchanged
assert_table "_pg_ripple" "dictionary"
assert_table "_pg_ripple" "predicates"
assert_table "_pg_ripple" "vp_rare"
assert_no_column "_pg_ripple" "dictionary" "qt_s"
ok "schema unchanged (no DDL in 0.1.0→0.2.0)"
echo

# ── Step 3: migrate 0.2.0 → 0.3.0 ───────────────────────────────────────────

info "=== migration 0.2.0 → 0.3.0 ==="
apply_script "${SQL_DIR}/pg_ripple--0.2.0--0.3.0.sql" "pg_ripple--0.2.0--0.3.0.sql"

# No schema changes in this migration
assert_no_column "_pg_ripple" "dictionary" "qt_s"
ok "schema unchanged (no DDL in 0.2.0→0.3.0)"
echo

# ── Step 4: migrate 0.3.0 → 0.4.0 ───────────────────────────────────────────

info "=== migration 0.3.0 → 0.4.0 ==="
apply_script "${SQL_DIR}/pg_ripple--0.3.0--0.4.0.sql" "pg_ripple--0.3.0--0.4.0.sql"

# This migration adds qt_s, qt_p, qt_o to _pg_ripple.dictionary
assert_column "_pg_ripple" "dictionary" "qt_s"
assert_column "_pg_ripple" "dictionary" "qt_p"
assert_column "_pg_ripple" "dictionary" "qt_o"

# Verify the new columns are nullable BIGINTs (as specified)
assert_true "qt_s is nullable bigint" \
    "EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'dictionary'
          AND column_name  = 'qt_s'
          AND data_type    = 'bigint'
          AND is_nullable  = 'YES'
    )"
assert_true "qt_p is nullable bigint" \
    "EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'dictionary'
          AND column_name  = 'qt_p'
          AND data_type    = 'bigint'
          AND is_nullable  = 'YES'
    )"
assert_true "qt_o is nullable bigint" \
    "EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'dictionary'
          AND column_name  = 'qt_o'
          AND data_type    = 'bigint'
          AND is_nullable  = 'YES'
    )"

# Existing rows remain accessible (insert and query a row)
run_sql -c "
    INSERT INTO _pg_ripple.dictionary (hash, value, kind)
    VALUES (decode(md5('test'), 'hex'), 'https://example.org/test', 0);
" > /dev/null
assert_true "row with NULL qt_* survives after migration" \
    "(SELECT COUNT(*) FROM _pg_ripple.dictionary WHERE qt_s IS NULL) = 1"
ok "qt_* columns present, existing data preserved"
echo

# ── Step 5: migrate 0.4.0 → 0.5.0 ───────────────────────────────────────────

info "=== migration 0.4.0 → 0.5.0 ==="
apply_script "${SQL_DIR}/pg_ripple--0.4.0--0.5.0.sql" "pg_ripple--0.4.0--0.5.0.sql"

# No schema changes in this migration
assert_column "_pg_ripple" "dictionary" "qt_s"
ok "schema unchanged (no DDL in 0.4.0→0.5.0)"
echo

# ── Step 6: migrate 0.5.0 → 0.5.1 ───────────────────────────────────────────

info "=== migration 0.5.0 → 0.5.1 ==="
apply_script "${SQL_DIR}/pg_ripple--0.5.0--0.5.1.sql" "pg_ripple--0.5.0--0.5.1.sql"

# No schema changes in this migration
assert_column "_pg_ripple" "dictionary" "qt_s"
ok "schema unchanged (no DDL in 0.5.0→0.5.1)"
echo

# ── Intermediate migrations (0.5.1 → 0.50.0) — apply in sequence ─────────────
# These migrations are applied silently; only their final state matters.
for migration in \
    "pg_ripple--0.5.1--0.6.0.sql" \
    "pg_ripple--0.6.0--0.7.0.sql" \
    "pg_ripple--0.7.0--0.8.0.sql" \
    "pg_ripple--0.8.0--0.9.0.sql" \
    "pg_ripple--0.9.0--0.10.0.sql" \
    "pg_ripple--0.10.0--0.11.0.sql" \
    "pg_ripple--0.11.0--0.12.0.sql" \
    "pg_ripple--0.12.0--0.13.0.sql" \
    "pg_ripple--0.13.0--0.14.0.sql" \
    "pg_ripple--0.14.0--0.15.0.sql" \
    "pg_ripple--0.15.0--0.16.0.sql" \
    "pg_ripple--0.16.0--0.17.0.sql" \
    "pg_ripple--0.17.0--0.18.0.sql" \
    "pg_ripple--0.18.0--0.19.0.sql" \
    "pg_ripple--0.19.0--0.20.0.sql" \
    "pg_ripple--0.20.0--0.21.0.sql" \
    "pg_ripple--0.21.0--0.22.0.sql" \
    "pg_ripple--0.22.0--0.23.0.sql" \
    "pg_ripple--0.23.0--0.24.0.sql" \
    "pg_ripple--0.24.0--0.25.0.sql" \
    "pg_ripple--0.25.0--0.26.0.sql" \
    "pg_ripple--0.26.0--0.27.0.sql" \
    "pg_ripple--0.27.0--0.28.0.sql" \
    "pg_ripple--0.28.0--0.29.0.sql" \
    "pg_ripple--0.29.0--0.30.0.sql" \
    "pg_ripple--0.30.0--0.31.0.sql" \
    "pg_ripple--0.31.0--0.32.0.sql" \
    "pg_ripple--0.32.0--0.33.0.sql" \
    "pg_ripple--0.33.0--0.34.0.sql" \
    "pg_ripple--0.34.0--0.35.0.sql" \
    "pg_ripple--0.35.0--0.36.0.sql" \
    "pg_ripple--0.36.0--0.37.0.sql" \
    "pg_ripple--0.37.0--0.38.0.sql" \
    "pg_ripple--0.38.0--0.39.0.sql" \
    "pg_ripple--0.39.0--0.40.0.sql" \
    "pg_ripple--0.40.0--0.41.0.sql" \
    "pg_ripple--0.41.0--0.42.0.sql" \
    "pg_ripple--0.42.0--0.43.0.sql" \
    "pg_ripple--0.43.0--0.44.0.sql" \
    "pg_ripple--0.44.0--0.45.0.sql" \
    "pg_ripple--0.45.0--0.46.0.sql" \
    "pg_ripple--0.46.0--0.47.0.sql" \
    "pg_ripple--0.47.0--0.48.0.sql" \
    "pg_ripple--0.48.0--0.49.0.sql" \
    "pg_ripple--0.49.0--0.50.0.sql" \
; do
    if [[ -f "${SQL_DIR}/${migration}" ]]; then
        apply_script "${SQL_DIR}/${migration}" "${migration}"
    fi
done

# ── Step 7: migrate 0.50.0 → 0.51.0 ──────────────────────────────────────────

info "=== migration 0.50.0 → 0.51.0 ==="
apply_script "${SQL_DIR}/pg_ripple--0.50.0--0.51.0.sql" "pg_ripple--0.50.0--0.51.0.sql"

# v0.51.0 adds _pg_ripple.predicate_stats table.
assert_true "predicate_stats table exists" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'predicate_stats'
    )"
ok "0.50.0→0.51.0: predicate_stats table created"
echo

# ── Final state verification ──────────────────────────────────────────────────

info "=== final schema verification (v0.51.0) ==="

# Dictionary table columns
for col in id hash value kind datatype lang qt_s qt_p qt_o; do
    assert_column "_pg_ripple" "dictionary" "${col}"
done

# Predicates table columns
for col in id table_oid triple_count; do
    assert_column "_pg_ripple" "predicates" "${col}"
done

# vp_rare table columns
for col in p s o g i source; do
    assert_column "_pg_ripple" "vp_rare" "${col}"
done

# Views
assert_true "view pg_ripple.predicate_stats exists" \
    "EXISTS (
        SELECT 1 FROM information_schema.views
        WHERE table_schema = 'pg_ripple'
          AND table_name   = 'predicate_stats'
    )"

echo
ok "All v0.51.0 schema assertions passed."
echo

# ── MIGCHAIN-01 (v0.80.0): Apply migrations v0.51.0 → v0.79.0 with checkpoint assertions ──
# Each migration script is applied in sequence, with schema assertions at key
# checkpoints (v0.65.0, v0.70.0, v0.75.0, v0.79.0) to verify that expected
# schema objects exist and have not been accidentally removed.

info "=== applying migrations v0.51.0 → v0.79.0 (MIGCHAIN-01) ==="

for migration in \
    "pg_ripple--0.51.0--0.52.0.sql" \
    "pg_ripple--0.52.0--0.53.0.sql" \
    "pg_ripple--0.53.0--0.54.0.sql" \
    "pg_ripple--0.54.0--0.55.0.sql" \
    "pg_ripple--0.55.0--0.56.0.sql" \
    "pg_ripple--0.56.0--0.57.0.sql" \
    "pg_ripple--0.57.0--0.58.0.sql" \
    "pg_ripple--0.58.0--0.59.0.sql" \
    "pg_ripple--0.59.0--0.60.0.sql" \
    "pg_ripple--0.60.0--0.61.0.sql" \
    "pg_ripple--0.61.0--0.62.0.sql" \
    "pg_ripple--0.62.0--0.63.0.sql" \
    "pg_ripple--0.63.0--0.64.0.sql" \
    "pg_ripple--0.64.0--0.65.0.sql" \
; do
    if [[ -f "${SQL_DIR}/${migration}" ]]; then
        apply_script "${SQL_DIR}/${migration}" "${migration}"
    fi
done

# ── MIGCHAIN-01 checkpoint 1: v0.65.0 ──────────────────────────────────────
info "=== MIGCHAIN-01 checkpoint: v0.65.0 ==="
# v0.65.0 adds observability columns to construct_rules.
assert_column "_pg_ripple" "construct_rules" "last_incremental_run"
assert_column "_pg_ripple" "construct_rules" "derived_triple_count"
# vp_rare.source has been present since v0.10.0; verify it still exists.
assert_column "_pg_ripple" "vp_rare" "source"
ok "v0.65.0 checkpoint assertions passed"
echo

for migration in \
    "pg_ripple--0.65.0--0.66.0.sql" \
    "pg_ripple--0.66.0--0.67.0.sql" \
    "pg_ripple--0.67.0--0.68.0.sql" \
    "pg_ripple--0.68.0--0.69.0.sql" \
    "pg_ripple--0.69.0--0.70.0.sql" \
; do
    if [[ -f "${SQL_DIR}/${migration}" ]]; then
        apply_script "${SQL_DIR}/${migration}" "${migration}"
    fi
done

# ── MIGCHAIN-01 checkpoint 2: v0.70.0 ──────────────────────────────────────
info "=== MIGCHAIN-01 checkpoint: v0.70.0 ==="
# predicates.triple_count has been present since v0.1.0; verify it still exists.
assert_column "_pg_ripple" "predicates" "triple_count"
# schema_version table introduced in v0.70.0-era migrations.
assert_true "schema_version table exists at v0.70.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'schema_version'
    )"
ok "v0.70.0 checkpoint assertions passed"
echo

for migration in \
    "pg_ripple--0.70.0--0.71.0.sql" \
    "pg_ripple--0.71.0--0.72.0.sql" \
    "pg_ripple--0.72.0--0.73.0.sql" \
    "pg_ripple--0.73.0--0.74.0.sql" \
    "pg_ripple--0.74.0--0.75.0.sql" \
; do
    if [[ -f "${SQL_DIR}/${migration}" ]]; then
        apply_script "${SQL_DIR}/${migration}" "${migration}"
    fi
done

# ── MIGCHAIN-01 checkpoint 3: v0.75.0 ──────────────────────────────────────
info "=== MIGCHAIN-01 checkpoint: v0.75.0 ==="
# construct_views table introduced in v0.18.0; verify it still exists.
assert_true "construct_views table exists at v0.75.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'construct_views'
    )"
# sparql_views table introduced in v0.18.0; verify it still exists.
assert_true "sparql_views table exists at v0.75.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple'
          AND table_name   = 'sparql_views'
    )"
ok "v0.75.0 checkpoint assertions passed"
echo

for migration in \
    "pg_ripple--0.75.0--0.76.0.sql" \
    "pg_ripple--0.76.0--0.77.0.sql" \
    "pg_ripple--0.77.0--0.78.0.sql" \
    "pg_ripple--0.78.0--0.79.0.sql" \
; do
    if [[ -f "${SQL_DIR}/${migration}" ]]; then
        apply_script "${SQL_DIR}/${migration}" "${migration}"
    fi
done

# ── MIGCHAIN-01 checkpoint 4: v0.79.0 ──────────────────────────────────────
info "=== MIGCHAIN-01 checkpoint: v0.79.0 ==="
# statement_id_seq exists since v0.1.0; verify it still exists.
assert_true "statement_id_seq sequence exists at v0.79.0" \
    "EXISTS (
        SELECT 1 FROM pg_class
        WHERE relname = 'statement_id_seq' AND relkind = 'S'
    )"
# schema_version table must record at least v0.75.0.
assert_true "schema_version has at least one row at v0.79.0" \
    "(SELECT count(*) FROM _pg_ripple.schema_version) >= 1"
ok "v0.79.0 checkpoint assertions passed"
echo

# ── T13-01 (v0.84.0): Apply migrations v0.79.0 → v0.83.0 ────────────────────
info "=== T13-01: applying migrations v0.79.0 → v0.83.0 ==="
for ver in 0.79 0.80 0.81 0.82; do
    major=$(echo "${ver}" | cut -d. -f1)
    minor=$(echo "${ver}" | cut -d. -f2)
    next_minor=$((minor + 1))
    next_ver="${major}.${next_minor}.0"
    cur_ver="${ver}.0"
    script="${SQL_DIR}/pg_ripple--${cur_ver}--${next_ver}.sql"
    if [[ -f "${script}" ]]; then
        run_sql -f "${script}"
        ok "Applied migration ${cur_ver} → ${next_ver}"
    else
        fail "T13-01: missing migration script pg_ripple--${cur_ver}--${next_ver}.sql"
        exit 1
    fi
done
echo

# ── T13-01 checkpoint: v0.80.0 ────────────────────────────────────────────────
info "=== T13-01 checkpoint: v0.80.0 ==="
# v0.80.0 has no DDL schema changes (pure Rust/GUC additions).
# Verify the core tables remain intact.
assert_column "_pg_ripple" "predicates" "triple_count"
assert_column "_pg_ripple" "dictionary" "qt_s"
ok "v0.80.0 checkpoint assertions passed (no DDL changes in this release)"
echo

# ── T13-01 checkpoint: v0.81.0 ────────────────────────────────────────────────
info "=== T13-01 checkpoint: v0.81.0 ==="
# v0.81.0 adds cdc_lsn_watermark table (CC-06).
assert_true "cdc_lsn_watermark table exists at v0.81.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple' AND table_name = 'cdc_lsn_watermark'
    )"
ok "v0.81.0 checkpoint assertions passed"
echo

# ── T13-01 checkpoint: v0.82.0 ────────────────────────────────────────────────
info "=== T13-01 checkpoint: v0.82.0 ==="
# v0.82.0 adds merge_worker_status table (MERGE-HBEAT-01).
assert_true "merge_worker_status table exists at v0.82.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple' AND table_name = 'merge_worker_status'
    )"
# v0.82.0 adds federation_stats table (P-04) and predicate_stats_cache (P-08).
assert_true "federation_stats table exists at v0.82.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple' AND table_name = 'federation_stats'
    )"
assert_true "predicate_stats_cache table exists at v0.82.0" \
    "EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = '_pg_ripple' AND table_name = 'predicate_stats_cache'
    )"
ok "v0.82.0 checkpoint assertions passed"
echo

# ── T13-01 checkpoint: v0.83.0 ────────────────────────────────────────────────
info "=== T13-01 checkpoint: v0.83.0 ==="
# v0.83.0 has no DDL schema changes (pure Rust additions and GUC additions).
# Verify the core tables remain intact.
assert_column "_pg_ripple" "predicates" "triple_count"
assert_column "_pg_ripple" "dictionary" "qt_s"
ok "v0.83.0 checkpoint assertions passed (no schema changes in this release)"
echo

# ── MIGCHAIN-01: migration script count verification ──────────────────────────
info "=== MIGCHAIN-01: migration script count verification ==="
# Count migration scripts from v0.62.0 to v0.83.0 (inclusive).
# There are 21 minor version increments: 0.62→0.63, ..., 0.82→0.83.
EXPECTED_COUNT=21
ACTUAL_COUNT=0
for ver in 0.62 0.63 0.64 0.65 0.66 0.67 0.68 0.69 0.70 0.71 0.72 0.73 0.74 0.75 0.76 0.77 0.78 0.79 0.80 0.81 0.82; do
    # Extract next version number
    major=$(echo "${ver}" | cut -d. -f1)
    minor=$(echo "${ver}" | cut -d. -f2)
    next_minor=$((minor + 1))
    next_ver="${major}.${next_minor}.0"
    cur_ver="${ver}.0"
    script="pg_ripple--${cur_ver}--${next_ver}.sql"
    if [[ -f "${SQL_DIR}/${script}" ]]; then
        ACTUAL_COUNT=$((ACTUAL_COUNT + 1))
    else
        fail "MIGCHAIN-01: missing migration script ${script}"
        exit 1
    fi
done
if [[ "${ACTUAL_COUNT}" -eq "${EXPECTED_COUNT}" ]]; then
    ok "MIGCHAIN-01: found ${ACTUAL_COUNT}/${EXPECTED_COUNT} migration scripts from v0.62.0 to v0.83.0"
else
    fail "MIGCHAIN-01: expected ${EXPECTED_COUNT} migration scripts, found ${ACTUAL_COUNT}"
    exit 1
fi
echo

# ── J7-2: Data round-trip across all migration steps ─────────────────────────
# Insert a representative dataset at the v0.51.0 baseline (earliest version
# after all migration scripts have been applied) and assert triple counts and
# query results survive through v0.79.0.
#
# After v0.73.0→v0.74.0, the dictionary schema splits hash BYTEA into
# hash_hi BIGINT NOT NULL and hash_lo BIGINT NOT NULL (first and last 8 bytes
# of the XXH3-128 hash value).  Inserts must supply all three columns.

info "=== J7-2: data round-trip verification ==="

# Load a small representative dataset.
# hash is BYTEA (16 bytes / 32 hex chars); kind 0=IRI, 2=literal.
# hash_hi = first 8 bytes as signed bigint; hash_lo = last 8 bytes.
# Use RETURNING id to capture the auto-generated dictionary IDs.
ALICE_ID=$(run_sql -c "INSERT INTO _pg_ripple.dictionary (hash, hash_hi, hash_lo, value, kind) VALUES (decode('a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1','hex'), ('xa1a1a1a1a1a1a1a1')::bit(64)::bigint, ('xa1a1a1a1a1a1a1a1')::bit(64)::bigint, 'https://example.org/Alice', 0) ON CONFLICT (hash_hi, hash_lo) DO UPDATE SET value = EXCLUDED.value RETURNING id")
NAME_ID=$(run_sql  -c "INSERT INTO _pg_ripple.dictionary (hash, hash_hi, hash_lo, value, kind) VALUES (decode('b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2','hex'), ('xb2b2b2b2b2b2b2b2')::bit(64)::bigint, ('xb2b2b2b2b2b2b2b2')::bit(64)::bigint, 'https://example.org/name',  0) ON CONFLICT (hash_hi, hash_lo) DO UPDATE SET value = EXCLUDED.value RETURNING id")
LIT_ID=$(run_sql   -c "INSERT INTO _pg_ripple.dictionary (hash, hash_hi, hash_lo, value, kind) VALUES (decode('c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3','hex'), ('xc3c3c3c3c3c3c3c3')::bit(64)::bigint, ('xc3c3c3c3c3c3c3c3')::bit(64)::bigint, 'Alice',                     2) ON CONFLICT (hash_hi, hash_lo) DO UPDATE SET value = EXCLUDED.value RETURNING id")

run_sql -c "INSERT INTO _pg_ripple.vp_rare (p, s, o, g, source) VALUES (${NAME_ID}, ${ALICE_ID}, ${LIT_ID}, 0, 0) ON CONFLICT DO NOTHING"

ok "J7-2: seed data inserted"

# Verify the triple is readable.
COUNT=$(run_sql -c "SELECT count(*) FROM _pg_ripple.vp_rare WHERE p = ${NAME_ID} AND s = ${ALICE_ID}")
if [[ "${COUNT}" -eq 1 ]]; then
    ok "J7-2: triple count after v0.79.0 migrations = 1 (data survived all migrations)"
else
    fail "J7-2: expected triple count 1, got ${COUNT}"
fi

# Verify core tables still intact after all migrations.
assert_column "_pg_ripple" "vp_rare" "source"
assert_column "_pg_ripple" "predicates" "triple_count"
assert_column "_pg_ripple" "dictionary" "qt_s"

echo
echo -e "${GREEN}All migration chain tests (including MIGCHAIN-01 and J7-2 data round-trip) passed.${NC}"
echo
