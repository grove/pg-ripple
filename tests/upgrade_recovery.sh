#!/usr/bin/env bash
# Exercise rollback, restart, and dump/restore around an extension upgrade.
# Requires a running PostgreSQL 18 instance with the candidate installed.
set -euo pipefail

PGHOST="${PGHOST:-${PGRX_HOST:-${HOME}/.pgrx}}"
PGPORT="${PGPORT:-${PGRX_PORT:-28818}}"
PGUSER="${PGUSER:-${PGRX_USER:-$(whoami)}}"
PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER")
DB="pg_ripple_upgrade_recovery_$$"
RESTORE_DB="${DB}_restore"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-recovery.XXXXXX")"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GRAPH="${ROOT}/target/migration-graph.json"

cleanup() {
    "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${RESTORE_DB}\";" >/dev/null 2>&1 || true
    "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${DB}\";" >/dev/null 2>&1 || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

python3 "${ROOT}/scripts/migration_graph.py" --output "${GRAPH}" >/dev/null
BASE_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["base_version"])' "${GRAPH}")"
TARGET_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["target_version"])' "${GRAPH}")"

PG_SHARE="$(pg_config --sharedir 2>/dev/null || true)"
if [[ ! -f "$PG_SHARE/extension/pg_ripple--${BASE_VERSION}.sql" ]]; then
    echo "SKIP: pg_ripple ${BASE_VERSION} installation script is not installed"
    exit 0
fi

"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"${DB}\";" >/dev/null
"${PSQL[@]}" -d "$DB" <<SQL
CREATE EXTENSION pg_ripple VERSION '${BASE_VERSION}';
SELECT pg_ripple.insert_triple(
    '<https://recovery.example/s>',
    '<https://recovery.example/p>',
    '"v"'
);
SQL

before="$(${PSQL[@]} -d "$DB" -tAc "SELECT pg_ripple.triple_count();" | tr -d ' ')"

pg_dump -Fc -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$DB" -f "$WORKDIR/backup.dump"

if "${PSQL[@]}" -d "$DB" -c "BEGIN; ALTER EXTENSION pg_ripple UPDATE TO '${TARGET_VERSION}'; SELECT 1 / 0; COMMIT;" >/dev/null 2>&1; then
    echo "FAIL: intentionally failed migration transaction succeeded" >&2
    exit 1
fi
version="$(${PSQL[@]} -d "$DB" -tAc "SELECT extversion FROM pg_extension WHERE extname='pg_ripple';" | tr -d '[:space:]')"
[[ "$version" == "$BASE_VERSION" ]] || { echo "FAIL: failed migration changed extension version to $version" >&2; exit 1; }

# Rerun the corrected migration after the failed transaction has rolled back.
"${PSQL[@]}" -d "$DB" -c "ALTER EXTENSION pg_ripple UPDATE TO '${TARGET_VERSION}';" >/dev/null
version="$(${PSQL[@]} -d "$DB" -tAc "SELECT extversion FROM pg_extension WHERE extname='pg_ripple';" | tr -d '[:space:]')"
[[ "$version" == "$TARGET_VERSION" ]] || { echo "FAIL: corrected migration did not apply" >&2; exit 1; }

data_dir="$(${PSQL[@]} -d "$DB" -tAc 'SHOW data_directory;' | tr -d '[:space:]')"
if ! command -v pg_ctl >/dev/null 2>&1; then
    echo "FAIL: pg_ctl is required for restart recovery verification" >&2
    exit 1
fi

if [[ "${PG_RIPPLE_RESILIENCE_REQUIRE_SIGKILL:-0}" == "1" ]]; then
    postmaster_pid_file="$data_dir/postmaster.pid"
    [[ -r "$postmaster_pid_file" ]] || {
        echo "FAIL: postmaster.pid is not readable for SIGKILL recovery verification" >&2
        exit 1
    }
    postmaster_pid="$(head -n 1 "$postmaster_pid_file")"
    [[ "$postmaster_pid" =~ ^[0-9]+$ && "$postmaster_pid" -gt 1 ]] || {
        echo "FAIL: invalid postmaster PID in $postmaster_pid_file" >&2
        exit 1
    }
    kill -KILL "$postmaster_pid"
    for attempt in $(seq 1 30); do
        if ! pg_ctl status -D "$data_dir" >/dev/null 2>&1; then
            break
        fi
        [[ "$attempt" -lt 30 ]] || {
            echo "FAIL: postmaster remained alive after SIGKILL" >&2
            exit 1
        }
        sleep 1
    done
    pg_ctl start -D "$data_dir" -w -t 60 >/dev/null
    echo "SIGKILL restart completed"
fi

pg_ctl -D "$data_dir" restart -m fast -w >/dev/null

after_restart="$(${PSQL[@]} -d "$DB" -tAc "SELECT pg_ripple.triple_count();" | tr -d ' ')"
[[ "$after_restart" == "$before" ]] || { echo "FAIL: triple count changed after restart" >&2; exit 1; }

"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"${RESTORE_DB}\";" >/dev/null
pg_restore -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$RESTORE_DB" "$WORKDIR/backup.dump"
"${PSQL[@]}" -d "$RESTORE_DB" -c "ALTER EXTENSION pg_ripple UPDATE TO '${TARGET_VERSION}';" >/dev/null
restored_version="$(${PSQL[@]} -d "$RESTORE_DB" -tAc "SELECT extversion FROM pg_extension WHERE extname='pg_ripple';" | tr -d '[:space:]')"
[[ "$restored_version" == "$TARGET_VERSION" ]] || { echo "FAIL: restored extension did not upgrade to $TARGET_VERSION" >&2; exit 1; }
restored="$(${PSQL[@]} -d "$RESTORE_DB" -tAc "SELECT pg_ripple.triple_count();" | tr -d ' ')"
[[ "$restored" == "$before" ]] || { echo "FAIL: triple count changed after backup restore" >&2; exit 1; }

echo "PASS: migration rollback, restart, backup restore, and upgrade preserved $before triple(s)"
